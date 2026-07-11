use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign, Not, Shl};

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Bitmask<T>(T);

pub struct BitIter<T>(T);
pub struct MaskIter<T>(T);
pub struct SubsetIter<T>(T, Option<T>);

macro_rules! bitmask_impl {
    ($($t:ty)*) => {$(
        #[allow(dead_code)]
        impl Bitmask<$t> {
            #[inline(always)]
            pub const fn from(val: $t) -> Self {
                Self(val)
            }

            #[inline(always)]
            pub fn empty() -> Self {
                Self(0)
            }
            
            #[inline(always)]
            pub fn singleton<S>(bit: S) -> Self where $t: Shl<S, Output=$t> {
                Self(1 << bit)
            }

            pub fn from_iter<I, S>(bits: I) -> Self where $t: Shl<S, Output=$t>, I: Iterator<Item = S> {
                Self(bits.fold(0, |acc, x| acc | (1 << x)))
            }

            #[inline(always)]
            pub fn as_bit_iter(&self) -> BitIter<$t> {
                BitIter::<$t>::from(self.0)
            }

            #[inline(always)]
            pub fn as_mask_iter(&self) -> MaskIter<$t> {
                MaskIter::<$t>::from(self.0)
            }

            #[inline(always)]
            pub fn as_subset_iter(&self) -> SubsetIter<$t> {
                SubsetIter::<$t>::from(self.0)
            }

            #[inline(always)]
            pub fn is_empty(&self) -> bool {
                self.0 == 0
            }

            #[inline(always)]
            pub fn is_not_empty(&self) -> bool {
                self.0 != 0
            }

            #[inline(always)]
            pub fn contains<S>(&self, bit: S) -> bool where $t: Shl<S, Output=$t> {
                (self.0 & (1 << bit)) != 0
            }

            #[inline(always)]
            pub fn count_ones(&self) -> u32 {
                self.0.count_ones()
            }

            #[inline(always)]
            pub fn set<S>(&mut self, bit: S) where $t: Shl<S, Output=$t> {
                self.0 |= (1 << bit);
            }

            #[inline(always)]
            pub fn unset<S>(&mut self, bit: S) where $t: Shl<S, Output=$t> {
                self.0 &= !(1 << bit);
            }

            #[inline(always)]
            pub fn max(&self) -> Option<usize> {
                if self.0 == 0 {
                    None
                } else {
                    Some((<$t>::BITS - 1 - self.0.leading_zeros()) as usize)
                }
            }
        }

        impl Not for Bitmask<$t> {
            type Output = Self;
            #[inline(always)]
            fn not(self) -> Self {
                Self(!self.0)
            }
        }

        impl BitAnd for Bitmask<$t> {
            type Output = Self;

            #[inline(always)]
            fn bitand(self, rhs: Self) -> Self {
                Self(self.0 & rhs.0)
            }
        }

        impl BitOr for Bitmask<$t> {
            type Output = Self;

            #[inline(always)]
            fn bitor(self, rhs: Self) -> Self {
                Self(self.0 | rhs.0)
            }
        }

        impl BitXor for Bitmask<$t> {
            type Output = Self;

            #[inline(always)]
            fn bitxor(self, rhs: Self) -> Self {
                Self(self.0 ^ rhs.0)
            }
        }

        impl BitAndAssign for Bitmask<$t> {
            #[inline(always)]
            fn bitand_assign(&mut self, rhs: Self) {
                self.0 &= rhs.0;
            }
        }

        impl BitOrAssign for Bitmask<$t> {
            #[inline(always)]
            fn bitor_assign(&mut self, rhs: Self) {
                self.0 |= rhs.0;
            }
        }

        impl BitXorAssign for Bitmask<$t> {
            #[inline(always)]
            fn bitxor_assign(&mut self, rhs: Self) {
                self.0 ^= rhs.0;
            }
        }
    )*}
}

macro_rules! bit_iter_impl {
    ($($t:ty)*) => {$(
        #[allow(dead_code)]
        impl BitIter<$t> {
            #[inline(always)]
            pub fn from(val: $t) -> Self {
                Self(val)
            }

            #[inline(always)]
            pub fn peek(&self) -> Option<usize> {
                if self.0 != 0 {
                    Some(self.0.trailing_zeros() as usize)
                } else {
                    None
                }
            }
        }

        impl Iterator for BitIter<$t> {
            type Item = usize;

            #[inline(always)]
            fn next(&mut self) -> Option<Self::Item> {
                if self.0 != 0 {
                    let result = self.0.trailing_zeros();
                    self.0 &= self.0.wrapping_sub(1);
                    Some(result as usize)
                } else {
                    None
                }
            }

            #[inline(always)]
            fn size_hint(&self) -> (usize, Option<usize>) {
                let result = self.0.count_ones() as usize;
                (result, Some(result))
            }
        }

        impl ExactSizeIterator for BitIter<$t> {
            #[inline(always)]
            fn len(&self) -> usize {
                self.0.count_ones() as usize
            }
        }
    )*}
}

macro_rules! mask_iter_impl {
    ($($t:ty)*) => {$(
        #[allow(dead_code)]
        impl MaskIter<$t> {
            #[inline(always)]
            pub fn from(val: $t) -> Self {
                Self(val)
            }

            #[inline(always)]
            pub fn peek(&self) -> Option<$t> {
                if self.0 != 0 {
                    Some(self.0 & ((!self.0).wrapping_add(1)))
                } else {
                    None
                }
            }
        }

        impl Iterator for MaskIter<$t> {
            type Item = $t;

            #[inline(always)]
            fn next(&mut self) -> Option<Self::Item> {
                if self.0 != 0 {
                    let result = self.0 & ((!self.0).wrapping_add(1));
                    self.0 ^= result;
                    Some(result)
                } else {
                    None
                }
            }

            #[inline(always)]
            fn size_hint(&self) -> (usize, Option<usize>) {
                let result = self.0.count_ones() as usize;
                (result, Some(result))
            }
        }

        impl ExactSizeIterator for MaskIter<$t> {
            #[inline(always)]
            fn len(&self) -> usize {
                self.0.count_ones() as usize
            }
        }
    )*}
}

macro_rules! subset_iter_impl {
    ($($t:ty)*) => {$(
        #[allow(dead_code)]
        impl SubsetIter<$t> {
            #[inline(always)]
            pub fn from(val: $t) -> Self {
                Self(val, Some(val))
            }
        }

        impl Iterator for SubsetIter<$t> {
            type Item = Bitmask<$t>;

            #[inline(always)]
            fn next(&mut self) -> Option<Self::Item> {
                if let Some(subset) = self.1 {
                    if subset == 0 { self.1 = None; }
                    else { self.1 = Some((subset - 1) & self.0); }
                    Some(Bitmask::<$t>::from(subset))
                } else {
                    None
                }
            }
        }
    )*}
}

bitmask_impl! { u8 u16 u32 u64 u128 usize }
bit_iter_impl! { u8 u16 u32 u64 u128 usize }
mask_iter_impl! { u8 u16 u32 u64 u128 usize }
subset_iter_impl! { u8 u16 u32 u64 u128 usize }
