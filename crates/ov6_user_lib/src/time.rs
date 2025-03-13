use core::ops::{Add, Sub, SubAssign};
pub use core::time::Duration;

use crate::os::ov6::syscall;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Instant(u64);

const TICKS_PER_SEC: u64 = 10;
const NANOS_PER_TICKS: u64 = NANOS_PER_SEC / TICKS_PER_SEC;

const NANOS_PER_SEC: u64 = 1_000_000_000;

pub(crate) trait DurationExt {
    fn as_ticks(&self) -> u64;
    fn from_ticks(ticks: u64) -> Self;
}

impl DurationExt for Duration {
    fn as_ticks(&self) -> u64 {
        self.as_secs() * TICKS_PER_SEC + u64::from(self.subsec_nanos()) / NANOS_PER_TICKS
    }

    fn from_ticks(ticks: u64) -> Self {
        Self::new(
            ticks / TICKS_PER_SEC,
            u32::try_from((ticks % TICKS_PER_SEC) * NANOS_PER_TICKS).unwrap(),
        )
    }
}

impl Instant {
    #[must_use]
    pub fn now() -> Self {
        let ticks = syscall::uptime();
        Self(ticks)
    }

    #[must_use]
    pub fn duration_since(&self, earlier: Self) -> Duration {
        self.checked_duration_since(earlier).unwrap_or_default()
    }

    #[must_use]
    pub fn checked_duration_since(&self, earlier: Self) -> Option<Duration> {
        self.0.checked_sub(earlier.0).map(Duration::from_ticks)
    }

    #[must_use]
    pub fn saturating_duration_since(&self, earlier: Self) -> Duration {
        self.checked_duration_since(earlier).unwrap_or_default()
    }

    #[must_use]
    pub fn elapsed(&self) -> Duration {
        Self::now() - *self
    }

    pub fn checked_add(&self, duration: Duration) -> Option<Self> {
        self.0.checked_add(duration.as_ticks()).map(Self)
    }

    pub fn checked_sub(&self, duration: Duration) -> Option<Self> {
        self.0.checked_sub(duration.as_ticks()).map(Self)
    }
}

impl Sub for Instant {
    type Output = Duration;

    fn sub(self, rhs: Self) -> Self::Output {
        Duration::from_ticks(self.0 - rhs.0)
    }
}

impl Add<Duration> for Instant {
    type Output = Self;

    fn add(self, rhs: Duration) -> Self::Output {
        Self(self.0 + rhs.as_ticks())
    }
}

impl Sub<Duration> for Instant {
    type Output = Self;

    fn sub(self, rhs: Duration) -> Self::Output {
        Self(self.0 - rhs.as_ticks())
    }
}

impl SubAssign for Instant {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0;
    }
}
