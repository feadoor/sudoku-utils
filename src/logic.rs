use itertools::Itertools;

use crate::bitmask::Bitmask;
use crate::sudoku::{ALL_DIGITS, BOX_INDICES, BOXES, COL_INDICES, COLS, PEERS, ROW_INDICES, ROWS, Sudoku, Sukaku};

/// Solver capable of performing basic logic:
/// - Naked and Hidden Singles
/// - Pointing and Claiming
/// - Naked and Hidden Subsets
pub struct BasicSolver {
    sukaku: Sukaku,
    placed: Bitmask<u128>,
    placed_count: usize,
    missing_from_rows: [Bitmask<u16>; 9],
    missing_from_cols: [Bitmask<u16>; 9],
    missing_from_boxes: [Bitmask<u16>; 9],
}

impl BasicSolver {

    /// Initialise a solver with the clues from the given Sudoku
    pub fn for_sudoku(sudoku: &Sudoku) -> Self {
        let sukaku = Sukaku::from_sudoku(sudoku);
        Self::for_sukaku(sukaku)
    }

    /// Initialise a solver with the clues from the given Sukaku
    pub fn for_sukaku(sukaku: Sukaku) -> Self {
        Self { 
            sukaku, placed: Bitmask::<u128>::empty(), placed_count: 0, 
            missing_from_rows: [ALL_DIGITS; 9], missing_from_cols: [ALL_DIGITS; 9], missing_from_boxes: [ALL_DIGITS; 9] 
        }
    }

    /// Carry out all basic deductions of the simplest kind for
    /// which any deductions exist
    pub fn step_basics(&mut self) -> Option<bool> {
        if self.do_naked_singles()? { return Some(true); }
        if self.do_hidden_singles()? { return Some(true); }
        Some(self.do_intersections() || self.do_subsets())
    }

    /// Carry out all basic deductions until no more remain
    pub fn solve_basics(&mut self) {
        while let Some(true) = self.step_basics() {}
    }

    /// Check if the puzzle is solved
    pub fn is_solved(&self) -> bool {
        self.placed_count == 81
    }

    /// Count the number of solved cells
    pub fn solved_cells(&self) -> usize {
        self.placed_count
    }

    /// Place the given digit (represented by a bitmask with a single set bit)
    /// in the location indexed by the given index.
    fn place(&mut self, idx: usize, mask: Bitmask<u16>) {
        self.sukaku[idx] = mask;
        for jdx in PEERS[idx] { self.sukaku[jdx] &= !mask; }
        self.placed.set(idx);
        self.placed_count += 1;
        self.missing_from_rows[ROW_INDICES[idx]] &= !mask;
        self.missing_from_cols[COL_INDICES[idx]] &= !mask;
        self.missing_from_boxes[BOX_INDICES[idx]] &= !mask;
    }

    /// Eliminate the given digits (represented by a bitmask) from the location
    /// indexed by the given index.
    fn eliminate(&mut self, idx: usize, mask: Bitmask<u16>) -> bool {
        if (self.sukaku[idx] & mask).is_not_empty() {
            self.sukaku[idx] &= !mask;
            true
        } else {
            false
        }
    }

    /// Find and apply all Naked Singles
    fn do_naked_singles(&mut self) -> Option<bool> {
        let mut made_progress = false;
        for idx in 0 .. 81 {
            if !self.placed.contains(idx) {
                match self.sukaku[idx].count_ones() {
                    0 => { return None; }
                    1 => {
                        self.place(idx, self.sukaku[idx]);
                        made_progress = true;
                    }
                    _ => {},
                }
            }
        }
        Some(made_progress)
    }

    /// Find and apply all Hidden Singles
    fn do_hidden_singles(&mut self) -> Option<bool> {
        let mut made_progress = false;

        macro_rules! do_hidden_singles {
            ($regions:expr, $missing:expr) => {
                for (region_idx, region) in $regions.iter().enumerate() {
                    let (mut at_least_once, mut more_than_once) = (Bitmask::<u16>::empty(), Bitmask::<u16>::empty());
                    for &idx in region.iter().filter(|&&idx| !self.placed.contains(idx)) {
                        let mask = self.sukaku[idx];
                        more_than_once |= at_least_once & mask;
                        at_least_once |= mask;
                    }
                    if at_least_once != $missing[region_idx] { return None; }
                    let exactly_once = at_least_once & !more_than_once;
                    if exactly_once.is_not_empty() {
                        for &idx in region {
                            match (self.sukaku[idx] & exactly_once).count_ones() {
                                0 => {},
                                1 => {
                                    self.place(idx, self.sukaku[idx] & exactly_once);
                                    made_progress = true;
                                },
                                _ => { return None; }
                            }
                        }
                    }
                }
            }
        }

        do_hidden_singles!(ROWS, self.missing_from_rows);
        do_hidden_singles!(COLS, self.missing_from_cols);
        do_hidden_singles!(BOXES, self.missing_from_boxes);

        Some(made_progress)
    }

    /// Find and apply all Pointing and Claiming steps
    fn do_intersections(&mut self) -> bool {
        let mut made_progress = false;

        macro_rules! do_intersections {
            ($left:expr, $left_indices:expr, $right:expr, $right_indices:expr, $missing:expr) => {
                for (left_idx, left) in $left.iter().enumerate() {
                    for mask in $missing[left_idx].as_mask_iter().map(Bitmask::<u16>::from) {
                        if let Ok(right_idx) = left.iter().filter(|&&idx| (self.sukaku[idx] & mask).is_not_empty()).map(|&idx| $right_indices[idx]).all_equal_value() {
                            for &idx in &$right[right_idx] {
                                if $left_indices[idx] != left_idx {
                                    made_progress |= self.eliminate(idx, mask);
                                }
                            }
                        }
                    }
                }
            }
        }

        do_intersections!(ROWS, ROW_INDICES, BOXES, BOX_INDICES, self.missing_from_rows);
        do_intersections!(COLS, COL_INDICES, BOXES, BOX_INDICES, self.missing_from_cols);
        do_intersections!(BOXES, BOX_INDICES, ROWS, ROW_INDICES, self.missing_from_boxes);
        do_intersections!(BOXES, BOX_INDICES, COLS, COL_INDICES, self.missing_from_boxes);

        made_progress
    }

    /// Find and apply all Naked and Hidden Subsets
    fn do_subsets(&mut self) -> bool {
        let mut made_progress = false;

        macro_rules! do_subsets {
            ($regions:expr, $missing:expr) => {
                for region in $regions.iter() {
                    let unsolved_cells = Bitmask::<u128>::from_iter(region.iter().copied().filter(|&idx| !self.placed.contains(idx)));
                    let unsolved_count = unsolved_cells.count_ones();
                    for subset in unsolved_cells.as_subset_iter() {
                        let sz = subset.count_ones();
                        if sz < 2 || sz > unsolved_count.saturating_sub(2) { continue; }
                        let covered = subset.as_bit_iter().map(|idx| self.sukaku[idx]).reduce(|a, b| a | b).unwrap();
                        if covered.count_ones() == sz {
                            for cell in (unsolved_cells & !subset).as_bit_iter() {
                                made_progress |= self.eliminate(cell, covered);
                            }
                        }
                    }
                }
            }
        }

        do_subsets!(ROWS, self.missing_from_rows);
        do_subsets!(COLS, self.missing_from_cols);
        do_subsets!(BOXES, self.missing_from_boxes);

        made_progress
    }
}
