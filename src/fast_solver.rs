use crate::{bit_iter::MaskIter, sudoku::Sudoku};

const N_DIGITS: usize = 9;
const N_BANDS: usize = 3;
const N_SUBBANDS: usize = N_DIGITS * N_BANDS;
const N_CELLS: usize = 81;

const NONE: u32 = 0;
const ALL: u32 = 0o_777_777_777;
const LOW9: u32 = 0o_777;

pub struct Unsolvable {}

/// Different ways of storing solutions - we can either:
/// - just count (faster)
/// - keep all the solutions (slower)
enum Solutions<'a> {
    Count(usize),
    Keep(&'a mut Vec<Sudoku>),
}

impl<'a> Solutions<'a> {

    fn len(&self) -> usize {
        match self {
            Solutions::Count(value) => *value,
            Solutions::Keep(sols) => sols.len(),
        }
    }
}

/// A helper type for unchecked indexing into arrays, which speeds up 
/// the solver by up to 10% on the hardest puzzles.
#[derive(Clone)]
struct UncheckedIndexArray<T, const N: usize>([T; N]);

impl<T, const N: usize> std::ops::Index<usize> for UncheckedIndexArray<T, N> {
    type Output = T;
    fn index(&self, index: usize) -> &Self::Output {
        unsafe { self.0.get_unchecked(index) }
    }
}

impl<T, const N: usize> std::ops::IndexMut<usize> for UncheckedIndexArray<T, N> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        unsafe { self.0.get_unchecked_mut(index) }
    }
}

/// Implementation is band-oriented
/// Each entry in one of these arrays is a 27-bit bitmask of possible positions within a horizontal band
///
/// possible_cells and prev_possible_cells contain one bitmask per digit per band
/// unsolved_cells and bivalue_cells contain one bitmask per band
#[derive(Clone)]
pub struct FastBruteForceSolver {
    possible_cells: UncheckedIndexArray<u32, N_SUBBANDS>,
    prev_possible_cells: UncheckedIndexArray<u32, N_SUBBANDS>,
    unsolved_cells: UncheckedIndexArray<u32, N_BANDS>,
    bivalue_cells: UncheckedIndexArray<u32, N_BANDS>,
}

impl FastBruteForceSolver {

    pub fn has_solution(sudoku: &Sudoku) -> bool {
        Self::from_sudoku(sudoku).map(|s| s.count_solutions_up_to(1) == 1).unwrap_or(false)
    }

    pub fn has_unique_solution(sudoku: &Sudoku) -> bool {
        Self::from_sudoku(sudoku).map(|s| s.count_solutions_up_to(2) == 1).unwrap_or(false)
    }

    pub fn count_solutions(sudoku: &Sudoku) -> usize {
        Self::from_sudoku(sudoku).map(|s| s.count_solutions_up_to(usize::MAX)).unwrap_or(0)
    }

    fn from_sudoku(sudoku: &Sudoku) -> Result<Self, Unsolvable> {
        let mut solver = Self {
            possible_cells: UncheckedIndexArray([ALL; N_SUBBANDS]),
            prev_possible_cells: UncheckedIndexArray([NONE; N_SUBBANDS]),
            unsolved_cells: UncheckedIndexArray([ALL; N_BANDS]),
            bivalue_cells: UncheckedIndexArray([NONE; N_BANDS]),
        };
        
        for (cell, value) in sudoku.digits().enumerate() {
            if *value != 0 {
                solver.insert_value(cell, *value)?
            }
        }

        Ok(solver)
    }

    fn all_solutions_up_to(self, limit: usize) -> Vec<Sudoku> {
        let mut solutions = Vec::new();
        self.solutions_up_to(limit, &mut Solutions::Keep(&mut solutions));
        solutions
    }

    fn count_solutions_up_to(self, limit: usize) -> usize {
        let mut solutions = Solutions::Count(0);
        self.solutions_up_to(limit, &mut solutions);
        solutions.len()
    }

    fn solutions_up_to(mut self, limit: usize, solutions: &mut Solutions) {
        if self.find_naked_singles().is_ok() {
            if self.solve(limit, solutions).is_ok() {
                self.guess(limit, solutions);
            }
        }
    }

    fn is_solved(&self) -> bool {
        self.unsolved_cells.0 == [NONE; N_BANDS]
    }

    /// Repeatedly use singles and locked candidates until no more deductions
    /// are possible.
    fn solve(&mut self, limit: usize, solutions: &mut Solutions) -> Result<(), Unsolvable> {

        // Force a recursion stop if we're at the solution limit
        if solutions.len() >= limit {
            return Err(Unsolvable {});
        }

        loop {
            self.find_locked_candidates_and_update()?;
            if self.is_solved() { return Ok(()); }
            if self.find_naked_singles()? { continue; }
            return Ok(());
        }
    }

    // If the puzzle is not solved, choose an unsolved cell and branch on it
    fn guess(&mut self, limit: usize, solutions: &mut Solutions) {
        if self.is_solved() {
            self.store_solution(solutions);
        } else if self.guess_bivalue(limit, solutions).is_ok() {
            self.guess_some_cell(limit, solutions);
        }
    }

    // Look for a bivalue cell to guess on and branch on it. We save these
    // cells while checking for naked singles, so this is basically a lookup.
    fn guess_bivalue(&mut self, limit: usize, solutions: &mut Solutions) -> Result<(), Unsolvable> {
        for band in 0 .. N_BANDS {

            // Get the first bivalue cell, if it exists
            let cell_mask = match MaskIter::new(self.bivalue_cells[band]).peek() {
                Some(mask) => mask,
                None => continue,
            };
            
            // Loop through all 9 digits and check if that digit is possible here
            let mut first = true;
            for subband in (band..).step_by(3) {
                if self.possible_cells[subband] & cell_mask != NONE {
                    if first { first = false;
                        let mut branch = self.clone();
                        branch.insert_value_by_mask(subband, cell_mask);
                        if branch.solve(limit, solutions).is_ok() {
                            branch.guess(limit, solutions);
                        }
                    } else {
                        self.insert_value_by_mask(subband, cell_mask);
                        if self.solve(limit, solutions).is_ok() {
                            self.guess(limit, solutions);
                        }
                        return Err(Unsolvable {});
                    }
                }
            }
        }

        Ok(())
    }

    /// Find an unsolved cell and branch on it.
    /// In the vast majority of cases, there is a cell with only 2 candidates,
    /// which means that guess_bivalue() will be called instead of this function.
    /// In cases where there is no bivalue cell it is valuable to find a cell with
    /// few candidates, but an exhaustive search is too expensive.
    /// As a compromise, up to 3 cells are searched and the one with the fewest
    /// candidates is used as the branching point.
    fn guess_some_cell(&mut self, limit: usize, solutions: &mut Solutions) {
        let best_guess = (0 .. N_BANDS).flat_map(|band| {
            // Get first unsolved cell, if it exists
            let one_unsolved_cell = MaskIter::new(self.unsolved_cells[band]).peek()?;
            let n_candidates = (band..).step_by(3).take(N_DIGITS)
                .filter(|&subband| self.possible_cells[subband] & one_unsolved_cell != NONE)
                .count();
            Some((n_candidates, band, one_unsolved_cell))
        }).min();

        let (count, band, unsolved_cell) = match best_guess {
            Some(min) => min,
            None => return,
        };

        // Check every digit
        let mut checked = 0;
        for subband in (band..).step_by(3) {
            if self.possible_cells[subband] & unsolved_cell != NONE {
                if checked < count - 1 { checked += 1;
                    let mut branch = self.clone();
                    branch.insert_value_by_mask(subband, unsolved_cell);
                    if branch.solve(limit, solutions).is_ok() {
                        branch.guess(limit, solutions);
                    }
                } else {
                    self.insert_value_by_mask(subband, unsolved_cell);
                    if self.solve(limit, solutions).is_ok() {
                        self.guess(limit, solutions);
                    }
                    return;
                }
            }
        }
    }

    /// Store the current solution
    fn store_solution(&self, solutions: &mut Solutions) {
        match solutions {
            Solutions::Count(count) => *count += 1,
            Solutions::Keep(sols) => sols.push(self.extract_solution()),
        }
    }

    /// Extract the solution as a Sudoku from the current solver state
    fn extract_solution(&self) -> Sudoku {
        let mut sudoku = [0; 81];
        for (subband, &mask) in self.possible_cells.0.iter().enumerate() {
            let digit = subband / 3;
            let base_cell_in_band = subband % 3 * 27;
            for cell_mask in MaskIter::new(mask) {
                let cell_in_band = cell_mask.trailing_zeros() as usize;
                sudoku[cell_in_band + base_cell_in_band] = digit as u8 + 1;
            }
        }
        Sudoku(sudoku)
    }

    /// Search for cells which only have one candidate and sets them.
    /// Also finds cells with 0 possibilities (puzzle is unsolvable), cells with
    /// 2 possibilities (good guess locations) and cells with 3 or more (bad guess locations)
    fn find_naked_singles(&mut self) -> Result<bool, Unsolvable> {
        
        let mut naked_single_applied = false;
        for band in 0 .. N_BANDS {
            
            // Masks of cells with >= 1, >= 2 and >= 3 candidates
            let (mut cells1, mut cells2, mut cells3) = (NONE, NONE, NONE);
            for subband in (band ..).step_by(3).take(N_DIGITS) {
                let band_mask = self.possible_cells[subband];
                cells3 |= cells2 & band_mask;
                cells2 |= cells1 & band_mask;
                cells1 |= band_mask;
            }

            if cells1 != ALL {
                return Err(Unsolvable {});
            }

            // Store bivalue cells
            self.bivalue_cells[band] = cells2 ^ cells3;

            // New singles, ignore previously solved ones
            let singles = (cells1 ^ cells2) & self.unsolved_cells[band];

            // Insert each of the new singles
            'insert: for cell_mask_single in MaskIter::new(singles) {

                // Mark that we've applied a naked single
                naked_single_applied = true;

                // Find the digit that can go in this single cell
                for digit in 0 .. N_DIGITS {
                    if self.possible_cells[digit * 3 + band] & cell_mask_single != NONE {
                        self.insert_value_by_mask(digit * 3 + band, cell_mask_single);
                        continue 'insert;
                    }
                }

                // If we get here, it's a forced empty cell
                return Err(Unsolvable {});
            }
        }

        Ok(naked_single_applied)
    }

    /// Search for minirows that must contain a particular digit because they are the
    /// only minirow in a row or block that still contains that candidate and remove
    /// those candidates from conflicting cells.
    ///
    /// Also updates the bitmasks to remove impossible candidates left behind by
    /// calling insert_value_by_mask.
    fn find_locked_candidates_and_update(&mut self) -> Result<(), Unsolvable> {

        loop {
            // Repeat until nothing can be found or updated any more
            // This is the hottest piece of code in the solver
            let mut found_nothing = true;

            // This loop runs faster unrolled
            if self.possible_cells[0] != self.prev_possible_cells[0] { found_nothing = false; self.find_locked_candidates_and_update_subband(0)?; }
            if self.possible_cells[1] != self.prev_possible_cells[1] { found_nothing = false; self.find_locked_candidates_and_update_subband(1)?; }
            if self.possible_cells[2] != self.prev_possible_cells[2] { found_nothing = false; self.find_locked_candidates_and_update_subband(2)?; }
            if self.possible_cells[3] != self.prev_possible_cells[3] { found_nothing = false; self.find_locked_candidates_and_update_subband(3)?; }
            if self.possible_cells[4] != self.prev_possible_cells[4] { found_nothing = false; self.find_locked_candidates_and_update_subband(4)?; }
            if self.possible_cells[5] != self.prev_possible_cells[5] { found_nothing = false; self.find_locked_candidates_and_update_subband(5)?; }
            if self.possible_cells[6] != self.prev_possible_cells[6] { found_nothing = false; self.find_locked_candidates_and_update_subband(6)?; }
            if self.possible_cells[7] != self.prev_possible_cells[7] { found_nothing = false; self.find_locked_candidates_and_update_subband(7)?; }
            if self.possible_cells[8] != self.prev_possible_cells[8] { found_nothing = false; self.find_locked_candidates_and_update_subband(8)?; }
            if self.possible_cells[9] != self.prev_possible_cells[9] { found_nothing = false; self.find_locked_candidates_and_update_subband(9)?; }
            if self.possible_cells[10] != self.prev_possible_cells[10] { found_nothing = false; self.find_locked_candidates_and_update_subband(10)?; }
            if self.possible_cells[11] != self.prev_possible_cells[11] { found_nothing = false; self.find_locked_candidates_and_update_subband(11)?; }
            if self.possible_cells[12] != self.prev_possible_cells[12] { found_nothing = false; self.find_locked_candidates_and_update_subband(12)?; }
            if self.possible_cells[13] != self.prev_possible_cells[13] { found_nothing = false; self.find_locked_candidates_and_update_subband(13)?; }
            if self.possible_cells[14] != self.prev_possible_cells[14] { found_nothing = false; self.find_locked_candidates_and_update_subband(14)?; }
            if self.possible_cells[15] != self.prev_possible_cells[15] { found_nothing = false; self.find_locked_candidates_and_update_subband(15)?; }
            if self.possible_cells[16] != self.prev_possible_cells[16] { found_nothing = false; self.find_locked_candidates_and_update_subband(16)?; }
            if self.possible_cells[17] != self.prev_possible_cells[17] { found_nothing = false; self.find_locked_candidates_and_update_subband(17)?; }
            if self.possible_cells[18] != self.prev_possible_cells[18] { found_nothing = false; self.find_locked_candidates_and_update_subband(18)?; }
            if self.possible_cells[19] != self.prev_possible_cells[19] { found_nothing = false; self.find_locked_candidates_and_update_subband(19)?; }
            if self.possible_cells[20] != self.prev_possible_cells[20] { found_nothing = false; self.find_locked_candidates_and_update_subband(20)?; }
            if self.possible_cells[21] != self.prev_possible_cells[21] { found_nothing = false; self.find_locked_candidates_and_update_subband(21)?; }
            if self.possible_cells[22] != self.prev_possible_cells[22] { found_nothing = false; self.find_locked_candidates_and_update_subband(22)?; }
            if self.possible_cells[23] != self.prev_possible_cells[23] { found_nothing = false; self.find_locked_candidates_and_update_subband(23)?; }
            if self.possible_cells[24] != self.prev_possible_cells[24] { found_nothing = false; self.find_locked_candidates_and_update_subband(24)?; }
            if self.possible_cells[25] != self.prev_possible_cells[25] { found_nothing = false; self.find_locked_candidates_and_update_subband(25)?; }
            if self.possible_cells[26] != self.prev_possible_cells[26] { found_nothing = false; self.find_locked_candidates_and_update_subband(26)?; }

            if found_nothing { return Ok(()); }
        }
    }

    /// Update locked candidates for a single subband
    #[inline(always)]
    fn find_locked_candidates_and_update_subband(&mut self, subband: usize) -> Result<(), Unsolvable> {
        let old_possible_cells = self.possible_cells[subband];

        // Find all locked candidates in the band, both pointing and claiming.
        // First, use a lookup to condense each row of 9 bits down to 3 bits, 1 for each minirow.
        // Save the results for the 3 rows in a band together in a 9-bit mask and use another
        // lookup to find impossible candidates.
        let shrink = shrink_mask(old_possible_cells & LOW9)
            | shrink_mask(old_possible_cells >> 9 & LOW9) << 3
            | shrink_mask(old_possible_cells >> 18) << 6;
        let possible_cells = old_possible_cells & nonconflicting_cells_same_band_by_locked_candidates(shrink);

        // Check for impossibility and then update the possible cells for this subband
        if possible_cells == NONE { return Err(Unsolvable {}); }
        self.prev_possible_cells[subband] = possible_cells;
        self.possible_cells[subband] = possible_cells;

        // Possible columns in subband, including already solved ones
        let possible_columns = (possible_cells | possible_cells >> 9 | possible_cells >> 18) & LOW9;

        // Check for locked candidates in the columns (pointing type)
        // This is also what's enforcing that a column cannot contain a digit
        // more than once, since that is ignored by insert_value_by_mask
        let nonconflicting_neighbours = nonconflicting_cells_neighbour_bands_by_locked_candidates(possible_columns);
        let (neighbour1, neighbour2) = neighbour_subbands(subband);
        self.possible_cells[neighbour1] &= nonconflicting_neighbours;
        self.possible_cells[neighbour2] &= nonconflicting_neighbours;

        // Minirows that are locked have no neighbouring minirows in the same row
        // or in the same box. If they are inside a box where only 1 column is
        // possible, then only 1 cell is possible and the value is placed in the
        // row.
        //
        // `solved_rows` is a 3-bit mask of the rows in the subband
        // Mapping from solved minirows to solved rows happens to need the
        // same mask as shrinking for locked candidates.
        let locked_candidates_intersection = locked_minirows(shrink) & column_single(possible_columns);
        let solved_rows = shrink_mask(locked_candidates_intersection);
        let solved_cells = row_mask(solved_rows) & possible_cells;

        // Delete candidates of other digits from all solved cells in current subband
        let band = subband % 3;
        let nonconflicting_cells = !solved_cells;
        self.unsolved_cells[band] &= nonconflicting_cells;
        for other_subband in (band..).step_by(3).take(N_DIGITS).filter(|&other| other != subband) {
            self.possible_cells[other_subband] &= nonconflicting_cells;
        }

        Ok(())
    }


    /// Insert a value given a subband index and a mask representing the cell it
    /// goes in. Clears candidates from other cells in the same row and box but
    /// does not clear from other cells in the same column, as this is not cheap
    /// in our board representation and will happen later when finding locked
    /// candidates.
    fn insert_value_by_mask(&mut self, subband: usize, mask: u32) {
        let cell = mask.trailing_zeros() as usize;
        self.possible_cells[subband] &= nonconflicting_cells_same_band(cell);
    }

    /// Insert starting values and clear candidates. Only used when initialising
    /// the solver from a given puzzle.
    fn insert_value(&mut self, cell: usize, value: u8) -> Result<(), Unsolvable> {
        let band = cell / 27;
        let subband = (value as usize - 1) * 3 + band;
        let cell_mask = 1 << (cell % 27);

        // Check if this digit is allowed in this position
        if self.possible_cells[subband] & cell_mask == NONE {
            return Err(Unsolvable {});
        }

        // Set the cell containing this digit to solved
        self.unsolved_cells[band] &= !cell_mask;

        // Remove the digit as a possibility from cell neighbours by row, column and box
        self.possible_cells[subband] &= nonconflicting_cells_same_band(cell);
        let nonconflicting_neighbours = nonconflicting_cells_neighbour_bands(cell);
        let (neighbour1, neighbour2) = neighbour_subbands(subband);
        self.possible_cells[neighbour1] &= nonconflicting_neighbours;
        self.possible_cells[neighbour2] &= nonconflicting_neighbours;

        // Remove other digits as a possibility from the same cell
        for digit_subband in (band ..).step_by(3).take(N_DIGITS) {
            self.possible_cells[digit_subband] &= !cell_mask;
        }
        self.possible_cells[subband] |= cell_mask;

        Ok(())
    }
}

#[inline]
fn nonconflicting_cells_same_band(cell: usize) -> u32 {
    static MASKS: UncheckedIndexArray<u32, N_CELLS> = UncheckedIndexArray([
        0o_770_770_001, 0o_770_770_002, 0o_770_770_004, 0o_707_707_010, 0o_707_707_020, 0o_707_707_040, 0o_077_077_100, 0o_077_077_200, 0o_077_077_400,
        0o_770_001_770, 0o_770_002_770, 0o_770_004_770, 0o_707_010_707, 0o_707_020_707, 0o_707_040_707, 0o_077_100_077, 0o_077_200_077, 0o_077_400_077,
        0o_001_770_770, 0o_002_770_770, 0o_004_770_770, 0o_010_707_707, 0o_020_707_707, 0o_040_707_707, 0o_100_077_077, 0o_200_077_077, 0o_400_077_077,
        0o_770_770_001, 0o_770_770_002, 0o_770_770_004, 0o_707_707_010, 0o_707_707_020, 0o_707_707_040, 0o_077_077_100, 0o_077_077_200, 0o_077_077_400,
        0o_770_001_770, 0o_770_002_770, 0o_770_004_770, 0o_707_010_707, 0o_707_020_707, 0o_707_040_707, 0o_077_100_077, 0o_077_200_077, 0o_077_400_077,
        0o_001_770_770, 0o_002_770_770, 0o_004_770_770, 0o_010_707_707, 0o_020_707_707, 0o_040_707_707, 0o_100_077_077, 0o_200_077_077, 0o_400_077_077,
        0o_770_770_001, 0o_770_770_002, 0o_770_770_004, 0o_707_707_010, 0o_707_707_020, 0o_707_707_040, 0o_077_077_100, 0o_077_077_200, 0o_077_077_400,
        0o_770_001_770, 0o_770_002_770, 0o_770_004_770, 0o_707_010_707, 0o_707_020_707, 0o_707_040_707, 0o_077_100_077, 0o_077_200_077, 0o_077_400_077,
        0o_001_770_770, 0o_002_770_770, 0o_004_770_770, 0o_010_707_707, 0o_020_707_707, 0o_040_707_707, 0o_100_077_077, 0o_200_077_077, 0o_400_077_077,
    ]);
    MASKS[cell]
}

#[inline]
fn nonconflicting_cells_neighbour_bands(cell: usize) -> u32 {
    static MASKS: UncheckedIndexArray<u32, N_CELLS> = UncheckedIndexArray([
        0o_776_776_776, 0o_775_775_775, 0o_773_773_773, 0o_767_767_767, 0o_757_757_757, 0o_737_737_737, 0o_677_677_677, 0o_577_577_577, 0o_377_377_377,
        0o_776_776_776, 0o_775_775_775, 0o_773_773_773, 0o_767_767_767, 0o_757_757_757, 0o_737_737_737, 0o_677_677_677, 0o_577_577_577, 0o_377_377_377,
        0o_776_776_776, 0o_775_775_775, 0o_773_773_773, 0o_767_767_767, 0o_757_757_757, 0o_737_737_737, 0o_677_677_677, 0o_577_577_577, 0o_377_377_377,
        0o_776_776_776, 0o_775_775_775, 0o_773_773_773, 0o_767_767_767, 0o_757_757_757, 0o_737_737_737, 0o_677_677_677, 0o_577_577_577, 0o_377_377_377,
        0o_776_776_776, 0o_775_775_775, 0o_773_773_773, 0o_767_767_767, 0o_757_757_757, 0o_737_737_737, 0o_677_677_677, 0o_577_577_577, 0o_377_377_377,
        0o_776_776_776, 0o_775_775_775, 0o_773_773_773, 0o_767_767_767, 0o_757_757_757, 0o_737_737_737, 0o_677_677_677, 0o_577_577_577, 0o_377_377_377,
        0o_776_776_776, 0o_775_775_775, 0o_773_773_773, 0o_767_767_767, 0o_757_757_757, 0o_737_737_737, 0o_677_677_677, 0o_577_577_577, 0o_377_377_377,
        0o_776_776_776, 0o_775_775_775, 0o_773_773_773, 0o_767_767_767, 0o_757_757_757, 0o_737_737_737, 0o_677_677_677, 0o_577_577_577, 0o_377_377_377,
        0o_776_776_776, 0o_775_775_775, 0o_773_773_773, 0o_767_767_767, 0o_757_757_757, 0o_737_737_737, 0o_677_677_677, 0o_577_577_577, 0o_377_377_377,
    ]);
    MASKS[cell]
}

#[inline]
fn nonconflicting_cells_same_band_by_locked_candidates(shrink: u32) -> u32 {
    static MASKS: UncheckedIndexArray<u32, 512> = UncheckedIndexArray([
        0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000,
        0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000,
        0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000,
        0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000,
        0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000,
        0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000,
        0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000,
        0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000,
        0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000,
        0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000,
        0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o007070700, 0o707070700, 0o007770700, 0o707770700,
        0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o077070700, 0o777070700, 0o777770700, 0o777770700,
        0o000000000, 0o000000000, 0o007700070, 0o077700070, 0o000000000, 0o000000000, 0o007770070, 0o077770070,
        0o000000000, 0o000000000, 0o707700070, 0o777700070, 0o000000000, 0o000000000, 0o777770070, 0o777770070,
        0o000000000, 0o000000000, 0o007700770, 0o777700770, 0o007070770, 0o777070770, 0o007770770, 0o777770770,
        0o000000000, 0o000000000, 0o707700770, 0o777700770, 0o077070770, 0o777070770, 0o777770770, 0o777770770,
        0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000,
        0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o070007700, 0o070707700, 0o770007700, 0o770707700,
        0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000,
        0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o077007700, 0o777707700, 0o777007700, 0o777707700,
        0o000000000, 0o070700007, 0o000000000, 0o077700007, 0o000000000, 0o070707007, 0o000000000, 0o077707007,
        0o000000000, 0o070700707, 0o000000000, 0o777700707, 0o070007707, 0o070707707, 0o777007707, 0o777707707,
        0o000000000, 0o770700007, 0o000000000, 0o777700007, 0o000000000, 0o777707007, 0o000000000, 0o777707007,
        0o000000000, 0o770700707, 0o000000000, 0o777700707, 0o077007707, 0o777707707, 0o777007707, 0o777707707,
        0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000,
        0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o070077700, 0o070777700, 0o770777700, 0o770777700,
        0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o007077700, 0o707777700, 0o007777700, 0o707777700,
        0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o077077700, 0o777777700, 0o777777700, 0o777777700,
        0o000000000, 0o070700077, 0o007700077, 0o077700077, 0o000000000, 0o070777077, 0o007777077, 0o077777077,
        0o000000000, 0o070700777, 0o707700777, 0o777700777, 0o070077777, 0o070777777, 0o777777777, 0o777777777,
        0o000000000, 0o770700777, 0o007700777, 0o777700777, 0o007077777, 0o777777777, 0o007777777, 0o777777777,
        0o000000000, 0o770700777, 0o707700777, 0o777700777, 0o077077777, 0o777777777, 0o777777777, 0o777777777,
        0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000,
        0o000000000, 0o000000000, 0o700007070, 0o700077070, 0o000000000, 0o000000000, 0o770007070, 0o770077070,
        0o000000000, 0o700070007, 0o000000000, 0o700077007, 0o000000000, 0o707070007, 0o000000000, 0o707077007,
        0o000000000, 0o700070077, 0o700007077, 0o700077077, 0o000000000, 0o777070077, 0o777007077, 0o777077077,
        0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000,
        0o000000000, 0o000000000, 0o707007070, 0o777077070, 0o000000000, 0o000000000, 0o777007070, 0o777077070,
        0o000000000, 0o770070007, 0o000000000, 0o777077007, 0o000000000, 0o777070007, 0o000000000, 0o777077007,
        0o000000000, 0o770070077, 0o707007077, 0o777077077, 0o000000000, 0o777070077, 0o777007077, 0o777077077,
        0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000,
        0o000000000, 0o000000000, 0o700707070, 0o700777070, 0o000000000, 0o000000000, 0o770777070, 0o770777070,
        0o000000000, 0o700070707, 0o000000000, 0o700777707, 0o007070707, 0o707070707, 0o007777707, 0o707777707,
        0o000000000, 0o700070777, 0o700707777, 0o700777777, 0o077070777, 0o777070777, 0o777777777, 0o777777777,
        0o000000000, 0o000000000, 0o007707070, 0o077777070, 0o000000000, 0o000000000, 0o007777070, 0o077777070,
        0o000000000, 0o000000000, 0o707707070, 0o777777070, 0o000000000, 0o000000000, 0o777777070, 0o777777070,
        0o000000000, 0o770070777, 0o007707777, 0o777777777, 0o007070777, 0o777070777, 0o007777777, 0o777777777,
        0o000000000, 0o770070777, 0o707707777, 0o777777777, 0o077070777, 0o777070777, 0o777777777, 0o777777777,
        0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000,
        0o000000000, 0o000000000, 0o700007770, 0o700777770, 0o070007770, 0o070777770, 0o770007770, 0o770777770,
        0o000000000, 0o700770007, 0o000000000, 0o700777007, 0o000000000, 0o707777007, 0o000000000, 0o707777007,
        0o000000000, 0o700770777, 0o700007777, 0o700777777, 0o077007777, 0o777777777, 0o777007777, 0o777777777,
        0o000000000, 0o070770007, 0o000000000, 0o077777007, 0o000000000, 0o070777007, 0o000000000, 0o077777007,
        0o000000000, 0o070770777, 0o707007777, 0o777777777, 0o070007777, 0o070777777, 0o777007777, 0o777777777,
        0o000000000, 0o770770007, 0o000000000, 0o777777007, 0o000000000, 0o777777007, 0o000000000, 0o777777007,
        0o000000000, 0o770770777, 0o707007777, 0o777777777, 0o077007777, 0o777777777, 0o777007777, 0o777777777,
        0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000, 0o000000000,
        0o000000000, 0o000000000, 0o700707770, 0o700777770, 0o070077770, 0o070777770, 0o770777770, 0o770777770,
        0o000000000, 0o700770707, 0o000000000, 0o700777707, 0o007077707, 0o707777707, 0o007777707, 0o707777707,
        0o000000000, 0o700770777, 0o700707777, 0o700777777, 0o077077777, 0o777777777, 0o777777777, 0o777777777,
        0o000000000, 0o070770077, 0o007707077, 0o077777077, 0o000000000, 0o070777077, 0o007777077, 0o077777077,
        0o000000000, 0o070770777, 0o707707777, 0o777777777, 0o070077777, 0o070777777, 0o777777777, 0o777777777,
        0o000000000, 0o770770777, 0o007707777, 0o777777777, 0o007077777, 0o777777777, 0o007777777, 0o777777777,
        0o000000000, 0o770770777, 0o707707777, 0o777777777, 0o077077777, 0o777777777, 0o777777777, 0o777777777,
    ]);
    MASKS[shrink as usize]
}

#[inline]
fn nonconflicting_cells_neighbour_bands_by_locked_candidates(columns: u32) -> u32 {
    static MASKS: UncheckedIndexArray<u32, 512> = UncheckedIndexArray([
        0o777777777, 0o776776776, 0o775775775, 0o777777777, 0o773773773, 0o777777777, 0o777777777, 0o777777777,
        0o767767767, 0o766766766, 0o765765765, 0o767767767, 0o763763763, 0o767767767, 0o767767767, 0o767767767,
        0o757757757, 0o756756756, 0o755755755, 0o757757757, 0o753753753, 0o757757757, 0o757757757, 0o757757757,
        0o777777777, 0o776776776, 0o775775775, 0o777777777, 0o773773773, 0o777777777, 0o777777777, 0o777777777,
        0o737737737, 0o736736736, 0o735735735, 0o737737737, 0o733733733, 0o737737737, 0o737737737, 0o737737737,
        0o777777777, 0o776776776, 0o775775775, 0o777777777, 0o773773773, 0o777777777, 0o777777777, 0o777777777,
        0o777777777, 0o776776776, 0o775775775, 0o777777777, 0o773773773, 0o777777777, 0o777777777, 0o777777777,
        0o777777777, 0o776776776, 0o775775775, 0o777777777, 0o773773773, 0o777777777, 0o777777777, 0o777777777,
        0o677677677, 0o676676676, 0o675675675, 0o677677677, 0o673673673, 0o677677677, 0o677677677, 0o677677677,
        0o667667667, 0o666666666, 0o665665665, 0o667667667, 0o663663663, 0o667667667, 0o667667667, 0o667667667,
        0o657657657, 0o656656656, 0o655655655, 0o657657657, 0o653653653, 0o657657657, 0o657657657, 0o657657657,
        0o677677677, 0o676676676, 0o675675675, 0o677677677, 0o673673673, 0o677677677, 0o677677677, 0o677677677,
        0o637637637, 0o636636636, 0o635635635, 0o637637637, 0o633633633, 0o637637637, 0o637637637, 0o637637637,
        0o677677677, 0o676676676, 0o675675675, 0o677677677, 0o673673673, 0o677677677, 0o677677677, 0o677677677,
        0o677677677, 0o676676676, 0o675675675, 0o677677677, 0o673673673, 0o677677677, 0o677677677, 0o677677677,
        0o677677677, 0o676676676, 0o675675675, 0o677677677, 0o673673673, 0o677677677, 0o677677677, 0o677677677,
        0o577577577, 0o576576576, 0o575575575, 0o577577577, 0o573573573, 0o577577577, 0o577577577, 0o577577577,
        0o567567567, 0o566566566, 0o565565565, 0o567567567, 0o563563563, 0o567567567, 0o567567567, 0o567567567,
        0o557557557, 0o556556556, 0o555555555, 0o557557557, 0o553553553, 0o557557557, 0o557557557, 0o557557557,
        0o577577577, 0o576576576, 0o575575575, 0o577577577, 0o573573573, 0o577577577, 0o577577577, 0o577577577,
        0o537537537, 0o536536536, 0o535535535, 0o537537537, 0o533533533, 0o537537537, 0o537537537, 0o537537537,
        0o577577577, 0o576576576, 0o575575575, 0o577577577, 0o573573573, 0o577577577, 0o577577577, 0o577577577,
        0o577577577, 0o576576576, 0o575575575, 0o577577577, 0o573573573, 0o577577577, 0o577577577, 0o577577577,
        0o577577577, 0o576576576, 0o575575575, 0o577577577, 0o573573573, 0o577577577, 0o577577577, 0o577577577,
        0o777777777, 0o776776776, 0o775775775, 0o777777777, 0o773773773, 0o777777777, 0o777777777, 0o777777777,
        0o767767767, 0o766766766, 0o765765765, 0o767767767, 0o763763763, 0o767767767, 0o767767767, 0o767767767,
        0o757757757, 0o756756756, 0o755755755, 0o757757757, 0o753753753, 0o757757757, 0o757757757, 0o757757757,
        0o777777777, 0o776776776, 0o775775775, 0o777777777, 0o773773773, 0o777777777, 0o777777777, 0o777777777,
        0o737737737, 0o736736736, 0o735735735, 0o737737737, 0o733733733, 0o737737737, 0o737737737, 0o737737737,
        0o777777777, 0o776776776, 0o775775775, 0o777777777, 0o773773773, 0o777777777, 0o777777777, 0o777777777,
        0o777777777, 0o776776776, 0o775775775, 0o777777777, 0o773773773, 0o777777777, 0o777777777, 0o777777777,
        0o777777777, 0o776776776, 0o775775775, 0o777777777, 0o773773773, 0o777777777, 0o777777777, 0o777777777,
        0o377377377, 0o376376376, 0o375375375, 0o377377377, 0o373373373, 0o377377377, 0o377377377, 0o377377377,
        0o367367367, 0o366366366, 0o365365365, 0o367367367, 0o363363363, 0o367367367, 0o367367367, 0o367367367,
        0o357357357, 0o356356356, 0o355355355, 0o357357357, 0o353353353, 0o357357357, 0o357357357, 0o357357357,
        0o377377377, 0o376376376, 0o375375375, 0o377377377, 0o373373373, 0o377377377, 0o377377377, 0o377377377,
        0o337337337, 0o336336336, 0o335335335, 0o337337337, 0o333333333, 0o337337337, 0o337337337, 0o337337337,
        0o377377377, 0o376376376, 0o375375375, 0o377377377, 0o373373373, 0o377377377, 0o377377377, 0o377377377,
        0o377377377, 0o376376376, 0o375375375, 0o377377377, 0o373373373, 0o377377377, 0o377377377, 0o377377377,
        0o377377377, 0o376376376, 0o375375375, 0o377377377, 0o373373373, 0o377377377, 0o377377377, 0o377377377,
        0o777777777, 0o776776776, 0o775775775, 0o777777777, 0o773773773, 0o777777777, 0o777777777, 0o777777777,
        0o767767767, 0o766766766, 0o765765765, 0o767767767, 0o763763763, 0o767767767, 0o767767767, 0o767767767,
        0o757757757, 0o756756756, 0o755755755, 0o757757757, 0o753753753, 0o757757757, 0o757757757, 0o757757757,
        0o777777777, 0o776776776, 0o775775775, 0o777777777, 0o773773773, 0o777777777, 0o777777777, 0o777777777,
        0o737737737, 0o736736736, 0o735735735, 0o737737737, 0o733733733, 0o737737737, 0o737737737, 0o737737737,
        0o777777777, 0o776776776, 0o775775775, 0o777777777, 0o773773773, 0o777777777, 0o777777777, 0o777777777,
        0o777777777, 0o776776776, 0o775775775, 0o777777777, 0o773773773, 0o777777777, 0o777777777, 0o777777777,
        0o777777777, 0o776776776, 0o775775775, 0o777777777, 0o773773773, 0o777777777, 0o777777777, 0o777777777,
        0o777777777, 0o776776776, 0o775775775, 0o777777777, 0o773773773, 0o777777777, 0o777777777, 0o777777777,
        0o767767767, 0o766766766, 0o765765765, 0o767767767, 0o763763763, 0o767767767, 0o767767767, 0o767767767,
        0o757757757, 0o756756756, 0o755755755, 0o757757757, 0o753753753, 0o757757757, 0o757757757, 0o757757757,
        0o777777777, 0o776776776, 0o775775775, 0o777777777, 0o773773773, 0o777777777, 0o777777777, 0o777777777,
        0o737737737, 0o736736736, 0o735735735, 0o737737737, 0o733733733, 0o737737737, 0o737737737, 0o737737737,
        0o777777777, 0o776776776, 0o775775775, 0o777777777, 0o773773773, 0o777777777, 0o777777777, 0o777777777,
        0o777777777, 0o776776776, 0o775775775, 0o777777777, 0o773773773, 0o777777777, 0o777777777, 0o777777777,
        0o777777777, 0o776776776, 0o775775775, 0o777777777, 0o773773773, 0o777777777, 0o777777777, 0o777777777,
        0o777777777, 0o776776776, 0o775775775, 0o777777777, 0o773773773, 0o777777777, 0o777777777, 0o777777777,
        0o767767767, 0o766766766, 0o765765765, 0o767767767, 0o763763763, 0o767767767, 0o767767767, 0o767767767,
        0o757757757, 0o756756756, 0o755755755, 0o757757757, 0o753753753, 0o757757757, 0o757757757, 0o757757757,
        0o777777777, 0o776776776, 0o775775775, 0o777777777, 0o773773773, 0o777777777, 0o777777777, 0o777777777,
        0o737737737, 0o736736736, 0o735735735, 0o737737737, 0o733733733, 0o737737737, 0o737737737, 0o737737737,
        0o777777777, 0o776776776, 0o775775775, 0o777777777, 0o773773773, 0o777777777, 0o777777777, 0o777777777,
        0o777777777, 0o776776776, 0o775775775, 0o777777777, 0o773773773, 0o777777777, 0o777777777, 0o777777777,
        0o777777777, 0o776776776, 0o775775775, 0o777777777, 0o773773773, 0o777777777, 0o777777777, 0o777777777,
    ]);
    MASKS[columns as usize]
}

#[inline]
fn locked_minirows(shrink: u32) -> u32 {
    static MASKS: UncheckedIndexArray<u32, 512> = UncheckedIndexArray([
        0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000,
        0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000,
        0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000,
        0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000,
        0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000,
        0o000, 0o000, 0o000, 0o000, 0o124, 0o124, 0o124, 0o124, 0o000, 0o000, 0o000, 0o000, 0o124, 0o124, 0o124, 0o124,
        0o000, 0o000, 0o142, 0o142, 0o000, 0o000, 0o142, 0o142, 0o000, 0o000, 0o142, 0o142, 0o000, 0o000, 0o142, 0o142,
        0o000, 0o000, 0o142, 0o142, 0o124, 0o124, 0o100, 0o100, 0o000, 0o000, 0o142, 0o142, 0o124, 0o124, 0o100, 0o100,
        0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o214, 0o214, 0o214, 0o214,
        0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o214, 0o214, 0o214, 0o214,
        0o000, 0o241, 0o000, 0o241, 0o000, 0o241, 0o000, 0o241, 0o000, 0o241, 0o000, 0o241, 0o214, 0o200, 0o214, 0o200,
        0o000, 0o241, 0o000, 0o241, 0o000, 0o241, 0o000, 0o241, 0o000, 0o241, 0o000, 0o241, 0o214, 0o200, 0o214, 0o200,
        0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o214, 0o214, 0o214, 0o214,
        0o000, 0o000, 0o000, 0o000, 0o124, 0o124, 0o124, 0o124, 0o000, 0o000, 0o000, 0o000, 0o004, 0o004, 0o004, 0o004,
        0o000, 0o241, 0o142, 0o040, 0o000, 0o241, 0o142, 0o040, 0o000, 0o241, 0o142, 0o040, 0o214, 0o200, 0o000, 0o000,
        0o000, 0o241, 0o142, 0o040, 0o124, 0o000, 0o100, 0o000, 0o000, 0o241, 0o142, 0o040, 0o004, 0o000, 0o000, 0o000,
        0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o412, 0o412, 0o000, 0o000, 0o412, 0o412,
        0o000, 0o421, 0o000, 0o421, 0o000, 0o421, 0o000, 0o421, 0o000, 0o421, 0o412, 0o400, 0o000, 0o421, 0o412, 0o400,
        0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o412, 0o412, 0o000, 0o000, 0o412, 0o412,
        0o000, 0o421, 0o000, 0o421, 0o000, 0o421, 0o000, 0o421, 0o000, 0o421, 0o412, 0o400, 0o000, 0o421, 0o412, 0o400,
        0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o412, 0o412, 0o000, 0o000, 0o412, 0o412,
        0o000, 0o421, 0o000, 0o421, 0o124, 0o020, 0o124, 0o020, 0o000, 0o421, 0o412, 0o400, 0o124, 0o020, 0o000, 0o000,
        0o000, 0o000, 0o142, 0o142, 0o000, 0o000, 0o142, 0o142, 0o000, 0o000, 0o002, 0o002, 0o000, 0o000, 0o002, 0o002,
        0o000, 0o421, 0o142, 0o000, 0o124, 0o020, 0o100, 0o000, 0o000, 0o421, 0o002, 0o000, 0o124, 0o020, 0o000, 0o000,
        0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o412, 0o412, 0o214, 0o214, 0o010, 0o010,
        0o000, 0o421, 0o000, 0o421, 0o000, 0o421, 0o000, 0o421, 0o000, 0o421, 0o412, 0o400, 0o214, 0o000, 0o010, 0o000,
        0o000, 0o241, 0o000, 0o241, 0o000, 0o241, 0o000, 0o241, 0o000, 0o241, 0o412, 0o000, 0o214, 0o200, 0o010, 0o000,
        0o000, 0o001, 0o000, 0o001, 0o000, 0o001, 0o000, 0o001, 0o000, 0o001, 0o412, 0o000, 0o214, 0o000, 0o010, 0o000,
        0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o412, 0o412, 0o214, 0o214, 0o010, 0o010,
        0o000, 0o421, 0o000, 0o421, 0o124, 0o020, 0o124, 0o020, 0o000, 0o421, 0o412, 0o400, 0o004, 0o000, 0o000, 0o000,
        0o000, 0o241, 0o142, 0o040, 0o000, 0o241, 0o142, 0o040, 0o000, 0o241, 0o002, 0o000, 0o214, 0o200, 0o000, 0o000,
        0o000, 0o001, 0o142, 0o000, 0o124, 0o000, 0o100, 0o000, 0o000, 0o001, 0o002, 0o000, 0o004, 0o000, 0o000, 0o000,
    ]);
    MASKS[shrink as usize]
}

#[inline]
fn column_single(shrink: u32) -> u32 {
    static MASKS: UncheckedIndexArray<u32, 512> = UncheckedIndexArray([
        0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000,
        0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000,
        0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000,
        0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000,
        0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o777, 0o777, 0o666, 0o777, 0o666, 0o666, 0o666,
        0o000, 0o777, 0o777, 0o666, 0o777, 0o666, 0o666, 0o666, 0o000, 0o555, 0o555, 0o444, 0o555, 0o444, 0o444, 0o444,
        0o000, 0o777, 0o777, 0o666, 0o777, 0o666, 0o666, 0o666, 0o000, 0o555, 0o555, 0o444, 0o555, 0o444, 0o444, 0o444,
        0o000, 0o555, 0o555, 0o444, 0o555, 0o444, 0o444, 0o444, 0o000, 0o555, 0o555, 0o444, 0o555, 0o444, 0o444, 0o444,
        0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o777, 0o777, 0o666, 0o777, 0o666, 0o666, 0o666,
        0o000, 0o777, 0o777, 0o666, 0o777, 0o666, 0o666, 0o666, 0o000, 0o555, 0o555, 0o444, 0o555, 0o444, 0o444, 0o444,
        0o000, 0o777, 0o777, 0o666, 0o777, 0o666, 0o666, 0o666, 0o000, 0o555, 0o555, 0o444, 0o555, 0o444, 0o444, 0o444,
        0o000, 0o555, 0o555, 0o444, 0o555, 0o444, 0o444, 0o444, 0o000, 0o555, 0o555, 0o444, 0o555, 0o444, 0o444, 0o444,
        0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o333, 0o333, 0o222, 0o333, 0o222, 0o222, 0o222,
        0o000, 0o333, 0o333, 0o222, 0o333, 0o222, 0o222, 0o222, 0o000, 0o111, 0o111, 0o000, 0o111, 0o000, 0o000, 0o000,
        0o000, 0o333, 0o333, 0o222, 0o333, 0o222, 0o222, 0o222, 0o000, 0o111, 0o111, 0o000, 0o111, 0o000, 0o000, 0o000,
        0o000, 0o111, 0o111, 0o000, 0o111, 0o000, 0o000, 0o000, 0o000, 0o111, 0o111, 0o000, 0o111, 0o000, 0o000, 0o000,
        0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o777, 0o777, 0o666, 0o777, 0o666, 0o666, 0o666,
        0o000, 0o777, 0o777, 0o666, 0o777, 0o666, 0o666, 0o666, 0o000, 0o555, 0o555, 0o444, 0o555, 0o444, 0o444, 0o444,
        0o000, 0o777, 0o777, 0o666, 0o777, 0o666, 0o666, 0o666, 0o000, 0o555, 0o555, 0o444, 0o555, 0o444, 0o444, 0o444,
        0o000, 0o555, 0o555, 0o444, 0o555, 0o444, 0o444, 0o444, 0o000, 0o555, 0o555, 0o444, 0o555, 0o444, 0o444, 0o444,
        0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o333, 0o333, 0o222, 0o333, 0o222, 0o222, 0o222,
        0o000, 0o333, 0o333, 0o222, 0o333, 0o222, 0o222, 0o222, 0o000, 0o111, 0o111, 0o000, 0o111, 0o000, 0o000, 0o000,
        0o000, 0o333, 0o333, 0o222, 0o333, 0o222, 0o222, 0o222, 0o000, 0o111, 0o111, 0o000, 0o111, 0o000, 0o000, 0o000,
        0o000, 0o111, 0o111, 0o000, 0o111, 0o000, 0o000, 0o000, 0o000, 0o111, 0o111, 0o000, 0o111, 0o000, 0o000, 0o000,
        0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o333, 0o333, 0o222, 0o333, 0o222, 0o222, 0o222,
        0o000, 0o333, 0o333, 0o222, 0o333, 0o222, 0o222, 0o222, 0o000, 0o111, 0o111, 0o000, 0o111, 0o000, 0o000, 0o000,
        0o000, 0o333, 0o333, 0o222, 0o333, 0o222, 0o222, 0o222, 0o000, 0o111, 0o111, 0o000, 0o111, 0o000, 0o000, 0o000,
        0o000, 0o111, 0o111, 0o000, 0o111, 0o000, 0o000, 0o000, 0o000, 0o111, 0o111, 0o000, 0o111, 0o000, 0o000, 0o000,
        0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o000, 0o333, 0o333, 0o222, 0o333, 0o222, 0o222, 0o222,
        0o000, 0o333, 0o333, 0o222, 0o333, 0o222, 0o222, 0o222, 0o000, 0o111, 0o111, 0o000, 0o111, 0o000, 0o000, 0o000,
        0o000, 0o333, 0o333, 0o222, 0o333, 0o222, 0o222, 0o222, 0o000, 0o111, 0o111, 0o000, 0o111, 0o000, 0o000, 0o000,
        0o000, 0o111, 0o111, 0o000, 0o111, 0o000, 0o000, 0o000, 0o000, 0o111, 0o111, 0o000, 0o111, 0o000, 0o000, 0o000,
    ]);
    MASKS[shrink as usize]
}

#[inline]
fn neighbour_subbands(subband: usize) -> (usize, usize) {
    static NEIGHBOURS: UncheckedIndexArray<(usize, usize), N_SUBBANDS> = UncheckedIndexArray([
        (1, 2), (2, 0), (0, 1),
        (4, 5), (5, 3), (3, 4),
        (7, 8), (8, 6), (6, 7),
        (10, 11), (11, 9), (9, 10),
        (13, 14), (14, 12), (12, 13),
        (16, 17), (17, 15), (15, 16),
        (19, 20), (20, 18), (18, 19),
        (22, 23), (23, 21), (21, 22),
        (25, 26), (26, 24), (24, 25),
    ]);
    NEIGHBOURS[subband]
}

#[inline]
fn row_mask(shrink_mask: u32) -> u32 {
    static MASKS: UncheckedIndexArray<u32, 8> = UncheckedIndexArray([
        0o_000_000_000, 0o_000_000_777, 0o_000_777_000, 0o_000_777_777,
        0o_777_000_000, 0o_777_000_777, 0o_777_777_000, 0o_777_777_777,
    ]);
    MASKS[shrink_mask as usize]
}

#[inline]
fn shrink_mask(cell_mask: u32) -> u32 {
    static MASKS: UncheckedIndexArray<u32, 512> = UncheckedIndexArray([
        0, 1, 1, 1, 1, 1, 1, 1, 2, 3, 3, 3, 3, 3, 3, 3, 2, 3, 3, 3, 3, 3, 3, 3, 2, 3, 3, 3, 3, 3, 3, 3,
        2, 3, 3, 3, 3, 3, 3, 3, 2, 3, 3, 3, 3, 3, 3, 3, 2, 3, 3, 3, 3, 3, 3, 3, 2, 3, 3, 3, 3, 3, 3, 3,
        4, 5, 5, 5, 5, 5, 5, 5, 6, 7, 7, 7, 7, 7, 7, 7, 6, 7, 7, 7, 7, 7, 7, 7, 6, 7, 7, 7, 7, 7, 7, 7,
        6, 7, 7, 7, 7, 7, 7, 7, 6, 7, 7, 7, 7, 7, 7, 7, 6, 7, 7, 7, 7, 7, 7, 7, 6, 7, 7, 7, 7, 7, 7, 7,
        4, 5, 5, 5, 5, 5, 5, 5, 6, 7, 7, 7, 7, 7, 7, 7, 6, 7, 7, 7, 7, 7, 7, 7, 6, 7, 7, 7, 7, 7, 7, 7,
        6, 7, 7, 7, 7, 7, 7, 7, 6, 7, 7, 7, 7, 7, 7, 7, 6, 7, 7, 7, 7, 7, 7, 7, 6, 7, 7, 7, 7, 7, 7, 7,
        4, 5, 5, 5, 5, 5, 5, 5, 6, 7, 7, 7, 7, 7, 7, 7, 6, 7, 7, 7, 7, 7, 7, 7, 6, 7, 7, 7, 7, 7, 7, 7,
        6, 7, 7, 7, 7, 7, 7, 7, 6, 7, 7, 7, 7, 7, 7, 7, 6, 7, 7, 7, 7, 7, 7, 7, 6, 7, 7, 7, 7, 7, 7, 7,
        4, 5, 5, 5, 5, 5, 5, 5, 6, 7, 7, 7, 7, 7, 7, 7, 6, 7, 7, 7, 7, 7, 7, 7, 6, 7, 7, 7, 7, 7, 7, 7,
        6, 7, 7, 7, 7, 7, 7, 7, 6, 7, 7, 7, 7, 7, 7, 7, 6, 7, 7, 7, 7, 7, 7, 7, 6, 7, 7, 7, 7, 7, 7, 7,
        4, 5, 5, 5, 5, 5, 5, 5, 6, 7, 7, 7, 7, 7, 7, 7, 6, 7, 7, 7, 7, 7, 7, 7, 6, 7, 7, 7, 7, 7, 7, 7,
        6, 7, 7, 7, 7, 7, 7, 7, 6, 7, 7, 7, 7, 7, 7, 7, 6, 7, 7, 7, 7, 7, 7, 7, 6, 7, 7, 7, 7, 7, 7, 7,
        4, 5, 5, 5, 5, 5, 5, 5, 6, 7, 7, 7, 7, 7, 7, 7, 6, 7, 7, 7, 7, 7, 7, 7, 6, 7, 7, 7, 7, 7, 7, 7,
        6, 7, 7, 7, 7, 7, 7, 7, 6, 7, 7, 7, 7, 7, 7, 7, 6, 7, 7, 7, 7, 7, 7, 7, 6, 7, 7, 7, 7, 7, 7, 7,
        4, 5, 5, 5, 5, 5, 5, 5, 6, 7, 7, 7, 7, 7, 7, 7, 6, 7, 7, 7, 7, 7, 7, 7, 6, 7, 7, 7, 7, 7, 7, 7,
        6, 7, 7, 7, 7, 7, 7, 7, 6, 7, 7, 7, 7, 7, 7, 7, 6, 7, 7, 7, 7, 7, 7, 7, 6, 7, 7, 7, 7, 7, 7, 7,
    ]);
    MASKS[cell_mask as usize]
}
