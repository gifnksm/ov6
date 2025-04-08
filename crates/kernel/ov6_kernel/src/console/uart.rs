//! Low-level driver routines for 16550a UART.
//!
//! This module provides functions to initialize and interact with the UART
//! hardware, including sending and receiving characters, handling interrupts,
//! and managing transmit buffers.

use core::{hint, ptr, sync::atomic::Ordering};

use super::print::PANICKED;
use crate::{
    console,
    error::KernelError,
    interrupt,
    memory::layout::UART0,
    sync::{SpinLock, SpinLockCondVar},
};

/// Returns a mutable pointer to the UART register at the given offset.
///
/// # Safety
///
/// This function is `unsafe` because it directly manipulates hardware
/// registers. The caller must ensure that the offset is valid and that the UART
/// hardware is properly initialized.
unsafe fn reg(offset: usize) -> *mut u8 {
    unsafe { ptr::with_exposed_provenance_mut::<u8>(UART0).byte_add(offset) }
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

/// Reads a value from the UART register at the given offset.
///
/// # Safety
///
/// This function is `unsafe` because it directly reads from hardware registers.
/// The caller must ensure that the offset is valid and that the UART hardware
/// is properly initialized.
unsafe fn read_reg(offset: usize) -> u8 {
    unsafe { reg(offset).read_volatile() }
}

/// Writes a value to the UART register at the given offset.
///
/// # Safety
///
/// This function is `unsafe` because it directly writes to hardware registers.
/// The caller must ensure that the offset is valid and that the UART hardware
/// is properly initialized.
unsafe fn write_reg(offset: usize, data: u8) {
    unsafe { reg(offset).write_volatile(data) }
}

/// The transmit output buffer.
///
/// This buffer is used to store characters that are waiting to be sent
/// to the UART hardware.
struct TxBuffer {
    buf: [u8; 32],
    /// Write next to `buf[tx_w % buf.len()]`
    tx_w: usize,
    /// Read next from `buf[tx_w % buf.len()]`
    tx_r: usize,
}

impl TxBuffer {
    /// Returns `true` if the buffer is full.
    fn is_full(&self) -> bool {
        self.tx_w == self.tx_r + self.buf.len()
    }

    /// Returns `true` if the buffer is empty.
    fn is_empty(&self) -> bool {
        self.tx_w == self.tx_r
    }

    /// Adds a character to the buffer.
    ///
    /// # Panics
    ///
    /// Panics if the buffer is full.
    fn put(&mut self, c: u8) {
        assert!(self.tx_w < self.tx_r + self.buf.len());
        self.buf[self.tx_w % self.buf.len()] = c;
        self.tx_w += 1;
    }

    /// Removes and returns a character from the buffer.
    ///
    /// # Panics
    ///
    /// Panics if the buffer is empty.
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
static TX_BUFFER_SPACE_AVAILABLE: SpinLockCondVar = SpinLockCondVar::new();

/// Initializes the UART hardware.
///
/// This function configures the UART registers for communication, including
/// setting the baud rate, enabling FIFOs, and enabling interrupts.
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

/// Adds a character to the output buffer and starts sending if the UART is
/// idle.
///
/// This function blocks if the output buffer is full. It is not suitable for
/// use in interrupt handlers.
///
/// # Errors
///
/// Returns an error if the thread is interrupted while waiting for buffer
/// space.
pub fn putc(c: u8) -> Result<(), KernelError> {
    let mut buffer = TX_BUFFER.lock();

    if PANICKED.load(Ordering::Relaxed) {
        loop {
            hint::spin_loop();
        }
    }

    while buffer.is_full() {
        // buffer is full
        // wait for start() to open up space in the buffer.
        buffer = TX_BUFFER_SPACE_AVAILABLE
            .wait(buffer)
            .map_err(|(_guard, e)| e)?;
    }
    buffer.put(c);
    start(&mut buffer);
    Ok(())
}

/// Sends a character to the UART synchronously.
///
/// This function does not use interrupts and is suitable for use in kernel
/// `printf()` and echoing characters. It spins until the UART's output register
/// is empty.
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
            let mut bytes = [0; 4];
            for b in c.encode_utf8(&mut bytes).as_bytes() {
                write_reg(THR, *b);
            }
        }
    });
}

/// Starts sending characters from the output buffer.
///
/// If the UART is idle and a character is waiting in the transmit buffer,
/// this function sends it.
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
        TX_BUFFER_SPACE_AVAILABLE.notify();

        unsafe {
            write_reg(THR, c);
        }
    }
}

/// Reads one input character from the UART.
///
/// Returns `None` if no character is waiting.
fn getc() -> Option<u8> {
    ((unsafe { read_reg(LSR) } & LSR_RX_READY) != 0).then(|| unsafe { read_reg(RHR) })
}

/// Handles a UART interrupt.
///
/// This function processes incoming characters and sends buffered characters.
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
