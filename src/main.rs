use indicatif::ProgressBar;
use itertools::Itertools;

use crate::filter::Filter;
use crate::pipeline::{GenerationBase, Pipeline, PipelineStep};
use crate::template::Template;

mod bit_iter;
mod fast_solver;
mod filter;
mod generator;
mod logic;
mod pipeline;
mod sudoku;
mod symmetry;
mod template;

fn main() {
    let bar = ProgressBar::new(100_000);
    let template = Template::from_str(&"\
        ..23..Y..\
        .1..4..Y.\
        ...Y....Y\
        .76......\
        8...Y..B.\
        9.....Y.B\
        .....X..A\
        .....X.A.\
        ...XX....\
    ".replace("A", "[12]").replace("B", "[34]").replace("X", "[56789]").replace("Y", "[123456789]"));
    let pipeline = Pipeline {
        base: GenerationBase::Template(template),
        steps: vec![
            PipelineStep::Filter(Filter::at_most_n_basic_placements(0)),
            PipelineStep::Filter(Filter::solves_with_basics_after_elims("56789r4c1,56789r4c6,56789r9c1,56789r9c6,4r6c4,1r7c3")),
        ],
    };
    pipeline.into_iter(&bar).for_each(|sudoku| {
        println!("{}", sudoku.digits().join(""));
    });
    bar.finish();
}
