OUTPUT_ARCH(%ARCH%)

BASE_ADDRESS = %KERNEL_BASE%;

ENTRY(_start)
SECTIONS
{
    . = BASE_ADDRESS;
    skernel = .;

    .text : ALIGN(4K) {
        stext = .;
        *(.text.boot)
        *(.text .text.*)
        . = ALIGN(4K);
        etext = .;
    }

    .rodata : ALIGN(4K) {
        srodata = .;
        *(.rodata .rodata.*)
        *(.srodata .srodata.*)
        *(.sdata2 .sdata2.*)
        . = ALIGN(4K);
        erodata = .;
    }

    .data : ALIGN(4K) {
        sdata = .;
        *(.data.boot_page_table)
        . = ALIGN(4K);
        *(.data .data.*)
        *(.sdata .sdata.*)
        *(.got .got.*)
        . = ALIGN(4K);
        edata = .;
    }

    . = ALIGN(4K);
    _percpu_start = .;
    .percpu 0x0 : AT(_percpu_start) ALIGN(4K) {
        _percpu_load_start = .;
        *(.percpu .percpu.*)
        _percpu_load_end = .;
        . = ALIGN(64);
        _percpu_size_aligned = .;

        . = _percpu_load_start + _percpu_size_aligned * %SMP%;
    }
    . = _percpu_start + SIZEOF(.percpu);

    . = ALIGN(4K);
    _percpu_end = .;

    . = ALIGN(4K);
    _edata = .;

    .bss : ALIGN(4K) {
        boot_stack = .;
        *(.bss.stack)
        . = ALIGN(4K);
        boot_stack_top = .;

        sbss = .;
        *(.bss .bss.*)
        *(.sbss .sbss.*)
        *(COMMON)
        . = ALIGN(4K);
        ebss = .;
    }
    
    ekernel = .;

	/DISCARD/ : {
        *(.comment) *(.gnu*) *(.note*) *(.eh_frame*)
    }
}
