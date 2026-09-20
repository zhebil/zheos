use core::alloc::Layout;

use crate::{heap::HEAP, memory::pfn::PAGE_SIZE};

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

    pub fn base(&self) -> usize {
        self.base
    }

    pub fn size(&self) -> usize {
        self.size
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
pub struct Context {
    sp: usize,
}

impl Context {
    pub fn new(sp: usize) -> Self {
        Context { sp }
    }

    pub fn sp(&self) -> usize {
        self.sp
    }
}

pub struct Task {
    id: usize,
    context: Context,
    stack: ContextStack,
}

impl Task {
    pub fn new(id: usize) -> Option<Self> {
        let stack = ContextStack::new()?;
        Some(Task {
            id,
            context: Context::new(stack.top()),
            stack,
        })
    }

    pub fn id(&self) -> usize {
        self.id
    }

    pub fn context(&self) -> &Context {
        &self.context
    }

    pub fn stack(&self) -> &ContextStack {
        &self.stack
    }
}
