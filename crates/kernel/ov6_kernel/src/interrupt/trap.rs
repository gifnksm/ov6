use core::mem;

use dataview::Pod;
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
use safe_cast::to_u32;

use super::{kernel_vec, plic, timer, trampoline};
use crate::{
    console::uart,
    cpu, fs, interrupt,
    memory::{
        PAGE_SIZE,
        layout::{KSTACK_PAGES, UART0_IRQ, VIRTIO0_IRQ},
    },
    println,
    proc::{self, Proc, ProcPrivateDataGuard, scheduler},
    syscall,
};

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod)]
pub struct TrapFrame {
    /// Kernel page table.
    pub kernel_satp: usize, // 0
    /// Top of process's kernel stack.
    pub kernel_sp: usize, // 8
    /// usertrap.
    pub kernel_trap: usize, // 16
    /// Saved user program counter.
    pub epc: usize, // 24
    /// saved kernel tp
    pub kernel_hartid: usize, // 32
    pub ra: usize,  // 40
    pub sp: usize,  // 48
    pub gp: usize,  // 56
    pub tp: usize,  // 64
    pub t0: usize,  // 72
    pub t1: usize,  // 80
    pub t2: usize,  // 88
    pub s0: usize,  // 96
    pub s1: usize,  // 104
    pub a0: usize,  // 112
    pub a1: usize,  // 120
    pub a2: usize,  // 128
    pub a3: usize,  // 136
    pub a4: usize,  // 144
    pub a5: usize,  // 152
    pub a6: usize,  // 160
    pub a7: usize,  // 168
    pub s2: usize,  // 176
    pub s3: usize,  // 184
    pub s4: usize,  // 192
    pub s5: usize,  // 200
    pub s6: usize,  // 208
    pub s7: usize,  // 216
    pub s8: usize,  // 224
    pub s9: usize,  // 232
    pub s10: usize, // 240
    pub s11: usize, // 248
    pub t3: usize,  // 256
    pub t4: usize,  // 264
    pub t5: usize,  // 272
    pub t6: usize,  // 280
}

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
    let mut private = p.borrow_private().unwrap();

    // save user program counter.
    private.trapframe_mut().epc = sepc::read();

    let scause: Trap<Interrupt, Exception> = scause::read().cause().try_into().unwrap();
    let mut which_dev = IntrKind::NotRecognized;
    match scause {
        Trap::Exception(Exception::UserEnvCall) => {
            // system call
            if p.shared().lock().killed() {
                proc::ops::exit(p, private, -1);
            }

            // sepc points to the ecall instruction,
            // but we want to return to the next instruction.
            private.trapframe_mut().epc += 4;

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
        proc::ops::exit(p, private, -1);
    }

    // gibe up the CPU if this is a timer interrupt.
    if which_dev == IntrKind::Timer {
        scheduler::yield_(p);
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
    let tf = private.trapframe_mut();
    tf.kernel_satp = satp::read().bits(); // kernel page table
    tf.kernel_sp = kstack.byte_add(KSTACK_PAGES * PAGE_SIZE).unwrap().addr(); // process's kernel stack
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
        sepc::write(private.trapframe().epc);
    }

    // tell trampoline.S the user page table to switch to.
    let satp = private.pagetable().satp().bits();
    drop(private);

    // jump to userret in trampoline.S at the top of memory, which
    // switches to the user page table, restores user registers,
    // and switches to user mode with sret.
    let trampoline_user_ret = trampoline::user_ret_addr();
    unsafe {
        let f: extern "C" fn(usize) = mem::transmute(trampoline_user_ret.addr());
        f(satp);
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
                scheduler::yield_(p);
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

            if irq == to_u32!(UART0_IRQ) {
                uart::handle_interrupt();
            } else if irq == to_u32!(VIRTIO0_IRQ) {
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
