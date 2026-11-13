// ITGmania's _fallback/Scripts/00 init.lua replaces Lua's generator with
// RageUtil/RandomNumbers.cpp. Keep its MT19937 state local to this song VM;
// compilation and cached playback must not consume another song's stream.
#[derive(Debug, Clone)]
pub(crate) struct SongLuaRandom {
    words: [u32; 624],
    index: usize,
}

impl SongLuaRandom {
    pub(crate) fn seeded(seed: u32) -> Self {
        let mut words = [0; 624];
        words[0] = seed;
        for index in 1..words.len() {
            let previous = words[index - 1];
            words[index] = 1_812_433_253_u32
                .wrapping_mul(previous ^ (previous >> 30))
                .wrapping_add(index as u32);
        }
        Self { words, index: 624 }
    }

    fn next(&mut self) -> u32 {
        if self.index == self.words.len() {
            for index in 0..self.words.len() {
                let pair = (self.words[index] & 0x8000_0000)
                    | (self.words[(index + 1) % 624] & 0x7fff_ffff);
                self.words[index] = self.words[(index + 397) % 624]
                    ^ (pair >> 1)
                    ^ if pair % 2 == 0 { 0 } else { 0x9908_b0df };
            }
            self.index = 0;
        }
        let mut value = self.words[self.index];
        self.index += 1;
        value ^= value >> 11;
        value ^= (value << 7) & 0x9d2c_5680;
        value ^= (value << 15) & 0xefc6_0000;
        value ^ (value >> 18)
    }

    pub(crate) fn real(&mut self) -> f64 {
        let low = self.next();
        let high = self.next();
        // Match std::uniform_real_distribution<double> in the platform's
        // native C++ library, including MSVC's discarded low eleven bits.
        #[cfg(target_os = "windows")]
        {
            ((u64::from(low >> 11) + (u64::from(high) << 21)) as f64) / 9_007_199_254_740_992.0
        }
        #[cfg(not(target_os = "windows"))]
        {
            (f64::from(low) + f64::from(high) * 4_294_967_296.0) / 18_446_744_073_709_551_616.0
        }
    }

    pub(crate) fn integer(&mut self, lower: i32, upper: i32) -> f64 {
        let width = (i64::from(upper) - i64::from(lower) + 1) as u64;
        if width == 1_u64 << 32 {
            return (i64::from(lower) + i64::from(self.next())) as f64;
        }
        let width = width as u32;
        #[cfg(target_vendor = "apple")]
        let offset = {
            // libc++ uses independent_bits_engine and rejects above the bound.
            if width == 1 {
                return f64::from(lower);
            }
            let mask = u32::MAX >> width.wrapping_sub(1).leading_zeros();
            loop {
                let value = self.next() & mask;
                if value < width {
                    break value;
                }
            }
        };
        #[cfg(not(target_vendor = "apple"))]
        let offset = {
            // MSVC and libstdc++ use multiply-high with unbiased rejection.
            let threshold = width.wrapping_neg() % width;
            loop {
                let product = u64::from(self.next()) * u64::from(width);
                if product as u32 >= threshold {
                    break (product >> 32) as u32;
                }
            }
        };
        (i64::from(lower) + i64::from(offset)) as f64
    }
}

impl Default for SongLuaRandom {
    fn default() -> Self {
        Self::seeded(1)
    }
}
