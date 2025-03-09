use core::{convert::Infallible, marker::PhantomData, num::NonZero};

use ov6_types::{fs::RawFd, process::ProcId};

use crate::{Register, RegisterValue, SyscallError};

impl<T, const N: usize> Register<T, N> {
    fn new(a: [usize; N]) -> Self {
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

    #[must_use]
    pub fn decode(self) -> T
    where
        T: RegisterValue<Repr = Self>,
    {
        T::decode(self)
    }
}

impl RegisterValue for Infallible {
    type Repr = Register<Self, 0>;

    fn encode(self) -> Self::Repr {
        unreachable!()
    }

    fn decode(_repr: Self::Repr) -> Self {
        unreachable!()
    }
}

impl RegisterValue for () {
    type Repr = Register<(), 0>;

    fn encode(self) -> Self::Repr {
        Register::new([])
    }

    fn decode(_: Self::Repr) -> Self {}
}

impl RegisterValue for usize {
    type Repr = Register<Self, 1>;

    fn encode(self) -> Self::Repr {
        Register::new([self])
    }

    fn decode(repr: Self::Repr) -> Self {
        repr.a[0]
    }
}

impl RegisterValue for isize {
    type Repr = Register<Self, 1>;

    fn encode(self) -> Self::Repr {
        Register::new([self.cast_unsigned()])
    }

    fn decode(repr: Self::Repr) -> Self {
        let [a0] = repr.a;
        a0.cast_signed()
    }
}

macro_rules! impl_number {
    ($base_ty:ty, $ty:ty) => {
        impl RegisterValue for $ty {
            type Repr = Register<Self, 1>;

            fn encode(self) -> Self::Repr {
                let n: $base_ty = self.try_into().unwrap();
                n.encode().map_type()
            }

            fn decode(repr: Self::Repr) -> Self {
                let n: $base_ty = repr.map_type().decode();
                n.try_into().unwrap()
            }
        }
    };
}

impl_number!(usize, u8);
impl_number!(usize, u16);
impl_number!(usize, u32);
impl_number!(usize, u64);
impl_number!(isize, i8);
impl_number!(isize, i16);
impl_number!(isize, i32);
impl_number!(isize, i64);

macro_rules! impl_nonzero {
    ($ty:ty) => {
        impl RegisterValue for Option<NonZero<$ty>> {
            type Repr = Register<Self, 1>;

            fn encode(self) -> Self::Repr {
                let n = self.map_or_else(Default::default, NonZero::get);
                n.encode().map_type()
            }

            fn decode(repr: Self::Repr) -> Self {
                let n = repr.map_type().decode();
                NonZero::new(n)
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
    type Repr = Register<Self, 1>;

    fn encode(self) -> Self::Repr {
        (self as isize).encode().map_type()
    }

    fn decode(repr: Self::Repr) -> Self {
        let n = repr.map_type().decode();
        Self::from_repr(n).unwrap_or(Self::Unknown)
    }
}

impl RegisterValue for Option<ProcId> {
    type Repr = Register<Self, 1>;

    fn encode(self) -> Self::Repr {
        self.map(ProcId::get).encode().map_type()
    }

    fn decode(repr: Self::Repr) -> Self {
        let n = repr.map_type().decode();
        Option::map(n, ProcId::new)
    }
}

impl RegisterValue for ProcId {
    type Repr = Register<Self, 1>;

    fn encode(self) -> Self::Repr {
        Some(self).encode().map_type()
    }

    fn decode(repr: Self::Repr) -> Self {
        repr.map_type::<Option<Self>>().decode().unwrap()
    }
}

impl RegisterValue for RawFd {
    type Repr = Register<Self, 1>;

    fn encode(self) -> Self::Repr {
        self.get().encode().map_type()
    }

    fn decode(repr: Self::Repr) -> Self {
        Self::new(repr.map_type().decode())
    }
}

macro_rules! impl_result0 {
    ($ty:ty) => {
        impl RegisterValue for Result<$ty, SyscallError> {
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

            fn decode(repr: Self::Repr) -> Self {
                let [a0, a1] = repr.a;
                match a0 {
                    0 => Ok(Register::new([]).decode()),
                    usize::MAX => Err(Register::new([a1]).decode()),
                    _ => panic!("invalid discriminant value: {a0}"),
                }
            }
        }
    };
}
impl_result0!(());
impl_result0!(Infallible);

macro_rules! impl_result1 {
    ($ty:ty) => {
        impl RegisterValue for Result<$ty, SyscallError> {
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

            fn decode(repr: Self::Repr) -> Self {
                let [a0, a1] = repr.a;
                match a0 {
                    0 => Ok(Register::new([a1]).decode()),
                    usize::MAX => Err(Register::new([a1]).decode()),
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
