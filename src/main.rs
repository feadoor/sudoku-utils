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
        ..23.....\
        .1..4....\
        ...Y.....\
        .76......\
        8......B.\
        9.....Y.B\
        .....X..A\
        .....X.A.\
        ...XX....\
    ".replace("A", "[12]").replace("B", "[34]").replace("X", "[56789]").replace("Y", "[123456789]"));
    let pipeline = Pipeline {
        base: GenerationBase::Template(template),
        steps: vec![
            PipelineStep::Expansion(Expansion::plus_n(4, DihedralSubgroup::DiagonalUrToDlSymm, "r1c1,r2c1,r3c1,r4c1,r7c1,r8c1,r9c1,r1c6,r2c6,r3c6,r4c6,r5c6,r6c6,r9c6,r4c4,r4c5,r4c7,r4c8,r4c9,r9c2,r9c3,r9c7,r9c8,r9c9")),
            PipelineStep::Filter(Filter::HasUniqueSolution),
            PipelineStep::Filter(Filter::at_most_n_basic_placements(0)),
            PipelineStep::Filter(Filter::solves_with_basics_after_elims("56789r4c1,56789r4c6,56789r9c1,56789r9c6,1r7c3,1r8c3,2r7c2,2r8c2,3r5c5,3r6c5,4r5c4,4r6c4")),
            PipelineStep::Filter(Filter::non_equivalent()),
        ],
    };
    pipeline.into_iter(&bar).for_each(|sudoku| {
        println!("{}", sudoku.digits().join(""));
    });
    bar.finish();
}
