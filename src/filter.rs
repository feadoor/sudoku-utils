use crate::fast_solver::FastBruteForceSolver;
use crate::logic::BasicSolver;
use crate::pipeline::RegionMaskedSudoku;

pub enum Filter {
    AtMostNBasicPlacements { n: usize },
    SolvesWithBasicsAfterElims { elims: Vec<((usize, usize), u8)> },
    HasAnySolution,
    HasUniqueSolution,
}

impl Filter {
    pub fn matches(&self, sudoku: &RegionMaskedSudoku) -> bool {
        match self {
            Self::AtMostNBasicPlacements { n } => at_most_n_basic_placements(*n, sudoku),
            Self::SolvesWithBasicsAfterElims { elims } => solves_with_basics_after_elims(elims, sudoku),
            Self::HasAnySolution => FastBruteForceSolver::has_solution(sudoku.sudoku()),
            Self::HasUniqueSolution => FastBruteForceSolver::has_unique_solution(sudoku.sudoku()),
        }
    }

    pub fn at_most_n_basic_placements(n: usize) -> Self {
        Self::AtMostNBasicPlacements { n }
    }

    pub fn solves_with_basics_after_elims(elim_str: &str) -> Self {
        let elims = elim_str.split(",").map(|s| s.trim());
        let elims = elims.flat_map(|elim| {
            let (digits, rc) = elim.split_once("r").unwrap();
            let (r, c) = rc.split_once("c").unwrap();
            let (r, c): (usize, usize) = (r.parse().unwrap(), c.parse().unwrap());
            digits.chars().map(|d| d.to_digit(10).unwrap() as u8).map(move |d| ((r - 1, c - 1), d))
        });
        Self::SolvesWithBasicsAfterElims { elims: elims.collect() }
    }
}

fn at_most_n_basic_placements(n: usize, sudoku: &RegionMaskedSudoku) -> bool {
    let missing_count = sudoku.empty_cells();
    let mut solver = BasicSolver::for_region_masked_sudoku(sudoku);
    while let Some(true) = solver.step_basics() {
        if solver.empty_cells() + n < missing_count {
            return false;
        }
    }
    true
}

fn solves_with_basics_after_elims(elims: &[((usize, usize), u8)], sudoku: &RegionMaskedSudoku) -> bool {
    let mut solver = BasicSolver::for_region_masked_sudoku(sudoku);
    solver.eliminate_candidates(elims);
    solver.solve_basics();
    solver.is_solved()
}
