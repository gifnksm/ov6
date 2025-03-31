OUTPUT_ARCH( "riscv" )
ENTRY( boot )

SECTIONS
{
  /*
   * ensure that bot is at 0x80000000,
   * where qemu's -kernel jumps.
   */
  . = 0x80000000;
  PROVIDE(_ov6_kernel_base = .);

  .text : {
    *(.text.init)
    *(.text .text.*)
    . = ALIGN(0x1000);
    _trampoline = .;
    *(trampsec)
    . = ALIGN(0x1000);
    ASSERT(. - _trampoline == 0x1000, "error: trampoline larger than one page");
    PROVIDE(_ov6_text_end = .);
  }

  .rodata : {
    . = ALIGN(16);
    *(.srodata .srodata.*) /* do not need to distinguish this from .rodata */
    . = ALIGN(16);
    *(.rodata .rodata.*)
  }

  .data : {
    . = ALIGN(16);
    *(.sdata .sdata.*) /* do not need to distinguish this from .data */
    . = ALIGN(16);
    *(.data .data.*)
  }

  .bss : {
    . = ALIGN(16);
    *(.sbss .sbss.*) /* do not need to distinguish this from .bss */
    . = ALIGN(16);
    *(.bss .bss.*)
  }

  PROVIDE(_ov6_kernel_end = .);
  PROVIDE(_ov6_phys_top = _ov6_kernel_base + 128 * 1024 * 1024);
}
