ENTRY(_start)

SECTIONS
{
  . = 0x40080000;

  .text : {
    *(.text*)
  }

  .rodata : {
    *(.rodata*)
  }

  .data : {
    *(.data*)
  }

  /* Last, so the 2048-byte alignment VBAR_EL1 needs pads this and not .text. */
  .vectors : {
    *(.vectors)
  }

  . = ALIGN(8);

  __bss_start = .;

  .bss : {
    *(.bss*)
    . = ALIGN(8);
  }
  __bss_end = .;

  . = ALIGN(16);
  . = . + 0x8000;
  __stack_top = .;
}
