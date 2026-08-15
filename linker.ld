ENTRY(_start)

SECTIONS
{
  . = 0x40000000;

  .text : {
    *(.vectors)
    *(.text*)
  }

  .rodata : {
    *(.rodata*)
  }

  .data : {
    *(.data*)
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
