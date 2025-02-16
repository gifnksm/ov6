//! low-level driver routines for 16550a UART.

use core::{hint, ptr, sync::atomic::Ordering};

use crate::{console, interrupt, memory::layout::UART0, proc, sync::SpinLock};

use super::print::PANICKED;

const unsafe fn reg(offset: usize) -> *mut u8 {
    unsafe { ptr::without_provenance_mut::<u8>(UART0).byte_add(offset) }
}

// the UART control registers.
// some have different meanings for
// read vs write.
// see http://byterunner.com/16550.html

/// receive holding register (for input bytes)
const RHR: usize = 0;
/// transmit holding register (for output bytes)
const THR: usize = 0;
/// interrupt enable register
const IER: usize = 1;
const IER_RX_ENABLE: u8 = 1 << 0;
const IER_TX_ENABLE: u8 = 1 << 1;
/// FIFO control register
const FCR: usize = 2;
const FCR_FIFO_ENABLE: u8 = 1 << 0;
/// clear the content of the two FIFOs
const FCR_FIFO_CLEAR: u8 = 3 << 1;
/// interrupt status register
const ISR: usize = 2;
/// line control register
const LCR: usize = 3;
const LCR_EIGHT_BITS: u8 = 3;
/// special mode to set baud rate
const LCR_BAUD_LATCH: u8 = 1 << 7;
/// line status register
const LSR: usize = 5;
/// input is waiting to be read from RHR
const LSR_RX_READY: u8 = 1 << 0;
/// THR can accept another character to send
const LSR_TX_IDLE: u8 = 1 << 5;

unsafe fn read_reg(offset: usize) -> u8 {
    unsafe { reg(offset).read_volatile() }
}

unsafe fn write_reg(offset: usize, data: u8) {
    unsafe { reg(offset).write_volatile(data) }
}

/// The transmit output buffer.
struct TxBuffer {
    buf: [u8; 32],
    /// Write next to buf[tx_w % buf.len()]
    tx_w: usize,
    /// Read next from buf[tx_w % buf.len()]
    tx_r: usize,
}

impl TxBuffer {
    fn is_full(&self) -> bool {
        self.tx_w == self.tx_r + self.buf.len()
    }

    fn is_empty(&self) -> bool {
        self.tx_w == self.tx_r
    }

    fn put(&mut self, c: u8) {
        assert!(self.tx_w < self.tx_r + self.buf.len());
        self.buf[self.tx_w % self.buf.len()] = c;
        self.tx_w += 1;
    }

    fn pop(&mut self) -> u8 {
        assert!(self.tx_r < self.tx_w);
        let c = self.buf[self.tx_r % self.buf.len()];
        self.tx_r += 1;
        c
    }
}

static TX_BUFFER: SpinLock<TxBuffer> = SpinLock::new(TxBuffer {
    buf: [0; 32],
    tx_w: 0,
    tx_r: 0,
});

pub fn init() {
    unsafe {
        // disable interrupts
        write_reg(IER, 0x00);

        // special mode to set baud rate.
        write_reg(LCR, LCR_BAUD_LATCH);

        // LSB for baud rate of 38.4K.
        write_reg(0, 0x03);

        // MSB for baud rate of 38.4K.
        write_reg(1, 0x00);

        // leave set-baud mode,
        // and set word length to 8 bits, no parity.
        write_reg(LCR, LCR_EIGHT_BITS);

        // reset and enable FIFOs.
        write_reg(FCR, FCR_FIFO_ENABLE | FCR_FIFO_CLEAR);

        // enable transmit and receive interrupts.
        write_reg(IER, IER_TX_ENABLE | IER_RX_ENABLE);
    }
}

/// Adds a character to the output buffer and tell the
/// UART to start sending if it isn't already.
///
/// Blocks if the output buffer is full.
/// Because it may block, it can't be called
/// from interrupts; it's only suitable for use
/// by write().
pub fn putc(c: char) {
    let mut buffer = TX_BUFFER.lock();

    if PANICKED.load(Ordering::Relaxed) {
        loop {
            hint::spin_loop();
        }
    }

    while buffer.is_full() {
        // buffer is full
        // wait for start() to open up space in the buffer.
        proc::sleep((&raw const buffer.tx_r).cast(), &mut buffer);
    }
    buffer.put(c as u8);
    start(&mut buffer);
}

/// Sends a character to the UART synchronously.
///
/// Alternate version of putc() that doesn't
/// use interrupts, for use by kernel printf() and
/// to echo characters.
///
/// It spins waiting for the uart's
/// output register to be empty.
pub fn putc_sync(c: char) {
    interrupt::with_push_disabled(|| {
        if PANICKED.load(Ordering::Relaxed) {
            loop {
                hint::spin_loop();
            }
        }

        // wait for Transmit Holding Empty to be set in LSR.
        while (unsafe { read_reg(LSR) } & LSR_TX_IDLE) == 0 {
            hint::spin_loop();
        }

        unsafe {
            write_reg(THR, c as u8);
        }
    });
}

/// Starts sending characters from the output buffer.
///
/// If the UART is idle, and a character is waiting
/// in the transmit buffer, send it.
///
/// Caller must hold the TX_BUFFER lock.
/// Called from both the top- and bottom-half.
fn start(buffer: &mut TxBuffer) {
    loop {
        if buffer.is_empty() {
            // transmit buffer is empty.
            unsafe {
                read_reg(ISR);
            }
            return;
        }

        if unsafe { read_reg(LSR) } & LSR_TX_IDLE == 0 {
            // the UART transmit holding register is full,
            // so we cannot give it another byte.
            // it will interrupt when it's ready for a new byte.
            return;
        }

        let c = buffer.pop();

        // maybe putc() is waiting for space in the buffer.
        proc::wakeup((&raw const buffer.tx_r).cast());

        unsafe {
            write_reg(THR, c);
        }
    }
}

/// Reads one input character from the UART.
///
/// Returns None if none is waiting.
fn getc() -> Option<u8> {
    if (unsafe { read_reg(LSR) } & LSR_RX_READY) != 0 {
        // input data is ready
        Some(unsafe { read_reg(RHR) })
    } else {
        None
    }
}

/// Handle a uart interrupt, raised because input
/// has arrived, or the uart is ready for more output, or
/// both.
///
/// Called from devintr().
pub fn handle_interrupt() {
    // read and process incoming characters.
    while let Some(c) = getc() {
        console::handle_interrupt(c);
    }

    // send buffered characters.
    let mut buffer = TX_BUFFER.lock();
    start(&mut buffer);
    drop(buffer);
}
