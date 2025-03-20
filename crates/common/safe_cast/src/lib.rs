#![cfg_attr(not(test), no_std)]

#[must_use]
#[expect(clippy::cast_possible_truncation)]
pub const fn to_u8(n: usize) -> u8 {
    assert!(n <= u8::MAX as usize);
    n as u8
}

#[must_use]
#[expect(clippy::cast_possible_truncation)]
pub const fn to_u16(n: usize) -> u16 {
    assert!(n <= u16::MAX as usize);
    n as u16
}

#[must_use]
#[expect(clippy::cast_possible_truncation)]
pub const fn to_u32(n: usize) -> u32 {
    assert!(n <= u32::MAX as usize);
    n as u32
}

#[must_use]
#[expect(clippy::cast_possible_truncation)]
pub const fn to_u64(n: usize) -> u64 {
    assert!(n <= u64::MAX as usize);
    n as u64
}

#[macro_export]
macro_rules! to_u8 {
    ($n:expr) => {
        const { $crate::to_u8($n) }
    };
}

#[macro_export]
macro_rules! to_u16 {
    ($n:expr) => {
        const { $crate::to_u16($n) }
    };
}

#[macro_export]
macro_rules! to_u32 {
    ($n:expr) => {
        const { $crate::to_u32($n) }
    };
}

#[macro_export]
macro_rules! to_u64 {
    ($n:expr) => {
        const { $crate::to_u64($n) }
    };
}

pub trait SafeFrom<T> {
    fn safe_from(value: T) -> Self;
}

pub trait SafeInto<T> {
    fn safe_into(self) -> T;
}

impl SafeFrom<u8> for usize {
    fn safe_from(value: u8) -> Self {
        value.into()
    }
}

impl SafeFrom<u16> for usize {
    fn safe_from(value: u16) -> Self {
        value.into()
    }
}

#[cfg(any(target_pointer_width = "32", target_pointer_width = "64"))]
impl SafeFrom<u32> for usize {
    fn safe_from(value: u32) -> Self {
        value.try_into().unwrap()
    }
}

#[cfg(target_pointer_width = "64")]
impl SafeFrom<u64> for usize {
    fn safe_from(value: u64) -> Self {
        value.try_into().unwrap()
    }
}

#[cfg(target_pointer_width = "64")]
impl SafeFrom<usize> for u64 {
    fn safe_from(value: usize) -> Self {
        value.try_into().unwrap()
    }
}

impl SafeFrom<i8> for isize {
    fn safe_from(value: i8) -> Self {
        value.into()
    }
}

impl SafeFrom<i16> for isize {
    fn safe_from(value: i16) -> Self {
        value.into()
    }
}

#[cfg(any(target_pointer_width = "32", target_pointer_width = "64"))]
impl SafeFrom<i32> for isize {
    fn safe_from(value: i32) -> Self {
        value.try_into().unwrap()
    }
}

#[cfg(target_pointer_width = "64")]
impl SafeFrom<i64> for isize {
    fn safe_from(value: i64) -> Self {
        value.try_into().unwrap()
    }
}

#[cfg(target_pointer_width = "64")]
impl SafeFrom<isize> for i64 {
    fn safe_from(value: isize) -> Self {
        value.try_into().unwrap()
    }
}

impl<T, U> SafeInto<U> for T
where
    U: SafeFrom<T>,
{
    fn safe_into(self) -> U {
        U::safe_from(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(target_pointer_width = "32", target_pointer_width = "64"))]
    #[test]
    fn test_u32_to_usize() {
        for n in [42, 0, u32::MAX] {
            let result: usize = SafeFrom::safe_from(n);
            assert_eq!(result, n.try_into().unwrap());
        }
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn test_u64_to_usize() {
        for n in [42, 0, u64::MAX] {
            let result: usize = SafeFrom::safe_from(n);
            assert_eq!(result, n.try_into().unwrap());
        }
    }

    #[cfg(any(target_pointer_width = "32", target_pointer_width = "64"))]
    #[test]
    fn test_i32_to_isize() {
        for n in [42, -42, 0, i32::MIN, i32::MAX] {
            let result: isize = SafeFrom::safe_from(n);
            assert_eq!(result, n.try_into().unwrap());
        }
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn test_i64_to_isize() {
        for n in [42, -42, 0, i64::MIN, i64::MAX] {
            let result: isize = SafeFrom::safe_from(n);
            assert_eq!(result, n.try_into().unwrap());
        }
    }
}
