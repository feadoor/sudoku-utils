use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};

use indicatif::ProgressBar;
use rayon::prelude::*;

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
    pub fn run_parallel<F>(self, bar: &ProgressBar, sink: F)
    where
        F: Fn(Sudoku) + Sync + Send,
    {
        let bar_length = bar.length().unwrap() as f64;
        let Pipeline { base, steps } = self;
        let steps = steps.as_slice();

        const PROGRESS_SCALE: f64 = (1u64 << 52) as f64;
        let progress = AtomicU64::new(0);
        let report = |fraction: f64| {
            if fraction <= 0.0 { return; }
            let units = (fraction * PROGRESS_SCALE) as u64;
            let total = progress.fetch_add(units, Ordering::Relaxed) + units;
            bar.set_position(((total as f64 / PROGRESS_SCALE) * bar_length) as u64);
        };

        let found = AtomicU64::new(0);
        bar.set_message("0 found");

        let mut base_seen = 0.0f64;
        base.iter()
            .map(move |(base_progress, base_scale, seed)| {
                report(base_progress - base_scale - base_seen);
                base_seen = base_progress;
                (base_scale, seed)
            })
            .par_bridge()
            .for_each(|(base_scale, seed)| {
                let reported = Cell::new(0.0f64);
                for sudoku in apply_steps(seed, base_scale, steps, &report, &reported) {
                    sink(sudoku.sudoku().clone());
                    bar.set_message(format!("{} found", found.fetch_add(1, Ordering::Relaxed) + 1));
                }
                report(base_scale * (1.0 - reported.get()));
            });

        bar.set_message(format!("{} found", found.load(Ordering::Relaxed)));
    }
}

fn apply_steps<'a>(
    seed: RegionMaskedSudoku,
    seed_scale: f64,
    steps: &'a [PipelineStep],
    report: &'a (dyn Fn(f64) + Sync),
    reported: &'a Cell<f64>,
) -> Box<dyn Iterator<Item = RegionMaskedSudoku> + 'a> {
    let mut iter: Box<dyn Iterator<Item = (f64, RegionMaskedSudoku)> + 'a> = Box::new(std::iter::once((seed_scale, seed)));
    for step in steps {
        match step {
            PipelineStep::Filter(filter) => {
                iter = Box::new(iter.filter(move |(_, sudoku)| filter.matches(sudoku)));
            }
            PipelineStep::Expansion(expansion) => {
                iter = Box::new(iter.flat_map(move |(scale, sudoku)| {
                    expansion.expand(sudoku).map(move |(subprogress, subscale, sudoku)| {
                        report(scale * (subprogress - reported.get()));
                        reported.set(subprogress);
                        (scale * subscale, sudoku)
                    })
                }));
            }
        }
    }
    Box::new(iter.map(|(_, sudoku)| sudoku))
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
