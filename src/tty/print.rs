#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let _ = write!($crate::drivers::pl011::uart(), $($arg)*);
    }};
}

#[macro_export]
macro_rules! println {
    () => {{
        use core::fmt::Write;
        let _ = writeln!($crate::drivers::pl011::uart());
    }};
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let _ = writeln!($crate::drivers::pl011::uart(), $($arg)*);
    }};
}
