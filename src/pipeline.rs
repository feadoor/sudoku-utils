use std::cell::Cell;
use std::rc::Rc;

use indicatif::ProgressBar;

use crate::bitmask::Bitmask;
use crate::expansion::Expansion;
use crate::filter::Filter;
use crate::generate::GenerationBase;
use crate::sudoku::Sudoku;

#[derive(Clone)]
pub struct RegionMaskedSudoku {
    sudoku: Sudoku,
    rows: [Bitmask<u16>; 9],
    cols: [Bitmask<u16>; 9],
    boxes: [Bitmask<u16>; 9],
}

pub enum PipelineStep {
    Filter(Filter),
    Expansion(Expansion),
}

pub struct Pipeline {
    pub base: GenerationBase,
    pub steps: Vec<PipelineStep>,
}

impl Pipeline {
    pub fn into_iter(self, bar: &ProgressBar) -> impl Iterator<Item = Sudoku> + '_ {
        const PROGRESS_UPDATE_INTERVAL: u64 = 4096;
        let bar_length = bar.length().unwrap() as f64;

        let mut base_counter = 0;
        let mut base_iterator: Box<dyn Iterator<Item = (f64, f64, RegionMaskedSudoku)>> = Box::new(self.base.iter().map(move |(progress, scale, sudoku)| {
            base_counter += 1;
            if base_counter % PROGRESS_UPDATE_INTERVAL == 0 {
                bar.set_position((bar_length * progress).trunc() as u64);
            }
            (progress, scale, sudoku)
        }));
        for step in self.steps {
            match step {
                PipelineStep::Filter(mut filter) => {
                    base_iterator = Box::new(base_iterator.filter(move |(_, _, sudoku)| filter.matches(sudoku)));
                }
                PipelineStep::Expansion(expansion) => {
                    let expansion_counter = Rc::new(Cell::new(0));
                    base_iterator = Box::new(base_iterator.flat_map(move |(progress, scale, sudoku)| {
                        let expansion_counter = expansion_counter.clone();
                        expansion.expand(sudoku).map(move |(subprogress, subscale, sudoku)| {
                            let true_progress = progress - scale + subprogress * scale;
                            expansion_counter.update(|count| count + 1);
                            if expansion_counter.get() % PROGRESS_UPDATE_INTERVAL == 0 {
                                bar.set_position((bar_length * true_progress).trunc() as u64);
                            }
                            (true_progress, scale * subscale, sudoku)
                        })
                    }))
                }
            }
        }
        base_iterator.map(|(_, _, sudoku)| sudoku.sudoku().clone())
    }
}

impl RegionMaskedSudoku {

    #[inline(always)]
    pub fn empty() -> Self {
        Self {
            sudoku: Sudoku::empty(),
            rows: [ALL_DIGITS; 9],
            cols: [ALL_DIGITS; 9],
            boxes: [ALL_DIGITS; 9],
        }
    }

    #[inline(always)]
    pub fn sudoku(&self) -> &Sudoku {
        &self.sudoku
    }

    #[inline(always)]
    pub fn empty_cells(&self) -> usize {
        self.rows.iter().map(|row| row.count_ones() as usize).sum()
    }

    #[inline(always)]
    pub fn place(&mut self, idx: usize, digit: u8) {
        self.sudoku[idx] = digit;
        self.rows[ROW_INDICES[idx]].unset(digit);
        self.cols[COL_INDICES[idx]].unset(digit);
        self.boxes[BOX_INDICES[idx]].unset(digit);
    }

    #[inline(always)]
    pub fn unplace(&mut self, idx: usize, digit: u8) {
        self.sudoku[idx] = 0;
        self.rows[ROW_INDICES[idx]].set(digit);
        self.cols[COL_INDICES[idx]].set(digit);
        self.boxes[BOX_INDICES[idx]].set(digit);
    }

    #[inline(always)]
    pub fn is_empty(&self, idx: usize) -> bool {
        self.sudoku[idx] == 0
    }

    #[inline(always)]
    pub fn candidates(&self, idx: usize) -> Bitmask<u16> {
        if self.sudoku[idx] != 0 { Bitmask::<u16>::singleton(self.sudoku[idx]) }
        else { self.rows[ROW_INDICES[idx]] & self.cols[COL_INDICES[idx]] & self.boxes[BOX_INDICES[idx]] }
    }
}

pub const ALL_DIGITS: Bitmask<u16> = Bitmask::<u16>::from(0b_111_111_111_0);

pub const ROW_INDICES: [usize; 81] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0,
    1, 1, 1, 1, 1, 1, 1, 1, 1,
    2, 2, 2, 2, 2, 2, 2, 2, 2,
    3, 3, 3, 3, 3, 3, 3, 3, 3,
    4, 4, 4, 4, 4, 4, 4, 4, 4,
    5, 5, 5, 5, 5, 5, 5, 5, 5,
    6, 6, 6, 6, 6, 6, 6, 6, 6,
    7, 7, 7, 7, 7, 7, 7, 7, 7,
    8, 8, 8, 8, 8, 8, 8, 8, 8,
];

pub const COL_INDICES: [usize; 81] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8,
    0, 1, 2, 3, 4, 5, 6, 7, 8,
    0, 1, 2, 3, 4, 5, 6, 7, 8,
    0, 1, 2, 3, 4, 5, 6, 7, 8,
    0, 1, 2, 3, 4, 5, 6, 7, 8,
    0, 1, 2, 3, 4, 5, 6, 7, 8,
    0, 1, 2, 3, 4, 5, 6, 7, 8,
    0, 1, 2, 3, 4, 5, 6, 7, 8,
    0, 1, 2, 3, 4, 5, 6, 7, 8,
];

pub const BOX_INDICES: [usize; 81] = [
    0, 0, 0, 1, 1, 1, 2, 2, 2,
    0, 0, 0, 1, 1, 1, 2, 2, 2,
    0, 0, 0, 1, 1, 1, 2, 2, 2,
    3, 3, 3, 4, 4, 4, 5, 5, 5,
    3, 3, 3, 4, 4, 4, 5, 5, 5,
    3, 3, 3, 4, 4, 4, 5, 5, 5,
    6, 6, 6, 7, 7, 7, 8, 8, 8,
    6, 6, 6, 7, 7, 7, 8, 8, 8,
    6, 6, 6, 7, 7, 7, 8, 8, 8,
];
