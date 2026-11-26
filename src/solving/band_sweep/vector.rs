//! The vector solver. See the module above for what it does.
//!
//! The three bands of a digit are held in three 32-bit lanes, two digits to a 256-bit
//! register, and updated using nothing but shifts and masks. So every stage works on two
//! digits at once, and the stages which relate the digits to one another see all of them in
//! five registers.

use super::tables::*;
use crate::utils::sudoku::Sudoku;
use std::arch::x86_64::*;

/// Applies the CPU features to each function so that they inline into one another.
macro_rules! simd {
    ($($item:item)*) => {$(
        #[target_feature(enable = "avx2,bmi1,bmi2,popcnt")]
        #[inline]
        $item
    )*};
}

// Within a lane, the cells of a band are arranged as `tables` describes.

struct Contradiction;

/// Two digits: the lower half holds one and the upper half the next. Within each half,
/// lanes 0, 1 and 2 hold the digit's three bands and lane 3 is zero.
type Pair = __m256i;
/// One digit, or something computed for each band.
type Bands = __m128i;

/// The candidate cells of each digit within each band. A tenth, empty digit pads the
/// array to five pairs.
#[derive(Clone, Copy)]
#[repr(C, align(32))]
struct State {
    x: [[u32; 4]; 10],
}

/// For each digit and band, the minirows (bits of `MINIROWS`) and the columns (bits of
/// `COLUMNS`) which the digit can occupy. Filled in by the cardinality stage, and so
/// valid whenever propagation ends without having solved the grid.
#[repr(C, align(32))]
struct Summary {
    rows: [[u32; 4]; 10],
    columns: [[u32; 4]; 10],
}

/// For each pair, the bytes of a lane-wise comparison which belong to real lanes.
const REAL_LANES: [u32; 5] = build_real_lanes();

const fn build_real_lanes() -> [u32; 5] {
    // Four bytes for each of a digit's three bands, in each half which holds a digit.
    let mut lanes = [0; 5];
    let mut digit = 0;
    while digit < 9 {
        lanes[digit / 2] |= 0x0fff << (16 * (digit % 2));
        digit += 1;
    }
    lanes
}
const ALL_PAIRS: u32 = 0b11111;

/// Whether the CPU has what this solver needs.
pub fn is_supported() -> bool {
    is_x86_feature_detected!("avx2")
        && is_x86_feature_detected!("bmi1")
        && is_x86_feature_detected!("bmi2")
        && is_x86_feature_detected!("popcnt")
}

// -----------------------------------------------------------------------------------------
// Lane-wise bit manipulation
// -----------------------------------------------------------------------------------------

macro_rules! shr {
    ($v:expr, $n:literal) => {
        _mm256_srli_epi32::<$n>($v)
    };
}
macro_rules! shl {
    ($v:expr, $n:literal) => {
        _mm256_slli_epi32::<$n>($v)
    };
}

simd! {
    fn splat(value: u32) -> Pair {
        _mm256_set1_epi32(value as i32)
    }

    fn and(a: Pair, b: Pair) -> Pair {
        _mm256_and_si256(a, b)
    }

    fn or(a: Pair, b: Pair) -> Pair {
        _mm256_or_si256(a, b)
    }

    fn xor(a: Pair, b: Pair) -> Pair {
        _mm256_xor_si256(a, b)
    }

    /// `a & !b`
    fn without(a: Pair, b: Pair) -> Pair {
        _mm256_andnot_si256(b, a)
    }

    fn is_zero(a: Pair) -> bool {
        _mm256_testz_si256(a, a) != 0
    }

    /// For a set of minirows: move each row's bits to the row above, cyclically.
    fn next_row(m: Pair) -> Pair {
        or(and(shr!(m, 3), splat(0o_011_011_011)), and(shl!(m, 6), splat(0o_100_100_100)))
    }

    /// For a set of minirows: move each row's bits two rows up, cyclically.
    fn next_row2(m: Pair) -> Pair {
        or(and(shr!(m, 6), splat(0o_001_001_001)), and(shl!(m, 3), splat(0o_110_110_110)))
    }

    /// For a set of minirows: move each box's bits to the box on its left, cyclically.
    fn next_box(m: Pair) -> Pair {
        and(or(shr!(m, 9), shl!(m, 18)), splat(MINIROWS))
    }

    /// For a set of minirows: move each box's bits two boxes left, cyclically.
    fn next_box2(m: Pair) -> Pair {
        and(or(shr!(m, 18), shl!(m, 9)), splat(MINIROWS))
    }

    /// For a set of columns: move each column's bit one place down within its box.
    fn next_column(c: Pair) -> Pair {
        or(and(shr!(c, 1), splat(0o_003_003_003)), and(shl!(c, 2), splat(0o_004_004_004)))
    }

    /// For a set of columns: move each column's bit two places down within its box.
    fn next_column2(c: Pair) -> Pair {
        or(and(shr!(c, 2), splat(0o_001_001_001)), and(shl!(c, 1), splat(0o_006_006_006)))
    }

    /// Move each band's lane to the band above, cyclically, within each digit.
    fn next_band(v: Pair) -> Pair {
        _mm256_shuffle_epi32::<0b11_00_10_01>(v)
    }

    /// Move each band's lane two bands up, cyclically, within each digit.
    fn next_band2(v: Pair) -> Pair {
        _mm256_shuffle_epi32::<0b11_01_00_10>(v)
    }

    /// The minirows of a band in which a set of cells has at least one member.
    fn minirows_of(cells: Pair) -> Pair {
        and(or(or(cells, shr!(cells, 1)), shr!(cells, 2)), splat(MINIROWS))
    }

    /// The columns of a band in which a set of cells has at least one member.
    fn columns_of(cells: Pair) -> Pair {
        and(or(or(cells, shr!(cells, 3)), shr!(cells, 6)), splat(COLUMNS))
    }

    /// The cells of a set of minirows.
    fn cells_of_minirows(m: Pair) -> Pair {
        _mm256_sub_epi32(shl!(m, 3), m)
    }

    /// The cells of a set of columns.
    fn cells_of_columns(c: Pair) -> Pair {
        or(or(c, shl!(c, 3)), shl!(c, 6))
    }

    /// The cells of every box which contains a member of the given set of cells.
    fn boxes_of(cells: Pair) -> Pair {
        // Adding all-but-the-top to the lower bits of a field carries into its top bit
        // exactly if any of them is set, and cannot carry out of the field.
        let rest = splat(FIELD_REST);
        let occupied = and(or(_mm256_add_epi32(and(cells, rest), rest), cells), splat(FIELD_HIGH));
        _mm256_sub_epi32(shl!(occupied, 1), shr!(occupied, 8))
    }

    /// The cells which are the only member of the given set within their box. Every box
    /// must contain at least one member, except in lanes which are entirely empty.
    fn alone_in_box(cells: Pair) -> Pair {
        // Subtracting one from each field clears its lowest member, leaving something
        // behind only if there was more than one.
        let others = and(cells, _mm256_sub_epi32(cells, splat(FIELD_LOW)));
        without(cells, boxes_of(others))
    }

    fn lo(v: Pair) -> Bands {
        _mm256_castsi256_si128(v)
    }

    fn hi(v: Pair) -> Bands {
        _mm256_extracti128_si256::<1>(v)
    }

    fn both(v: Bands) -> Pair {
        _mm256_broadcastsi128_si256(v)
    }

    /// Whether the three real lanes all equal `value`.
    fn bands_all_equal(v: Bands, value: u32) -> bool {
        _mm_movemask_epi8(_mm_cmpeq_epi32(v, _mm_set1_epi32(value as i32))) & 0xfff == 0xfff
    }

    fn load(lanes: &[[u32; 4]; 10], pair: usize) -> Pair {
        debug_assert!(pair < 5);
        // SAFETY: a pair is two adjacent digits, and the containing struct is aligned.
        unsafe { _mm256_load_si256(lanes.as_ptr().add(2 * pair).cast()) }
    }

    fn store(lanes: &mut [[u32; 4]; 10], pair: usize, value: Pair) {
        debug_assert!(pair < 5);
        // SAFETY: as for `load`.
        unsafe { _mm256_store_si256(lanes.as_mut_ptr().add(2 * pair).cast(), value) }
    }
}

// The same manipulations for a single lane, used off the hot path.

fn cells_of_columns_1(c: u32) -> u32 {
    c | c << 3 | c << 6
}

// -----------------------------------------------------------------------------------------
// Propagation
// -----------------------------------------------------------------------------------------

simd! {
    /// Filter a pair of digits until nothing changes. `real_lanes` is the pair's entry
    /// in `REAL_LANES`.
    fn filter(mut x: Pair, real_lanes: u32) -> Result<Pair, Contradiction> {
        loop {
            // Rows: keep the minirows which belong to some arrangement of the digit
            // within the band, an arrangement being one minirow per row and per box.
            let m = minirows_of(x);
            let (b1, b2) = (next_box(m), next_box2(m));
            let live_rows = and(m, or(and(next_row(b1), next_row2(b2)), and(next_row2(b1), next_row(b2))));
            let x1 = and(x, cells_of_minirows(live_rows));

            // Columns: keep the columns which belong to some arrangement of the digit
            // within each stack, an arrangement being one minicolumn per band and per
            // column.
            let c = columns_of(x1);
            let (n1, n2) = (next_column(c), next_column2(c));
            let live_columns = and(
                c,
                or(and(next_band(n1), next_band2(n2)), and(next_band(n2), next_band2(n1))),
            );
            let x2 = and(x1, cells_of_columns(live_columns));

            // A digit must have somewhere to go in every band.
            let empty = _mm256_cmpeq_epi32(x2, _mm256_setzero_si256());
            if _mm256_movemask_epi8(empty) as u32 & real_lanes != 0 {
                return Err(Contradiction);
            }
            // Each filter leaves its own input unchanged if applied again, and the column
            // filter came last. So this is a fixpoint unless clearing columns emptied a
            // minirow which the row filter had kept.
            let stale_rows = xor(minirows_of(x2), live_rows);
            x = x2;
            if is_zero(stale_rows) {
                return Ok(x);
            }
        }
    }

    /// Settle cells: where a digit is confined to one cell of a box, or a cell is left
    /// with one digit, the digit owns the cell. Other digits are removed from it, and the
    /// digit is removed from the rest of the box. Requires the filters to be at a
    /// fixpoint. Returns the pairs which changed, those among them which now need
    /// filtering again, and whether the grid is solved.
    fn settle(state: &mut State) -> Result<(u32, u32, bool), Contradiction> {
        let x = [
            load(&state.x, 0),
            load(&state.x, 1),
            load(&state.x, 2),
            load(&state.x, 3),
            load(&state.x, 4),
        ];

        // The cells with at least one and at least two candidate digits. Even and odd
        // digits are accumulated separately in the two halves and then combined.
        let (mut one, mut two) = (_mm256_setzero_si256(), _mm256_setzero_si256());
        for pair in x {
            two = or(two, and(one, pair));
            one = or(one, pair);
        }
        let at_least_one = _mm_or_si128(lo(one), hi(one));
        let at_least_two =
            _mm_or_si128(_mm_or_si128(lo(two), hi(two)), _mm_and_si128(lo(one), hi(one)));
        if !bands_all_equal(at_least_one, ALL) {
            return Err(Contradiction);
        }
        let single = both(_mm_andnot_si128(at_least_two, at_least_one));

        let zero = _mm256_setzero_si256();
        let mut owned = [zero; 5];
        let (mut owned_by_any, mut unsettled) = (zero, zero);
        for i in 0..5 {
            let confined = alone_in_box(x[i]);
            owned[i] = or(confined, and(x[i], single));
            owned_by_any = or(owned_by_any, owned[i]);
            // There is something to do only where a digit is confined to a cell which
            // still has other candidates, or a cell has one candidate which still has
            // other places to go in the box.
            unsettled = or(unsettled, and(x[i], xor(confined, single)));
        }
        if is_zero(unsettled) {
            return Ok((0, 0, bands_all_equal(at_least_two, 0)));
        }
        let owned_by_any = both(_mm_or_si128(lo(owned_by_any), hi(owned_by_any)));

        let (mut changed, mut unfiltered) = (0, 0);
        for i in 0..5 {
            let owned_by_other = without(owned_by_any, owned[i]);
            let rest_of_box = without(boxes_of(owned[i]), owned[i]);
            let cells = without(without(x[i], owned_by_other), rest_of_box);
            // Done without branching, since which pairs change is hard to predict.
            store(&mut state.x, i, cells);
            changed |= ((_mm256_testc_si256(cells, x[i]) == 0) as u32) << i;
            // The filters see only which minirows and columns are occupied, so a pair
            // in which those are unchanged is still at their fixpoint.
            let difference = or(
                xor(minirows_of(cells), minirows_of(x[i])),
                xor(columns_of(cells), columns_of(x[i])),
            );
            unfiltered |= (!is_zero(difference) as u32) << i;
        }
        Ok((changed, unfiltered, false))
    }

    /// Add a set of positions per digit to running tallies of the positions which occur
    /// in at least one, two, three and four of the sets seen so far.
    fn tally(t: &mut [Pair; 4], set: Pair) {
        t[3] = or(t[3], and(t[2], set));
        t[2] = or(t[2], and(t[1], set));
        t[1] = or(t[1], and(t[0], set));
        t[0] = or(t[0], set);
    }

    /// Combine the tallies of the even digits (lower half) and odd digits (upper half)
    /// into the positions occurring in at least three and at least four sets overall.
    fn at_least_three_and_four(t: &[Pair; 4]) -> (Bands, Bands) {
        let (e1, e2, e3, e4) = (lo(t[0]), lo(t[1]), lo(t[2]), lo(t[3]));
        let (o1, o2, o3, o4) = (hi(t[0]), hi(t[1]), hi(t[2]), hi(t[3]));
        let three = _mm_or_si128(
            _mm_or_si128(e3, o3),
            _mm_or_si128(_mm_and_si128(e2, o1), _mm_and_si128(e1, o2)),
        );
        let four = _mm_or_si128(
            _mm_or_si128(_mm_or_si128(e4, o4), _mm_and_si128(e2, o2)),
            _mm_or_si128(_mm_and_si128(e3, o1), _mm_and_si128(e1, o3)),
        );
        (three, four)
    }

    /// Every minirow and minicolumn holds exactly three digits. So where only three
    /// digits can go in one, they all do, and where three are certain to, no other can.
    /// Returns the pairs which changed. Also fills in the summary.
    fn apply_triad_cardinality(state: &mut State, summary: &mut Summary) -> Result<u32, Contradiction> {
        // Two tallies are kept, each covering two things at once in separate bits of a
        // lane: where digits could go, and (shifted up) where they are certain to go.
        let zero = _mm256_setzero_si256();
        let (mut rows, mut columns) = ([zero; 4], [zero; 4]);
        for i in 0..5 {
            let x = load(&state.x, i);
            let m = minirows_of(x);
            let c = columns_of(x);
            store(&mut summary.rows, i, m);
            store(&mut summary.columns, i, c);
            // A digit is certain of a minirow which is its only one in that row, and of
            // a column which it can occupy in neither of the other two bands.
            let certain_rows = without(m, or(next_box(m), next_box2(m)));
            let certain_columns = without(c, or(next_band(c), next_band2(c)));
            tally(&mut rows, or(m, shl!(certain_rows, 1)));
            tally(&mut columns, or(c, shl!(certain_columns, 3)));
        }

        let (row_three, row_four) = at_least_three_and_four(&rows);
        let (column_three, column_four) = at_least_three_and_four(&columns);
        // At least three digits must be available to each minirow and minicolumn, and no
        // more than three may be certain of it.
        let rows_mask = _mm_set1_epi32(MINIROWS as i32);
        let columns_mask = _mm_set1_epi32(COLUMNS as i32);
        let short_rows = _mm_andnot_si128(row_three, rows_mask);
        let short_columns = _mm_andnot_si128(column_three, columns_mask);
        let overfull = _mm_or_si128(
            _mm_andnot_si128(rows_mask, row_four),
            _mm_andnot_si128(columns_mask, column_four),
        );
        let impossible = _mm_or_si128(_mm_or_si128(short_rows, short_columns), overfull);
        // Lane 3 is not a band, and is bound to look short.
        if _mm_movemask_epi8(_mm_cmpeq_epi32(impossible, _mm_setzero_si128())) & 0xfff != 0xfff {
            return Err(Contradiction);
        }
        let row_exact = _mm_and_si128(_mm_andnot_si128(row_four, row_three), rows_mask);
        let row_full = _mm_and_si128(_mm_srli_epi32::<1>(row_three), rows_mask);
        let column_exact = _mm_and_si128(_mm_andnot_si128(column_four, column_three), columns_mask);
        let column_full = _mm_and_si128(_mm_srli_epi32::<3>(column_three), columns_mask);
        let any = _mm_or_si128(_mm_or_si128(row_exact, row_full), _mm_or_si128(column_exact, column_full));
        if _mm_testz_si128(any, any) != 0 {
            return Ok(0);
        }

        // Something may be deducible, for the digits which are not already certain.
        let (row_exact, row_full) = (both(row_exact), both(row_full));
        let (column_exact, column_full) = (both(column_exact), both(column_full));
        let mut changed = 0;
        for i in 0..5 {
            let x = load(&state.x, i);
            let m = load(&summary.rows, i);
            let c = load(&summary.columns, i);
            let undecided_rows = and(m, or(next_box(m), next_box2(m)));
            let undecided_columns = and(c, or(next_band(c), next_band2(c)));

            // A digit forced into a minirow leaves the other minirows of that box and
            // of that row. One shut out of a minirow just leaves it.
            let forced = and(undecided_rows, row_exact);
            let vacated_rows = or(
                or(or(next_row(forced), next_row2(forced)), or(next_box(forced), next_box2(forced))),
                and(undecided_rows, row_full),
            );
            // A digit forced into a column within a band leaves the other columns of
            // that box, and leaves the column in the other two bands.
            let forced = and(undecided_columns, column_exact);
            let vacated_columns = or(
                or(or(next_column(forced), next_column2(forced)), or(next_band(forced), next_band2(forced))),
                and(undecided_columns, column_full),
            );

            let cells = without(without(x, cells_of_minirows(vacated_rows)), cells_of_columns(vacated_columns));
            if _mm256_testc_si256(cells, x) == 0 {
                store(&mut state.x, i, cells);
                changed |= 1 << i;
            }
        }
        if changed != 0 {
        }
        Ok(changed)
    }

    /// Apply every deduction until none makes progress, cheapest first, starting from
    /// the given set of pairs which have changed. Returns whether the grid is solved.
    fn propagate(state: &mut State, mut dirty: u32, summary: &mut Summary) -> Result<bool, Contradiction> {
        loop {
            while dirty != 0 {
                let i = dirty.trailing_zeros() as usize;
                dirty &= dirty - 1;
                let filtered = filter(load(&state.x, i), REAL_LANES[i])?;
                store(&mut state.x, i, filtered);
            }
            let solved = loop {
                let (changed, unfiltered, solved) = settle(state)?;
                dirty = unfiltered;
                if unfiltered != 0 || changed == 0 {
                    break solved;
                }
            };
            if dirty != 0 {
                continue;
            }
            if solved {
                return Ok(true);
            }
            dirty = apply_triad_cardinality(state, summary)?;
            if dirty == 0 {
                return Ok(false);
            }
        }
    }
}

// -----------------------------------------------------------------------------------------
// Search
// -----------------------------------------------------------------------------------------

simd! {
    /// Pack the columns a digit can occupy in each of the three bands into one word, so
    /// that the minicolumns of stack `s` are the nine bits starting at `9 * s`, with
    /// that of band `b` and column `c` at bit `3 * b + c` among them.
    fn pack_columns(columns: &[u32; 4]) -> u32 {
        columns[0] | columns[1] << 3 | columns[2] << 6
    }

    fn search(state: &mut State, dirty: u32, limit: usize, count: &mut usize) {
        let mut summary = Summary { rows: [[0; 4]; 10], columns: [[0; 4]; 10] };
        match propagate(state, dirty, &mut summary) {
            Err(Contradiction) => return,
            Ok(true) => {
                *count += 1;
                return;
            }
            Ok(false) => {}
        }

        // Count the configurations of each digit in each band (entries 0-2) and each
        // stack (entries 3-5), by using the relevant nine bits to index a table.
        let mut counts = [[0u32; 9]; 6];
        let mut totals = [0u32; 6];
        for digit in 0..9 {
            let rows = &summary.rows[digit];
            let packed = pack_columns(&summary.columns[digit]);
            for i in 0..3 {
                let in_band = _pext_u32(rows[i], MINIROWS) as usize;
                let in_stack = (packed >> (9 * i) & 0o777) as usize;
                // SAFETY: both are nine bits, and the table has 512 entries.
                let (n_band, n_stack) = unsafe {
                    (*CONFIG_COUNT.get_unchecked(in_band) as u32, *CONFIG_COUNT.get_unchecked(in_stack) as u32)
                };
                counts[i][digit] = n_band;
                counts[3 + i][digit] = n_stack;
                totals[i] += n_band;
                totals[3 + i] += n_stack;
            }
        }

        // Branch within the band or stack with the fewest configurations in total, on
        // the digit with the fewest. Propagation settles every cell before all of them
        // are down to one configuration, so there is always something to branch on.
        let (mut best, mut best_total) = (usize::MAX, u32::MAX);
        for i in 0..6 {
            if totals[i] > 9 && totals[i] < best_total {
                (best, best_total) = (i, totals[i]);
            }
        }
        if best == usize::MAX {
            *count += 1;
            return;
        }
        let (mut digit, mut n) = (0, u32::MAX);
        for d in 0..9 {
            let candidate = counts[best][d];
            if candidate >= 2 && candidate < n {
                (digit, n) = (d, candidate);
            }
        }

        // Try each of the digit's configurations there in turn, the last of them in
        // place rather than on a copy.
        let packed = pack_columns(&summary.columns[digit]);
        let positions =
            if best < 3 { _pext_u32(summary.rows[digit][best], MINIROWS) } else { packed >> (9 * (best - 3)) };
        let mut configs = CONFIGS_WITHIN[(positions & 0o777) as usize] as u32;
        loop {
            let config = configs.trailing_zeros() as usize;
            configs &= configs - 1;
            if configs == 0 {
                assume_config(state, best, digit, config);
                search(state, 1 << (digit / 2), limit, count);
                return;
            }
            let mut child = *state;
            assume_config(&mut child, best, digit, config);
            search(&mut child, 1 << (digit / 2), limit, count);
            if *count >= limit {
                return;
            }
        }
    }

    /// Restrict a digit to one configuration within a band (0-2) or stack (3-5).
    fn assume_config(state: &mut State, band_or_stack: usize, digit: usize, config: usize) {
        if band_or_stack < 3 {
            state.x[digit][band_or_stack] &= CONFIG_ROW_CELLS[config];
        } else {
            // Within the stack, each band keeps only the configuration's column.
            let shift = 9 * (band_or_stack - 3);
            for band in 0..3 {
                let column = CONFIGS[config] >> (3 * band) & 0o7;
                let columns = COLUMNS & !(0o7 << shift) | column << shift;
                state.x[digit][band] &= cells_of_columns_1(columns);
            }
        }
    }

    /// A mask with a bit set for each cell of the grid which holds a clue.
    fn clue_mask(sudoku: &Sudoku) -> u128 {
        let digits = sudoku.0.as_ptr();
        // SAFETY: the three loads cover bytes 0..32, 32..64 and 64..80 of the 81 bytes.
        let (empty_0, empty_32, empty_64) = unsafe {
            (
                _mm256_cmpeq_epi8(_mm256_loadu_si256(digits.cast()), _mm256_setzero_si256()),
                _mm256_cmpeq_epi8(_mm256_loadu_si256(digits.add(32).cast()), _mm256_setzero_si256()),
                _mm_cmpeq_epi8(_mm_loadu_si128(digits.add(64).cast()), _mm_setzero_si128()),
            )
        };
        let clues_0 = !(_mm256_movemask_epi8(empty_0) as u32) as u128;
        let clues_32 = !(_mm256_movemask_epi8(empty_32) as u32) as u128;
        let clues_64 = (!_mm_movemask_epi8(empty_64) as u32 & 0xffff) as u128;
        let clue_80 = (sudoku.0[80] != 0) as u128;
        clues_0 | clues_32 << 32 | clues_64 << 64 | clue_80 << 80
    }

    fn init(sudoku: &Sudoku) -> Option<State> {
        // For each digit and band: the cells where the digit is given, and the boxes
        // containing those cells. For each band: the cells where anything is given.
        let mut given = [[0u32; 4]; 10];
        let mut given_boxes = [[0u32; 4]; 10];
        let mut filled = [0u32; 3];

        let mut clues = clue_mask(sudoku);
        while clues != 0 {
            let cell = clues.trailing_zeros() as usize;
            clues &= clues - 1;
            let digit = (sudoku.0[cell] as usize).wrapping_sub(1);
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

        // A digit keeps the cells where it is given, and otherwise loses every filled
        // cell and every box where it is given.
        let mut state = State { x: [[0; 4]; 10] };
        for digit in 0..9 {
            for band in 0..3 {
                state.x[digit][band] = ALL & !filled[band] & !given_boxes[digit][band] | given[digit][band];
            }
        }
        Some(state)
    }

    /// Count the solutions of `sudoku`, stopping early once `limit` have been found. Only
    /// to be called if `is_supported()`.
    pub fn count_up_to(sudoku: &Sudoku, limit: usize) -> usize {
        let mut count = 0;
        if limit > 0 {
            if let Some(mut state) = init(sudoku) {
                search(&mut state, ALL_PAIRS, limit, &mut count);
            }
        }
        count
    }
}
