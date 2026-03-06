ENTRY(_start)

PHDRS
{
  text PT_LOAD FLAGS(5);
  data PT_LOAD FLAGS(6);
}

SECTIONS
{
  . = 0x700000000100;

  .text : ALIGN(0x1000) {
    *(.text .text.*)
    *(.rodata .rodata.*)
  } :text

  .data : ALIGN(0x1000) {
    *(.data .data.*)
  } :data

  .bss : ALIGN(0x1000) {
    *(.bss .bss.*)
    *(COMMON)
  } :data
}
