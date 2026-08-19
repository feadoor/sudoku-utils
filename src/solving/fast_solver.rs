use crate::utils::bitmask::MaskIter;
use crate::utils::sudoku::Sudoku;

const N_DIGITS: usize = 9;
const N_BANDS: usize = 3;
const N_SUBBANDS: usize = N_DIGITS * N_BANDS;
const N_CELLS: usize = 81;

const NONE: u32 = 0;
const ALL: u32 = 0o_777_777_777;
const LOW9: u32 = 0o_777;

/// A sentinel signalling that a  partial grid cannot be completed.
struct Contradiction;

enum Status {
    Contradiction,
    Solved,
    Stuck,
}

/// A helper type for unchecked indexing into arrays, which speeds up the solver
/// by up to 10% on the hardest puzzles.
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

/// A single node in the search, using a band-oriented data representation.
///
/// `possible[digit * 3 + band]`: a 27-bit mask of the cells within `band` which can hold `digit`
/// `unsolved[band]`: cells within the band not yet holding a value
/// `bivalue[band]`: cells within the band which have exactly two candidates
/// `prev_possible`: snapshots `possible` after the last locked-candidate sweep so subsequent sweeps
/// can skip subbands that have not changed since the previous one
#[derive(Clone)]
pub struct FastBruteForceSolver {
    possible: UncheckedIndexArray<u32, N_SUBBANDS>,
    prev_possible: UncheckedIndexArray<u32, N_SUBBANDS>,
    unsolved: UncheckedIndexArray<u32, N_BANDS>,
    bivalue: UncheckedIndexArray<u32, N_BANDS>,
}

impl FastBruteForceSolver {

    pub fn has_solution(sudoku: &Sudoku) -> bool {
        Self::count_up_to(sudoku, 1) == 1
    }

    pub fn has_unique_solution(sudoku: &Sudoku) -> bool {
        Self::count_up_to(sudoku, 2) == 1
    }

    pub fn count_solutions(sudoku: &Sudoku) -> usize {
        Self::count_up_to(sudoku, usize::MAX)
    }

    pub fn is_minimal(sudoku: &Sudoku) -> bool {
        if !Self::has_unique_solution(sudoku) { return false; }
        let mut sudoku = sudoku.clone();
        (0 .. 81).all(|idx| sudoku[idx] == 0 || {
            let d = sudoku[idx]; sudoku[idx] = 0;
            if Self::has_unique_solution(&sudoku) { return false; }
            sudoku[idx] = d; true
        })
    }

    pub fn count_up_to(sudoku: &Sudoku, limit: usize) -> usize {
        match Self::from_sudoku(sudoku) {
            Some(mut solver) => {
                let mut count = 0;
                solver.search(limit, &mut count);
                count
            }
            None => 0,
        }
    }

    fn search(&mut self, limit: usize, count: &mut usize) {
        match self.propagate() {
            Status::Contradiction => {}
            Status::Solved => *count += 1,
            Status::Stuck => {
                let (band, cell_mask) = self.choose_branch_cell();

                // Find the candidates that can still occupy this cell.
                let mut candidates = [0usize; N_DIGITS];
                let mut n = 0;
                for subband in (band..).step_by(N_BANDS).take(N_DIGITS) {
                    if self.possible[subband] & cell_mask != NONE {
                        candidates[n] = subband;
                        n += 1;
                    }
                }

                // Try every candidate but the last on a fresh clone and reuse `self` in place
                // for the final candidate, so a k-candidate cell costs only k - 1 clones.
                for &subband in candidates[..n.saturating_sub(1)].iter() {
                    if *count >= limit {
                        return;
                    }
                    let mut child = self.clone();
                    child.assign(subband, cell_mask);
                    child.search(limit, count);
                }
                if n > 0 && *count < limit {
                    self.assign(candidates[n - 1], cell_mask);
                    self.search(limit, count);
                }
            }
        }
    }

    fn propagate(&mut self) -> Status {
        match self.run_deductions() {
            Err(Contradiction) => Status::Contradiction,
            Ok(()) if self.is_solved() => Status::Solved,
            Ok(()) => Status::Stuck,
        }
    }

    fn run_deductions(&mut self) -> Result<(), Contradiction> {
        loop {
            self.eliminate_locked_candidates()?;
            if self.is_solved() {
                return Ok(());
            }
            if !self.assign_naked_singles()? {
                return Ok(());
            }
        }
    }

    fn is_solved(&self) -> bool {
        self.unsolved.0 == [NONE; N_BANDS]
    }

    /// Choose a cell to branch on. If there are any bivalue cells, which is almost always the case
    /// and which are found as a side effect of the propagation step, choose one of them.
    /// Otherwise, finding a cell with as few candidates as possible is valuable but an exhaustive
    /// scan is too expensive, so examine the first unsolved cell in each band and take the one with
    /// the fewest candidates.
    fn choose_branch_cell(&self) -> (usize, u32) {
        for band in 0..N_BANDS {
            if let Some(cell_mask) = MaskIter::<u32>::from(self.bivalue[band]).peek() {
                return (band, cell_mask);
            }
        }

        (0..N_BANDS)
            .filter_map(|band| {
                let cell_mask = MaskIter::<u32>::from(self.unsolved[band]).peek()?;
                let candidates = (band..)
                    .step_by(N_BANDS)
                    .take(N_DIGITS)
                    .filter(|&subband| self.possible[subband] & cell_mask != NONE)
                    .count();
                Some((candidates, band, cell_mask))
            })
            .min()
            .map(|(_, band, cell_mask)| (band, cell_mask))
            .expect("a stuck grid always has an unsolved cell")
    }

    fn from_sudoku(sudoku: &Sudoku) -> Option<Self> {
        let mut solver = Self {
            possible: UncheckedIndexArray([ALL; N_SUBBANDS]),
            prev_possible: UncheckedIndexArray([NONE; N_SUBBANDS]),
            unsolved: UncheckedIndexArray([ALL; N_BANDS]),
            bivalue: UncheckedIndexArray([NONE; N_BANDS]),
        };

        for (cell, &value) in sudoku.digits().enumerate() {
            if value != 0 {
                solver.insert_value(cell, value).ok()?;
            }
        }

        Some(solver)
    }

    /// Insert a given clue, clearing candidates from the same row and box, from
    /// neighbouring bands column-wise and from other digits in this cell. Only used
    /// during initial grid construction.
    fn insert_value(&mut self, cell: usize, value: u8) -> Result<(), Contradiction> {
        let band = cell / 27;
        let subband = (value as usize - 1) * N_BANDS + band;
        let cell_mask = 1 << (cell % 27);

        if self.possible[subband] & cell_mask == NONE {
            return Err(Contradiction);
        }

        self.unsolved[band] &= !cell_mask;

        self.possible[subband] &= nonconflicting_cells_same_band(cell);
        let nonconflicting_neighbours = nonconflicting_cells_neighbour_bands(cell);
        let (neighbour1, neighbour2) = neighbour_subbands(subband);
        self.possible[neighbour1] &= nonconflicting_neighbours;
        self.possible[neighbour2] &= nonconflicting_neighbours;

        for digit_subband in (band..).step_by(N_BANDS).take(N_DIGITS) {
            self.possible[digit_subband] &= !cell_mask;
        }
        self.possible[subband] |= cell_mask;

        Ok(())
    }

    /// Place a digit at the cell given by `cell_mask` within `subband`, clearing
    /// it from the rest of that row and box. Does not clear the column or the cell's
    /// other digits; this is handled by the next locked-candidate sweep.
    fn assign(&mut self, subband: usize, cell_mask: u32) {
        let cell = cell_mask.trailing_zeros() as usize;
        self.possible[subband] &= nonconflicting_cells_same_band(cell);
    }

    /// Find cells with a single candidate and place them. Also records, per
    /// band, which cells have exactly two candidates and detects cells with zero
    /// candidates. Returns whether any single was placed.
    fn assign_naked_singles(&mut self) -> Result<bool, Contradiction> {
        let mut placed_any = false;
        for band in 0..N_BANDS {
            // Masks of cells (in this band) with >=1, >=2 and >=3 candidates.
            let (mut cells1, mut cells2, mut cells3) = (NONE, NONE, NONE);
            for subband in (band..).step_by(N_BANDS).take(N_DIGITS) {
                let band_mask = self.possible[subband];
                cells3 |= cells2 & band_mask;
                cells2 |= cells1 & band_mask;
                cells1 |= band_mask;
            }

            // Every cell must have at least one candidate.
            if cells1 != ALL {
                return Err(Contradiction);
            }

            // Cells with exactly two candidates.
            self.bivalue[band] = cells2 ^ cells3;

            // New singles: exactly one candidate, not already solved.
            let singles = (cells1 ^ cells2) & self.unsolved[band];

            'insert: for cell_mask in MaskIter::<u32>::from(singles) {
                placed_any = true;

                // Find and place the one digit that fits here.
                for digit in 0..N_DIGITS {
                    if self.possible[digit * N_BANDS + band] & cell_mask != NONE {
                        self.assign(digit * N_BANDS + band, cell_mask);
                        continue 'insert;
                    }
                }

                // No digit fits: the cell is forced empty.
                return Err(Contradiction);
            }
        }

        Ok(placed_any)
    }

    /// Apply locked-candidate eliminations across every subband that has
    /// changed since the last sweep, repeating until there are no further
    /// changes. The unrolled loop runs faster than a loop over (0 .. 27).
    fn eliminate_locked_candidates(&mut self) -> Result<(), Contradiction> {
        macro_rules! sweep {
            ($found:ident; $($subband:literal)*) => {$(
                if self.possible[$subband] != self.prev_possible[$subband] {
                    $found = true;
                    self.eliminate_locked_candidates_subband($subband)?;
                }
            )*};
        }

        loop {
            let mut found_something = false;
            sweep!(found_something; 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26);
            if !found_something {
                return Ok(());
            }
        }
    }

    /// Locked-candidate processing for one subband: find pointing/claiming
    /// candidates within the band, propagate the resulting column constraints to
    /// the neighbouring bands, and mark any cells that become solved.
    #[inline(always)]
    fn eliminate_locked_candidates_subband(&mut self, subband: usize) -> Result<(), Contradiction> {
        let old_possible_cells = self.possible[subband];

        // Condense each row of 9 bits to 3 (one bit per minirow) with a lookup,
        // pack the band's three rows into a 9-bit key, and look up the cells
        // that survive locked-candidate elimination within the band.
        let shrink = shrink_mask(old_possible_cells & LOW9)
            | shrink_mask(old_possible_cells >> 9 & LOW9) << 3
            | shrink_mask(old_possible_cells >> 18) << 6;
        let possible_cells = old_possible_cells & nonconflicting_cells_same_band_by_locked_candidates(shrink);

        if possible_cells == NONE {
            return Err(Contradiction);
        }
        self.prev_possible[subband] = possible_cells;
        self.possible[subband] = possible_cells;

        // Columns (within the band) that still hold this digit, solved or not.
        let possible_columns = (possible_cells | possible_cells >> 9 | possible_cells >> 18) & LOW9;

        // Propagate column constraints to the two neighbouring bands. This is
        // also what forbids a digit appearing twice in a column, since `assign`
        // ignores columns.
        let nonconflicting_neighbours = nonconflicting_cells_neighbour_bands_by_locked_candidates(possible_columns);
        let (neighbour1, neighbour2) = neighbour_subbands(subband);
        self.possible[neighbour1] &= nonconflicting_neighbours;
        self.possible[neighbour2] &= nonconflicting_neighbours;

        // A minirow locked to a single column places the digit in that cell.
        let locked_candidates_intersection = locked_minirows(shrink) & column_single(possible_columns);
        let solved_rows = shrink_mask(locked_candidates_intersection);
        let solved_cells = row_mask(solved_rows) & possible_cells;

        // Remove every other digit from the cells solved above.
        let band = subband % N_BANDS;
        let nonconflicting_cells = !solved_cells;
        self.unsolved[band] &= nonconflicting_cells;
        for other in (band..).step_by(N_BANDS).take(N_DIGITS).filter(|&other| other != subband) {
            self.possible[other] &= nonconflicting_cells;
        }

        Ok(())
    }
}

#[inline]
fn nonconflicting_cells_same_band(cell: usize) -> u32 {
    const MASKS: UncheckedIndexArray<u32, N_CELLS> = UncheckedIndexArray([
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
    const MASKS: UncheckedIndexArray<u32, N_CELLS> = UncheckedIndexArray([
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
    const MASKS: UncheckedIndexArray<u32, 512> = UncheckedIndexArray([
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
    const MASKS: UncheckedIndexArray<u32, 512> = UncheckedIndexArray([
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
    const MASKS: UncheckedIndexArray<u32, 512> = UncheckedIndexArray([
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
    const MASKS: UncheckedIndexArray<u32, 512> = UncheckedIndexArray([
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
    const NEIGHBOURS: UncheckedIndexArray<(usize, usize), N_SUBBANDS> = UncheckedIndexArray([
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
    const MASKS: UncheckedIndexArray<u32, 8> = UncheckedIndexArray([
        0o_000_000_000, 0o_000_000_777, 0o_000_777_000, 0o_000_777_777,
        0o_777_000_000, 0o_777_000_777, 0o_777_777_000, 0o_777_777_777,
    ]);
    MASKS[shrink_mask as usize]
}

#[inline]
fn shrink_mask(cell_mask: u32) -> u32 {
    const MASKS: UncheckedIndexArray<u32, 512> = UncheckedIndexArray([
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
