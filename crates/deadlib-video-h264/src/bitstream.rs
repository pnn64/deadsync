/// Bitstream reader for parsing H.264 NAL unit RBSP data.
/// Reads bits left-to-right (MSB first).
///
/// Reads can extend into virtual zero padding. End-of-data checks and
/// the CABAC reader use the actual RBSP length.
pub struct BitstreamReader<'a> {
    data: &'a [u8],
    byte_offset: usize,
    bit_offset: u8, // 0-7, bits already consumed in current byte
}

/// Maximum overread allowed for the end-of-slice checks in the CAVLC path.
const PADDING: usize = 128;

impl<'a> BitstreamReader<'a> {
    pub fn new(rbsp: &'a [u8]) -> Self {
        Self {
            data: rbsp,
            byte_offset: 0,
            bit_offset: 0,
        }
    }

    /// Read a single bit or a zero from the virtual padding.
    #[inline(always)]
    pub fn read_bit(&mut self) -> Result<u8, &'static str> {
        let byte = match self.data.get(self.byte_offset) {
            Some(&byte) => byte,
            None if self.byte_offset - self.data.len() < PADDING => 0,
            None => return Err("end of bitstream"),
        };
        let bit = (byte >> (7 - self.bit_offset)) & 1;
        self.bit_offset += 1;
        if self.bit_offset == 8 {
            self.bit_offset = 0;
            self.byte_offset += 1;
        }
        Ok(bit)
    }

    /// Read `n` bits (up to 32) as a u32.
    pub fn read_bits(&mut self, n: u8) -> Result<u32, &'static str> {
        let mut val: u32 = 0;
        for _ in 0..n {
            val = (val << 1) | self.read_bit()? as u32;
        }
        Ok(val)
    }

    /// Peek at the next `n` bits without consuming them. Zero-pads if fewer bits remain.
    pub fn peek_bits(&self, n: u8) -> u32 {
        let mut val: u32 = 0;
        let mut byte_offset = self.byte_offset;
        let mut bit_offset = self.bit_offset;
        for _ in 0..n {
            let byte = self.data.get(byte_offset).copied().unwrap_or(0);
            val = (val << 1) | u32::from((byte >> (7 - bit_offset)) & 1);
            bit_offset += 1;
            if bit_offset == 8 {
                bit_offset = 0;
                byte_offset += 1;
            }
        }
        val
    }

    /// Advance position by `n` bits without reading them.
    pub fn skip_bits(&mut self, n: u8) {
        let total = self.bit_offset as usize + n as usize;
        self.byte_offset += total / 8;
        self.bit_offset = (total % 8) as u8;
    }

    /// Read an unsigned Exp-Golomb coded value (ue(v)).
    pub fn read_ue(&mut self) -> Result<u32, &'static str> {
        let mut leading_zeros: u32 = 0;
        while self.read_bit()? == 0 {
            leading_zeros += 1;
            if leading_zeros > 31 {
                return Err("exp-golomb overflow");
            }
        }
        if leading_zeros == 0 {
            return Ok(0);
        }
        let suffix = self.read_bits(leading_zeros as u8)?;
        Ok((1 << leading_zeros) - 1 + suffix)
    }

    /// Read a truncated Exp-Golomb coded value (te(v)).
    /// When max == 1, reads a single bit: bit=0 → value=1, bit=1 → value=0.
    /// When max > 1, reads a standard ue(v).
    pub fn read_te(&mut self, max: u32) -> Result<u32, &'static str> {
        if max > 1 {
            self.read_ue()
        } else {
            // Spec 9.1: value = 1 - bit
            Ok(1 - self.read_bit()? as u32)
        }
    }

    /// Read a signed Exp-Golomb coded value (se(v)).
    pub fn read_se(&mut self) -> Result<i32, &'static str> {
        let code = self.read_ue()?;
        let val = code.div_ceil(2) as i32;
        if code % 2 == 0 {
            Ok(-val)
        } else {
            Ok(val)
        }
    }

    /// Returns true if there is more RBSP data before the trailing bits.
    /// The RBSP trailing bits are: a stop bit (1) followed by zero bits to byte-align.
    pub fn more_rbsp_data(&self) -> bool {
        if self.byte_offset >= self.data.len() {
            return false;
        }

        // Find the position of the last non-zero byte in the actual data
        let mut last_nz = self.data.len();
        while last_nz > self.byte_offset && self.data[last_nz - 1] == 0 {
            last_nz -= 1;
        }
        if last_nz <= self.byte_offset {
            return false;
        }

        // If we're before the last non-zero byte, there's definitely more data
        if self.byte_offset < last_nz - 1 {
            return true;
        }

        // We're in the last non-zero byte. The RBSP stop bit is the lowest set bit.
        // Everything above it (towards MSB) is data. Check if there are unconsumed
        // data bits above the stop bit.
        let byte = self.data[self.byte_offset];
        let stop_bit = byte.trailing_zeros();
        let data_bits_in_byte = 7 - stop_bit as u8;
        self.bit_offset < data_bits_in_byte
    }

    /// Advance to the next byte boundary.
    pub fn align_to_byte(&mut self) {
        if self.bit_offset != 0 {
            self.bit_offset = 0;
            self.byte_offset += 1;
        }
    }

    /// Move the read cursor to a `(byte_offset, bit_offset)` previously
    /// returned by [`position`](Self::position). Offsets past the end of the
    /// data are clamped, so a stale position cannot cause out-of-bounds reads.
    pub fn set_position(&mut self, pos: (usize, u8)) {
        self.byte_offset = pos.0.min(self.data.len());
        self.bit_offset = pos.1 & 7;
    }

    pub fn position(&self) -> (usize, u8) {
        (self.byte_offset, self.bit_offset)
    }

    /// Get the byte-aligned position for CABAC initialization.
    /// Aligns to the next byte boundary and returns (byte_offset, reference to data).
    pub fn cabac_start(&mut self) -> (usize, &'a [u8]) {
        // CABAC starts after cabac_alignment_one_bit + byte alignment
        // The slice header is followed by a 1-bit then byte-aligned CABAC data
        if self.bit_offset != 0 {
            self.bit_offset = 0;
            self.byte_offset += 1;
        }
        (self.byte_offset, self.data)
    }

    pub fn bits_remaining(&self) -> usize {
        if self.byte_offset >= self.data.len() {
            return 0;
        }
        (self.data.len() - self.byte_offset) * 8 - self.bit_offset as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_bits() {
        let data = [0b1010_0011, 0b1100_0000];
        let mut r = BitstreamReader::new(&data);
        assert_eq!(r.read_bits(4).unwrap(), 0b1010);
        assert_eq!(r.read_bits(4).unwrap(), 0b0011);
        assert_eq!(r.read_bits(2).unwrap(), 0b11);
    }

    #[test]
    fn test_read_ue() {
        // ue(0) = 1 -> code 0
        // ue(1) = 010 -> code 1
        // ue(2) = 011 -> code 2
        // ue(3) = 00100 -> code 3
        // Bit pattern: 1 010 011 00100 = 1010 0110 0100 = 0xA64
        let data = [0xA6, 0x40];
        let mut r = BitstreamReader::new(&data);
        assert_eq!(r.read_ue().unwrap(), 0);
        assert_eq!(r.read_ue().unwrap(), 1);
        assert_eq!(r.read_ue().unwrap(), 2);
        assert_eq!(r.read_ue().unwrap(), 3);
    }

    #[test]
    fn test_read_se() {
        // se mapping: code 0->0, 1->1, 2->-1, 3->2, 4->-2
        // ue values:   0       1       2       3       4
        // bits:        1      010     011    00100   00101
        // Combined: 1 010 011 00100 00101 = 1010 0110 0100 0010 1
        let data = [0xA6, 0x42, 0x80];
        let mut r = BitstreamReader::new(&data);
        assert_eq!(r.read_se().unwrap(), 0);
        assert_eq!(r.read_se().unwrap(), 1);
        assert_eq!(r.read_se().unwrap(), -1);
        assert_eq!(r.read_se().unwrap(), 2);
        assert_eq!(r.read_se().unwrap(), -2);
    }

    #[test]
    fn test_padding_allows_overread() {
        // After reading all real data, reads return 0 (from padding) rather than panicking
        let data = [0xFF];
        let mut r = BitstreamReader::new(&data);
        assert_eq!(r.bits_remaining(), 8);
        assert_eq!(r.peek_bits(12), 0xFF0);
        assert_eq!(r.read_bits(8).unwrap(), 0xFF);
        assert_eq!(r.bits_remaining(), 0);
        // Reading into padding returns 0 bits without panic
        assert_eq!(r.read_bits(8).unwrap(), 0);
    }

    /// Reading past the virtual padding must report end of input.
    #[test]
    fn test_read_past_buffer_returns_err() {
        let data = [0xAB; 4]; // 4 bytes = 32 bits
        let mut r = BitstreamReader::new(&data);
        for _ in 0..data.len() {
            assert_eq!(r.read_bits(8), Ok(0xAB));
        }
        for _ in 0..PADDING {
            assert_eq!(r.read_bits(8), Ok(0));
        }
        assert_eq!(r.read_bit(), Err("end of bitstream"));
    }
}

/// MSB-first bit writer for building parameter sets and slice headers in
/// tests (no emulation prevention: the result is RBSP).
#[cfg(test)]
#[derive(Default)]
pub(crate) struct BitWriter {
    bytes: alloc::vec::Vec<u8>,
    bits: usize,
}

#[cfg(test)]
impl BitWriter {
    /// `width` bits of `value`, MSB first.
    pub(crate) fn u(&mut self, width: u32, value: u32) -> &mut Self {
        for shift in (0..width).rev() {
            if self.bits.is_multiple_of(8) {
                self.bytes.push(0);
            }
            if (value >> shift) & 1 != 0 {
                *self.bytes.last_mut().unwrap() |= 0x80 >> (self.bits % 8);
            }
            self.bits += 1;
        }
        self
    }

    /// ue(v).
    pub(crate) fn ue(&mut self, value: u32) -> &mut Self {
        let code = value as u64 + 1;
        let width = 64 - code.leading_zeros();
        self.u(width - 1, 0);
        for shift in (0..width).rev() {
            self.u(1, ((code >> shift) & 1) as u32);
        }
        self
    }

    /// se(v).
    pub(crate) fn se(&mut self, value: i32) -> &mut Self {
        let code = if value > 0 {
            2 * value as u32 - 1
        } else {
            2 * value.unsigned_abs()
        };
        self.ue(code)
    }

    /// Zero bits up to the next byte boundary.
    pub(crate) fn align(&mut self) -> &mut Self {
        while !self.bits.is_multiple_of(8) {
            self.u(1, 0);
        }
        self
    }

    /// The RBSP: the bits written, a stop bit and zero bits to a byte.
    pub(crate) fn finish(&mut self) -> alloc::vec::Vec<u8> {
        self.u(1, 1);
        while !self.bits.is_multiple_of(8) {
            self.u(1, 0);
        }
        core::mem::take(&mut self.bytes)
    }
}
