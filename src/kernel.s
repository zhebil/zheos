.global _start

_start:
                ldr     x0,  =__stack_top
                mov     sp,  x0
                ldr     x0,  =__bss_start
                ldr     x1,  =__bss_end
zero_bss_loop:
                cmp     x0,  x1
                b.hs    bss_done
                str     xzr, [x0], #8
                b       zero_bss_loop
bss_done:
rust_main:
                bl      kmain

loop:
                b      loop
