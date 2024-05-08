pub struct Bitmap<'a>(&'a mut [u8]);

impl<'a> Bitmap<'a> {
    pub fn new(bmap: &'a mut [u8]) -> Self {
        Self(bmap)
    }

    pub fn is_bit_clear(&self, bit: usize) -> bool {
        self.0[bit / 8] & (1 << (bit % 8)) == 0
    }

    pub fn is_bit_set(&self, bit: usize) -> bool {
        !self.is_bit_clear(bit)
    }

    pub fn set_bit(&mut self, bit: usize) {
        self.0[bit / 8] |= 1 << (bit % 8);
    }

    pub fn clear_bit(&mut self, bit: usize) {
        self.0[bit / 8] &= !(1 << (bit % 8));
    }

    /// Find the first clear bit in the range `[start, end)``
    pub fn find_clear(&self, start: usize, end: usize) -> Option<usize> {
        for i in start..end {
            if self.is_bit_clear(i) {
                return Some(i);
            }
        }
        None
    }
}
