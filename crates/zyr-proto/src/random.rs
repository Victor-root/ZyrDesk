//! Random values for sensitive uses.
//!
//! Everything here draws from the operating system generator: what it
//! makes, the names of the links an engine is reached by among them,
//! must stay unpredictable to any other local user.

use rand::distr::{Alphanumeric, SampleString};

/// Alphanumeric string drawn from the system generator.
pub fn alphanumeric_string(length: usize) -> String {
    Alphanumeric.sample_string(&mut rand::rng(), length)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_is_honoured_and_characters_are_safe() {
        let drawn = alphanumeric_string(32);
        assert_eq!(drawn.len(), 32);
        assert!(drawn.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn two_draws_differ() {
        assert_ne!(alphanumeric_string(32), alphanumeric_string(32));
    }
}
