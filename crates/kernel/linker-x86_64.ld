/* cSpell:disable */
OUTPUT_FORMAT(elf64-x86-64)

ENTRY(kenter)

PHDRS
{
    text PT_LOAD;
    rodata PT_LOAD;
    data PT_LOAD;
}

SECTIONS
{
    . = 0xffffffff80000000;

    __kernel_start = .;

    .text : {
        __kernel_text_start = .;
        *(.text .text.*)
        __kernel_text_end = .;

        __interrupt_handlers_start = .;
        *(.interrupt_handlers)
        __interrupt_handlers_end = .;
    } :text

    . = ALIGN(CONSTANT(MAXPAGESIZE));

    .rodata : {
        __kernel_rodata_start = .;
        *(.rodata .rodata.*)
        __kernel_rodata_end = .;
    } :rodata

    . = ALIGN(CONSTANT(MAXPAGESIZE));

    .data : {
        __kernel_data_start = .;
        *(.data .data.*)

        /* Place the sections that contain the Limine requests as part of the .data */
        /* output section. */
        KEEP(*(.requests_start_marker))
        KEEP(*(.requests))
        KEEP(*(.requests_end_marker))
        __kernel_data_end = .;
    } :data

    .bss : {
        __kernel_bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        __kernel_bss_end = .;
    } :data

    __kernel_end = .;

    /* Keep debug sections for symbol extraction */
    .debug_abbrev   0 : { *(.debug_abbrev) }
    .debug_info     0 : { *(.debug_info) }
    .debug_line     0 : { *(.debug_line) }
    .debug_str      0 : { *(.debug_str) }
    .debug_ranges   0 : { *(.debug_ranges) }
    .debug_loc      0 : { *(.debug_loc) }
    .debug_pubnames 0 : { *(.debug_pubnames) }
    .debug_pubtypes 0 : { *(.debug_pubtypes) }
    .debug_frame    0 : { *(.debug_frame) }
    .debug_aranges  0 : { *(.debug_aranges) }
    .debug_line_str 0 : { *(.debug_line_str) }

    /DISCARD/ : {
        *(.eh_frame*)
        *(.note .note.*)
    }
}