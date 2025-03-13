use core::mem;

use riscv::{
    interrupt::{
        Trap,
        supervisor::{Exception, Interrupt},
    },
    register::{
        satp, scause, sepc,
        sstatus::{self, SPP},
        stval,
        stvec::{self, Stvec, TrapMode},
    },
};

use super::{kernel_vec, plic, timer, trampoline};
use crate::{
    console::uart,
    cpu, fs, interrupt,
    memory::{
        PAGE_SIZE,
        layout::{KSTACK_PAGES, UART0_IRQ, VIRTIO0_IRQ},
    },
    println,
    proc::{self, Proc, ProcPrivateDataGuard},
    syscall,
};

pub fn init_hart() {
    let mut stvec = stvec::Stvec::from_bits(0);
    stvec.set_address(kernel_vec::kernel_vec as usize);
    stvec.set_trap_mode(TrapMode::Direct);
    unsafe {
        stvec::write(stvec);
    }
}

/// Handles an interrupt, exception, or system call from user space.
///
/// Called from trampoline.S
extern "C" fn trap_user() {
    assert_eq!(sstatus::read().spp(), SPP::User, "from user mode");

    // send interrupts and exceptions to kerneltrap(),
    // since we're now in the kernel.
    let mut stvec = stvec::Stvec::from_bits(0);
    stvec.set_address(kernel_vec::kernel_vec as usize);
    stvec.set_trap_mode(TrapMode::Direct);
    unsafe {
        stvec::write(stvec);
    }

    let p = Proc::current();
    let Some(mut private) = p.borrow_private() else {
        // process is already exited (in zombie state)
        proc::yield_(p);
        unreachable!();
    };

    // save user program counter.
    private.trapframe_mut().unwrap().epc = sepc::read();

    let scause: Trap<Interrupt, Exception> = scause::read().cause().try_into().unwrap();
    let mut which_dev = IntrKind::NotRecognized;
    match scause {
        Trap::Exception(Exception::UserEnvCall) => {
            // system call
            if p.shared().lock().killed() {
                proc::exit(p, private, -1);
            }

            // sepc points to the ecall instruction,
            // but we want to return to the next instruction.
            private.trapframe_mut().unwrap().epc += 4;

            // an interrupt will change sepc, scause, and sstatus,
            // so enable only now that we're done with those registers.
            interrupt::enable();

            let mut private_opt = Some(private);
            syscall::syscall(p, &mut private_opt);
            private = private_opt.unwrap();
        }
        Trap::Exception(e) => {
            let mut shared = p.shared().lock();
            let pid = shared.pid();
            let name = shared.name().display();
            let sepc = sepc::read();
            let stval = stval::read();
            println!("usertrap: exception {e:?} pid={pid} name={name}");
            println!("          sepc={sepc:#x} stval={stval:#x}");
            shared.kill();
        }
        Trap::Interrupt(int) => {
            which_dev = handle_dev_interrupt(int);
            if which_dev == IntrKind::NotRecognized {
                let mut shared = p.shared().lock();
                let pid = shared.pid();
                let name = shared.name().display();
                let sepc = sepc::read();
                let stval = stval::read();
                println!("usertrap: unexpected interrupt {int:?} pid={pid} name={name}");
                println!("          sepc={sepc:#x} stval={stval:#x}");
                shared.kill();
            }
        }
    }

    if p.shared().lock().killed() {
        proc::exit(p, private, -1);
    }

    // gibe up the CPU if this is a timer interrupt.
    if which_dev == IntrKind::Timer {
        proc::yield_(p);
    }

    trap_user_ret(private);
}

/// Returns to user space
pub fn trap_user_ret(mut private: ProcPrivateDataGuard) {
    // we're about to switch destination of traps from
    // kerneltrap() to usertrap(), so turn off interrupts until
    // we're back in user space, where usertrap() is correct.
    interrupt::disable();

    // send syscalls, interrupts, and exceptions to uservec in trampoline.S
    let trampoline_uservec = trampoline::user_vec_addr();
    let mut stvec = Stvec::from_bits(0);
    stvec.set_address(trampoline_uservec.addr());
    stvec.set_trap_mode(TrapMode::Direct);
    unsafe {
        stvec::write(stvec);
    }

    // set up trapframe values that uservec will need when
    // the process next traps into the kernel.
    let kstack = private.kstack();
    let tf = private.trapframe_mut().unwrap();
    tf.kernel_satp = satp::read().bits(); // kernel page table
    tf.kernel_sp = kstack.byte_add(KSTACK_PAGES * PAGE_SIZE).addr(); // process's kernel stack
    tf.kernel_trap = trap_user as usize;
    tf.kernel_hartid = cpu::id();

    // set up the registers that trampoline.S's sret will use
    // to get to user space.

    // set S Previous Privilege mode to User.
    unsafe {
        sstatus::set_spp(SPP::User);
        sstatus::set_spie();
    }

    // set S Exception Program Counter to the saved user pc.
    unsafe {
        sepc::write(private.trapframe().unwrap().epc);
    }

    // tell trampoline.S the user page table to switch to.
    let satp = (8 << 60) | (private.pagetable().unwrap().phys_page_num().value());
    drop(private);

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
    let scause: Trap<Interrupt, Exception> = scause::read().cause().try_into().unwrap();

    assert_eq!(sstatus.spp(), SPP::Supervisor, "from supervisor mode");
    assert!(!interrupt::is_enabled());

    let (int, which_dev) = match scause {
        Trap::Exception(e) => {
            let stval = stval::read();
            println!("kernel trap: exception {e:#?}");
            println!("             sepc={sepc:#x} stval={stval:#x}");
            panic!("unexpected trap (exception)");
        }
        Trap::Interrupt(int) => (int, handle_dev_interrupt(int)),
    };

    match which_dev {
        IntrKind::Timer => {
            // give up the CPU if this is a timer interrupt.
            if let Some(p) = Proc::try_current() {
                proc::yield_(p)
            }
        }
        IntrKind::Other => {}
        IntrKind::NotRecognized => {
            let stval = stval::read();
            println!("kernel trap: interrupt {int:?}");
            println!("             sepc={sepc:#x} stval={stval:#x}");
            panic!("unexpected trap (interrupt)");
        }
    }

    // the yield_() may have caused some traps to occur,
    // so restore trap registers for use by kernelvec's sepc instruction.
    unsafe {
        sepc::write(sepc);
    }
    unsafe {
        sstatus::write(sstatus);
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
fn handle_dev_interrupt(int: Interrupt) -> IntrKind {
    match int {
        Interrupt::SupervisorSoft => IntrKind::NotRecognized,
        Interrupt::SupervisorTimer => {
            timer::handle_interrupt();
            IntrKind::Timer
        }
        Interrupt::SupervisorExternal => {
            // this is a supervisor external interrupt, via PLIC.

            // irq indicates which device interrupted.
            let irq = plic::claim();

            if irq == UART0_IRQ {
                uart::handle_interrupt();
            } else if irq == VIRTIO0_IRQ {
                fs::virtio_disk::handle_interrupt();
            } else if irq > 0 {
                println!("unexpected interrupt irq={irq}");
            }

            // the PLIC allows each device to raise at most one
            // interrupt at a time; tell the PLIC the device is
            // now allowed to interrupt again.
            if irq > 0 {
                plic::complete(irq);
            }
            IntrKind::Other
        }
    }
}
