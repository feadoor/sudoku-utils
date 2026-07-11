use rand::{Rng, RngExt};

use crate::utils::bitmask::Bitmask;

fn binom(n: u128, k: u128) -> u128 {
    let k = k.min(n - k);
    let mut acc: u128 = 1;
    for i in 0 .. k {
        acc = acc * (n - i) / (i + 1);
    }
    acc
}

pub struct MaskSampler {
    min_clues: usize,
    max_clues: usize,
    cumulative: Vec<u128>,
}

impl MaskSampler {
    pub fn new(min_clues: usize, max_clues: usize) -> Self {
        let mut cumulative = Vec::with_capacity(max_clues - min_clues + 1);
        let mut total = 0;
        for n in min_clues ..= max_clues {
            total += binom(81, n as u128);
            cumulative.push(total);
        }
        Self { min_clues, max_clues, cumulative }
    }

    fn space_size(&self) -> u128 {
        *self.cumulative.last().unwrap()
    }

    fn sample_clue_count(&self, rng: &mut impl Rng) -> usize {
        let x = rng.random_range(0 .. self.space_size());
        let idx = self.cumulative.partition_point(|&c| c <= x);
        self.min_clues + idx
    }

    pub fn sample(&self, rng: &mut impl Rng) -> Bitmask<u128> {
        let n = self.sample_clue_count(rng);
        let mut cells: [u8; 81] = core::array::from_fn(|i| i as u8);
        let mut mask = Bitmask::<u128>::empty();
        for idx in 0 .. n {
            let jdx = rng.random_range(idx .. 81);
            cells.swap(idx, jdx);
            mask.set(cells[idx]);
        }
        mask
    }
}
