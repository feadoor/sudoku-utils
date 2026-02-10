use std::ops::{Index, IndexMut};

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Sudoku(pub [u8; 81]);

impl Sudoku {
    #[inline(always)]
    pub fn empty() -> Self {
        Self([0; 81])
    }

    pub fn digits(&self) -> impl Iterator<Item = &u8> {
        self.0.iter()
    }
}

impl Index<usize> for Sudoku {
    type Output = u8;

    #[inline(always)]
    fn index(&self, index: usize) -> &u8 {
        &self.0[index]
    }
}

impl IndexMut<usize> for Sudoku {
    #[inline(always)]
    fn index_mut(&mut self, index: usize) -> &mut u8 {
        &mut self.0[index]
    }
}

impl Index<(usize, usize)> for Sudoku {
    type Output = u8;

    #[inline(always)]
    fn index(&self, (r, c): (usize, usize)) -> &u8 {
        &self.0[9 * r + c]
    }
}

impl IndexMut<(usize, usize)> for Sudoku {
    #[inline(always)]
    fn index_mut(&mut self, (r, c): (usize, usize)) -> &mut u8 {
        &mut self.0[9 * r + c]
    }
}
