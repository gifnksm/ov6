.DELETE_ON_ERROR:
.SECONDARY:
.SECONDEXPANSION:

PROFILE=release
ifeq ($(PROFILE),debug)
# profile name `debug` is reserved, so we can't run `cargo <command> --profile debug`.
CARGO_PROFILE_FLAG=
else
CARGO_PROFILE_FLAG=--profile $(PROFILE)
endif

R=target/ov6/$(PROFILE)
I=target/ov6/initcode
RN=target/$(PROFILE)

RUST_CROSS_TARGET=riscv64imac-unknown-none-elf
RX=target/$(RUST_CROSS_TARGET)/$(PROFILE)
RXI=target/$(RUST_CROSS_TARGET)/initcode

OV6_UTILS=\
	abort\
	cat\
	echo\
	grep\
	halt\
	hello\
	init\
	kill\
	ln\
	ls\
	mkdir\
	reboot\
	rm\
	sh\
	wc\
	zombie\

OV6_USER_TESTS=\
	forktest\
	grind\
	stressfs\
	usertests\

OV6_FS_UTILS=\
	mkfs\

FS_CONTENTS=$(patsubst %,$R/%,$(OV6_UTILS)) $(patsubst %,$R/%,$(OV6_USER_TESTS))

QEMU = qemu-system-riscv64

OBJCOPY = llvm-objcopy

all: $R/kernel $I/initcode $(R_OV6_UTILS) $(R_OV6_USER_TESTS) fs.img

# create separate debuginfo file
# https://users.rust-lang.org/t/how-to-gdb-with-split-debug-files/102989/3
target/ov6/%.debug: target/$(RUST_CROSS_TARGET)/% | $$(dir $$@)
	$(OBJCOPY) --only-keep-debug $< $@

target/ov6/%: target/$(RUST_CROSS_TARGET)/% target/ov6/%.debug | $$(dir $$@)
	$(OBJCOPY) --strip-debug --strip-unneeded --remove-section=".gnu_debuglink" --add-gnu-debuglink="$@.debug" $< $@

target/$(RUST_CROSS_TARGET)/initcode/initcode: FORCE
	cargo build -p ov6_utilities --bin initcode --profile initcode --target $(RUST_CROSS_TARGET)

$I/initcode.bin: $I/initcode
	$(OBJCOPY) -S -O binary $< $@

$(RX)/kernel: $I/initcode.bin FORCE
	INIT_CODE_PATH="$(PWD)/$I/initcode.bin" \
		cargo build -p ov6_kernel $(CARGO_PROFILE_FLAG) --target $(RUST_CROSS_TARGET) --features initcode_env

$(RX)/%.stamp: FORCE
	cargo build -p $(patsubst %.stamp,%,$(notdir $@)) --target $(RUST_CROSS_TARGET) $(CARGO_PROFILE_FLAG)
	touch $@

$(RN)/%.stamp: FORCE
	cargo build -p $(patsubst %.stamp,%,$(notdir $@)) $(CARGO_PROFILE_FLAG)
	touch $@

$(foreach exe,$(OV6_UTILS),$(eval $$(RX)/$(exe): $$(RX)/ov6_utilities.stamp))
$(foreach exe,$(OV6_USER_TESTS),$(eval $$(RX)/$(exe): $$(RX)/ov6_user_tests.stamp))
$(foreach exe,$(OV6_FS_UTILS),$(eval $$(RN)/$(exe): $$(RN)/ov6_fs_utilities.stamp))

%/:
	mkdir -p $@

fs.img: $(RN)/mkfs README $(FS_CONTENTS)
	$(RN)/mkfs $@ README $(FS_CONTENTS)

clean:
	rm -f fs.img .gdbinit
	cargo clean

check: cargo-clippy typos doc

test: cargo-test cargo-miri-test

cargo-clippy:
	cargo clippy --workspace

cargo-test:
	cargo nextest run --workspace

cargo-miri-test:
	cargo miri nextest run --workspace

typos:
	typos

doc:
	cargo doc --workspace --document-private-items
	cargo doc --workspace --document-private-items --target $(RUST_CROSS_TARGET)

# try to generate a unique GDB port
GDBPORT = $(shell expr `id -u` % 5000 + 25000)
# QEMU's gdb stub command line changed in 0.11
QEMUGDB = $(shell if $(QEMU) -help | grep -q '^-gdb'; \
	then echo "-gdb tcp::$(GDBPORT)"; \
	else echo "-s -p $(GDBPORT)"; fi)
ifndef CPUS
CPUS := 3
endif

QEMUOPTS = -machine virt -bios none -kernel $R/kernel -m 128M -smp $(CPUS) -nographic
QEMUOPTS += -global virtio-mmio.force-legacy=false
QEMUOPTS += -drive file=fs.img,if=none,format=raw,id=x0
QEMUOPTS += -device virtio-blk-device,drive=x0,bus=virtio-mmio-bus.0

ifdef QEMU_LOG
QEMUOPTS += -d unimp,guest_errors,int -D target/qemu.log
endif

qemu: $R/kernel fs.img
	$(QEMU) $(QEMUOPTS)

.gdbinit: .gdbinit.tmpl-riscv
	sed "s/:1234/:$(GDBPORT)/" < $^ > $@

qemu-gdb: $R/kernel .gdbinit fs.img
	@echo "*** Now run 'gdb' in another window." 1>&2
	$(QEMU) $(QEMUOPTS) -S $(QEMUGDB)

FORCE:
.PHONY: FORCE all clean qemu qemu-gdb check cargo-clippy typos
