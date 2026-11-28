// The seeded xorshift RNG from `mnist` (copied; a shared utility crate is still deferred).
// Used for the validation split, word dropout and batch order, so training is reproducible.
// Burn's own random numbers (weight initialisation) are seeded separately via `device.seed`.

pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        // xorshift with state 0 would output 0 forever, so swap in a fixed non-zero constant.
        let state = if seed == 0 { 0x9E3779B97F4A7C15 } else { seed };
        Rng { state }
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    /// Uniform in [0, 1): keep the top 53 bits, which is exactly an f64's precision.
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Uniform in [min, max).
    pub fn range(&mut self, min: f64, max: f64) -> f64 {
        min + (max - min) * self.next_f64()
    }

    /// True with probability `p`.
    pub fn chance(&mut self, p: f64) -> bool {
        self.next_f64() < p
    }

    /// Fisher–Yates shuffle: walk from the end, swapping each slot with a random earlier
    /// (or the same) slot. Every permutation is equally likely, apart from a modulo bias
    /// of about n / 2^64, which is far too small to matter here.
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = (self.next_u64() % (i as u64 + 1)) as usize;
            items.swap(i, j);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_same_seed_same_sequence() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn test_zero_seed_is_remapped() {
        let mut rng = Rng::new(0);
        assert!((0..10).any(|_| rng.next_u64() != 0));
    }

    #[test]
    fn test_same_seed_same_shuffle() {
        let mut a: Vec<usize> = (0..50).collect();
        let mut b = a.clone();
        Rng::new(9).shuffle(&mut a);
        Rng::new(9).shuffle(&mut b);
        assert_eq!(a, b);
    }

    #[test]
    fn test_shuffle_is_a_permutation() {
        let original: Vec<usize> = (0..100).collect();
        let mut shuffled = original.clone();
        Rng::new(1).shuffle(&mut shuffled);

        assert_ne!(shuffled, original, "100 items should not stay in order");
        let mut sorted = shuffled.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, original, "every item appears exactly once");
    }

    #[test]
    fn test_shuffle_empty_and_single() {
        let mut empty: Vec<u8> = Vec::new();
        Rng::new(1).shuffle(&mut empty);
        let mut one = vec![7];
        Rng::new(1).shuffle(&mut one);
        assert_eq!(one, vec![7]);
    }
}
