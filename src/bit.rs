pub const fn bit_mask(bit: u32) -> u32 {
    1 << bit
}

pub const fn compose_u32(high: u32, low: u32) -> u64 {
    (high as u64) << 32 | low as u64
}
