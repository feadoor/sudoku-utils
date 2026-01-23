use std::cell::RefCell;
use std::iter::{empty, once};
use std::rc::Rc;

use itertools::Itertools;

use crate::bitmask::Bitmask;
use crate::dfs_with_progress::{DepthFirstSearcherWithProgress, DepthFirstTraversable};
use crate::pipeline::RegionMaskedSudoku;
use crate::symmetry::DihedralSubgroup;

pub enum Expansion {
    PlusN { n: usize, symmetry: DihedralSubgroup, excluded_cells: Vec<(usize, usize)> },
}

impl Expansion {
    pub fn expand(&self, sudoku: Rc<RefCell<RegionMaskedSudoku>>) -> Box<dyn Iterator<Item = (f64, f64, Rc<RefCell<RegionMaskedSudoku>>)>> {
        match self {
            Self::PlusN { n, symmetry, excluded_cells } => {
                let root = PlusNSearchState::for_sudoku_and_symmetry(*n, sudoku, *symmetry, excluded_cells);
                Box::new(DepthFirstSearcherWithProgress::new(root))
            }
        }
    }

    pub fn plus_n(n: usize, symmetry: DihedralSubgroup, excluded_cells_str: &str) -> Self {
        let excluded_cells = excluded_cells_str.split(",").map(|s| s.trim()).map(|s| {
            let (_, rc) = s.split_once("r").unwrap();
            let (r, c) = rc.split("c").map(|d| d.parse::<usize>().unwrap()).collect_tuple().unwrap();
            (r - 1, c - 1)
        }).collect();
        Self::PlusN { n, symmetry, excluded_cells }
    }
}

struct PlusNSearchState {
    sudoku: Rc<RefCell<RegionMaskedSudoku>>,
    orbits: [Bitmask<u128>; 81],
    allowed_cells: Bitmask<u128>,
    placed_cells: Bitmask<u128>,
    required_cells: Bitmask<u128>,
    pending_placement: Option<usize>,
    placements_remaining: usize,
}

impl PlusNSearchState {
    pub fn for_sudoku_and_symmetry(n: usize, sudoku: Rc<RefCell<RegionMaskedSudoku>>, symmetry: DihedralSubgroup, excluded_cells: &[(usize, usize)]) -> Self {
        let orbits: [_; 81] = symmetry.orbits().iter().map(|cells| Bitmask::<u128>::from_iter(cells.iter().copied())).collect_array().unwrap();
        let clue_cells = Bitmask::<u128>::from_iter((0 .. 81).filter(|&idx| !sudoku.borrow().is_empty(idx)));
        let required_cells = clue_cells.as_bit_iter().map(|cell| orbits[cell]).fold(Bitmask::<u128>::empty(), |acc, x| acc | x) & !clue_cells;
        
        let mut allowed_cells = Bitmask::<u128>::from_iter((0 .. 81).filter(|&idx| orbits[idx].as_bit_iter().peek() == Some(idx)));
        excluded_cells.iter().map(|&(y, x)| 9 * y + x).chain(clue_cells.as_bit_iter()).for_each(|idx| allowed_cells &= !orbits[idx]);

        Self {
            sudoku, 
            orbits, allowed_cells, 
            required_cells, pending_placement: None, placed_cells: Bitmask::<u128>::empty(), placements_remaining: n,
        }
    }
}

enum PlusNSearchStep {
    AddCell(usize),
    PlaceDigit(usize, u8),
}

impl DepthFirstTraversable for PlusNSearchState {
    type Step = PlusNSearchStep;
    type Output = Rc<RefCell<RegionMaskedSudoku>>;

    fn next_steps(&mut self) -> Box<dyn ExactSizeIterator<Item = Self::Step>> {
        if let Some(idx) = self.pending_placement {
            Box::new(self.sudoku.borrow().candidates(idx).as_bit_iter().map(move |d| PlusNSearchStep::PlaceDigit(idx, d as u8)))
        } else if self.required_cells.is_not_empty() {
            Box::new(once(PlusNSearchStep::AddCell(self.required_cells.as_bit_iter().peek().unwrap())))
        } else if let start @ 0 .. 81 = self.placed_cells.max().map(|it| it + 1).unwrap_or(0) {
            let candidate_cells = Bitmask::<u128>::from(((1 << (81 - start)) - 1) << start) & self.allowed_cells;
            Box::new(candidate_cells.as_bit_iter().map(|cell| PlusNSearchStep::AddCell(cell)))
        } else {
            Box::new(empty())
        }
    }

    fn apply_step(&mut self, step: &Self::Step) {
        match step {
            &PlusNSearchStep::AddCell(cell) => {
                if self.required_cells.is_empty() { 
                    self.required_cells = self.orbits[cell];
                    self.placed_cells.set(cell);
                }
                self.required_cells.unset(cell);
                self.pending_placement = Some(cell);
                self.placements_remaining -= 1;
            }
            &PlusNSearchStep::PlaceDigit(cell, d) => {
                self.sudoku.borrow_mut().place(cell, d);
                self.pending_placement = None;
            }
        }
    }
    
    fn revert_step(&mut self, step: &Self::Step) {
        match step {
            &PlusNSearchStep::PlaceDigit(cell, d) => {
                self.sudoku.borrow_mut().unplace(cell, d);
                self.pending_placement = Some(cell);
            }
            &PlusNSearchStep::AddCell(cell) => {
                self.placed_cells.unset(cell);
                self.required_cells.set(cell);
                if self.required_cells == self.orbits[cell] { self.required_cells = Bitmask::<u128>::empty() }
                self.pending_placement = None;
                self.placements_remaining += 1;
            }
        }
    }

    fn should_prune(&mut self) -> bool {
        self.required_cells.count_ones() as usize > self.placements_remaining || self.pending_placement.is_none() && self.placements_remaining == 0
    }

    fn output(&mut self) -> Option<Self::Output> {
        (self.required_cells.is_empty() && self.pending_placement.is_none()).then(|| self.sudoku.clone())
    }
}
