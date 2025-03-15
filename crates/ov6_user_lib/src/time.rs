pub use core::time::Duration;
use core::{
    arch::asm,
    ops::{Add, Sub, SubAssign},
};

const TICKS_PER_SEC: u64 = 10;
const NANOS_PER_TICKS: u64 = NANOS_PER_SEC / TICKS_PER_SEC;

const NANOS_PER_SEC: u64 = 1_000_000_000;

pub(crate) trait DurationExt {
    fn as_ticks(&self) -> u64;
}

impl DurationExt for Duration {
    fn as_ticks(&self) -> u64 {
        self.as_secs() * TICKS_PER_SEC + u64::from(self.subsec_nanos()) / NANOS_PER_TICKS
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Instant {
    nanos: u64,
}

impl Instant {
    #[must_use]
    pub fn now() -> Self {
        let time: u64;
        unsafe {
            asm!("csrr {}, time", out(reg) time);
        }
        Self { nanos: time * 100 }
    }

    #[must_use]
    pub fn duration_since(&self, earlier: Self) -> Duration {
        self.checked_duration_since(earlier).unwrap_or_default()
    }

    #[must_use]
    pub fn checked_duration_since(&self, earlier: Self) -> Option<Duration> {
        self.nanos
            .checked_sub(earlier.nanos)
            .map(Duration::from_nanos)
    }

    #[must_use]
    pub fn saturating_duration_since(&self, earlier: Self) -> Duration {
        self.checked_duration_since(earlier).unwrap_or_default()
    }

    #[must_use]
    pub fn elapsed(&self) -> Duration {
        Self::now() - *self
    }

    #[must_use]
    pub fn checked_add(&self, duration: Duration) -> Option<Self> {
        let nanos = self
            .nanos
            .checked_add(duration.as_nanos().try_into().ok()?)?;
        Some(Self { nanos })
    }

    #[must_use]
    pub fn checked_sub(&self, duration: Duration) -> Option<Self> {
        let nanos = self
            .nanos
            .checked_sub(duration.as_nanos().try_into().ok()?)?;
        Some(Self { nanos })
    }
}

impl Sub for Instant {
    type Output = Duration;

    fn sub(self, rhs: Self) -> Self::Output {
        Duration::from_nanos(self.nanos - rhs.nanos)
    }
}

impl Add<Duration> for Instant {
    type Output = Self;

    fn add(self, rhs: Duration) -> Self::Output {
        Self {
            nanos: self.nanos + u64::try_from(rhs.as_nanos()).unwrap(),
        }
    }
}

impl Sub<Duration> for Instant {
    type Output = Self;

    fn sub(self, rhs: Duration) -> Self::Output {
        Self {
            nanos: self.nanos - u64::try_from(rhs.as_nanos()).unwrap(),
        }
    }
}

impl SubAssign for Instant {
    fn sub_assign(&mut self, rhs: Self) {
        self.nanos -= rhs.nanos;
    }
}
