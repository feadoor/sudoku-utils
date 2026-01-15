use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign, Not, Shl};

use crate::bit_iter::{BitIter, MaskIter};

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Bitmask<T>(T);

macro_rules! bitmask_impl {
    ($($t:ty)*) => {$(
        impl Bitmask<$t> {
            #[inline(always)]
            pub const fn from(val: $t) -> Self {
                Self(val)
            }

            #[inline(always)]
            pub const fn empty() -> Self {
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
            pub fn into_bit_iter(self) -> BitIter<$t> {
                BitIter::<$t>::from(self.0)
            }

            #[inline(always)]
            pub fn into_mask_iter(self) -> MaskIter<$t> {
                MaskIter::<$t>::from(self.0)
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

bitmask_impl! { u8 u16 u32 u64 u128 usize }
