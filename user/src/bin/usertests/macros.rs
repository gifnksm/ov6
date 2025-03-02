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

#[macro_export]
macro_rules! test_entry {
    ($name:path) => {
        (stringify!($name), $name)
    };
}
