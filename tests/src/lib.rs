#![no_std]

#[macro_export]
macro_rules! message {
    ($($msg:tt)*) => {
        {
            let prog = ::ov6_user_lib::env::arg0().display();
            ::ov6_user_lib::eprintln!("{prog}: {msg}", msg = ::core::format_args!($($msg)*));
        }
    }
}
