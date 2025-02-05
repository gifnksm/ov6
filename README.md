# Rust port of xv6 for RISC-V

This project is a Rust port of the [xv6 operating system for RISC-V][xv6-riscv].

xv6 is a simple Unix-like operating system developed by MIT for teaching purposes.

[xv6-riscv]: https://github.com/mit-pdos/xv6-riscv/

## Progress

See Languages graph in the right side of this repository.

## Posts

I am writing a series of posts about this project in Japanese:

* [xv6-riscv を Rust に移植する](https://zenn.dev/gifnksm/scraps/071a3d0b176e14)

## Requirements

To build the project, you need to have the following tools installed.

* RISC-V toolchain:

    GCC cross-compiler and binutils for RISC-V is required to build the project.

    For Arch Linux, you can install the toolchain by running:

    ```bash
    sudo pacman -S riscv64-elf-gcc riscv64-elf-binutils
    ```

* QEMU:

    [QEMU RISC-V system emulator][qemu-riscv] is required to run the operating system.

    For Arch Linux, you can install QEMU by running:

    ```bash
    sudo pacman -S qemu-system-riscv
    ```

* Rust:

    See [rustup.rs] for installation instructions.

    Required components and targets are automatically installed by `rustup`, poweed by `rust-toolchain.toml` file.

## Building

To build the operation system, run:

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

`.gdbinit` file is provided to automatically connect to the QEMU instance.

To enable auto-loading of `.gdbinit` file, run:

```bash
mkdir -p ~/.config/gdb/gdbinit
echo "add-auto-load-safe-path $(pwd)/.gdbinit" >> ~/.config/gdb/gdbinit
```
