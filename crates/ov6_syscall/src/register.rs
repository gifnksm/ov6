use core::{convert::Infallible, marker::PhantomData, num::NonZero};

use ov6_types::{fs::RawFd, process::ProcId};

use crate::{
    OpenFlags, Register, RegisterDecodeError, RegisterValue, UserMutRef, UserMutSlice, UserRef,
    UserSlice, error::SyscallError,
};

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
}

macro_rules! impl_value {
    ([$($bound:tt)*] $ty:ty, $err:ty, $n:expr, $enc:ident, $dec:ident) => {
        impl<$($bound)*> RegisterValue for $ty {
            type DecodeError = $err;
            type Repr = Register<Self, $n>;

            fn encode(self) -> Self::Repr {
                $enc(self)
            }

            fn try_decode(repr: Self::Repr) -> Result<Self, Self::DecodeError> {
                $dec(repr)
            }
        }
    };
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

impl RegisterValue for u64 {
    type DecodeError = Infallible;
    type Repr = Register<Self, 1>;

    fn encode(self) -> Self::Repr {
        usize::from_ne_bytes(self.to_ne_bytes()).encode().map_type()
    }

    fn try_decode(repr: Self::Repr) -> Result<Self, Self::DecodeError> {
        let [a0] = repr.a;
        Ok(Self::from_ne_bytes(a0.to_ne_bytes()))
    }
}

impl RegisterValue for i64 {
    type DecodeError = Infallible;
    type Repr = Register<Self, 1>;

    fn encode(self) -> Self::Repr {
        usize::from_ne_bytes(self.to_ne_bytes()).encode().map_type()
    }

    fn try_decode(repr: Self::Repr) -> Result<Self, Self::DecodeError> {
        let [a0] = repr.a;
        Ok(Self::from_ne_bytes(a0.to_ne_bytes()))
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
impl_number!(usize, u32);
impl_number!(isize, i8);
impl_number!(isize, i16);
impl_number!(isize, i32);

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
        let n = repr.map_type().try_decode()?;
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
    type DecodeError = Infallible;
    type Repr = Register<Self, 1>;

    fn encode(self) -> Self::Repr {
        self.get().encode().map_type()
    }

    fn try_decode(repr: Self::Repr) -> Result<Self, Self::DecodeError> {
        let n = repr.map_type().try_decode()?;
        Ok(Self::new(n))
    }
}

impl RegisterValue for OpenFlags {
    type DecodeError = RegisterDecodeError;
    type Repr = Register<Self, 1>;

    fn encode(self) -> Self::Repr {
        self.bits().encode().map_type()
    }

    fn try_decode(repr: Self::Repr) -> Result<Self, Self::DecodeError> {
        let bits = repr.map_type().try_decode()?;
        Self::from_bits(bits).ok_or(RegisterDecodeError::InvalidOpenFlags(bits))
    }
}

impl<T> RegisterValue for UserRef<T>
where
    T: ?Sized,
{
    type DecodeError = Infallible;
    type Repr = Register<Self, 1>;

    fn encode(self) -> Self::Repr {
        self.addr.encode().map_type()
    }

    fn try_decode(repr: Self::Repr) -> Result<Self, Self::DecodeError> {
        let addr = repr.map_type().try_decode()?;
        Ok(Self {
            addr,
            _phantom: PhantomData,
        })
    }
}

impl<T> RegisterValue for UserMutRef<T>
where
    T: ?Sized,
{
    type DecodeError = Infallible;
    type Repr = Register<Self, 1>;

    fn encode(self) -> Self::Repr {
        self.addr.encode().map_type()
    }

    fn try_decode(repr: Self::Repr) -> Result<Self, Self::DecodeError> {
        let addr = repr.map_type().try_decode()?;
        Ok(Self {
            addr,
            _phantom: PhantomData,
        })
    }
}

impl<T> RegisterValue for UserSlice<T> {
    type DecodeError = Infallible;
    type Repr = Register<Self, 2>;

    fn encode(self) -> Self::Repr {
        let [a0] = self.addr.encode().a;
        let [a1] = self.len.encode().a;
        Self::Repr::new([a0, a1])
    }

    fn try_decode(repr: Self::Repr) -> Result<Self, Self::DecodeError> {
        let [a0, a1] = repr.a;
        let addr = Register::new([a0]).try_decode()?;
        let len = Register::new([a1]).try_decode()?;
        Ok(Self {
            addr,
            len,
            _phantom: PhantomData,
        })
    }
}

impl<T> RegisterValue for UserMutSlice<T> {
    type DecodeError = Infallible;
    type Repr = Register<Self, 2>;

    fn encode(self) -> Self::Repr {
        let [a0] = self.addr.encode().a;
        let [a1] = self.len.encode().a;
        Self::Repr::new([a0, a1])
    }

    fn try_decode(repr: Self::Repr) -> Result<Self, Self::DecodeError> {
        let [a0, a1] = repr.a;
        let addr = Register::new([a0]).try_decode()?;
        let len = Register::new([a1]).try_decode()?;
        Ok(Self {
            addr,
            len,
            _phantom: PhantomData,
        })
    }
}

fn result_encode_01<T, E>(res: Result<T, E>) -> Register<Result<T, E>, 2>
where
    T: RegisterValue<Repr = Register<T, 0>>,
    E: RegisterValue<Repr = Register<E, 1>>,
{
    match res {
        Ok(v1) => {
            let a0 = 0;
            let [] = v1.encode().a;
            Register::new([a0, 0])
        }
        Err(v1) => {
            let a0 = usize::MAX;
            let [a1] = v1.encode().a;
            Register::new([a0, a1])
        }
    }
}

fn result_decode_01<T, E>(
    repr: Register<Result<T, E>, 2>,
) -> Result<Result<T, E>, RegisterDecodeError>
where
    T: RegisterValue<Repr = Register<T, 0>>,
    E: RegisterValue<Repr = Register<E, 1>>,
    RegisterDecodeError: From<T::DecodeError> + From<E::DecodeError>,
{
    let [a0, a1] = repr.a;
    match a0 {
        0 => {
            let v1 = Register::new([]).try_decode()?;
            Ok(Ok(v1))
        }
        usize::MAX => {
            let v1 = Register::new([a1]).try_decode()?;
            Ok(Err(v1))
        }
        n => Err(RegisterDecodeError::InvalidResultDesignator(n)),
    }
}

fn result_encode_11<T, E>(res: Result<T, E>) -> Register<Result<T, E>, 2>
where
    T: RegisterValue<Repr = Register<T, 1>>,
    E: RegisterValue<Repr = Register<E, 1>>,
{
    match res {
        Ok(v1) => {
            let a0 = 0;
            let [a1] = v1.encode().a;
            Register::new([a0, a1])
        }
        Err(v1) => {
            let a0 = usize::MAX;
            let [a1] = v1.encode().a;
            Register::new([a0, a1])
        }
    }
}

fn result_decode_11<T, E>(
    repr: Register<Result<T, E>, 2>,
) -> Result<Result<T, E>, RegisterDecodeError>
where
    T: RegisterValue<Repr = Register<T, 1>>,
    E: RegisterValue<Repr = Register<E, 1>>,
    RegisterDecodeError: From<T::DecodeError> + From<E::DecodeError>,
{
    let [a0, a1] = repr.a;
    match a0 {
        0 => {
            let v1 = Register::new([a1]).try_decode()?;
            Ok(Ok(v1))
        }
        usize::MAX => {
            let v1 = Register::new([a1]).try_decode()?;
            Ok(Err(v1))
        }
        n => Err(RegisterDecodeError::InvalidResultDesignator(n)),
    }
}

impl_value!([] Result<(), SyscallError>, RegisterDecodeError, 2, result_encode_01, result_decode_01);
impl_value!([] Result<Infallible, SyscallError>, RegisterDecodeError, 2, result_encode_01, result_decode_01);
impl_value!([] Result<usize, SyscallError>, RegisterDecodeError, 2, result_encode_11, result_decode_11);
impl_value!([] Result<Option<ProcId>, SyscallError>, RegisterDecodeError, 2, result_encode_11, result_decode_11);
impl_value!([] Result<ProcId, SyscallError>, RegisterDecodeError, 2, result_encode_11, result_decode_11);
impl_value!([] Result<RawFd, SyscallError>, RegisterDecodeError, 2, result_encode_11, result_decode_11);

fn tuple1_encode<T, const N: usize>((v0,): (T,)) -> Register<(T,), N>
where
    T: RegisterValue<Repr = Register<T, N>>,
{
    Register::new(v0.encode().a)
}

fn tuple1_decode<T, const N: usize>(repr: Register<(T,), N>) -> Result<(T,), T::DecodeError>
where
    T: RegisterValue<Repr = Register<T, N>>,
{
    let v0 = Register::new(repr.a).try_decode()?;
    Ok((v0,))
}

fn tuple_encode_11<T, U>((v0, v1): (T, U)) -> Register<(T, U), 2>
where
    T: RegisterValue<Repr = Register<T, 1>>,
    U: RegisterValue<Repr = Register<U, 1>>,
{
    let [a0] = v0.encode().a;
    let [a1] = v1.encode().a;
    Register::new([a0, a1])
}

fn tuple_decode_11<T, U, E>(repr: Register<(T, U), 2>) -> Result<(T, U), E>
where
    T: RegisterValue<Repr = Register<T, 1>>,
    U: RegisterValue<Repr = Register<U, 1>>,
    E: From<T::DecodeError> + From<U::DecodeError>,
{
    let [a0, a1] = repr.a;
    let v0 = Register::new([a0]).try_decode()?;
    let v1 = Register::new([a1]).try_decode()?;
    Ok((v0, v1))
}

fn tuple_encode_12<T, U>((v0, v1): (T, U)) -> Register<(T, U), 3>
where
    T: RegisterValue<Repr = Register<T, 1>>,
    U: RegisterValue<Repr = Register<U, 2>>,
{
    let [a0] = v0.encode().a;
    let [a1, a2] = v1.encode().a;
    Register::new([a0, a1, a2])
}

fn tuple_decode_12<T, U, E>(repr: Register<(T, U), 3>) -> Result<(T, U), E>
where
    T: RegisterValue<Repr = Register<T, 1>>,
    U: RegisterValue<Repr = Register<U, 2>>,
    E: From<T::DecodeError> + From<U::DecodeError>,
{
    let [a0, a1, a2] = repr.a;
    let v0 = Register::new([a0]).try_decode()?;
    let v1 = Register::new([a1, a2]).try_decode()?;
    Ok((v0, v1))
}

fn tuple_encode_21<T, U>((v0, v1): (T, U)) -> Register<(T, U), 3>
where
    T: RegisterValue<Repr = Register<T, 2>>,
    U: RegisterValue<Repr = Register<U, 1>>,
{
    let [a0, a1] = v0.encode().a;
    let [a2] = v1.encode().a;
    Register::new([a0, a1, a2])
}

fn tuple_decode_21<T, U, E>(repr: Register<(T, U), 3>) -> Result<(T, U), E>
where
    T: RegisterValue<Repr = Register<T, 2>>,
    U: RegisterValue<Repr = Register<U, 1>>,
    E: From<T::DecodeError> + From<U::DecodeError>,
{
    let [a0, a1, a2] = repr.a;
    let v0 = Register::new([a0, a1]).try_decode()?;
    let v1 = Register::new([a2]).try_decode()?;
    Ok((v0, v1))
}

fn tuple_encode_22<T, U>((v0, v1): (T, U)) -> Register<(T, U), 4>
where
    T: RegisterValue<Repr = Register<T, 2>>,
    U: RegisterValue<Repr = Register<U, 2>>,
{
    let [a0, a1] = v0.encode().a;
    let [a2, a3] = v1.encode().a;
    Register::new([a0, a1, a2, a3])
}

fn tuple_decode_22<T, U, E>(repr: Register<(T, U), 4>) -> Result<(T, U), E>
where
    T: RegisterValue<Repr = Register<T, 2>>,
    U: RegisterValue<Repr = Register<U, 2>>,
    E: From<T::DecodeError> + From<U::DecodeError>,
{
    let [a0, a1, a2, a3] = repr.a;
    let v0 = Register::new([a0, a1]).try_decode()?;
    let v1 = Register::new([a2, a3]).try_decode()?;
    Ok((v0, v1))
}

fn tuple_encode_211<T, U, V>((v0, v1, v2): (T, U, V)) -> Register<(T, U, V), 4>
where
    T: RegisterValue<Repr = Register<T, 2>>,
    U: RegisterValue<Repr = Register<U, 1>>,
    V: RegisterValue<Repr = Register<V, 1>>,
{
    let [a0, a1] = v0.encode().a;
    let [a2] = v1.encode().a;
    let [a3] = v2.encode().a;
    Register::new([a0, a1, a2, a3])
}

fn tuple_decode_211<T, U, V, E>(repr: Register<(T, U, V), 4>) -> Result<(T, U, V), E>
where
    T: RegisterValue<Repr = Register<T, 2>>,
    U: RegisterValue<Repr = Register<U, 1>>,
    V: RegisterValue<Repr = Register<V, 1>>,
    E: From<T::DecodeError> + From<U::DecodeError> + From<V::DecodeError>,
{
    let [a0, a1, a2, a3] = repr.a;
    let v0 = Register::new([a0, a1]).try_decode()?;
    let v1 = Register::new([a2]).try_decode()?;
    let v2 = Register::new([a3]).try_decode()?;
    Ok((v0, v1, v2))
}

impl_value!(
    [](u16,),
    RegisterDecodeError,
    1,
    tuple1_encode,
    tuple1_decode
);

impl_value!(
    [](i32,),
    RegisterDecodeError,
    1,
    tuple1_encode,
    tuple1_decode
);
impl_value!([](u64,), Infallible, 1, tuple1_encode, tuple1_decode);
impl_value!([](isize,), Infallible, 1, tuple1_encode, tuple1_decode);
impl_value!([](RawFd,), Infallible, 1, tuple1_encode, tuple1_decode);
impl_value!(
    [](ProcId,),
    RegisterDecodeError,
    1,
    tuple1_encode,
    tuple1_decode
);
impl_value!([T: ?Sized](UserRef<T>,), Infallible, 1, tuple1_encode, tuple1_decode);
impl_value!([T: ?Sized](UserMutRef<T>,), Infallible, 1, tuple1_encode, tuple1_decode);
impl_value!([T](UserSlice<T>,), Infallible, 2, tuple1_encode, tuple1_decode);
impl_value!([T](UserMutSlice<T>,), Infallible, 2, tuple1_encode, tuple1_decode);

impl_value!([T: ?Sized] (RawFd, UserMutRef<T>), Infallible, 2, tuple_encode_11, tuple_decode_11);
impl_value!([T: ?Sized] (RawFd, UserRef<T>), Infallible, 2, tuple_encode_11, tuple_decode_11);
impl_value!([T] (UserSlice<T>, OpenFlags), RegisterDecodeError, 3, tuple_encode_21, tuple_decode_21);
impl_value!([T, U] (UserSlice<T>, UserSlice<U>), Infallible, 4, tuple_encode_22, tuple_decode_22);

impl_value!([T] (RawFd, UserSlice<T>), Infallible, 3, tuple_encode_12, tuple_decode_12);
impl_value!([T] (RawFd, UserMutSlice<T>), Infallible, 3, tuple_encode_12, tuple_decode_12);
impl_value!([T: ?Sized, U] (UserRef<T>, UserSlice<U>), Infallible, 3, tuple_encode_12, tuple_decode_12);

impl_value!([T] (UserSlice<T>, u32, i16), RegisterDecodeError, 4, tuple_encode_211, tuple_decode_211);
