use rand::distr::{Alphanumeric, SampleString as _};

#[must_use]
pub fn random_str(len: usize) -> String {
    let mut rng = rand::rng();
    Alphanumeric.sample_string(&mut rng, len)
}
