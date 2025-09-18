use std::fs::File;
use std::io::{BufRead, BufReader};
use std::ops::{Index, IndexMut};
use std::time::Instant;

use fast_solver::FastBruteForceSolver;
use itertools::Itertools;

mod fast_solver;
mod symmetry;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Sudoku([u8; 81]);

fn from_str(s: &str) -> Sudoku {
    let mut result = [0; 81];
    for (idx, c) in s.chars().enumerate() {
        result[idx] = c.to_digit(10).map(|v| v as u8).unwrap_or(0);
    }
    Sudoku(result)
}

impl Index<(usize, usize)> for Sudoku {
    type Output = u8;

    fn index(&self, (r, c): (usize, usize)) -> &u8 {
        &self.0[9 * r + c]
    }
}

impl IndexMut<(usize, usize)> for Sudoku {
    fn index_mut(&mut self, (r, c): (usize, usize)) -> &mut u8 {
        &mut self.0[9 * r + c]
    }
}

fn main() {
    let file = File::open("data/minlex-testcases").expect("Input file not present");
    let lines = BufReader::new(file).lines().map(|l| l.expect("Error reading from file"));
    let test_cases = lines.filter(|l| !l.is_empty()).map(|line| {
        let (sudoku_str, expected_str) = line.split_ascii_whitespace().collect_tuple().expect("Wrong number of items on line");
        (from_str(sudoku_str), from_str(expected_str))
    }).collect_vec();
    let n_test_cases = test_cases.len();

    let start_time = Instant::now();
    for (sudoku, expected) in test_cases {
        let minlexed = symmetry::minlex(&sudoku);
        if minlexed != expected {
            println!(
                "Failed: {} (got {}, expected {})", 
                sudoku.0.iter().map(|d| if *d == 0 { '.' } else { char::from_digit(*d as u32, 10).unwrap() }).join(""), 
                minlexed.0.iter().map(|d| if *d == 0 { '.' } else { char::from_digit(*d as u32, 10).unwrap() }).join(""), 
                expected.0.iter().map(|d| if *d == 0 { '.' } else { char::from_digit(*d as u32, 10).unwrap() }).join(""),
            );
        }
    }
    let total_time = start_time.elapsed();

    println!("Minlexed {} puzzles in {:?} ({:?} per puzzle)", n_test_cases, total_time, total_time / (n_test_cases as u32));
}
