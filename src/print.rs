#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let _ = write!($crate::uart::uart(), $($arg)*);
    }};
}

#[macro_export]
macro_rules! println {
    () => {{
        use core::fmt::Write;
        let _ = writeln!($crate::uart::uart());
    }};
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let _ = writeln!($crate::uart::uart(), $($arg)*);
    }};
}
