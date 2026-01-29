use indicatif::{ProgressBar, ProgressStyle};
use itertools::Itertools;

use crate::expansion::Expansion;
use crate::filter::Filter;
use crate::generate::GenerationBase;
use crate::pipeline::{Pipeline, PipelineStep};
use crate::symmetry::DihedralSubgroup;
use crate::template::Template;

mod bitmask;
mod dfs_with_progress;
mod expansion;
mod fast_solver;
mod filter;
mod generate;
mod logic;
mod minlex;
mod pipeline;
mod sudoku;
mod symmetry;
mod template;

fn main() {
    let bar = ProgressBar::new(100_000);
    bar.set_style(ProgressStyle::with_template("[{elapsed_precise}] {bar:50} {percent_precise}%")
        .unwrap()
        .progress_chars("#~."));
    let template = Template::from_str(&"\
        .56.7.8.9\
        X........\
        X.1.....X\
        ...1.....\
        X.......X\
        ...2.3...\
        X.2...3.X\
        X.......X\
        .XX.X.XX.\
    ".replace("X", "[56789]"));
    let pipeline = Pipeline {
        base: GenerationBase::Template(template),
        steps: vec![
            PipelineStep::Expansion(Expansion::plus_n(4, DihedralSubgroup::DiagonalUrToDlSymm, "r1c1,r1c4,r1c6,r1c8,r4c1,r6c1,r9c1,r9c4,r9c6,r9c9,r2c9,r4c9,r6c9")),
            PipelineStep::Filter(Filter::HasUniqueSolution),
            PipelineStep::Filter(Filter::at_most_n_basic_placements(1)),
            PipelineStep::Filter(Filter::solves_with_basics_after_elims("4r4c6,4r1c1,4r9c1,4r9c9")),
        ],
    };
    pipeline.into_iter(&bar).for_each(|sudoku| {
        println!("{}", sudoku.digits().join(""));
    });
    bar.finish();
}
