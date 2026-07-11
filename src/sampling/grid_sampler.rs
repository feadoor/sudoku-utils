use itertools::Itertools;
use rand::{Rng, RngExt};
use rand::seq::{IndexedRandom, SliceRandom};

use crate::utils::sudoku::Sudoku;

type Box = [[u8; 3]; 3];

const PERM_3: [[usize; 3]; 6] = [
    [0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0],
];

const COMBS_6_3: [[usize; 3]; 20] = [
    [0, 1, 2], [0, 1, 3], [0, 1, 4], [0, 1, 5],
    [0, 2, 3], [0, 2, 4], [0, 2, 5],
    [0, 3, 4], [0, 3, 5],
    [0, 4, 5],
    [1, 2, 3], [1, 2, 4], [1, 2, 5],
    [1, 3, 4], [1, 3, 5],
    [1, 4, 5],
    [2, 3, 4], [2, 3, 5],
    [2, 4, 5],
    [3, 4, 5],
];

fn transpose(bx: &Box) -> Box {
    (0 .. 3).map(|idx|
        (0 .. 3).map(|jdx| bx[jdx][idx])
            .collect_array().unwrap()
    ).collect_array().unwrap()
}

fn hstack(box1: &Box, box2: &Box) -> [[u8; 6]; 3] {
    [
        [box1[0][0], box1[0][1], box1[0][2], box2[0][0], box2[0][1], box2[0][2]],
        [box1[1][0], box1[1][1], box1[1][2], box2[1][0], box2[1][1], box2[1][2]],
        [box1[2][0], box1[2][1], box1[2][2], box2[2][0], box2[2][1], box2[2][2]],
    ]
}

fn permute_3(row: &[u8; 3], box_v: &Box, rng: &mut impl Rng) -> Option<[u8; 3]> {
    let col_has = |jdx: usize, val: u8| box_v[0][jdx] == val || box_v[1][jdx] == val || box_v[2][jdx] == val;
    let valid: Option<[[u8; 3]; 2]> = PERM_3.iter()
        .map(|perm| [row[perm[0]], row[perm[1]], row[perm[2]]])
        .filter(|candidate| (0 .. 3).all(|jdx| !col_has(jdx, candidate[jdx])))
        .collect_array();
    valid.and_then(|rows| rows.choose(rng).copied())
}

fn finish_box(band: &[[u8; 6]; 3], box_v: &Box, rng: &mut impl Rng) -> Option<Box> {
    let mut bx = [[0; 3]; 3];
    for idx in 0 .. 3 {
        let missing: [u8; 3] = (1 ..= 9).filter(|d| !band[idx].contains(d)).collect_array().unwrap();
        bx[idx] = permute_3(&missing, box_v, rng)?;
    }
    Some(bx)
}

fn random_box(rng: &mut impl Rng) -> Box {
    let mut digits = [1, 2, 3, 4, 5, 6, 7, 8, 9];
    digits.shuffle(rng);
    [[digits[0], digits[1], digits[2]], [digits[3], digits[4], digits[5]], [digits[6], digits[7], digits[8]]]
}

fn attempt(rng: &mut impl Rng) -> Option<Sudoku> {
    // Fill the diagonal boxes with entirely random boxes
    let (box1, box5, box9) = (random_box(rng), random_box(rng), random_box(rng));

    // Box 2
    // Begin by selecting three random digits for its row 1 from those in rows 2 and 3 of box 1
    let pool = [box1[1][0], box1[1][1], box1[1][2], box1[2][0], box1[2][1], box1[2][2]];
    let combination = COMBS_6_3.choose(rng).unwrap();
    let row1 = [pool[combination[0]], pool[combination[1]], pool[combination[2]]];

    // Forced leftovers: box 1 row 2 -> row 3, box 1 row 3 -> row 2
    let mut row2: Vec<_> = box1[2].iter().copied().filter(|d| !row1.contains(d)).collect();
    let mut row3: Vec<_> = box1[1].iter().copied().filter(|d| !row1.contains(d)).collect();

    let r = rng.random_range(0 .. 3);
    let long_is_row_2 = row2.len() > row3.len();
    let long_len = row2.len().max(row3.len());

    if long_len == 3 {
        // 3/0 split: reject 2 of 3 branches to preserve uniformity
        if r != 0 { return None; }
        if long_is_row_2 { row3.extend_from_slice(&box1[0]); }
        else { row2.extend_from_slice(&box1[0]); }
    } else {
        // 2/1 split: `r` chooses which of box 1's row 1 digits joins the long row
        let add_long = box1[0][r];
        let add_short_1 = box1[0][(r + 1) % 3];
        let add_short_2 = box1[0][(r + 2) % 3];
        if long_is_row_2 { row2.push(add_long); row3.push(add_short_1); row3.push(add_short_2); }
        else { row3.push(add_long); row2.push(add_short_1); row2.push(add_short_2); }
    }
    let row2 = row2.as_array().unwrap();
    let row3 = row3.as_array().unwrap();

    // Permute the three rows at random, avoiding vertical clashes
    let row1 = permute_3(&row1, &box5, rng)?;
    let row2 = permute_3(&row2, &box5, rng)?;
    let row3 = permute_3(&row3, &box5, rng)?;
    let box2 = [row1, row2, row3];

    // Box 3 = finish_box(hstack(box1, box2), box9)
    let box3 = finish_box(&hstack(&box1, &box2), &box9, rng)?;

    // Box 6 = finish_box(hstack(box3^T, box9^T), box5^T)^T
    let box6 = transpose(&finish_box(&hstack(&transpose(&box3), &transpose(&box9)), &transpose(&box5), rng)?);

    // Box 8 = finish_box(hstack(box2^T, box5^T), box9^T)^T
    let box8 = transpose(&finish_box(&hstack(&transpose(&box2), &transpose(&box5)), &transpose(&box9), rng)?);

    // Box 4 = finish_box(hstack(box5, box6), box1)
    let box4 = finish_box(&hstack(&box5, &box6), &box1, rng)?;

    // Box 7 is fully forced, but we must check for contradictions
    let mut box7 = [[0; 3]; 3];
    for d in 1 ..= 9 {
        for row in 0 .. 3 {
            if !box8[row].contains(&d) && !box9[row].contains(&d) {
                for col in 0..3 {
                    if !(0 .. 3).any(|rr| box1[rr][col] == d || box4[rr][col] == d) {
                        if box7[row][col] != 0 { return None; }
                        box7[row][col] = d;
                    }
                }
            }
        }
    }

    // Create the grid from the boxes
    Some(Sudoku([
        box1[0][0], box1[0][1], box1[0][2], box2[0][0], box2[0][1], box2[0][2], box3[0][0], box3[0][1], box3[0][2],
        box1[1][0], box1[1][1], box1[1][2], box2[1][0], box2[1][1], box2[1][2], box3[1][0], box3[1][1], box3[1][2],
        box1[2][0], box1[2][1], box1[2][2], box2[2][0], box2[2][1], box2[2][2], box3[2][0], box3[2][1], box3[2][2],
        box4[0][0], box4[0][1], box4[0][2], box5[0][0], box5[0][1], box5[0][2], box6[0][0], box6[0][1], box6[0][2],
        box4[1][0], box4[1][1], box4[1][2], box5[1][0], box5[1][1], box5[1][2], box6[1][0], box6[1][1], box6[1][2],
        box4[2][0], box4[2][1], box4[2][2], box5[2][0], box5[2][1], box5[2][2], box6[2][0], box6[2][1], box6[2][2],
        box7[0][0], box7[0][1], box7[0][2], box8[0][0], box8[0][1], box8[0][2], box9[0][0], box9[0][1], box9[0][2],
        box7[1][0], box7[1][1], box7[1][2], box8[1][0], box8[1][1], box8[1][2], box9[1][0], box9[1][1], box9[1][2],
        box7[2][0], box7[2][1], box7[2][2], box8[2][0], box8[2][1], box8[2][2], box9[2][0], box9[2][1], box9[2][2],
    ]))
}

pub fn sample_solved_grid(rng: &mut impl Rng) -> Sudoku {
    loop {
        if let Some(grid) = attempt(rng) {
            return grid;
        }
    }
}
