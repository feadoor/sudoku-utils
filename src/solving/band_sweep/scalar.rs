//! The scalar solver. See the module above for what it does.
//!
//! It handles one band of one digit at a time in an ordinary word, and needs no particular
//! CPU features. The filters are answered by a table indexed by the nine minirows (or
//! minicolumns) concerned, and what follows from a digit's candidates alone is kept from one
//! stage to the next instead of being worked out again.

use super::tables::*;
use crate::utils::sudoku::Sudoku;

/// Count the solutions of `sudoku`, stopping early once `limit` have been found.
pub fn count_up_to(sudoku: &Sudoku, limit: usize) -> usize {
    let mut count = 0;
    if limit > 0 {
        if let Some(mut state) = init(sudoku) {
            search(&mut state, ALL_DIGITS, limit, &mut count);
        }
    }
    count
}

// Within a band's mask, the cells are arranged as `tables` describes.

const ALL_DIGITS: u32 = 0o777;

struct Contradiction;

/// The candidates of every digit, with some things worked out from them.
///
/// The filters see only which minirows and columns a digit occupies (its "footprint"), and
/// everything but `cells` here depends only on the footprint. So `confined` is worked out
/// when a digit is filtered, and stays right until it next needs filtering. The others are
/// wanted less often, and are brought up to date for the digits in `stale` when they are.
#[derive(Clone, Copy)]
struct State {
    /// `cells[digit][band]`: the cells of the band which can hold the digit.
    cells: [[u32; 3]; 9],
    /// `confined[digit][band]`: the cells which are the digit's only place in their box.
    confined: [[u32; 3]; 9],
    /// `footprint[digit][band]`: in the lower half, the minirows the digit can occupy (bits
    /// of `MINIROWS`) and, one bit higher, those it is certain to occupy. In the upper half
    /// likewise the columns (bits of `COLUMNS`) and, three bits higher, the certain ones.
    footprint: [[u64; 3]; 9],
    /// `counts[digit]`: one byte for each band (0-2) and stack (3-5), holding the number of
    /// configurations open to the digit there.
    counts: [u64; 9],
    /// The digits filtered since `footprint` and `counts` were last worked out for them.
    stale: u32,
}

/// The parts of a footprint which say where a digit could go, as opposed to where it must.
const COULD_GO: u64 = MINIROWS as u64 | (COLUMNS as u64) << 32;

// ---------------------------------------------------------------------------------------------
// Tables
// ---------------------------------------------------------------------------------------------

/// For each set of positions, those of them which belong to some configuration within it.
static LIVE_POSITIONS: [u16; 512] = build_live_positions();
/// The same for the minirows of a band, as the cells of the minirows which survive.
static LIVE_ROW_CELLS: [u32; 512] = build_live_row_cells();
const fn build_live_positions() -> [u16; 512] {
    let mut live = [0; 512];
    let mut set = 0;
    while set < 512 {
        let mut config = 0;
        while config < 6 {
            if set as u32 & CONFIGS[config] == CONFIGS[config] {
                live[set] |= CONFIGS[config] as u16;
            }
            config += 1;
        }
        set += 1;
    }
    live
}

const fn build_live_row_cells() -> [u32; 512] {
    let live = build_live_positions();
    let mut cells = [0; 512];
    let mut set = 0;
    while set < 512 {
        cells[set] = row_cells_of_positions(live[set] as u32);
        set += 1;
    }
    cells
}

// ---------------------------------------------------------------------------------------------
// Bit manipulation within one band
// ---------------------------------------------------------------------------------------------

/// The minirows in which a set of cells has at least one member.
#[inline(always)]
fn minirows_of(cells: u32) -> u32 {
    (cells | cells >> 1 | cells >> 2) & MINIROWS
}

/// The columns in which a set of cells has at least one member.
#[inline(always)]
fn columns_of(cells: u32) -> u32 {
    (cells | cells >> 3 | cells >> 6) & COLUMNS
}

#[inline(always)]
fn cells_of_minirows(minirows: u32) -> u32 {
    (minirows << 3) - minirows
}

#[inline(always)]
fn cells_of_columns(columns: u32) -> u32 {
    columns | columns << 3 | columns << 6
}

/// Gather a set of minirows (bit `9 * box + 3 * row`) into positions (bit `3 * box + row`).
#[inline(always)]
fn positions_of_minirows(minirows: u32) -> usize {
    let within_boxes = (minirows | minirows >> 2 | minirows >> 4) & COLUMNS;
    ((within_boxes | within_boxes >> 6 | within_boxes >> 12) & 0o777) as usize
}

/// Gather the columns a digit can occupy in each of the three bands into one word, so that
/// the minicolumns of stack `s` are positions (bit `3 * band + column`) starting at bit
/// `9 * s`.
#[inline(always)]
fn positions_of_columns(columns: &[u32; 3]) -> u32 {
    columns[0] | columns[1] << 3 | columns[2] << 6
}

/// For a set of minirows: move each row's bits to the row above, cyclically.
#[inline(always)]
fn next_row(m: u32) -> u32 {
    m >> 3 & 0o_011_011_011 | m << 6 & 0o_100_100_100
}

/// For a set of minirows: move each row's bits two rows up, cyclically.
#[inline(always)]
fn next_row2(m: u32) -> u32 {
    m >> 6 & 0o_001_001_001 | m << 3 & 0o_110_110_110
}

/// For a set of minirows: move each box's bits to the box on its left, cyclically.
#[inline(always)]
fn next_box(m: u32) -> u32 {
    (m >> 9 | m << 18) & MINIROWS
}

/// For a set of minirows: move each box's bits two boxes left, cyclically.
#[inline(always)]
fn next_box2(m: u32) -> u32 {
    (m >> 18 | m << 9) & MINIROWS
}

/// For a set of columns: move each column's bit one place down within its box, cyclically.
#[inline(always)]
fn next_column(c: u32) -> u32 {
    c >> 1 & 0o_003_003_003 | c << 2 & 0o_004_004_004
}

/// For a set of columns: move each column's bit two places down within its box, cyclically.
#[inline(always)]
fn next_column2(c: u32) -> u32 {
    c >> 2 & 0o_001_001_001 | c << 1 & 0o_006_006_006
}

/// The cells of every box which contains a member of the given set of cells.
#[inline(always)]
fn boxes_of(cells: u32) -> u32 {
    // Adding all-but-the-top to the lower bits of a field carries into its top bit exactly
    // if any of them is set, and cannot carry out of the field.
    let occupied = ((cells & FIELD_REST) + FIELD_REST | cells) & FIELD_HIGH;
    (occupied << 1) - (occupied >> 8)
}

/// The cells which are the only member of the given set within their box. Every box must
/// contain at least one member.
#[inline(always)]
fn alone_in_box(cells: u32) -> u32 {
    // Subtracting one from each field clears its lowest member, leaving something behind
    // only if there was more than one.
    let others = cells & cells.wrapping_sub(FIELD_LOW);
    cells & !boxes_of(others)
}

// ---------------------------------------------------------------------------------------------
// Propagation
// ---------------------------------------------------------------------------------------------

/// Filter one digit until nothing changes, then work out what follows from its footprint.
#[inline(always)]
fn filter(state: &mut State, digit: usize) -> Result<(), Contradiction> {
    let mut x = state.cells[digit];
    let mut minirows = [minirows_of(x[0]), minirows_of(x[1]), minirows_of(x[2])];
    loop {
        // Rows: keep the minirows which belong to some arrangement of the digit within the
        // band.
        let in_bands = [
            positions_of_minirows(minirows[0]),
            positions_of_minirows(minirows[1]),
            positions_of_minirows(minirows[2]),
        ];
        let live_rows = [LIVE_ROW_CELLS[in_bands[0]], LIVE_ROW_CELLS[in_bands[1]], LIVE_ROW_CELLS[in_bands[2]]];
        let x1 = [x[0] & live_rows[0], x[1] & live_rows[1], x[2] & live_rows[2]];

        // Columns: keep the columns which belong to some arrangement of the digit within
        // each stack.
        let positions = positions_of_columns(&[columns_of(x1[0]), columns_of(x1[1]), columns_of(x1[2])]);
        let in_stacks =
            [(positions & 0o777) as usize, (positions >> 9 & 0o777) as usize, (positions >> 18 & 0o777) as usize];
        let live = LIVE_POSITIONS[in_stacks[0]] as u32
            | (LIVE_POSITIONS[in_stacks[1]] as u32) << 9
            | (LIVE_POSITIONS[in_stacks[2]] as u32) << 18;
        let columns = [live & COLUMNS, live >> 3 & COLUMNS, live >> 6 & COLUMNS];
        x = [
            x1[0] & cells_of_columns(columns[0]),
            x1[1] & cells_of_columns(columns[1]),
            x1[2] & cells_of_columns(columns[2]),
        ];

        // A digit must have somewhere to go in every band.
        if x[0] == 0 || x[1] == 0 || x[2] == 0 {
            return Err(Contradiction);
        }
        // Each filter leaves its own input unchanged if applied again, and the column filter
        // came last. So this is a fixpoint unless clearing columns emptied a minirow which
        // the row filter had kept.
        minirows = [minirows_of(x[0]), minirows_of(x[1]), minirows_of(x[2])];
        if minirows[0] != live_rows[0] & MINIROWS
            || minirows[1] != live_rows[1] & MINIROWS
            || minirows[2] != live_rows[2] & MINIROWS
        {
            continue;
        }

        state.cells[digit] = x;
        state.confined[digit] = [alone_in_box(x[0]), alone_in_box(x[1]), alone_in_box(x[2])];
        state.stale |= 1 << digit;
        return Ok(());
    }
}

/// Bring `footprint` and `counts` up to date.
#[inline(always)]
fn refresh(state: &mut State) {
    while state.stale != 0 {
        let digit = state.stale.trailing_zeros() as usize;
        state.stale &= state.stale - 1;
        let x = &state.cells[digit];
        let minirows = [minirows_of(x[0]), minirows_of(x[1]), minirows_of(x[2])];
        let columns = [columns_of(x[0]), columns_of(x[1]), columns_of(x[2])];
        for band in 0..3 {
            // A digit is certain of a minirow which is its only one in that row, and of a
            // column which it can occupy in neither of the other two bands.
            let m = minirows[band];
            let certain_rows = m & !(next_box(m) | next_box2(m));
            let certain_columns = columns[band] & !(columns[(band + 1) % 3] | columns[(band + 2) % 3]);
            state.footprint[digit][band] =
                (m | certain_rows << 1) as u64 | ((columns[band] | certain_columns << 3) as u64) << 32;
        }
        let in_stacks = positions_of_columns(&columns);
        state.counts[digit] = CONFIG_COUNT[positions_of_minirows(minirows[0])] as u64
            | (CONFIG_COUNT[positions_of_minirows(minirows[1])] as u64) << 8
            | (CONFIG_COUNT[positions_of_minirows(minirows[2])] as u64) << 16
            | (CONFIG_COUNT[(in_stacks & 0o777) as usize] as u64) << 24
            | (CONFIG_COUNT[(in_stacks >> 9 & 0o777) as usize] as u64) << 32
            | (CONFIG_COUNT[(in_stacks >> 18 & 0o777) as usize] as u64) << 40;
    }
}

/// Settle cells: where a digit is confined to one cell of a box, or a cell is left with one
/// digit, the digit owns the cell. Other digits are removed from it, and the digit is removed
/// from the rest of the box. Requires the filters to be at a fixpoint. Returns the digits
/// which changed, and whether the grid is solved.
#[inline(always)]
fn settle(state: &mut State) -> Result<(u32, bool), Contradiction> {
    let (mut changed, mut any_two) = (0, 0);
    // Cells of different bands have nothing to do with one another here.
    for band in 0..3 {
        // The cells with at least one and at least two candidate digits, and those to which
        // some digit is confined.
        let (mut one, mut two, mut confined_any) = (0, 0, 0);
        for digit in 0..9 {
            two |= one & state.cells[digit][band];
            one |= state.cells[digit][band];
            confined_any |= state.confined[digit][band];
        }
        if one != ALL {
            return Err(Contradiction);
        }
        any_two |= two;
        let single = one & !two;
        // There is something to do only where a digit is confined to a cell which still has
        // other candidates, or a cell has one candidate which still has other places to go
        // in the box.
        if confined_any & two | single & !confined_any == 0 {
            continue;
        }

        for digit in 0..9 {
            let x = state.cells[digit][band];
            let confined = state.confined[digit][band];
            // The cells which have come to hold only this digit.
            let lone = x & single & !confined;
            let cells = x & !(confined_any & !confined) & !(boxes_of(lone) & !lone);
            state.cells[digit][band] = cells;
            changed |= ((cells != x) as u32) << digit;
        }
    }
    Ok((changed, changed == 0 && any_two == 0))
}

/// Every minirow and minicolumn holds exactly three digits. So where only three digits can
/// go in one, they all do, and where three are certain to, no other can. Returns the digits
/// which changed.
#[inline(always)]
fn apply_triad_cardinality(state: &mut State) -> Result<u32, Contradiction> {
    refresh(state);
    // For each band: the minirows and columns which exactly three digits can occupy
    // ("exact"), and those which three digits are certain to occupy ("full"), both in the
    // form of `COULD_GO`.
    let (mut exact, mut full) = ([0u64; 3], [0u64; 3]);
    let (mut impossible, mut any) = (0, 0);
    for band in 0..3 {
        // The parts of a footprint which occur in at least one, two, three and four of the
        // digits' footprints.
        let mut tally = [0u64; 4];
        for digit in 0..9 {
            let set = state.footprint[digit][band];
            tally[3] |= tally[2] & set;
            tally[2] |= tally[1] & set;
            tally[1] |= tally[0] & set;
            tally[0] |= set;
        }
        let (three, four) = (tally[2], tally[3]);
        // At least three digits must be available to each minirow and minicolumn, and no
        // more than three may be certain of it.
        impossible |= COULD_GO & !three | four & !COULD_GO;
        exact[band] = three & !four & COULD_GO;
        full[band] = three >> 1 & MINIROWS as u64 | three >> 3 & (COLUMNS as u64) << 32;
        any |= exact[band] | full[band];
    }
    if impossible != 0 {
        return Err(Contradiction);
    }
    if any == 0 {
        return Ok(0);
    }

    // Something may be deducible, for the digits which are not already certain.
    let mut changed = 0;
    for digit in 0..9 {
        let f = state.footprint[digit];
        let undecided = [
            f[0] & !(f[0] >> 1) & MINIROWS as u64 | f[0] & !(f[0] >> 3) & (COLUMNS as u64) << 32,
            f[1] & !(f[1] >> 1) & MINIROWS as u64 | f[1] & !(f[1] >> 3) & (COLUMNS as u64) << 32,
            f[2] & !(f[2] >> 1) & MINIROWS as u64 | f[2] & !(f[2] >> 3) & (COLUMNS as u64) << 32,
        ];
        let forced = [undecided[0] & exact[0], undecided[1] & exact[1], undecided[2] & exact[2]];
        let shut_out = [undecided[0] & full[0], undecided[1] & full[1], undecided[2] & full[2]];
        if forced[0] | forced[1] | forced[2] | shut_out[0] | shut_out[1] | shut_out[2] == 0 {
            continue;
        }
        let forced_columns = [(forced[0] >> 32) as u32, (forced[1] >> 32) as u32, (forced[2] >> 32) as u32];
        for band in 0..3 {
            // A digit forced into a minirow leaves the other minirows of that box and of
            // that row. One shut out of a minirow just leaves it.
            let rows = forced[band] as u32;
            let vacated_rows =
                next_row(rows) | next_row2(rows) | next_box(rows) | next_box2(rows) | shut_out[band] as u32;
            // A digit forced into a column within a band leaves the other columns of that
            // box, and leaves the column in the other two bands.
            let columns = forced_columns[band];
            let vacated_columns = next_column(columns)
                | next_column2(columns)
                | forced_columns[(band + 1) % 3]
                | forced_columns[(band + 2) % 3]
                | (shut_out[band] >> 32) as u32;

            let x = state.cells[digit][band];
            let cells = x & !cells_of_minirows(vacated_rows) & !cells_of_columns(vacated_columns);
            state.cells[digit][band] = cells;
            changed |= ((cells != x) as u32) << digit;
        }
    }
    Ok(changed)
}

/// Apply every deduction until none makes progress, cheapest first, starting from the given
/// set of digits which have changed. Returns whether the grid is solved.
#[inline(always)]
fn propagate(state: &mut State, mut dirty: u32) -> Result<bool, Contradiction> {
    loop {
        while dirty != 0 {
            let digit = dirty.trailing_zeros() as usize;
            dirty &= dirty - 1;
            filter(state, digit)?;
        }
        // A digit which settling changes is filtered again even if its footprint is the
        // same, which is cheaper than finding out whether it is.
        let solved;
        (dirty, solved) = settle(state)?;
        if dirty != 0 {
            continue;
        }
        if solved {
            return Ok(true);
        }
        dirty = apply_triad_cardinality(state)?;
        if dirty == 0 {
            return Ok(false);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------------------------

fn search(state: &mut State, dirty: u32, limit: usize, count: &mut usize) {
    match propagate(state, dirty) {
        Err(Contradiction) => return,
        Ok(true) => {
            *count += 1;
            return;
        }
        Ok(false) => {}
    }

    // Branch within the band or stack with the fewest configurations in total, on the digit
    // with the fewest. Propagation settles every cell before all of them are down to one
    // configuration, so there is always something to branch on.
    let mut totals = 0;
    for digit in 0..9 {
        totals += state.counts[digit];
    }
    // The smallest is found by packing each number above its index, without branching, since
    // which way the comparisons go is hard to predict.
    let mut best = u64::MAX;
    for i in 0..6 {
        let total = totals >> (8 * i) & 0xff;
        let key = if total > 9 { total << 8 | i } else { u64::MAX };
        best = best.min(key);
    }
    if best == u64::MAX {
        *count += 1;
        return;
    }
    let best = (best & 0xff) as usize;
    let mut fewest = u64::MAX;
    for digit in 0..9 {
        let candidate = state.counts[digit] >> (8 * best) & 0xff;
        let key = if candidate >= 2 { candidate << 8 | digit as u64 } else { u64::MAX };
        fewest = fewest.min(key);
    }
    let digit = (fewest & 0xff) as usize;

    // Try each of the digit's configurations there in turn, the last of them in place rather
    // than on a copy.
    let f = &state.footprint[digit];
    let positions = if best < 3 {
        positions_of_minirows(f[best] as u32 & MINIROWS)
    } else {
        let columns = [(f[0] >> 32) as u32 & COLUMNS, (f[1] >> 32) as u32 & COLUMNS, (f[2] >> 32) as u32 & COLUMNS];
        (positions_of_columns(&columns) >> (9 * (best - 3)) & 0o777) as usize
    };
    let mut configs = CONFIGS_WITHIN[positions] as u32;
    loop {
        let config = configs.trailing_zeros() as usize;
        configs &= configs - 1;
        if configs == 0 {
            assume_config(state, best, digit, config);
            search(state, 1 << digit, limit, count);
            return;
        }
        let mut child = *state;
        assume_config(&mut child, best, digit, config);
        search(&mut child, 1 << digit, limit, count);
        if *count >= limit {
            return;
        }
    }
}

/// Restrict a digit to one configuration within a band (0-2) or stack (3-5).
fn assume_config(state: &mut State, band_or_stack: usize, digit: usize, config: usize) {
    if band_or_stack < 3 {
        state.cells[digit][band_or_stack] &= CONFIG_ROW_CELLS[config];
    } else {
        // Within the stack, each band keeps only the configuration's column.
        let shift = 9 * (band_or_stack - 3);
        for band in 0..3 {
            let column = CONFIGS[config] >> (3 * band) & 0o7;
            let columns = COLUMNS & !(0o7 << shift) | column << shift;
            state.cells[digit][band] &= cells_of_columns(columns);
        }
    }
}

fn init(sudoku: &Sudoku) -> Option<State> {
    // Find the clues first, so that the work below is done only for them without a branch
    // on each cell, which would be hard to predict.
    let mut clues = 0u128;
    for (cell, &value) in sudoku.digits().enumerate() {
        clues |= ((value != 0) as u128) << cell;
    }

    // For each digit and band: the cells where the digit is given, and the boxes containing
    // those cells. For each band: the cells where anything is given.
    let mut given = [[0u32; 3]; 9];
    let mut given_boxes = [[0u32; 3]; 9];
    let mut filled = [0u32; 3];
    while clues != 0 {
        let cell = clues.trailing_zeros() as usize;
        clues &= clues - 1;
        let digit = sudoku[cell] as usize - 1;
        if digit >= 9 {
            return None;
        }
        let (band, position) = CELL_POSITION[cell];
        let (band, position) = (band as usize, position as u32);
        let bit = 1u32 << position;
        let in_box = 0o777 << (position - position % 9);
        // The same digit twice in one box could otherwise go unnoticed.
        if given_boxes[digit][band] & in_box != 0 {
            return None;
        }
        given[digit][band] |= bit;
        given_boxes[digit][band] |= in_box;
        filled[band] |= bit;
    }

    // A digit keeps the cells where it is given, and otherwise loses every filled cell and
    // every box where it is given. The rest is filled in when the digits are first filtered.
    let mut state =
        State { cells: [[0; 3]; 9], confined: [[0; 3]; 9], footprint: [[0; 3]; 9], counts: [0; 9], stale: 0 };
    for digit in 0..9 {
        for band in 0..3 {
            state.cells[digit][band] = ALL & !filled[band] & !given_boxes[digit][band] | given[digit][band];
        }
    }
    Some(state)
}
