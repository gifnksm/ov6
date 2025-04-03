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
macro_rules! exit {
    ($($msg:tt)*) => {
        {
            $crate::message!($($msg)*);
            ::ov6_user_lib::process::exit(1);
        }
    }
}

#[macro_export]
macro_rules! message_err {
    ($e:expr) => {
        {
            let prog = ::ov6_user_lib::env::arg0().display();
            ::ov6_user_lib::eprintln!("{prog}: {e}", e = $e);
        }
    };
    ($e:expr, $($msg:tt)*) => {
        {
            let prog = ::ov6_user_lib::env::arg0().display();
            ::ov6_user_lib::eprintln!("{prog}: {msg}: {e}", msg = ::core::format_args!($($msg)*), e = $e);
        }
    };
}

#[macro_export]
macro_rules! exit_err {
    ($e:expr, $($msg:tt)*) => {
        {
            $crate::message_err!($e, $($msg)*);
            ::ov6_user_lib::process::exit(1);
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
macro_rules! expect {
    ($result:expr, $value:pat, $($msg:tt)+) => {
        if !matches!($result, $value) {
            panic!(
                "{}: Expected {:?}, got {:?}: {}",
                stringify!($result),
                stringify!($value),
                $result,
                format_args!($($msg)+),
            );
        }
    };
    ($result:expr, $value:pat $(,)?) => {
        $crate::expect!($result, $value, "");
    };
}
