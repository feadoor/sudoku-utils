use std::ops::{BitAnd, BitAndAssign, BitXorAssign};

use num_traits::{PrimInt, Unsigned, WrappingAdd, WrappingSub};

pub struct BitIter<T>(T);

impl<T: PrimInt + Unsigned> BitIter<T> {
    pub fn new(val: T) -> Self {
        Self(val)
    }

    pub fn peek(&self) -> Option<u32> {
        (self.0 != T::zero()).then(|| self.0.trailing_zeros())
    }
}

impl<T: PrimInt + Unsigned + WrappingSub + BitAndAssign> Iterator for BitIter<T> {
    type Item = u32;

    fn next(&mut self) -> Option<Self::Item> {
        self.peek().map(|res| { self.0 &= self.0.wrapping_sub(&T::one()); res })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let sz = self.0.count_ones();
        (sz as usize, Some(sz as usize))
    }

    fn count(self) -> usize {
        self.0.count_ones() as usize
    }
}

pub struct MaskIter<T>(T);

impl<T: PrimInt + Unsigned + BitAnd + WrappingAdd> MaskIter<T> {
    pub fn new(val: T) -> Self {
        Self(val)
    }
    
    pub fn peek(&self) -> Option<T> {
        (self.0 != T::zero()).then(|| self.0 & ((!self.0).wrapping_add(&T::one())))
    }
}

impl<T: PrimInt + Unsigned + BitAnd + BitXorAssign + WrappingAdd> Iterator for MaskIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.peek().map(|res| { self.0 ^= res; res })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let sz = self.0.count_ones();
        (sz as usize, Some(sz as usize))
    }

    fn count(self) -> usize {
        self.0.count_ones() as usize
    }
}
