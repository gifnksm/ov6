# ov6 - Oxide xv6

ov6 is a 100% Rust-written hobby OS project based on [xv6 operating system for RISC-V][xv6-riscv].

xv6 is a simple Unix-like operating system developed by MIT for teaching purposes.

[xv6-riscv]: https://github.com/mit-pdos/xv6-riscv/

## Requirements

To build the project, you need to have the following tools installed.

* QEMU:

    [QEMU RISC-V system emulator][qemu-riscv] is required to run the operating system.

    For Arch Linux, you can install QEMU by running:

    ```bash
    sudo pacman -S qemu-system-riscv
    ```

* Rust:

    See [rustup.rs] for installation instructions.

    Required components and targets are automatically installed by `rustup`, powered by the `rust-toolchain.toml` file.

    ```bash
    cd <path to ov6>
    rustup toolchain install
    ```

## Building

To build the operating system, run:

```bash
make
```

[qemu-riscv]: https://www.qemu.org/docs/master/system/target-riscv.html
[rustup.rs]: https://rustup.rs/

## Running

To run the operating system in QEMU, run:

```bash
make qemu
```

## Debugging

If you want to attach GDB to the QEMU instance, run:

```bash
make qemu-gdb
```

then, in another terminal, run:

```bash
gdb
(gdb) source .gdbinit
```

The `.gdbinit` file is provided to automatically connect to the QEMU instance.

To enable auto-loading of the `.gdbinit` file, run:

```bash
mkdir -p ~/.config/gdb/gdbinit
echo "add-auto-load-safe-path $(pwd)/.gdbinit" >> ~/.config/gdb/gdbinit
```
