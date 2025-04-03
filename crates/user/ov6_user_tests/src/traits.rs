use core::convert::Infallible;

pub trait OrExit<T, E> {
    fn or_exit<F>(self, exit: F) -> T
    where
        F: FnOnce(E) -> Infallible;
}

impl<T, E> OrExit<T, E> for Result<T, E> {
    #[track_caller]
    fn or_exit<F>(self, exit: F) -> T
    where
        F: FnOnce(E) -> Infallible,
    {
        match self {
            Ok(val) => val,
            Err(e) => {
                let _: Infallible = exit(e);
                unreachable!()
            }
        }
    }
}
