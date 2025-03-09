use core::{convert::Infallible, marker::PhantomData, num::NonZero};

use ov6_types::{fs::RawFd, process::ProcId};

use crate::{Register, RegisterDecodeError, RegisterValue, SyscallError};

impl<T, const N: usize> Register<T, N> {
    pub fn new(a: [usize; N]) -> Self {
        Self {
            a,
            _phantom: PhantomData,
        }
    }

    fn map_type<U>(self) -> Register<U, N> {
        Register {
            a: self.a,
            _phantom: PhantomData,
        }
    }

    pub fn try_decode(self) -> Result<T, T::DecodeError>
    where
        T: RegisterValue<Repr = Self>,
    {
        T::try_decode(self)
    }

    #[must_use]
    pub fn decode(self) -> T
    where
        T: RegisterValue<Repr = Self>,
    {
        Self::try_decode(self).unwrap()
    }
}

impl RegisterValue for Infallible {
    type DecodeError = Self;
    type Repr = Register<Self, 0>;

    fn encode(self) -> Self::Repr {
        unreachable!()
    }

    fn try_decode(_repr: Self::Repr) -> Result<Self, Self::DecodeError> {
        unreachable!()
    }
}

impl RegisterValue for () {
    type DecodeError = Infallible;
    type Repr = Register<(), 0>;

    fn encode(self) -> Self::Repr {
        Register::new([])
    }

    fn try_decode(_: Self::Repr) -> Result<Self, Self::DecodeError> {
        Ok(())
    }
}

impl<T> RegisterValue for (T,)
where
    T: RegisterValue,
{
    type DecodeError = T::DecodeError;
    type Repr = T::Repr;

    fn encode(self) -> Self::Repr {
        let (x,) = self;
        T::encode(x)
    }

    fn try_decode(repr: Self::Repr) -> Result<Self, Self::DecodeError> {
        let x = T::try_decode(repr)?;
        Ok((x,))
    }
}

impl RegisterValue for usize {
    type DecodeError = Infallible;
    type Repr = Register<Self, 1>;

    fn encode(self) -> Self::Repr {
        Register::new([self])
    }

    fn try_decode(repr: Self::Repr) -> Result<Self, Self::DecodeError> {
        Ok(repr.a[0])
    }
}

impl RegisterValue for isize {
    type DecodeError = Infallible;
    type Repr = Register<Self, 1>;

    fn encode(self) -> Self::Repr {
        Register::new([self.cast_unsigned()])
    }

    fn try_decode(repr: Self::Repr) -> Result<Self, Self::DecodeError> {
        let [a0] = repr.a;
        Ok(a0.cast_signed())
    }
}

macro_rules! impl_number {
    ($base_ty:ty, $ty:ty) => {
        impl RegisterValue for $ty {
            type DecodeError = RegisterDecodeError;
            type Repr = Register<Self, 1>;

            fn encode(self) -> Self::Repr {
                const _: () = const {
                    assert!(
                        size_of::<$ty>() <= size_of::<$base_ty>(),
                        concat!(
                            "base_ty(",
                            stringify!($base_ty),
                            ") must be greater than ty (",
                            stringify!($ty),
                            ")"
                        ),
                    );
                };
                // this conversion must be success
                let n: $base_ty = self.try_into().unwrap();
                n.encode().map_type()
            }

            fn try_decode(repr: Self::Repr) -> Result<Self, Self::DecodeError> {
                let n: $base_ty = repr.map_type().try_decode()?;
                Ok(n.try_into()?)
            }
        }
    };
}

impl_number!(usize, u8);
impl_number!(usize, u16);
impl_number!(isize, i8);
impl_number!(isize, i16);
impl_number!(usize, u32);
impl_number!(usize, u64);
impl_number!(isize, i32);
impl_number!(isize, i64);

macro_rules! impl_nonzero {
    ($ty:ty) => {
        impl RegisterValue for Option<NonZero<$ty>> {
            type DecodeError = RegisterDecodeError;
            type Repr = Register<Self, 1>;

            fn encode(self) -> Self::Repr {
                let n = self.map_or_else(Default::default, NonZero::get);
                n.encode().map_type()
            }

            fn try_decode(repr: Self::Repr) -> Result<Self, Self::DecodeError> {
                let n = repr.map_type().try_decode()?;
                Ok(NonZero::new(n))
            }
        }
    };
}

impl_nonzero!(u8);
impl_nonzero!(u16);
impl_nonzero!(u32);
impl_nonzero!(u64);
impl_nonzero!(i8);
impl_nonzero!(i16);
impl_nonzero!(i32);
impl_nonzero!(i64);

impl RegisterValue for SyscallError {
    type DecodeError = RegisterDecodeError;
    type Repr = Register<Self, 1>;

    fn encode(self) -> Self::Repr {
        (self as isize).encode().map_type()
    }

    fn try_decode(repr: Self::Repr) -> Result<Self, Self::DecodeError> {
        let Ok(n) = repr.map_type().try_decode();
        Self::from_repr(n).ok_or(RegisterDecodeError::InvalidSyscallErrorNo(n))
    }
}

impl RegisterValue for Option<ProcId> {
    type DecodeError = RegisterDecodeError;
    type Repr = Register<Self, 1>;

    fn encode(self) -> Self::Repr {
        self.map(ProcId::get).encode().map_type()
    }

    fn try_decode(repr: Self::Repr) -> Result<Self, RegisterDecodeError> {
        let n = repr.map_type().try_decode()?;
        Ok(Option::map(n, ProcId::new))
    }
}

impl RegisterValue for ProcId {
    type DecodeError = RegisterDecodeError;
    type Repr = Register<Self, 1>;

    fn encode(self) -> Self::Repr {
        Some(self).encode().map_type()
    }

    fn try_decode(repr: Self::Repr) -> Result<Self, Self::DecodeError> {
        repr.map_type::<Option<Self>>()
            .try_decode()?
            .ok_or(RegisterDecodeError::UnexpectedZero)
    }
}

impl RegisterValue for RawFd {
    type DecodeError = RegisterDecodeError;
    type Repr = Register<Self, 1>;

    fn encode(self) -> Self::Repr {
        self.get().encode().map_type()
    }

    fn try_decode(repr: Self::Repr) -> Result<Self, Self::DecodeError> {
        let Ok(n) = repr.map_type().try_decode();
        Ok(Self::new(n))
    }
}

impl RegisterValue for Result<(), SyscallError> {
    type DecodeError = RegisterDecodeError;
    type Repr = Register<Self, 2>;

    fn encode(self) -> Self::Repr {
        match self {
            Ok(v) => {
                let [] = v.encode().a;
                Self::Repr::new([0, 0])
            }
            Err(e) => {
                let [a1] = e.encode().a;
                Self::Repr::new([usize::MAX, a1])
            }
        }
    }

    fn try_decode(repr: Self::Repr) -> Result<Self, Self::DecodeError> {
        let [a0, a1] = repr.a;
        match a0 {
            0 => {
                let Ok(x) = Register::new([]).try_decode();
                Ok(Ok(x))
            }
            usize::MAX => {
                let x = Register::new([a1]).try_decode()?;
                Ok(Err(x))
            }
            _ => panic!("invalid discriminant value: {a0}"),
        }
    }
}

impl RegisterValue for Result<Infallible, SyscallError> {
    type DecodeError = RegisterDecodeError;
    type Repr = Register<Self, 2>;

    fn encode(self) -> Self::Repr {
        match self {
            Ok(v) => {
                let [] = v.encode().a;
                Self::Repr::new([0, 0])
            }
            Err(e) => {
                let [a1] = e.encode().a;
                Self::Repr::new([usize::MAX, a1])
            }
        }
    }

    fn try_decode(repr: Self::Repr) -> Result<Self, Self::DecodeError> {
        let [a0, a1] = repr.a;
        match a0 {
            0 => unreachable!(),
            usize::MAX => {
                let x = Register::new([a1]).try_decode()?;
                Ok(Err(x))
            }
            _ => panic!("invalid discriminant value: {a0}"),
        }
    }
}

macro_rules! impl_result1 {
    ($ty:ty) => {
        impl RegisterValue for Result<$ty, SyscallError> {
            type DecodeError = RegisterDecodeError;
            type Repr = Register<Self, 2>;

            fn encode(self) -> Self::Repr {
                match self {
                    Ok(v) => {
                        let [a1] = v.encode().a;
                        Self::Repr::new([0, a1])
                    }
                    Err(e) => {
                        let [a1] = e.encode().a;
                        Self::Repr::new([usize::MAX, a1])
                    }
                }
            }

            fn try_decode(repr: Self::Repr) -> Result<Self, Self::DecodeError> {
                let [a0, a1] = repr.a;
                match a0 {
                    0 => Ok(Ok(Register::new([a1]).try_decode()?)),
                    usize::MAX => Ok(Err(Register::new([a1]).try_decode()?)),
                    _ => panic!("invalid discriminant value: {a0}"),
                }
            }
        }
    };
}

impl_result1!(usize);
impl_result1!(Option<ProcId>);
impl_result1!(ProcId);
impl_result1!(RawFd);
