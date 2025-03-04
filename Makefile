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

R=target/xv6/$(PROFILE)
I=target/xv6/initcode
RN=target/$(PROFILE)

RUST_CROSS_TARGET=riscv64gc-unknown-none-elf
RX=target/$(RUST_CROSS_TARGET)/$(PROFILE)

RUPROGS=\
	cat\
	echo\
	forktest\
	grep\
	grind\
	hello\
	init\
	kill\
	ln\
	ls\
	mkdir\
	rm\
	sh\
	stressfs\
	wc\
	zombie\
	usertests\

RX_RUPROGS=$(patsubst %,$(RX)/%,$(RUPROGS))
R_RUPROGS=$(patsubst %,$R/%,$(RUPROGS))

QEMU = qemu-system-riscv64

OBJCOPY = llvm-objcopy

all: $R/kernel $I/initcode $(R_RUPROGS) fs.img

# create separate debuginfo file
# https://users.rust-lang.org/t/how-to-gdb-with-split-debug-files/102989/3
target/xv6/%.debug: target/$(RUST_CROSS_TARGET)/% | $$(dir $$@)
	$(OBJCOPY) --only-keep-debug $< $@

target/xv6/%: target/$(RUST_CROSS_TARGET)/% target/xv6/%.debug | $$(dir $$@)
	$(OBJCOPY) --strip-debug --strip-unneeded --remove-section=".gnu_debuglink" --add-gnu-debuglink="$@.debug" $< $@

target/$(RUST_CROSS_TARGET)/initcode/initcode:
	cargo build -p user --bin initcode --profile initcode --target $(RUST_CROSS_TARGET)

$I/initcode.bin: $I/initcode
	$(OBJCOPY) -S -O binary $< $@

$(RX)/kernel: $I/initcode.bin
	INIT_CODE_PATH="$(PWD)/$I/initcode.bin" \
		cargo build -p kernel $(CARGO_PROFILE_FLAG) --target $(RUST_CROSS_TARGET) --features initcode_env

define user_rule
$$(RX)/$(1):
	cargo build -p user --bin $(1) $$(CARGO_PROFILE_FLAG) --target $$(RUST_CROSS_TARGET)

endef

$(foreach u,$(RUPROGS),$(eval $(call user_rule,$(u))))

$(RN)/mkfs:
	cargo build -p mkfs $(CARGO_PROFILE_FLAG)

%/:
	mkdir -p $@

fs.img: $(RN)/mkfs README $(R_RUPROGS)
	$(RN)/mkfs $@ README $(R_RUPROGS)

# convert Cargo's .d file (absolute path -> relative path)
target/%.rel.d: target/%.d
	sed "s@$$(pwd)/@@g" $< > $@

-include $(RX)/kernel.rel.d
-include $(addsuffix .rel.d,$(RX_RUPROGS))
-include $(RN)/mkfs.rel.d
-include user/*.d

clean:
	rm -f fs.img .gdbinit
	cargo clean

check: cargo-clippy typos doc

test: cargo-test cargo-miri-test

cargo-clippy:
	cargo clippy --workspace

cargo-test:
	cargo test --workspace

cargo-miri-test:
	cargo miri test --workspace

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

qemu: $R/kernel fs.img
	$(QEMU) $(QEMUOPTS)

.gdbinit: .gdbinit.tmpl-riscv
	sed "s/:1234/:$(GDBPORT)/" < $^ > $@

qemu-gdb: $R/kernel .gdbinit fs.img
	@echo "*** Now run 'gdb' in another window." 1>&2
	$(QEMU) $(QEMUOPTS) -S $(QEMUGDB)

FORCE:
.PHONY: FORCE all clean qemu qemu-gdb check cargo-clippy typos
