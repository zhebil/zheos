use core::{alloc::Layout, arch::global_asm};

use crate::{heap::HEAP, memory::pfn::PAGE_SIZE};

global_asm!(include_str!("switch.s"));

unsafe extern "C" {
    pub fn switch(from: *mut Context, to: *const Context);
}

pub struct ContextStack {
    base: usize,
    size: usize,
}

const CONTEXT_STACK_SIZE: usize = 16 * 1024; // 16 KiB
const SWITCH_FRAME_SIZE: usize = 160;

const CONTEXT_STACK_LAYOUT: Layout = match Layout::from_size_align(CONTEXT_STACK_SIZE, PAGE_SIZE) {
    Ok(layout) => layout,
    Err(_) => panic!("Failed to build context stack layout"),
};

impl ContextStack {
    pub fn new() -> Option<Self> {
        HEAP.with(|heap| {
            let ptr = heap.alloc_layout(CONTEXT_STACK_LAYOUT)?;
            Some(ContextStack {
                base: ptr,
                size: CONTEXT_STACK_SIZE,
            })
        })
    }

    pub fn top(&self) -> usize {
        self.base + self.size
    }
}

impl Drop for ContextStack {
    fn drop(&mut self) {
        HEAP.with(|heap| {
            heap.free_layout(self.base, CONTEXT_STACK_LAYOUT);
        });
    }
}

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
    pub const fn new(sp: usize) -> Self {
        Context { sp }
    }
}

pub struct Task {
    #[allow(dead_code)]
    id: usize,
    context: Context,
    // Never read, only dropped: its Drop frees the memory context.sp points into.
    #[allow(dead_code)]
    stack: ContextStack,
}

impl Task {
    pub fn new(id: usize, entry: fn() -> !) -> Option<Self> {
        let stack = ContextStack::new()?;
        // Leave room for the switch frame at the top of the stack
        let frame_top = stack.top() - size_of::<SwitchFrame>();
        let frame = frame_top as *mut SwitchFrame;

        unsafe {
            frame.write(SwitchFrame {
                x30: entry as usize,
                ..Default::default()
            });
        }

        Some(Task {
            id,
            context: Context::new(frame_top),
            stack,
        })
    }

    pub fn context(&self) -> &Context {
        &self.context
    }

    pub fn context_mut(&mut self) -> &mut Context {
        &mut self.context
    }
}
