use indicatif::ProgressBar;

use crate::dfs_with_progress::DepthFirstSearcherWithProgress;
use crate::filter::Filter;
use crate::generator::GeneratorState;
use crate::sudoku::Sudoku;
use crate::template::Template;

pub enum GenerationBase {
    Template(Template),
}

impl GenerationBase {
    fn iter(&self) -> Box<dyn Iterator<Item = (f64, f64, Sudoku)>> {
        match self {
            Self::Template(template) => Box::new(DepthFirstSearcherWithProgress::new(GeneratorState::for_template(template))),
        }
    }
}

pub enum PipelineStep {
    Filter(Filter),
}

pub struct Pipeline {
    pub base: GenerationBase,
    pub steps: Vec<PipelineStep>,
}

impl Pipeline {
    pub fn into_iter(self, bar: &ProgressBar) -> impl Iterator<Item = Sudoku> + '_ {
        let mut base_iterator: Box<dyn Iterator<Item = (f64, f64, Sudoku)>> = Box::new(self.base.iter().map(|(progress, scale, sudoku)| {
            bar.set_position(((bar.length().unwrap() as f64) * progress).trunc() as u64);
            (progress, scale, sudoku)
        }));
        for step in self.steps {
            match step {
                PipelineStep::Filter(filter) => {
                    base_iterator = Box::new(base_iterator.filter(move |(_, _, sudoku)| filter.matches(sudoku)));
                }
            }
        }
        base_iterator.map(|(_, _, sudoku)| sudoku)
    }
}
