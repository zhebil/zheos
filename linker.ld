ENTRY(_start)

SECTIONS
{
  . = 0x40080000;

  __image_start = .;

  /*
   * Executable segment: instructions plus the exception vectors, which need the
   * same permissions and so share the group. VBAR_EL1 wants .vectors 2048-byte
   * aligned; that padding lands inside a page this segment already pays for.
   */
  .text : {
    KEEP(*(.text._start))
    *(.text*)
  }
  __text_end = .;

  .vectors : ALIGN(2048) {
    __vectors_start = .;
    *(.vectors)
    __vectors_end = .;
  }

  . = ALIGN(4096);
  __exec_end = .;

  /* Read-only segment: constants, never executed. */
  .rodata : {
    *(.rodata*)
  }

  . = ALIGN(4096);
  __rodata_end = .;

  /* Writable segment: initialised data, then zeroed data, then the stack. */
  .data : {
    *(.data*)
  }
  __data_end = .;

  . = ALIGN(8);
  __bss_start = .;

  .bss : {
    *(.bss*)
    . = ALIGN(8);
  }
  __bss_end = .;

  . = ALIGN(4096);
  __stack_bottom = .;
  . = . + 0x8000;
  __stack_top = .;
}
