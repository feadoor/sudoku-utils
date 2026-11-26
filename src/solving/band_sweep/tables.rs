//! Constants and tables shared by the vector and scalar solvers.

// Both solvers hold, for each digit and each band, a mask of the cells of the band which can
// hold the digit. Within a mask, the 27 cells are arranged box by box: bit
// `9 * box + 3 * row + column`, where `box`, `row` and `column` are each 0-2 and `column` is
// relative to the box. So each box is a 9-bit field, each minirow is three adjacent bits, and
// a column is three bits spaced three apart within one field.

pub(super) const ALL: u32 = 0o_777_777_777;
/// One bit per minirow of a band, at the position of the minirow's first cell.
pub(super) const MINIROWS: u32 = 0o_111_111_111;
/// One bit per column of a band, at the position of the column's first cell.
pub(super) const COLUMNS: u32 = 0o_007_007_007;
/// The lowest bit, the highest bit, and all but the highest bit of each box's field.
pub(super) const FIELD_LOW: u32 = 0o_001_001_001;
pub(super) const FIELD_HIGH: u32 = 0o_400_400_400;
pub(super) const FIELD_REST: u32 = 0o_377_377_377;

// Several tables are indexed by a set of nine "positions", three groups of three. For a
// digit within a band, position `3 * box + row` is a minirow. For a digit within a stack,
// position `3 * band + column` is a minicolumn. Either way the digit must end up with one
// position from each group, and one at each place within a group: a "configuration".

/// The six configurations.
pub(super) const CONFIGS: [u32; 6] = build_configs();
/// For each set of positions, the number of configurations within it.
pub(super) static CONFIG_COUNT: [u8; 512] = build_config_count();
/// For each set of positions, the configurations within it, as a set of indices into
/// `CONFIGS`.
pub(super) static CONFIGS_WITHIN: [u8; 512] = build_configs_within();
/// The cells of a band which make up each configuration of minirows.
pub(super) const CONFIG_ROW_CELLS: [u32; 6] = build_config_row_cells();
/// For each cell of the grid, its band and its bit within the band's mask.
pub(super) static CELL_POSITION: [(u8, u8); 81] = build_cell_position();

const fn build_configs() -> [u32; 6] {
    // The arrangement which puts group `k` at place `k`, and the one which puts it at place
    // `2 - k`, each moved round by zero, one and two groups.
    let mut configs = [0; 6];
    let mut config = 0;
    while config < 6 {
        let mut base = 0;
        let mut group = 0;
        while group < 3 {
            let place = if config < 3 { group } else { 2 - group };
            base |= 1 << (3 * group + place);
            group += 1;
        }
        let shift = 3 * (config % 3);
        configs[config] = (base << shift | base >> (9 - shift)) & 0o777;
        config += 1;
    }
    configs
}

const fn build_config_count() -> [u8; 512] {
    let mut counts = [0; 512];
    let mut set = 0;
    while set < 512 {
        let mut config = 0;
        while config < 6 {
            if set as u32 & CONFIGS[config] == CONFIGS[config] {
                counts[set] += 1;
            }
            config += 1;
        }
        set += 1;
    }
    counts
}

const fn build_configs_within() -> [u8; 512] {
    let mut within = [0; 512];
    let mut set = 0;
    while set < 512 {
        let mut config = 0;
        while config < 6 {
            if set as u32 & CONFIGS[config] == CONFIGS[config] {
                within[set] |= 1 << config;
            }
            config += 1;
        }
        set += 1;
    }
    within
}

/// The cells of a set of minirows given as positions: position `p` is the three cells
/// starting at bit `3 * p`.
pub(super) const fn row_cells_of_positions(positions: u32) -> u32 {
    let mut cells = 0;
    let mut position = 0;
    while position < 9 {
        if positions >> position & 1 != 0 {
            cells |= 0o7 << (3 * position);
        }
        position += 1;
    }
    cells
}

const fn build_config_row_cells() -> [u32; 6] {
    let mut cells = [0; 6];
    let mut config = 0;
    while config < 6 {
        cells[config] = row_cells_of_positions(CONFIGS[config]);
        config += 1;
    }
    cells
}

const fn build_cell_position() -> [(u8, u8); 81] {
    let mut table = [(0, 0); 81];
    let mut cell = 0;
    while cell < 81 {
        let (row, column) = (cell / 9, cell % 9);
        table[cell] = ((row / 3) as u8, (9 * (column / 3) + 3 * (row % 3) + column % 3) as u8);
        cell += 1;
    }
    table
}
