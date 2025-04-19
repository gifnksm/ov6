//! Helper utilities for integration tests.
//!
//! This module provides utility functions to assist with integration testing,
//! such as generating random strings.

use rand::distr::{Alphanumeric, SampleString as _};

/// Generates a random alphanumeric string of the specified length.
///
/// This function uses a random number generator to produce a string
/// consisting of alphanumeric characters.
///
/// # Examples
///
/// ```
/// let random_string = random_str(10);
/// println!("Random string: {}", random_string);
/// ```
#[must_use]
pub fn random_str(len: usize) -> String {
    let mut rng = rand::rng();
    Alphanumeric.sample_string(&mut rng, len)
}
