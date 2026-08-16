// The exception vector table.
//
// When the CPU cannot carry on, it jumps to VBAR_EL1 + offset and runs whatever
// is there. This file is the "whatever is there".
//
// It calls one Rust function, which you have to write:
//
//   #[unsafe(no_mangle)]
//   pub extern "C" fn handle_exception(
//       esr: u64, far: u64, elr: u64, kind: u64, frame: *const [u64; 31],
//   ) -> !
//
//   esr   - why it happened (Exception Syndrome Register)
//   far   - which address it tried to touch (Fault Address Register)
//   elr   - the instruction that failed (Exception Link Register)
//   kind  - 0 if this was a synchronous fault, 1 if it was any of the other 15 slots
//   frame - points at the saved x0..x30, in order, so frame[7] is x7


// A slot in the table. Every slot must sit on a 128-byte boundary because the
// CPU works out where to jump by multiplying the kind of exception by 128.
//
// A slot does nothing but branch. It must not write to a single register: the
// registers still hold the failing code's values, and they are the evidence.
.macro          vector_slot handler
                .balign 0x80
                b       \handler
.endm


// Push x0 through x30 onto the stack, in the order the registers are numbered.
//
// The hardware saved nothing for us, so this has to happen before any other
// instruction touches a register. `stp` stores a pair at once, so 31 registers
// take 16 instructions instead of 31.
//
// 31 registers is 248 bytes; we take 256 because the stack pointer has to stay
// a multiple of 16 or the CPU faults on the next push - inside the handler,
// which is the worst place to fault.
.macro          save_all_registers
                sub     sp,  sp,  #256
                stp     x0,  x1,  [sp, #16 * 0]
                stp     x2,  x3,  [sp, #16 * 1]
                stp     x4,  x5,  [sp, #16 * 2]
                stp     x6,  x7,  [sp, #16 * 3]
                stp     x8,  x9,  [sp, #16 * 4]
                stp     x10, x11, [sp, #16 * 5]
                stp     x12, x13, [sp, #16 * 6]
                stp     x14, x15, [sp, #16 * 7]
                stp     x16, x17, [sp, #16 * 8]
                stp     x18, x19, [sp, #16 * 9]
                stp     x20, x21, [sp, #16 * 10]
                stp     x22, x23, [sp, #16 * 11]
                stp     x24, x25, [sp, #16 * 12]
                stp     x26, x27, [sp, #16 * 13]
                stp     x28, x29, [sp, #16 * 14]
                str     x30,      [sp, #16 * 15]
.endm

.macro  restore_all_registers
                ldp     x30, x31, [sp, #16 * 15]
                ldp     x28, x29, [sp, #16 * 14]
                ldp     x26, x27, [sp, #16 * 13]
                ldp     x24, x25, [sp, #16 * 12]
                ldp     x22, x23, [sp, #16 * 11]
                ldp     x20, x21, [sp, #16 * 10]
                ldp     x18, x19, [sp, #16 * 9]
                ldp     x16, x17, [sp, #16 * 8]
                ldp     x14, x15, [sp, #16 * 7]
                ldp     x12, x13, [sp, #16 * 6]
                ldp     x10, x11, [sp, #16 * 5]
                ldp     x8,  x9,  [sp, #16 * 4]
                ldp     x6,  x7,  [sp, #16 * 3]
                ldp     x4,  x5,  [sp, #16 * 2]
                ldp     x2,  x3,  [sp, #16 * 1]
                ldp     x0,  x1,  [sp, #16 * 0]
                add     sp,  sp,  #256
.endm


// The table itself. 2048-byte aligned because the CPU ignores the low 11 bits
// of VBAR_EL1 - put it anywhere else and it silently reads the wrong address.
.section .vectors, "ax"
                .balign 2048
                .global vector_table
vector_table:

// Fault at our own level, but while the stack pointer register in use is SP_EL0.
// We never run that way, so none of these four can happen.
                vector_slot unexpected_entry    // 0x000  synchronous
                vector_slot unexpected_entry    // 0x080  IRQ
                vector_slot unexpected_entry    // 0x100  FIQ
                vector_slot unexpected_entry    // 0x180  error

// Fault at our own level, using SP_EL1. This is us. Everything the kernel does
// wrong to itself arrives in this group.
                vector_slot sync_entry          // 0x200  synchronous  <-- the one
                vector_slot irq_entry           // 0x280  IRQ
                vector_slot unexpected_entry    // 0x300  FIQ
                vector_slot unexpected_entry    // 0x380  error

// Fault in 64-bit code running at a lower, less privileged level. There is no
// such code yet - that arrives when the kernel starts running user programs.
                vector_slot unexpected_entry    // 0x400  synchronous
                vector_slot unexpected_entry    // 0x480  IRQ
                vector_slot unexpected_entry    // 0x500  FIQ
                vector_slot unexpected_entry    // 0x580  error

// Same, but for 32-bit code. This machine never runs any.
                vector_slot unexpected_entry    // 0x600  synchronous
                vector_slot unexpected_entry    // 0x680  IRQ
                vector_slot unexpected_entry    // 0x700  FIQ
                vector_slot unexpected_entry    // 0x780  error


// Out here we are past the table, so there is no 128-byte limit any more and
// the code can be as long as it needs to be.
.text
sync_entry:
                save_all_registers
                // Safe to write registers now: the originals are on the stack.
                mov     x3,  #0
                b       report_exception

unexpected_entry:
                save_all_registers
                mov     x3,  #1
                b       report_exception

irq_entry:
                save_all_registers
                bl      handle_interrupt
                restore_all_registers
                eret

report_exception:
                // Copy the CPU's own account of what happened into x0, x1, x2.
                // `mrs` is the only way to read a system register.
                //
                // A Rust `extern "C"` function reads its arguments from x0
                // onwards, so putting them here is what makes the call work.
                mrs     x0,  esr_el1
                mrs     x1,  far_el1
                mrs     x2,  elr_el1

                // x3 was set by whichever entry branched here. sp still points
                // at the block we just pushed, so it is the address of the
                // saved registers - but only until the callee pushes its own
                // frame, which is why it has to be handed over now.
                mov     x4,  sp

                bl      handle_exception

                // handle_exception never returns, so this is only a safety net.
park:
                wfi
                b       park
