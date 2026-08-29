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

pub fn alloc(frames: &mut Frames, pfn: Pfn) -> Option<usize> {
    match frames.page(pfn) {
        Entry::Slab {
            class,
            free_head,
            in_use,
            next_partial,
            prev_partial,
        } => {
            // No free space
            if free_head == FREE_HEAD_EMPTY_VALUE {
                return None;
            }

            let head = free_head;
            let size = CLASSES.get(class as usize)?;

            if head as usize >= PAGE_SIZE / size {
                return None;
            }

            let address = pfn.to_addr() + head as usize * size;
            let next_free_head = unsafe { *(address as *mut u16) };

            frames.set_page(
                pfn,
                Entry::Slab {
                    class,
                    free_head: next_free_head,
                    in_use: in_use + 1,
                    next_partial,
                    prev_partial,
                },
            );

            Some(address)
        }
        // It is not a slab page
        Entry::Buddy { .. } => None,
    }
}

pub fn free(frames: &mut Frames, pfn: Pfn, address: usize) -> Option<()> {
    let Entry::Slab {
        class,
        free_head,
        in_use,
        next_partial,
        prev_partial,
    } = frames.page(pfn)
    else {
        return None;
    };

    let size = CLASSES.get(class as usize)?;
    let base = pfn.to_addr();

    // Make sure address is inside the slab page
    if address < base {
        return None;
    }

    let offset = address - base;

    if offset >= PAGE_SIZE || !offset.is_multiple_of(*size) {
        return None;
    }

    let idx = (offset / size) as u16;

    // Make sure the slot is not already freed or empty
    if in_use == 0 || idx == free_head {
        return None;
    }

    // Make the freed slot point to the previous free slot
    unsafe { (*(address as *mut u16)) = free_head };

    frames.set_page(
        pfn,
        Entry::Slab {
            class,
            free_head: idx,
            in_use: in_use - 1,
            next_partial,
            prev_partial,
        },
    );

    Some(())
}

pub fn chain_len(frames: &Frames, pfn: Pfn) -> Option<usize> {
    let Entry::Slab {
        class, free_head, ..
    } = frames.page(pfn)
    else {
        return None;
    };

    let size = CLASSES.get(class as usize)?;
    let slots = PAGE_SIZE / size;
    let base = pfn.to_addr();

    let mut index = free_head;
    let mut count = 0;

    while index != FREE_HEAD_EMPTY_VALUE {
        if index as usize >= slots || count == slots {
            return None;
        }

        index = unsafe { *((base + index as usize * size) as *const u16) };
        count += 1;
    }

    Some(count)
}
