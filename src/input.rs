use crate::cpu;
use crate::ring_buffer::RingBuffer;
use core::cell::UnsafeCell;

#[derive(Debug, Clone, Copy)]
pub struct InputByte {
    pub byte: u8,
    pub error: bool,
}

impl InputByte {
    const EMPTY: Self = Self {
        byte: 0,
        error: false,
    };
}

struct InputBuffer(UnsafeCell<RingBuffer<InputByte>>);

// SAFETY: both methods below run inside cpu::without_interrupts, and on one core
// an interrupt is the only thing that can cut in, so a push and a pop can never
// overlap. The masking is load-bearing here - unlike HandlerTable, whose two
// sides simply never run in the same phase, push and pop both write head, tail
// and full, and both are live at the same time.
unsafe impl Sync for InputBuffer {}

impl InputBuffer {
    const fn new() -> Self {
        Self(UnsafeCell::new(RingBuffer::new(InputByte::EMPTY)))
    }

    fn push(&self, value: InputByte) {
        unsafe { (*self.0.get()).push(value) }
    }

    fn pop(&self) -> Option<InputByte> {
        unsafe { (*self.0.get()).pop() }
    }
}

static INPUT_BUFFER: InputBuffer = InputBuffer::new();

pub fn getc() -> InputByte {
    loop {
        if let Some(input_byte) = cpu::without_interrupts(|| -> Option<InputByte> {
            let input_byte = INPUT_BUFFER.pop();

            if input_byte.is_some() {
                return input_byte;
            }

            cpu::wait_for_interrupt();
            None
        }) {
            return input_byte;
        }
    }
}

pub fn push_character(input_byte: InputByte) {
    cpu::without_interrupts(|| INPUT_BUFFER.push(input_byte))
}
