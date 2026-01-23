use std::cell::RefCell;
use std::iter::empty;
use std::rc::Rc;

use crate::bitmask::{BitIter, Bitmask};
use crate::dfs_with_progress::{DepthFirstSearcherWithProgress, DepthFirstTraversable};
use crate::pipeline::RegionMaskedSudoku;
use crate::template::{Template, TemplateDigit};

pub enum GenerationBase {
    Template(Template),
}

impl GenerationBase {
    pub fn iter(&self) -> Box<dyn Iterator<Item = (f64, f64, Rc<RefCell<RegionMaskedSudoku>>)>> {
        match self {
            Self::Template(template) => Box::new(DepthFirstSearcherWithProgress::new(TemplateGeneratorState::for_template(template))),
        }
    }
}

/// A structure capable of iterating over all partial Sudoku grids fitting
/// a particular template.
struct TemplateGeneratorState {
    sudoku: Rc<RefCell<RegionMaskedSudoku>>,
    wildcards: Vec<(usize, Bitmask<u16>)>,
    placement_count: usize,
}

impl TemplateGeneratorState {
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
            placement_count: 0, 
            sudoku: Rc::new(RefCell::new(RegionMaskedSudoku::empty())),
        }
    }

    // Decide which digit placement to branch on - use the one with the smallest branching factor
    fn best_branch_digit(&self) -> Option<(usize, BitIter<u16>)> {
        self.wildcards.iter()
            .filter(|&&(idx, _)| self.sudoku.borrow().is_empty(idx))
            .map(|&(idx, mask)| (idx, (mask & self.sudoku.borrow().candidates(idx)).as_bit_iter()))
            .min_by_key(|(_, bits)| bits.len())
    }
}

impl DepthFirstTraversable for TemplateGeneratorState {
    type Step = (usize, u8);
    type Output = Rc<RefCell<RegionMaskedSudoku>>;

    fn next_steps(&mut self) -> Box<dyn ExactSizeIterator<Item = Self::Step>> {
        if let Some((idx, digits)) = self.best_branch_digit() {
            Box::new(digits.map(move |d| (idx, d as u8)))
        } else {
            Box::new(empty())
        }
    }

    fn apply_step(&mut self, &(idx, d): &Self::Step) {
        self.sudoku.borrow_mut().place(idx, d);
        self.placement_count += 1;
    }

    fn revert_step(&mut self, &(idx, d): &Self::Step) {
        self.sudoku.borrow_mut().unplace(idx, d);
        self.placement_count -=1 ;
    }

    fn should_prune(&mut self) -> bool {
        false
    }

    fn output(&mut self) -> Option<Self::Output> {
        (self.placement_count == self.wildcards.len()).then(|| self.sudoku.clone())
    }
}
