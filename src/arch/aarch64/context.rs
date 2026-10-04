use core::arch::global_asm;

global_asm!(include_str!("switch.s"));

unsafe extern "C" {
    pub fn switch(from: *mut Context, to: *const Context);
    // Used as address only
    fn trampoline() -> !;
}

const SWITCH_FRAME_SIZE: usize = 160;

#[repr(C)]
#[derive(Default)]
struct SwitchFrame {
    x19: usize,
    x20: usize,
    x21: usize,
    x22: usize,
    x23: usize,
    x24: usize,
    x25: usize,
    x26: usize,
    x27: usize,
    x28: usize,
    x29: usize,
    x30: usize,
    d8: u64,
    d9: u64,
    d10: u64,
    d11: u64,
    d12: u64,
    d13: u64,
    d14: u64,
    d15: u64,
}

const _: () = assert!(size_of::<SwitchFrame>() == SWITCH_FRAME_SIZE);

#[repr(C)]
pub struct Context {
    sp: usize,
}

impl Context {
    pub const fn running() -> Self {
        Context { sp: 0 }
    }

    pub fn new(stack_top: usize, entry: fn()) -> Self {
        // Leave room for the switch frame at the top of the stack
        let frame_top = stack_top - size_of::<SwitchFrame>();
        let frame = frame_top as *mut SwitchFrame;

        unsafe {
            frame.write(SwitchFrame {
                x30: trampoline as *const () as usize,
                x19: entry as usize,
                ..Default::default()
            });
        }

        Context { sp: frame_top }
    }
}
