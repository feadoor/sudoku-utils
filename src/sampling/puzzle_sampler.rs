use itertools::Itertools;
use rand::Rng;

use crate::sampling::grid_sampler::sample_solved_grid;
use crate::sampling::mask_sampler::MaskSampler;
use crate::solving::fast_solver::FastBruteForceSolver;
use crate::utils::bitmask::Bitmask;
use crate::utils::sudoku::Sudoku;

const ROWS: [Bitmask<u128>; 9] = [
    Bitmask::<u128>::from(0b000000000_000000000_000000000_000000000_000000000_000000000_000000000_000000000_111111111),
    Bitmask::<u128>::from(0b000000000_000000000_000000000_000000000_000000000_000000000_000000000_111111111_000000000),
    Bitmask::<u128>::from(0b000000000_000000000_000000000_000000000_000000000_000000000_111111111_000000000_000000000),
    Bitmask::<u128>::from(0b000000000_000000000_000000000_000000000_000000000_111111111_000000000_000000000_000000000),
    Bitmask::<u128>::from(0b000000000_000000000_000000000_000000000_111111111_000000000_000000000_000000000_000000000),
    Bitmask::<u128>::from(0b000000000_000000000_000000000_111111111_000000000_000000000_000000000_000000000_000000000),
    Bitmask::<u128>::from(0b000000000_000000000_111111111_000000000_000000000_000000000_000000000_000000000_000000000),
    Bitmask::<u128>::from(0b000000000_111111111_000000000_000000000_000000000_000000000_000000000_000000000_000000000),
    Bitmask::<u128>::from(0b111111111_000000000_000000000_000000000_000000000_000000000_000000000_000000000_000000000),
];

const COLS: [Bitmask<u128>; 9] = [
    Bitmask::<u128>::from(0b000000001_000000001_000000001_000000001_000000001_000000001_000000001_000000001_000000001),
    Bitmask::<u128>::from(0b000000010_000000010_000000010_000000010_000000010_000000010_000000010_000000010_000000010),
    Bitmask::<u128>::from(0b000000100_000000100_000000100_000000100_000000100_000000100_000000100_000000100_000000100),
    Bitmask::<u128>::from(0b000001000_000001000_000001000_000001000_000001000_000001000_000001000_000001000_000001000),
    Bitmask::<u128>::from(0b000010000_000010000_000010000_000010000_000010000_000010000_000010000_000010000_000010000),
    Bitmask::<u128>::from(0b000100000_000100000_000100000_000100000_000100000_000100000_000100000_000100000_000100000),
    Bitmask::<u128>::from(0b001000000_001000000_001000000_001000000_001000000_001000000_001000000_001000000_001000000),
    Bitmask::<u128>::from(0b010000000_010000000_010000000_010000000_010000000_010000000_010000000_010000000_010000000),
    Bitmask::<u128>::from(0b100000000_100000000_100000000_100000000_100000000_100000000_100000000_100000000_100000000),
];


pub struct PuzzleSampler {
    mask_sampler: MaskSampler,
}

impl PuzzleSampler {
    pub fn new(min_clues: usize, max_clues: usize) -> Self {
        Self { mask_sampler: MaskSampler::new(min_clues, max_clues) }
    }

    fn validate_mask(&mask: &Bitmask<u128>) -> bool {
        !((ROWS[0] & mask).is_empty() && (ROWS[1] & mask).is_empty()) &&
        !((ROWS[0] & mask).is_empty() && (ROWS[2] & mask).is_empty()) &&
        !((ROWS[1] & mask).is_empty() && (ROWS[2] & mask).is_empty()) &&
        !((ROWS[3] & mask).is_empty() && (ROWS[4] & mask).is_empty()) &&
        !((ROWS[3] & mask).is_empty() && (ROWS[5] & mask).is_empty()) &&
        !((ROWS[4] & mask).is_empty() && (ROWS[5] & mask).is_empty()) &&
        !((ROWS[6] & mask).is_empty() && (ROWS[7] & mask).is_empty()) &&
        !((ROWS[6] & mask).is_empty() && (ROWS[8] & mask).is_empty()) &&
        !((ROWS[7] & mask).is_empty() && (ROWS[8] & mask).is_empty()) &&
        !((COLS[0] & mask).is_empty() && (COLS[1] & mask).is_empty()) &&
        !((COLS[0] & mask).is_empty() && (COLS[2] & mask).is_empty()) &&
        !((COLS[1] & mask).is_empty() && (COLS[2] & mask).is_empty()) &&
        !((COLS[3] & mask).is_empty() && (COLS[4] & mask).is_empty()) &&
        !((COLS[3] & mask).is_empty() && (COLS[5] & mask).is_empty()) &&
        !((COLS[4] & mask).is_empty() && (COLS[5] & mask).is_empty()) &&
        !((COLS[6] & mask).is_empty() && (COLS[7] & mask).is_empty()) &&
        !((COLS[6] & mask).is_empty() && (COLS[8] & mask).is_empty()) &&
        !((COLS[7] & mask).is_empty() && (COLS[8] & mask).is_empty())
    }

    fn generate_mask(&self, rng: &mut impl Rng) -> Bitmask<u128> {
        std::iter::from_fn(|| Some(self.mask_sampler.sample(rng))).filter(Self::validate_mask).next().unwrap()
    }

    fn generate_clues(&self, rng: &mut impl Rng) -> Sudoku {
        let solved_grid = sample_solved_grid(rng);
        let mask = self.generate_mask(rng);
        Sudoku((0 .. 81).map(|idx|
            if mask.contains(idx) { solved_grid[idx] } else { 0 }
        ).collect_array().unwrap())
    }

    fn attempt(&self, rng: &mut impl Rng) -> Option<Sudoku> {
        let clues = self.generate_clues(rng);
        if FastBruteForceSolver::has_unique_solution(&clues) {
            Some(clues)
        } else {
            None
        }
    }

    fn attempt_minimal(&self, rng: &mut impl Rng) -> Option<Sudoku> {
        let clues = self.generate_clues(rng);
        if FastBruteForceSolver::is_minimal(&clues) {
            Some(clues)
        } else {
            None
        }
    }

    pub fn sample(&self, rng: &mut impl Rng) -> Sudoku {
        loop {
            if let Some(puzzle) = self.attempt(rng) {
                return puzzle;
            }
        }
    }

    pub fn sample_minimal(&self, rng: &mut impl Rng) -> Sudoku {
        loop {
            if let Some(puzzle) = self.attempt_minimal(rng) {
                return puzzle;
            }
        }
    }
}
