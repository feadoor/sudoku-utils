pub trait DepthFirstTraversable {
    type Step;
    type Output;

    fn next_steps(&mut self) -> Box<dyn ExactSizeIterator<Item = Self::Step>>;
    fn apply_step(&mut self, step: &Self::Step);
    fn revert_step(&mut self, step: &Self::Step);
    fn should_prune(&mut self) -> bool;
    fn output(&mut self) -> Option<Self::Output>;
}

pub struct DepthFirstSearcherWithProgress<T, S> {
    state: T,
    levels: Vec<(Box<dyn ExactSizeIterator<Item = S>>, Option<S>, f64)>,
    progress: f64,
}

impl<T: DepthFirstTraversable> DepthFirstSearcherWithProgress<T, T::Step> {

    /// A new search with the given state as the root
    pub fn new(start_state: T) -> Self {
        Self {
            state: start_state,
            levels: Vec::new(),
            progress: 0.0,
        }
    }

    /// Take a single step onwards in the depth-first search
    fn step(&mut self) -> bool {

        // Pop all of the levels whose steps have been fully explored.
        // If everything is popped then we've finished the search.
        while let Some((steps, _, _)) = self.levels.last() {
            if steps.len() == 0 {
                let (_, step, _) = self.levels.pop().unwrap();
                if let Some(step) = step { self.state.revert_step(&step); }
                if self.levels.len() == 0 { return false; }
            } else {
                break;
            }
        }

        // Advance the deepest level by one step
        if let Some((steps, prev_step, _)) = self.levels.last_mut() {
            if let Some(prev_step) = prev_step { self.state.revert_step(prev_step); }
            let next_step = steps.next().unwrap();
            self.state.apply_step(&next_step);
            *prev_step = Some(next_step);
        }

        // Check if we should prune at this state and don't go deeper if so
        if self.state.should_prune() { 
            self.progress += self.progress_increment();
            return true;
        }

        // Deepen the search by one level if possible
        let next_steps = self.state.next_steps();
        if next_steps.len() > 0 {
            let next_increment = self.progress_increment() / next_steps.len() as f64;
            self.levels.push((next_steps, None, next_increment));
        } else {
            self.progress += self.progress_increment();
        }

        true
    }

    /// Get the progress increment associated with a step at the current level
    fn progress_increment(&self) -> f64 {
        self.levels.last().map(|(_, _, inc)| *inc).unwrap_or(1.0)
    }
}

impl<T: DepthFirstTraversable> Iterator for DepthFirstSearcherWithProgress<T, T::Step> {
    type Item = (f64, f64, T::Output);

    fn next(&mut self) -> Option<Self::Item> {
        while self.step() {
            if let Some(output) = self.state.output() {
                return Some((self.progress, self.progress_increment(), output));
            }
        }
        None
    }
}
