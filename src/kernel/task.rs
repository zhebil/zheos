use core::alloc::Layout;

use crate::{
    arch::context::Context,
    mm::{HEAP, PAGE_SIZE},
};

pub struct ContextStack {
    base: usize,
    size: usize,
}

const CONTEXT_STACK_SIZE: usize = 16 * 1024; // 16 KiB

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Ready,
    Blocked,
    Dead,
}

pub struct Task {
    #[allow(dead_code)]
    id: usize,
    context: Context,
    // Never read, only dropped: its Drop frees the memory context.sp points into.
    // None for the boot task, which runs on the stack linker.ld reserves.
    #[allow(dead_code)]
    stack: Option<ContextStack>,

    ticks: usize,

    state: State,
}

impl Task {
    pub fn new(id: usize, entry: fn()) -> Option<Self> {
        let stack = ContextStack::new()?;

        Some(Task {
            id,
            context: Context::new(stack.top(), entry),
            stack: Some(stack),
            ticks: 0,
            state: State::Ready,
        })
    }

    /// The code already running, adopted as a task. It needs no stack and no
    /// frame, only a context for its first switch to save sp into.
    pub const fn boot(id: usize) -> Self {
        Task {
            id,
            context: Context::running(),
            stack: None,
            ticks: 0,
            state: State::Ready,
        }
    }
    pub fn context(&self) -> &Context {
        &self.context
    }

    pub fn context_mut(&mut self) -> &mut Context {
        &mut self.context
    }

    pub fn ticks(&self) -> usize {
        self.ticks
    }

    pub fn id(&self) -> usize {
        self.id
    }

    pub fn tick(&mut self) {
        self.ticks += 1;
    }

    pub fn state(&self) -> State {
        self.state
    }

    pub fn set_state(&mut self, state: State) {
        self.state = state;
    }
}
