//! Random values for sensitive uses.
//!
//! Everything here draws from the operating system generator: the host
//! engine credentials and the pairing code must stay unpredictable to
//! any other local user.

use rand::RngExt;
use rand::distr::{Alphanumeric, SampleString};

/// Alphanumeric string drawn from the system generator.
pub fn alphanumeric_string(length: usize) -> String {
    Alphanumeric.sample_string(&mut rand::rng(), length)
}

/// Four-digit pairing code, leading zeros included.
///
/// The format is imposed by the engines' pairing protocol.
pub fn pairing_pin() -> String {
    format!("{:04}", rand::rng().random_range(0..10_000u16))
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

    #[test]
    fn the_pin_always_has_four_digits() {
        for _ in 0..500 {
            let pin = pairing_pin();
            assert_eq!(pin.len(), 4, "{pin}");
            assert!(pin.chars().all(|c| c.is_ascii_digit()), "{pin}");
        }
    }
}
