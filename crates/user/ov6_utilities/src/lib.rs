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

#[macro_export]
macro_rules! usage_and_exit {
    ($($args:tt)*) => {
        {
            let prog = ::ov6_user_lib::env::arg0().display();
            ::ov6_user_lib::eprintln!("Usage: {prog} {args}", args = ::core::format_args!($($args)*));
            ::ov6_user_lib::process::exit(1);
        }
    };
}

#[macro_export]
macro_rules! try_or {
    ($res:expr, $on_err:expr, $e:ident => $($msg:tt)*) => {
        match $res {
            Ok(val) => val,
            Err($e) => {
                $crate::message!($($msg)*);
                $on_err
            }
        }
    }
}

#[macro_export]
macro_rules! try_or_exit {
    ($res:expr, $e:ident => $($msg:tt)*) => {
        match $res {
            Ok(val) => val,
            Err($e) => {
                $crate::exit!($($msg)*);
            }
        }
    }
}

#[macro_export]
macro_rules! try_or_panic {
    ($res:expr, $e:ident => $($msg:tt)*) => {
        match $res {
            Ok(val) => val,
            Err($e) => {
                let prog = ::ov6_user_lib::env::arg0().display();
                ::core::panic!("{prog}: {msg}", msg = ::core::format_args!($($msg)*));
            }
        }
    }
}

#[macro_export]
macro_rules! ensure_or {
    ($cond:expr, $on_false:expr, $($msg:tt)*) => {
        if !$cond {
            $crate::message!($($msg)*);
            $on_false
        }
    }
}

#[macro_export]
macro_rules! ensure_or_exit {
    ($cond:expr, $($msg:tt)*) => {
        if !$cond {
            $crate::exit!($($msg)*);
        }
    }
}

#[macro_export]
macro_rules! exit {
    ($($msg:tt)*) => {
        {
            $crate::message!($($msg)*);
            ::ov6_user_lib::process::exit(1);
        }
    }
}
