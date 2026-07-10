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
            PipelineStep::Filter(Filter::at_most_n_basic_placements(3)),
            PipelineStep::Filter(Filter::solves_with_basics_after_elims("3r8c6")),
            PipelineStep::Filter(Filter::non_equivalent()),
        ],
    };
    pipeline.run_parallel(&bar, |sudoku| {
        println!("{}", sudoku.digits().join(""));
    });
    bar.finish();
}
