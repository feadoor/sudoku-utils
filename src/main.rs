use indicatif::{ProgressBar, ProgressStyle};
use itertools::Itertools;
use rayon::prelude::*;
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::generation::expansion::Expansion;
use crate::generation::filter::Filter;
use crate::generation::generate::GenerationBase;
use crate::generation::pipeline::{Pipeline, PipelineStep};
use crate::generation::template::Template;
use crate::sampling::puzzle_sampler::PuzzleSampler;
use crate::symmetry::symmetry::DihedralSubgroup;

mod generation;
mod sampling;
mod solving;
mod symmetry;
mod utils;

fn example_generation() {
    let bar = ProgressBar::new(100_000);
    bar.set_style(ProgressStyle::with_template("[{elapsed_precise}] {bar:50} {percent_precise}% [{msg}]")
        .unwrap()
        .progress_chars("#~."));
    let template = Template::from_str(&"\
        .........\
        .A...4.A.\
        3..5.6..Y\
        ....1....\
        ....2....\
        ....3....\
        Y..7.8..3\
        .Y.9...Y.\
        .........\
    ".replace("A", "[12]").replace("Y", "[456789]"));
    let pipeline = Pipeline {
        base: GenerationBase::Template(template),
        steps: vec![
            PipelineStep::Expansion(Expansion::plus_n(6, DihedralSubgroup::CentralSymm, "r1c1,r1c2,r1c3,r1c4,r1c5,r1c6,r1c7,r1c8,r1c9,r2c1,r2c2,r2c3,r2c4,r2c5,r2c6,r2c7,r2c8,r2c9,r3c1,r3c2,r3c3,r3c4,r3c5,r3c6,r3c7,r3c8,r3c9,r7c1,r7c2,r7c3,r7c4,r7c5,r7c6,r7c7,r7c8,r7c9,r8c1,r8c2,r8c3,r8c4,r8c5,r8c6,r8c7,r8c8,r8c9,r9c1,r9c2,r9c3,r9c4,r9c5,r9c6,r9c7,r9c8,r9c9")),
            PipelineStep::Filter(Filter::HasUniqueSolution),
            PipelineStep::Filter(Filter::at_most_n_basic_placements(1)),
            PipelineStep::Filter(Filter::solves_with_basics_after_elims("3r8c6")),
            PipelineStep::Filter(Filter::non_equivalent()),
        ],
    };
    pipeline.run_parallel(&bar, |sudoku| {
        println!("{}", sudoku.digits().join(""));
    });
    bar.finish();
}

fn example_sampling() {
    let min_clues = 24;
    let max_clues = 28;
    let n_puzzles = 1_000;
    let sampler = PuzzleSampler::new(min_clues, max_clues);

    let out = std::sync::Mutex::new(std::io::stdout());
    let count = AtomicU64::new(0);
    let histogram = (0 ..= 81).map(|_| AtomicU64::new(0)).collect_vec();

    let bar = ProgressBar::new(n_puzzles);
    bar.set_style(ProgressStyle::with_template("[{elapsed_precise}] {bar:50} {percent_precise}% [{human_pos} / {human_len}]")
        .unwrap()
        .progress_chars("#~."));
    bar.set_position(0);

    (0 .. n_puzzles).into_par_iter().for_each_init(
        rand::rng,
        |rng, _idx| {
            let sudoku = sampler.sample(rng);
            let clue_count = sudoku.0.iter().filter(|&&d| d != 0).count();
            histogram[clue_count].fetch_add(1, Ordering::Relaxed);

            let mut line = sudoku.digits().join(""); line.push('\n');
            let mut writer = out.lock().unwrap();
            writer.write_all(line.as_bytes()).unwrap();

            bar.set_position(count.fetch_add(1, Ordering::Relaxed) + 1);
        }
    );

    bar.finish();
    let counts = histogram.iter().map(|a| a.load(Ordering::Relaxed)).collect_vec();
    report_histogram(&counts, &mut std::io::stderr());
}

fn report_histogram(counts: &[u64], w: &mut impl Write) {
    let total: u64 = counts.iter().sum();
    if total == 0 {
        let _ = writeln!(w, "no puzzles generated");
        return;
    }
    let occupied: Vec<usize> = (0..counts.len()).filter(|&i| counts[i] > 0).collect();
    let (lo, hi) = (occupied[0], *occupied.last().unwrap());
    let peak = *counts.iter().max().unwrap();
    let weighted: u128 = counts.iter().enumerate().map(|(i, &c)| i as u128 * c as u128).sum();
    let mean = weighted as f64 / total as f64;

    let _ = writeln!(w, "\nclue-count distribution ({total} puzzles):");
    for n in lo..=hi {
        let c = counts[n];
        // bar scaled to a 40-column max, so the shape is visible at any total
        let bar_len = if peak > 0 { (c as usize * 40 + peak as usize / 2) / peak as usize } else { 0 };
        let bar = "#".repeat(bar_len);
        let pct = 100.0 * c as f64 / total as f64;
        let _ = writeln!(w, "  {n:>2}: {c:>7}  {pct:5.1}%  {bar}");
    }
    let _ = writeln!(w, "  min {lo}, max {hi}, mean {mean:.2}");
}


fn main() {
    example_sampling();
}
