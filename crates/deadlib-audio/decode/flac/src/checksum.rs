//! FLAC CRC polynomials and RFC 1321 MD5, implemented without dependencies.
const fn crc8_table() -> [u8; 256] {
    let mut table = [0; 256];
    let mut i = 0;
    while i < 256 {
        let mut x = i as u8;
        let mut j = 0;
        while j < 8 {
            x = (x << 1) ^ if x & 0x80 != 0 { 7 } else { 0 };
            j += 1;
        }
        table[i] = x;
        i += 1;
    }
    table
}
const fn crc16_table() -> [u16; 256] {
    let mut table = [0; 256];
    let mut i = 0;
    while i < 256 {
        let mut x = (i as u16) << 8;
        let mut j = 0;
        while j < 8 {
            x = (x << 1) ^ if x & 0x8000 != 0 { 0x8005 } else { 0 };
            j += 1;
        }
        table[i] = x;
        i += 1;
    }
    table
}
const CRC8: [u8; 256] = crc8_table();
const CRC16: [u16; 256] = crc16_table();
pub fn crc8(data: &[u8]) -> u8 {
    data.iter().fold(0, |crc, &x| CRC8[(crc ^ x) as usize])
}
pub fn crc16(data: &[u8]) -> u16 {
    data.iter().fold(0, |crc, &x| {
        (crc << 8) ^ CRC16[((crc >> 8) as u8 ^ x) as usize]
    })
}

#[derive(Clone)]
pub struct Md5 {
    state: [u32; 4],
    buffer: [u8; 64],
    len: u64,
}
impl Default for Md5 {
    fn default() -> Self {
        Self {
            state: [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476],
            buffer: [0; 64],
            len: 0,
        }
    }
}
impl Md5 {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn update(&mut self, mut data: &[u8]) {
        let used = (self.len % 64) as usize;
        self.len = self.len.wrapping_add(data.len() as u64);
        if used != 0 {
            let n = data.len().min(64 - used);
            self.buffer[used..used + n].copy_from_slice(&data[..n]);
            data = &data[n..];
            if used + n != 64 {
                return;
            }
            compress(&mut self.state, &self.buffer);
        }
        while data.len() >= 64 {
            compress(&mut self.state, data[..64].try_into().unwrap());
            data = &data[64..];
        }
        self.buffer[..data.len()].copy_from_slice(data);
    }
    pub fn finalize(mut self) -> [u8; 16] {
        let bit_len = self.len.wrapping_mul(8);
        self.update(&[0x80]);
        let zeros = (56 + 64 - (self.len % 64)) % 64;
        self.update(&[0; 64][..zeros as usize]);
        self.update(&bit_len.to_le_bytes());
        let mut result = [0; 16];
        for (chunk, word) in result.as_chunks_mut::<4>().0.iter_mut().zip(self.state) {
            chunk.copy_from_slice(&word.to_le_bytes());
        }
        result
    }
}
fn compress(state: &mut [u32; 4], block: &[u8; 64]) {
    const K: [u32; 64] = [
        0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613,
        0xfd469501, 0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193,
        0xa679438e, 0x49b40821, 0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d,
        0x02441453, 0xd8a1e681, 0xe7d3fbc8, 0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed,
        0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a, 0xfffa3942, 0x8771f681, 0x6d9d6122,
        0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70, 0x289b7ec6, 0xeaa127fa,
        0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665, 0xf4292244,
        0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
        0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb,
        0xeb86d391,
    ];
    const S: [[u32; 4]; 4] = [
        [7, 12, 17, 22],
        [5, 9, 14, 20],
        [4, 11, 16, 23],
        [6, 10, 15, 21],
    ];
    let mut words = [0u32; 16];
    for (word, bytes) in words.iter_mut().zip(block.as_chunks::<4>().0) {
        *word = u32::from_le_bytes(*bytes);
    }
    let [mut a, mut b, mut c, mut d] = *state;
    for i in 0..64 {
        let (f, g) = match i {
            0..=15 => ((b & c) | (!b & d), i),
            16..=31 => ((d & b) | (!d & c), (5 * i + 1) % 16),
            32..=47 => (b ^ c ^ d, (3 * i + 5) % 16),
            _ => (c ^ (b | !d), (7 * i) % 16),
        };
        let next = b.wrapping_add(
            a.wrapping_add(f)
                .wrapping_add(K[i])
                .wrapping_add(words[g])
                .rotate_left(S[i / 16][i % 4]),
        );
        a = d;
        d = c;
        c = b;
        b = next;
    }
    for (word, value) in state.iter_mut().zip([a, b, c, d]) {
        *word = word.wrapping_add(value);
    }
}
