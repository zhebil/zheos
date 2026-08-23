use crate::dtb::Region;

pub fn image() -> Region {
    unsafe extern "C" {
        static __image_start: u8;
        static __stack_top: u8;
    }

    let start = &raw const __image_start as usize;
    let end = &raw const __stack_top as usize;

    Region {
        base: start,
        size: end - start,
    }
}
