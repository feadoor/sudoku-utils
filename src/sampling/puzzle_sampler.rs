use itertools::Itertools;
use rand::Rng;
use crate::sampling::grid_sampler::sample_solved_grid;
use crate::sampling::mask_sampler::MaskSampler;
use crate::solving::fast_solver::FastBruteForceSolver;
use crate::utils::sudoku::Sudoku;

pub struct PuzzleSampler {
    mask_sampler: MaskSampler,
}

impl PuzzleSampler {
    pub fn new(min_clues: usize, max_clues: usize) -> Self {
        Self { mask_sampler: MaskSampler::new(min_clues, max_clues) }
    }

    fn generate_clues(&self, rng: &mut impl Rng) -> Sudoku {
        let solved_grid = sample_solved_grid(rng);
        let mask = self.mask_sampler.sample(rng);
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
