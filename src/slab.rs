use core::alloc::Layout;

use crate::{
    frames::{Entry, Frames},
    memory::pfn::{PAGE_SIZE, Pfn},
};

const CLASSES_COUNT: usize = 9; // 8, 16, 32, 64, 128, 256, 512, 1024, 2048

pub const CLASSES: [usize; CLASSES_COUNT] = [8, 16, 32, 64, 128, 256, 512, 1024, 2048];

pub fn class_of(layout: Layout) -> Option<usize> {
    let size = layout.size();
    let align = layout.align();

    for (i, &c) in CLASSES.iter().enumerate() {
        if c < size {
            continue;
        }

        if !c.is_multiple_of(align) {
            continue;
        }
        return Some(i);
    }

    None
}

// Max possible slots count is 512, so 1023 is safe to use as sentinel value
const FREE_HEAD_EMPTY_VALUE: u16 = 1023;

const LINK_SLOT_EMPTY_VALUE: u32 = (1 << 19) - 1;

pub fn init(frames: &mut Frames, page_pfn: Pfn, class_idx: usize) -> Option<()> {
    let size = CLASSES.get(class_idx)?;

    let base = page_pfn.to_addr();

    let number_of_slots = PAGE_SIZE / size;

    for i in 0..number_of_slots {
        let current_base = base + i * size;

        if i < number_of_slots - 1 {
            unsafe { *(current_base as *mut u16) = (i + 1) as u16 };
        } else {
            // TODO: think about that
            unsafe { *(current_base as *mut u16) = FREE_HEAD_EMPTY_VALUE };
        }
    }

    frames.set_page(
        page_pfn,
        Entry::Slab {
            class: class_idx as u8,
            // First slot is free
            free_head: 0,
            in_use: 0,
            next_partial: LINK_SLOT_EMPTY_VALUE,
            prev_partial: LINK_SLOT_EMPTY_VALUE,
        },
    );

    Some(())
}
