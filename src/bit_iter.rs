pub struct BitIter<T>(T);
pub struct MaskIter<T>(T);

macro_rules! bit_iter_impl {
    ($($t:ty)*) => {$(
        impl BitIter<$t> {
            #[inline(always)]
            pub const fn from(val: $t) -> Self {
                Self(val)
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
        impl MaskIter<$t> {
            #[inline(always)]
            pub const fn from(val: $t) -> Self {
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

bit_iter_impl! { u8 u16 u32 u64 u128 usize }
mask_iter_impl! { u8 u16 u32 u64 u128 usize }
