use crate::ring_buffer::RingBuffer;
use crate::{cpu, lock::SpinLock};

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

static INPUT_BUFFER: SpinLock<RingBuffer<InputByte>> =
    SpinLock::new(RingBuffer::new(InputByte::EMPTY));

pub fn getc() -> InputByte {
    loop {
        if let Some(input_byte) = cpu::without_interrupts(|| -> Option<InputByte> {
            let input_byte = INPUT_BUFFER.lock().pop();

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
    INPUT_BUFFER.lock().push(input_byte)
}
