MAKEFLAGS += -j
MAKEFLSGS += --warn-undefined-variables
MAKEFLSGS += --no-builtin-rules
MAKEFLAGS += --no-builtin-variables

SHELL := bash
.SHELLFLAGS := -eu -o pipefail -c
.DELETE_ON_ERROR:
.SECONDARY:
.SECONDEXPANSION:

.PHONY: default
default: all

PROFILE=release
ifeq ($(PROFILE),debug)
# profile name `debug` is reserved, so we can't run `cargo <command> --profile debug`.
CARGO_PROFILE_FLAG=
else
CARGO_PROFILE_FLAG=--profile $(PROFILE)
endif

R=target/ov6/$(PROFILE)

RUST_CROSS_TARGET=riscv64imac-unknown-none-elf
RX=target/$(RUST_CROSS_TARGET)/$(PROFILE)

RX_CARGO_FLAGS=$(CARGO_PROFILE_FLAG) --target $(RUST_CROSS_TARGET) -Z build-std=core,alloc,compiler_builtins
RX_RUST_FLAGS=-C relocation-model=static -C force-frame-pointers=yes

RN_PKGS=ov6_fs_utilities ov6_integration_tests ov6_net_utilities

OV6_KERNEL=\
	kernel\

OV6_SERVICES=\
	init\

OV6_UTILS=\
	abort\
	cat\
	echo\
	false\
	find\
	grep\
	halt\
	hello\
	kill\
	ln\
	ls\
	mkdir\
	pingpong\
	primes\
	reboot\
	rm\
	sh\
	sleep\
	trace\
	true\
	uptime\
	wc\
	xargs\
	zombie\

OV6_USER_TESTS=\
	alarmtest\
	cowtest\
	forktest\
	grind\
	kpgtbl\
	nettest\
	stressfs\
	sysinfo\
	upgtbl\
	usertests\

OV6_FS_UTILS=\
	mkfs\

FS_CONTENTS=$(addprefix $R/,$(OV6_SERVICES) $(OV6_UTILS) $(OV6_USER_TESTS))

QEMU = qemu-system-riscv64

OBJCOPY = llvm-objcopy

# create separate debuginfo file
# https://users.rust-lang.org/t/how-to-gdb-with-split-debug-files/102989/3
target/ov6/%.debug: target/$(RUST_CROSS_TARGET)/% | $$(dir $$@)
	$(OBJCOPY) --only-keep-debug $< $@

target/ov6/%: target/$(RUST_CROSS_TARGET)/% target/ov6/%.debug | $$(dir $$@)
	$(OBJCOPY) --strip-debug --strip-unneeded --remove-section=".gnu_debuglink" --add-gnu-debuglink="$@.debug" $< $@

$(RX)/%.stamp: FORCE
	RUSTFLAGS="$(RX_RUST_FLAGS)" \
		cargo build -p $(patsubst %.stamp,%,$(notdir $@)) $(RX_CARGO_FLAGS)
	touch $@

$(foreach exe,$(OV6_KERNEL),$(eval $$(RX)/$(exe): $$(RX)/ov6_kernel.stamp))
$(foreach exe,$(OV6_SERVICES),$(eval $$(RX)/$(exe): $$(RX)/ov6_services.stamp))
$(foreach exe,$(OV6_UTILS),$(eval $$(RX)/$(exe): $$(RX)/ov6_utilities.stamp))
$(foreach exe,$(OV6_USER_TESTS),$(eval $$(RX)/$(exe): $$(RX)/ov6_user_tests.stamp))

%/:
	mkdir -p $@

$R/fs.img: README $(FS_CONTENTS)
	cargo run --bin mkfs -- $@ README $(FS_CONTENTS)

.PHONY: all
all: $R/kernel $R/fs.img

.PHONY: clean
clean:
	rm -f .gdbinit
	cargo clean

.PHONY: check
check: cargo-clippy typos cargo-doc

.PHONY: test
test: cargo-test-build cargo-miri-test-build .WAIT cargo-test .WAIT cargo-miri-test


.PHONY: cargo-clippy
cargo-clippy: cargo-clippy-lib cargo-clippy-bins cargo-clippy-tests cargo-clippy-benches cargo-clippy-examples

.PHONY: cargo-clippy-lib
cargo-clippy-lib:
	cargo hack clippy --workspace --lib

.PHONY: cargo-clippy-bins
cargo-clippy-bins:
	cargo hack clippy --workspace --bins

.PHONY: cargo-clippy-tests
cargo-clippy-tests:
	cargo hack clippy --workspace --tests --exclude ov6_kernel

.PHONY: cargo-clippy-benches
cargo-clippy-benches:
	cargo hack clippy --workspace --benches --exclude ov6_kernel

.PHONY: cargo-clippy-examples
cargo-clippy-examples:
	cargo hack clippy --workspace --examples

.PHONY: cargo-clippy-cross
cargo-clippy-cross: cargo-clippy-cross-lib cargo-clippy-cross-bins

.PHONY: cargo-clippy-cross-lib
cargo-clippy-cross-lib:
	cargo hack clippy --workspace --lib $(addprefix --exclude ,$(RN_PKGS)) --target $(RUST_CROSS_TARGET)

.PHONY: cargo-clippy-cross-bins
cargo-clippy-cross-bins:
	cargo hack clippy --workspace --bins $(addprefix --exclude ,$(RN_PKGS)) --target $(RUST_CROSS_TARGET)

.PHONY: cargo-test
cargo-test: cargo-test-build
	cargo nextest run --workspace

.PHONY: cargo-test-build
cargo-test-build: $R/kernel $R/fs.img
	cargo nextest run --workspace --no-run

.PHONY: cargo-miri-test
cargo-miri-test: cargo-miri-test-build
	cargo miri nextest run --workspace

.PHONY: cargo-miri-test-build
cargo-miri-test-build:
	cargo miri nextest run --workspace --no-run

.PHONY: typos
typos:
	typos

.PHONY: cargo-doc
cargo-doc:
	cargo hack doc --workspace --no-deps --document-private-items
	cargo hack doc --workspace --no-deps --document-private-items --target $(RUST_CROSS_TARGET) \
		$(addprefix --exclude ,$(RN_PKGS))

# try to generate a unique GDB port
GDB_PORT = $(shell expr `id -u` % 5000 + 25000)
QEMU_GDB_TCP_OPTS = -gdb tcp::$(GDB_PORT)
QEMU_GDB_SOCK_OPTS = -chardev socket,path=$(GDB_SOCK),server=on,wait=off,id=gdb0 -gdb chardev:gdb0

SERVER_PORT = $(shell expr `id -u` % 5000 + 25099)
export SERVER_PORT
FWD_PORT1 = $(shell expr `id -u` % 5000 + 25999)
FWD_PORT2 = $(shell expr `id -u` % 5000 + 30999)

ifndef CPUS
CPUS := 3
endif

QEMU_KERNEL=$R/kernel
QEMU_FS=$R/fs.img
QEMU_MONITOR_SOCK=target/qemu-monitor.socket

QEMU_OPTS = -machine virt -bios none -kernel $(QEMU_KERNEL) -m 128M -smp $(CPUS) -nographic
QEMU_OPTS += -global virtio-mmio.force-legacy=false
QEMU_OPTS += -drive file=$(QEMU_FS),if=none,format=raw,id=x0
QEMU_OPTS += -device virtio-blk-device,drive=x0,bus=virtio-mmio-bus.0
QEMU_OPTS += -netdev user,id=net0,hostfwd=udp::$(FWD_PORT1)-:2000,hostfwd=udp::$(FWD_PORT2)-:2001
QEMU_OPTS += -object filter-dump,id=net0,netdev=net0,file=target/packets.pcap
QEMU_OPTS += -device e1000,netdev=net0,bus=pcie.0
ifdef QEMU_MONITOR_FWD
QEMU_OPTS += -monitor unix:$(QEMU_MONITOR_SOCK),server,nowait
endif

ifdef QEMU_LOG
QEMU_OPTS += -d unimp,guest_errors,int -D target/qemu.log
endif

.PHONY: qemu
qemu: $(QEMU_KERNEL) $(QEMU_FS)
	$(QEMU) $(QEMU_OPTS)

.gdbinit: .gdbinit.tmpl-riscv
	sed "s/:1234/:$(GDB_PORT)/" < $^ > $@

.PHONY: qemu-gdb
qemu-gdb: $(QEMU_KERNEL) $(QEMU_FS) .gdbinit
	@echo "*** Now run 'gdb' in another window." 1>&2
	$(QEMU) $(QEMU_OPTS) -S $(QEMU_GDB_TCP_OPTS)

.PHONY: qemu-gdb-noinit
qemu-gdb-noinit: $(QEMU_KERNEL) $(QEMU_FS)
	@echo "*** Running qemu ***" 1>&2
	@echo "kernel: $(QEMU_KERNEL:$(CURDIR)/%=%)" 1>&2
	@echo "fs: $(QEMU_FS:$(CURDIR)/%=%)" 1>&2
	$(QEMU) $(QEMU_OPTS) -S $(QEMU_GDB_SOCK_OPTS)

FORCE:
.PHONY: FORCE
