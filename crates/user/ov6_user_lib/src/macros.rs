#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        #[expect(clippy::used_underscore_items)]
        $crate::io::_print(format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! println {
    () => {
        $crate::print!("\n")
    };
    ($($arg:tt)*) => {
        $crate::print!("{}\n", format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! eprint {
    ($($arg:tt)*) => {
        #[expect(clippy::used_underscore_items)]
        $crate::io::_eprint(format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! eprintln {
    () => {
        $crate::eprint!("\n")
    };
    ($($arg:tt)*) => {
        $crate::eprint!("{}\n", format_args!($($arg)*))
    };
}
