use std::iter::empty;

use crate::bit_iter::BitIter;
use crate::bitmask::Bitmask;
use crate::dfs_with_progress::DepthFirstTraversable;
use crate::sudoku::{ALL_DIGITS, BOX_INDICES, COL_INDICES, ROW_INDICES, Sudoku};
use crate::template::{Template, TemplateDigit};

/// A structure capable of iterating over all partial Sudoku grids fitting
/// a particular template.
pub struct GeneratorState {
    puzzle: Sudoku,
    wildcards: Vec<(usize, Bitmask<u16>)>,
    used_placements: [bool; 81],
    placement_count: usize,
    rows: [Bitmask<u16>; 9],
    cols: [Bitmask<u16>; 9],
    boxes: [Bitmask<u16>; 9],
}

impl GeneratorState {
    pub fn for_template(template: &Template) -> Self {
        let wildcards = template.digits().enumerate().filter_map(|(idx, digit)| {
            match digit {
                TemplateDigit::Empty => None,
                &TemplateDigit::Given(d) => Some((idx, Bitmask::<u16>::singleton(d))),
                TemplateDigit::Wildcard(ds) => Some((idx, Bitmask::<u16>::from_iter(ds.iter().copied())))
            }
        }).collect();

        Self {
            wildcards,
            used_placements: [false; 81], placement_count: 0, 
            puzzle: Sudoku::empty(),
            rows: [ALL_DIGITS; 9], cols: [ALL_DIGITS; 9], boxes: [ALL_DIGITS; 9], 
        }
    }

    // Decide which digit placement to branch on - use the one with the smallest branching factor
    fn best_branch_digit(&self) -> Option<(usize, BitIter<u16>)> {
        self.wildcards.iter()
            .filter(|&&(idx, _)| !self.used_placements[idx])
            .map(|&(idx, mask)| (idx, (mask & self.rows[ROW_INDICES[idx]] & self.cols[COL_INDICES[idx]] & self.boxes[BOX_INDICES[idx]]).into_bit_iter()))
            .min_by_key(|(_, bits)| bits.len())
    }
}

impl DepthFirstTraversable for GeneratorState {
    type Step = (usize, u8);
    type Output = Sudoku;

    fn next_steps(&mut self) -> Box<dyn ExactSizeIterator<Item = Self::Step>> {
        if let Some((idx, digits)) = self.best_branch_digit() {
            Box::new(digits.map(move |d| (idx, d as u8)))
        } else {
            Box::new(empty())
        }
    }

    fn apply_step(&mut self, &(idx, d): &Self::Step) {
        self.puzzle[idx] = d;
        self.rows[ROW_INDICES[idx]].unset(d);
        self.cols[COL_INDICES[idx]].unset(d);
        self.boxes[BOX_INDICES[idx]].unset(d);
        self.used_placements[idx] = true;
        self.placement_count += 1;
    }

    fn revert_step(&mut self, &(idx, d): &Self::Step) {
        self.puzzle[idx] = 0;
        self.rows[ROW_INDICES[idx]].set(d);
        self.cols[COL_INDICES[idx]].set(d);
        self.boxes[BOX_INDICES[idx]].set(d);
        self.used_placements[idx] = false;
        self.placement_count -=1 ;
    }

    fn should_prune(&mut self) -> bool {
        false
    }

    fn output(&mut self) -> Option<Self::Output> {
        (self.placement_count == self.wildcards.len()).then(|| self.puzzle.clone())
    }
}
