use core::{arch::asm, mem, ptr};

use riscv::{
    interrupt::Trap,
    register::{
        satp, scause, sepc,
        sstatus::{self, SPP},
        stval,
        stvec::{self, TrapMode},
    },
};

use crate::{
    console::uart,
    cpu, interrupt,
    memory::{
        layout::{UART0_IRQ, VIRTIO0_IRQ},
        vm::PAGE_SIZE,
    },
    println,
    proc::{self, Proc},
    sync::SpinLock,
    syscall, virtio_disk,
};

use super::{kernel_vec, plic, trampoline};

pub static TICKS: SpinLock<u64> = SpinLock::new(0);

pub fn init_hart() {
    unsafe {
        stvec::write(kernel_vec::kernel_vec as usize, TrapMode::Direct);
    }
}

/// Handles an interrupt, exception, or system call from user space.
///
/// Called from trampoline.S
extern "C" fn trap_user() {
    let mut which_dev = IntrKind::NotRecognized;

    assert_eq!(sstatus::read().spp(), SPP::User, "from user mode");

    // send interrupts and exceptions to kerneltrap(),
    // since we're now in the kernel.
    unsafe {
        stvec::write(kernel_vec::kernel_vec as usize, TrapMode::Direct);
    }

    let p = Proc::current();

    // save user program counter.
    p.trapframe_mut().unwrap().epc = sepc::read() as u64;

    let cause = scause::read().cause();
    if cause == Trap::Exception(8) {
        // system call

        if p.killed() {
            proc::exit(p, -1);
        }

        // sepc points to the ecall instruction,
        // but we want to return to the next instruction.
        p.trapframe_mut().unwrap().epc += 4;

        // an interrupt will change sepc, scause, and sstatus,
        // so enable only now that we're done with those registers.
        interrupt::enable();

        syscall::syscall(p);
    } else {
        which_dev = handle_dev_interrupt();
        if which_dev == IntrKind::NotRecognized {
            println!(
                "usertrap: unexpected scause {:#x} pid={}\n",
                scause::read().bits(),
                p.pid(),
            );
            println!(
                "         sepc={:#x} stval={:#x}",
                sepc::read(),
                stval::read(),
            );
            p.set_killed();
        }
    }

    if p.killed() {
        proc::exit(p, -1);
    }

    // gibe up the CPU if this is a timer interrupt.
    if which_dev == IntrKind::Timer {
        proc::yield_(p);
    }

    trap_user_ret(p);
}

/// Returns to user space
pub fn trap_user_ret(p: &Proc) {
    // we're about to switch destination of traps from
    // kerneltrap() to usertrap(), so turn off interrupts until
    // we're back in user space, where usertrap() is correct.
    interrupt::disable();

    // send syscalls, interrupts, and exceptions to uservec in trampoline.S
    let trampoline_uservec = trampoline::user_vec_addr();
    unsafe {
        stvec::write(trampoline_uservec.addr(), TrapMode::Direct);
    }

    // set up trapframe values that uservec will need when
    // the process next traps into the kernel.
    let kstack = p.kstack();
    let tf = p.trapframe_mut().unwrap();
    tf.kernel_satp = satp::read().bits() as u64; // kernel page table
    tf.kernel_sp = (kstack + PAGE_SIZE) as u64; // process's kernel stack
    tf.kernel_trap = (trap_user as usize) as u64;
    let hartid: u64;
    unsafe { asm!("mv {}, tp", out(reg) hartid) };
    tf.kernel_hartid = hartid;

    // set up the registers that trampoline.S's sret will use
    // to get to user space.

    // set S Previous Privilege mode to User.
    unsafe {
        sstatus::set_spp(SPP::User);
        sstatus::set_spie();
    }

    // set S Exception Program Counter to the saved user pc.
    sepc::write(p.trapframe().unwrap().epc as usize);

    // tell trampoline.S the user page table to switch to.
    let satp = (8 << 60) | (ptr::from_ref(p.pagetable().unwrap()).addr() >> 12);

    // jump to userret in trampoline.S at the top of memory, which
    // switches to the user page table, restores user registers,
    // and switches to user mode with sret.
    let trampoline_user_ret = trampoline::user_ret_addr();
    unsafe {
        let f: extern "C" fn(u64) = mem::transmute(trampoline_user_ret.addr());
        f(satp as u64);
    }
}

/// Interrupts and exceptions from kernel code go here via kernelvec,
/// on whatever the current kernel stack is.
pub extern "C" fn trap_kernel() {
    let sepc = sepc::read();
    let sstatus = sstatus::read();
    let sstatus_bits: usize;
    unsafe {
        asm!("csrr {}, sstatus", out(reg) sstatus_bits);
    }
    let scause = scause::read();

    assert_eq!(sstatus.spp(), SPP::Supervisor, "from supervisor mode");
    assert!(!interrupt::is_enabled());

    let which_dev = handle_dev_interrupt();
    if which_dev == IntrKind::NotRecognized {
        println!(
            "scause={:#x} sepc={:#x} stval={:#x}",
            scause.bits(),
            sepc,
            stval::read()
        );
        panic!("unexpected trap");
    }

    // give up the CPU if this is a timer interrupt.
    if which_dev == IntrKind::Timer {
        if let Some(p) = Proc::try_current() {
            proc::yield_(p)
        }
    }

    // the yield_() may have caused some traps to occur,
    // so restore trap registers for use by kernelvec's sepc instruction.
    sepc::write(sepc);
    unsafe {
        asm!("csrw sstatus, {}", in(reg) sstatus_bits);
    }
}

fn handle_clock_interrupt() {
    if cpu::id() == 0 {
        let mut ticks = TICKS.lock();
        *ticks += 1;
        proc::wakeup((&raw const TICKS).cast());
        drop(ticks);
    }

    // ask for the next timer interrupt. this also clears
    // the interrupt request. 100_0000 is about a tenth
    // of a second.
    let time: usize;
    unsafe {
        asm!("csrr {}, time", out(reg) time);
        asm!("csrw stimecmp, {}", in(reg) time + 100_0000);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IntrKind {
    Timer,
    Other,
    NotRecognized,
}

/// Check if it's an external interrupt of software interrupt,
/// and handle it.
///
/// return 2 if timer interrupt,
/// 1 if other device,
/// 0 if not recognized
fn handle_dev_interrupt() -> IntrKind {
    let scause = scause::read();

    if scause.cause() == Trap::Interrupt(9) {
        // this is a supervisor external interrupt, via PLIC.

        // irq indicates which device interrupted.
        let irq = plic::claim();

        if irq == UART0_IRQ {
            uart::handle_interrupt();
        } else if irq == VIRTIO0_IRQ {
            virtio_disk::handle_interrupt();
        } else if irq > 0 {
            println!("unexpected interrupt irq={irq}");
        }

        // the PLIC allows each device to raise at most one
        // interrupt at a time; tell the PLIC the device is
        // now allowed to interrupt again.
        if irq > 0 {
            plic::complete(irq);
        }
        return IntrKind::Other;
    }

    if scause.cause() == Trap::Interrupt(5) {
        // timer interrupt
        handle_clock_interrupt();
        return IntrKind::Timer;
    }

    IntrKind::NotRecognized
}
