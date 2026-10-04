use core::fmt::Display;

mod board;
mod dtb;

pub use board::{Board, EARLYCON_UART, Gic};
pub use dtb::Dtb;

pub enum DiscoverError {
    NoTree(usize),
    Missing(&'static str),
}

impl Display for DiscoverError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NoTree(ptr) => write!(f, "No device tree at {ptr:#010x}"),
            Self::Missing(missing) => write!(f, "Device tree has no {missing}"),
        }
    }
}

pub fn discover(dtb_ptr: usize) -> Result<(Dtb<'static>, Board), DiscoverError> {
    let dtb = unsafe { Dtb::from_ptr(dtb_ptr) }.ok_or(DiscoverError::NoTree(dtb_ptr))?;
    let board = Board::discover(&dtb).map_err(DiscoverError::Missing)?;

    Ok((dtb, board))
}
