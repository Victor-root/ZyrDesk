//! What the tests of this crate share: a seeded source of noise, so that
//! a failure seen once is seen again with the same seed.

/// A small deterministic pseudo-random generator (SplitMix64).
pub(crate) struct Noise(u64);

impl Noise {
    pub(crate) fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub(crate) fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A number in `0..bound`.
    pub(crate) fn below(&mut self, bound: usize) -> usize {
        (self.next_u64() % bound as u64) as usize
    }

    /// True with the given chance, in percent.
    pub(crate) fn percent(&mut self, chance: usize) -> bool {
        self.below(100) < chance
    }

    pub(crate) fn bytes(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| self.next_u64() as u8).collect()
    }

    /// A damaged copy of a valid message: cut short, grown, or with a
    /// few bytes changed, which reaches further into a decoder than pure
    /// noise ever does.
    pub(crate) fn mangled(&mut self, valid: &[u8]) -> Vec<u8> {
        let mut bytes = valid.to_vec();
        match self.below(4) {
            0 => bytes.truncate(self.below(bytes.len() + 1)),
            1 => {
                let more = self.below(8) + 1;
                bytes.extend(self.bytes(more));
            }
            _ => {
                for _ in 0..=self.below(3) {
                    if !bytes.is_empty() {
                        let at = self.below(bytes.len());
                        bytes[at] = self.next_u64() as u8;
                    }
                }
            }
        }
        bytes
    }

    /// Some input for a decoder: either pure noise or a damaged copy of
    /// one of the valid messages given.
    pub(crate) fn garbage(&mut self, valid: &[Vec<u8>]) -> Vec<u8> {
        if valid.is_empty() || self.percent(30) {
            let len = self.below(64);
            self.bytes(len)
        } else {
            let pick = self.below(valid.len());
            self.mangled(&valid[pick])
        }
    }
}
