// Hand the CPU from one task to another.
//
// Declared on the Rust side as an unsafe extern "C" function taking two
// arguments:
//
//   from - where to write the stack pointer of the task being left
//   to   - where to read the stack pointer of the task being entered
//
// Both point at a #[repr(C)] Context whose first field is that stack pointer,
// so offset 0 is the whole contract between this file and Rust.
//
// Only the callee-saved half of the register file is here. Whoever called
// switch already accepted that x0-x18 and v0-v7/v16-v31 would be clobbered by
// any call, so those are the caller's problem and we can leave them alone.
// d8-d15 rather than q8-q15 for the same reason: AAPCS64 promises only the
// bottom 64 bits of v8-v15 across a call.
//
// 20 registers, 160 bytes, and 160 is a multiple of 16 so sp stays legal.

.global switch
.text
switch:
                sub     sp,  sp,  #160
                stp     x19, x20, [sp, #16 * 0]
                stp     x21, x22, [sp, #16 * 1]
                stp     x23, x24, [sp, #16 * 2]
                stp     x25, x26, [sp, #16 * 3]
                stp     x27, x28, [sp, #16 * 4]
                stp     x29, x30, [sp, #16 * 5]
                stp     d8,  d9,  [sp, #16 * 6]
                stp     d10, d11, [sp, #16 * 7]
                stp     d12, d13, [sp, #16 * 8]
                stp     d14, d15, [sp, #16 * 9]

                // Record sp only now that it points at the frame above. Through
                // a scratch register because sp cannot be a load/store operand:
                // register 31 in that slot means xzr, not the stack pointer.
                mov     x9,  sp
                str     x9,  [x0]

                ldr     x9,  [x1]
                mov     sp,  x9

                ldp     d14, d15, [sp, #16 * 9]
                ldp     d12, d13, [sp, #16 * 8]
                ldp     d10, d11, [sp, #16 * 7]
                ldp     d8,  d9,  [sp, #16 * 6]
                ldp     x29, x30, [sp, #16 * 5]
                ldp     x27, x28, [sp, #16 * 4]
                ldp     x25, x26, [sp, #16 * 3]
                ldp     x23, x24, [sp, #16 * 2]
                ldp     x21, x22, [sp, #16 * 1]
                ldp     x19, x20, [sp, #16 * 0]
                add     sp,  sp,  #160

                // x30 came off the new stack, so this returns into the task we
                // just entered rather than to the caller above.
                ret
