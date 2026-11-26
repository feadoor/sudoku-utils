//! The brute-force solver: counts the solutions of a puzzle by propagation and search.
//!
//! The state is, for each digit and each band, a mask of the cells of the band which can hold
//! the digit. Propagation has three stages, each run only when the one before has nothing
//! left to do:
//!
//!  1. **Filtering**, which concerns one digit at a time. Within a band a digit takes one
//!     minirow in each box, using each row once; minirows which belong to no such arrangement
//!     are cleared. Likewise, within a stack a digit takes one minicolumn in each band, using
//!     each column once.
//!
//!  2. **Settling**, which relates the digits to one another. A digit confined to one cell of
//!     a box, or a cell left with one digit, means that the digit owns the cell: the other
//!     digits are removed from it, and the digit is removed from the rest of the box.
//!
//!  3. **Triad cardinality.** Every minirow and minicolumn holds exactly three digits, so
//!     where only three can go in one they all do, and where three are certain to, no other
//!     can.
//!
//! The search then branches on the arrangements of one digit within one band or stack.
//!
//! There are two implementations, which make the same deductions and the same choices:
//! `vector`, which needs AVX2 and BMI2, and `scalar`, which runs anywhere. `BandSweepSolver`
//! picks between them according to the CPU.

pub mod scalar;
mod tables;
#[cfg(target_arch = "x86_64")]
pub mod vector;

use crate::utils::sudoku::Sudoku;

pub struct BandSweepSolver;

impl BandSweepSolver {
    pub fn has_solution(sudoku: &Sudoku) -> bool {
        Self::count_up_to(sudoku, 1) == 1
    }

    pub fn has_unique_solution(sudoku: &Sudoku) -> bool {
        Self::count_up_to(sudoku, 2) == 1
    }

    pub fn count_solutions(sudoku: &Sudoku) -> usize {
        Self::count_up_to(sudoku, usize::MAX)
    }

    /// Whether the puzzle has a unique solution which it would lose were any clue removed.
    pub fn is_minimal(sudoku: &Sudoku) -> bool {
        if !Self::has_unique_solution(sudoku) {
            return false;
        }
        let mut sudoku = sudoku.clone();
        (0..81).all(|cell| {
            let digit = std::mem::take(&mut sudoku[cell]);
            let needed = digit == 0 || !Self::has_unique_solution(&sudoku);
            sudoku[cell] = digit;
            needed
        })
    }

    /// Count the solutions of `sudoku`, stopping early once `limit` have been found.
    pub fn count_up_to(sudoku: &Sudoku, limit: usize) -> usize {
        #[cfg(target_arch = "x86_64")]
        if vector::is_supported() {
            // SAFETY: the CPU features which the vector solver needs have just been detected.
            return unsafe { vector::count_up_to(sudoku, limit) };
        }
        scalar::count_up_to(sudoku, limit)
    }
}
