use rand::distr::{Alphanumeric, SampleString as _};

pub fn random_str(len: usize) -> String {
    let mut rng = rand::rng();
    Alphanumeric.sample_string(&mut rng, len)
}
