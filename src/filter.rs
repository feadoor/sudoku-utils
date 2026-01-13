use crate::logic::BasicSolver;
use crate::sudoku::{Sudoku, Sukaku};

pub enum Filter {
    AtMostNBasicPlacements { n: usize },
    SolvesWithBasicsAfterElims { elims: Vec<((usize, usize), u8)> }
}

impl Filter {
    pub fn matches(&self, sudoku: &Sudoku) -> bool {
        match self {
            Self::AtMostNBasicPlacements { n } => at_most_n_basic_placements(*n, sudoku),
            Self::SolvesWithBasicsAfterElims { elims } => solves_with_basics_after_elims(elims, sudoku),
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

fn at_most_n_basic_placements(n: usize, sudoku: &Sudoku) -> bool {
    let clue_count = sudoku.solved_cells();
    let mut solver = BasicSolver::for_sudoku(sudoku);
    while solver.step_basics() {
        if solver.solved_cells() > clue_count + n {
            return false;
        }
    }
    true
}

fn solves_with_basics_after_elims(elims: &[((usize, usize), u8)], sudoku: &Sudoku) -> bool {
    let mut sukaku = Sukaku::from_sudoku(sudoku);
    for &((row, col), clue) in elims { sukaku[(row, col)] &= !(1 << clue) }
    let mut solver = BasicSolver::for_sukaku(sukaku);
    solver.solve_basics();
    solver.is_solved()
}
