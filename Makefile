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

U=user
R=target/xv6/$(PROFILE)
RN=target/$(PROFILE)

RUST_CROSS_TARGET=riscv64gc-unknown-none-elf
RX=target/$(RUST_CROSS_TARGET)/$(PROFILE)

RUPROGS=\
	cat\
	echo\
	forktest\
	grep\
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
	grind\

RX_RUPROGS=$(patsubst %,$(RX)/%,$(RUPROGS))
R_RUPROGS=$(patsubst %,$R/%,$(RUPROGS))

UPROGS=\
	$U/_usertests\

# riscv64-unknown-elf- or riscv64-linux-gnu-
# perhaps in /opt/riscv/bin
#TOOLPREFIX =

# Try to infer the correct TOOLPREFIX if not set
ifndef TOOLPREFIX
TOOLPREFIX := $(shell if riscv64-elf-objdump -i 2>&1 | grep 'elf64-big' >/dev/null 2>&1; \
	then echo 'riscv64-elf-'; \
	elif riscv64-unknown-elf-objdump -i 2>&1 | grep 'elf64-big' >/dev/null 2>&1; \
	then echo 'riscv64-unknown-elf-'; \
	elif riscv64-linux-gnu-objdump -i 2>&1 | grep 'elf64-big' >/dev/null 2>&1; \
	then echo 'riscv64-linux-gnu-'; \
	elif riscv64-unknown-linux-gnu-objdump -i 2>&1 | grep 'elf64-big' >/dev/null 2>&1; \
	then echo 'riscv64-unknown-linux-gnu-'; \
	else echo "***" 1>&2; \
	echo "*** Error: Couldn't find a riscv64 version of GCC/binutils." 1>&2; \
	echo "*** To turn off this error, run 'gmake TOOLPREFIX= ...'." 1>&2; \
	echo "***" 1>&2; exit 1; fi)
endif

QEMU = qemu-system-riscv64

CC = $(TOOLPREFIX)gcc
AS = $(TOOLPREFIX)gas
LD = $(TOOLPREFIX)ld
OBJCOPY = $(TOOLPREFIX)objcopy
OBJDUMP = $(TOOLPREFIX)objdump

CFLAGS = -Wall -Werror -O -fno-omit-frame-pointer -ggdb -gdwarf-2
CFLAGS += -MD
CFLAGS += -mcmodel=medany
# CFLAGS += -ffreestanding -fno-common -nostdlib -mno-relax
CFLAGS += -fno-common -nostdlib
CFLAGS += -fno-builtin-strncpy -fno-builtin-strncmp -fno-builtin-strlen -fno-builtin-memset
CFLAGS += -fno-builtin-memmove -fno-builtin-memcmp -fno-builtin-log -fno-builtin-bzero
CFLAGS += -fno-builtin-strchr -fno-builtin-exit -fno-builtin-malloc -fno-builtin-putc
CFLAGS += -fno-builtin-free
CFLAGS += -fno-builtin-memcpy -Wno-main
CFLAGS += -fno-builtin-printf -fno-builtin-fprintf -fno-builtin-vprintf
CFLAGS += -I.
CFLAGS += $(shell $(CC) -fno-stack-protector -E -x c /dev/null >/dev/null 2>&1 && echo -fno-stack-protector)

# Disable PIE when possible (for Ubuntu 16.10 toolchain)
ifneq ($(shell $(CC) -dumpspecs 2>/dev/null | grep -e '[^f]no-pie'),)
CFLAGS += -fno-pie -no-pie
endif
ifneq ($(shell $(CC) -dumpspecs 2>/dev/null | grep -e '[^f]nopie'),)
CFLAGS += -fno-pie -nopie
endif

LDFLAGS = -z max-page-size=4096 --gc-sections

all: $R/kernel $R/kernel.asm $R/kernel.sym fs.img $U/initcode
all: $(UPROGS)
all: $(R_RUPROGS) $(addsuffix .sym,$(R_RUPROGS)) $(addsuffix .asm,$(R_RUPROGS))

%.asm: %
	$(OBJDUMP) -SC $< > $@

%.sym: %
	$(OBJDUMP) -t $< | sed '1,/SYMBOL TABLE/d; s/ .* / /; /^$$/d' | c++filt > $@

$U/initcode: $U/initcode.S
	$(CC) $(CFLAGS) -march=rv64g -nostdinc -I. -Ikernel -c $U/initcode.S -o $U/initcode.o
	$(LD) $(LDFLAGS) -N -e start -Ttext 0 -o $U/initcode.out $U/initcode.o
	$(OBJCOPY) -S -O binary $U/initcode.out $U/initcode
	$(OBJDUMP) -SC $U/initcode.o > $U/initcode.asm

ULIB = $U/ulib.o $U/usys.o $U/printf.o $U/umalloc.o

_%: %.o $(ULIB) $U/user.ld
	$(LD) $(LDFLAGS) -T $U/user.ld -e _start -o $@ $< $(ULIB)
	$(OBJDUMP) -SC $@ > $*.asm
	$(OBJDUMP) -t $@ | sed '1,/SYMBOL TABLE/d; s/ .* / /; /^$$/d' | c++filt > $*.sym

$U/usys.S : $U/usys.pl
	perl $U/usys.pl > $U/usys.S

$U/usys.o : $U/usys.S
	$(CC) $(CFLAGS) -c -o $U/usys.o $U/usys.S

$(RX)/kernel:
	cargo build -p kernel $(CARGO_PROFILE_FLAG) --target $(RUST_CROSS_TARGET)

define user_rule
$$(RX)/$(1):
	cargo build -p user --bin $(1) $$(CARGO_PROFILE_FLAG) --target $$(RUST_CROSS_TARGET)

endef

$(foreach u,$(RUPROGS),$(eval $(call user_rule,$(u))))

$(RN)/mkfs:
	cargo build -p mkfs $(CARGO_PROFILE_FLAG)

%/:
	mkdir -p $@

# create separate debuginfo file
# https://users.rust-lang.org/t/how-to-gdb-with-split-debug-files/102989/3
$R/%.debug: $(RX)/% | $$(dir $$@)
	$(OBJCOPY) --only-keep-debug $< $@

$R/%: $(RX)/% $(R)/%.debug | $$(dir $$@)
	$(OBJCOPY) --strip-debug --strip-unneeded --remove-section=".gnu_debuglink" --add-gnu-debuglink="$@.debug" $< $@

fs.img: $(RN)/mkfs README $(UPROGS) $(R_RUPROGS)
	$(RN)/mkfs $@ README $(UPROGS) $(R_RUPROGS)

# convert Cargo's .d file (absolute path -> relative path)
target/%.rel.d: target/%.d
	sed "s@$$(pwd)/@@g" $< > $@

-include $(RX)/kernel.rel.d
-include $(addsuffix .rel.d,$(RX_RUPROGS))
-include $(RN)/mkfs.rel.d
-include user/*.d

clean:
	rm -f *.tex *.dvi *.idx *.aux *.log *.ind *.ilg \
	*/*.o */*.d */*.asm */*.sym \
	$U/initcode $U/initcode.out fs.img \
	.gdbinit \
	$U/usys.S \
	$(UPROGS)
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
