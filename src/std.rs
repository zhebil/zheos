#[macro_export]
macro_rules! println {
    ($($arg:tt)*) => {
        let _ = write!(uart(), $($arg)*);
        let _ = writeln!(uart());
    };
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        let _ = write!(uart(), $($arg)*);
    };
}
