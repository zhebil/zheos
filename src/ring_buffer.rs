// Must stay a power of two, or every `%` below becomes a udiv in the interrupt
// handler instead of a single AND.
const RING_BUFFER_SIZE: usize = 256;

pub struct RingBuffer<T: Copy> {
    buffer: [T; RING_BUFFER_SIZE],
    head: usize,
    tail: usize,
    full: bool,
}

impl<T: Copy> RingBuffer<T> {
    pub const fn new(default: T) -> Self {
        Self {
            buffer: [default; RING_BUFFER_SIZE],
            head: 0,
            tail: 0,
            full: false,
        }
    }

    pub fn push(&mut self, value: T) {
        if self.full {
            return;
        }
        // The mask is redundant - tail is always in range - but it is what lets
        // LLVM drop the bounds check, and it folds into the addressing mode for
        // free. Removing it costs four instructions and a panic call site.
        self.buffer[self.tail % RING_BUFFER_SIZE] = value;
        self.tail = (self.tail + 1) % RING_BUFFER_SIZE;
        self.full = self.tail == self.head;
    }

    pub fn pop(&mut self) -> Option<T> {
        if !self.full && self.tail == self.head {
            return None;
        }
        let value = self.buffer[self.head % RING_BUFFER_SIZE];
        self.head = (self.head + 1) % RING_BUFFER_SIZE;
        self.full = false;
        Some(value)
    }
}
