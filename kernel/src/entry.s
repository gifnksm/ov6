        # Workaround for spurious LLVM error
        # See also:
        #  - <https://github.com/rust-embedded/riscv/issues/175>
        #  - <https://github.com/rust-embedded/riscv/pull/176>
.attribute arch, "rv64g"
        # qemu -kernel loads the kernel at 0x80000000
        # and causes each hart (i.e. CPU) to jump there.
        # kernel.ld causes the following code to
        # be placed at 0x80000000.
.section .text.init
.global _entry
_entry:
        # set up a stack for kernel.
        # STACK0 is declared in start.rs,
        # with a 4096-byte stack per CPU.
        # sp = STACK0 + (hartid * STACK_SIZE)
        la sp, {STACK0}
        li a0, {STACK_SIZE}
        csrr a1, mhartid
        addi a1, a1, 1
        mul a0, a0, a1
        add sp, sp, a0
        # jump to start() in start.rs
        call {start}
spin:
        j spin
