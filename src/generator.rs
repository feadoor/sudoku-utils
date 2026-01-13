use crate::bit_iter::BitIter;
use crate::fast_solver::FastBruteForceSolver;
use crate::sudoku::{ALL_DIGITS, BOX_INDICES, COL_INDICES, ROW_INDICES, Sudoku};
use crate::template::{Template, TemplateDigit};

/// A structure capable of iterating over all partial Sudoku grids fitting
/// a particular template.
pub struct Generator {
    puzzle: Sudoku,
    wildcards: Vec<(usize, u16)>,
    placements: Vec<(usize, BitIter<u16>)>,
    used_placements: [bool; 81],
    placement_count: usize,
    progress: f64,
    progress_increments: Vec<f64>,
    rows: [u16; 9],
    cols: [u16; 9],
    boxes: [u16; 9],
}

impl Generator {
    pub fn for_template(template: &Template) -> Self {
        let (mut puzzle, mut rows, mut cols, mut boxes) = (Sudoku::empty(), [ALL_DIGITS; 9], [ALL_DIGITS; 9], [ALL_DIGITS; 9]);
        let mut wildcards = Vec::new();

        for (idx, digit) in template.digits().enumerate() {
            match digit {
                TemplateDigit::Empty => {},
                &TemplateDigit::Given(d) => {
                    puzzle[idx] = d;
                    rows[ROW_INDICES[idx]] ^= 1 << d;
                    cols[COL_INDICES[idx]] ^= 1 << d;
                    boxes[BOX_INDICES[idx]] ^= 1 << d;
                },
                TemplateDigit::Wildcard(ds) => {
                    let bitmask = ds.iter().map(|&d| 1 << d).reduce(|a, b| a | b).unwrap_or(0);
                    wildcards.push((idx, bitmask));
                }
            }
        }

        Self { 
            placements: Vec::with_capacity(wildcards.len()), used_placements: [false; 81], placement_count: 0, 
            progress: 0.0, progress_increments: Vec::with_capacity(wildcards.len()),
            puzzle, wildcards, 
            rows, cols, boxes 
        }
    }

    // Decide which digit placement to branch on - use the one with the smallest branching factor
    fn best_branch_digit(&self) -> (usize, BitIter<u16>) {
        self.wildcards.iter()
            .filter(|&&(idx, _)| !self.used_placements[idx])
            .map(|&(idx, mask)| (idx, BitIter::new(mask & self.rows[ROW_INDICES[idx]] & self.cols[COL_INDICES[idx]] & self.boxes[BOX_INDICES[idx]])))
            .min_by_key(|(_, bits)| bits.size_hint().0)
            .unwrap()
    }

    // Place a single digit in the partial puzzle
    fn place(&mut self, idx: usize, d: u8) {
        self.puzzle[idx] = d;
        self.rows[ROW_INDICES[idx]] ^= 1 << d;
        self.cols[COL_INDICES[idx]] ^= 1 << d;
        self.boxes[BOX_INDICES[idx]] ^= 1 << d;
        self.used_placements[idx] = true;
        self.placement_count += 1;
    }

    // Remove the digit at the given location in the partial puzzle
    fn unplace(&mut self, idx: usize) {
        let d = self.puzzle[idx];
        if d != 0 {
            self.rows[ROW_INDICES[idx]] ^= 1 << d;
            self.cols[COL_INDICES[idx]] ^= 1 << d;
            self.boxes[BOX_INDICES[idx]] ^= 1 << d;
            self.puzzle[idx] = 0;
            self.used_placements[idx] = false;
            self.placement_count -=1 ;
        }
    }

    // Take a single step onwards in the depth-first placement search
    fn step(&mut self) -> bool {

        // Pop all of the locations for which we've fully explored their digits.
        // If we've finished the search (i.e. everything has been popped) then return.
        while let Some(placement) = self.placements.last() {
            if placement.1.peek().is_none() { 
                let placement = self.placements.pop().unwrap();
                self.progress_increments.pop();
                self.unplace(placement.0);
                if self.placements.len() == 0 { return false; }
            }
            else { break; }
        }
        
        // Advance the deepest placement by one digit
        if let Some((&mut idx, d)) = self.placements.last_mut().and_then(|(idx, bits)| bits.next().map(|bit| (idx, bit as u8))) {
            self.unplace(idx);
            self.place(idx, d);
            if self.placement_count == self.wildcards.len() { self.progress += self.progress_increments.last().unwrap(); }
        }

        // Deepen the search by one level, branching on the placement with the smallest branching factor
        if self.placements.len() < self.wildcards.len() {
            if FastBruteForceSolver::has_solution(&self.puzzle) {
                self.placements.push(self.best_branch_digit());
                self.progress_increments.push(self.progress_increments.last().unwrap_or(&1.0) / (self.placements.last().unwrap().1.size_hint().0 as f64));
            } else {
                self.progress += self.progress_increments.last().unwrap();
            }
        }

        true
    }
}

impl Iterator for Generator  {
    type Item = (f64, f64, Sudoku);

    fn next(&mut self) -> Option<Self::Item> {
        while self.step() {
            if self.placement_count == self.wildcards.len() && FastBruteForceSolver::has_solution(&self.puzzle) {
                return Some((self.progress, *self.progress_increments.last().unwrap_or(&1.0), self.puzzle.clone()));
            }
        }
        None
    }
}
