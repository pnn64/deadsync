use crate::{Error, reserve};

/// Retains the last 511 main-data bytes, independently of frame/header bytes.
/// At most four 12-bit part2_3 fields are assembled in the reusable decode view.
pub struct Reservoir {
    history: [u8; 511],
    retained: usize,
    main: Vec<u8>,
}
impl Default for Reservoir {
    fn default() -> Self {
        Self::new()
    }
}
impl Reservoir {
    pub fn new() -> Self {
        Self {
            history: [0; 511],
            retained: 0,
            main: Vec::new(),
        }
    }
    pub fn clear(&mut self) {
        self.retained = 0;
        self.main.clear();
    }
    pub fn retained_bytes(&self) -> usize {
        self.retained
    }
    fn remember(&mut self, data: &[u8]) {
        if data.len() >= 511 {
            self.history.copy_from_slice(&data[data.len() - 511..]);
            self.retained = 511;
        } else {
            let drop = (self.retained + data.len()).saturating_sub(511);
            self.history.copy_within(drop..self.retained, 0);
            self.retained -= drop;
            self.history[self.retained..self.retained + data.len()].copy_from_slice(data);
            self.retained += data.len();
        }
    }
    /// Uses the frame's payload directly when it needs no history prefix.
    /// The public copying API keeps its independent payload lifetime.
    pub(crate) fn push_frame<'a>(
        &'a mut self,
        back: usize,
        payload: &'a [u8],
        required_bits: usize,
    ) -> Result<&'a [u8], Error> {
        if back == 0 && required_bits <= 4 * 4095 {
            let bytes = required_bits.div_ceil(8);
            if bytes <= payload.len() {
                self.main.clear();
                self.remember(payload);
                return Ok(&payload[..bytes]);
            }
        }
        self.push(back, payload, required_bits)
    }
    /// Consumes one complete frame's main-data payload. Missing-reservoir and
    /// truncated-main-data errors still retain this payload for resynchronization.
    /// Allocation/limit errors do not consume it. Headers and side info stay out.
    pub fn push(
        &mut self,
        back: usize,
        payload: &[u8],
        required_bits: usize,
    ) -> Result<&[u8], Error> {
        if back > 511 || required_bits > 4 * 4095 {
            return Err(Error::LimitExceeded("Layer III main data"));
        }
        let bytes = required_bits.div_ceil(8);
        self.main.clear();
        reserve(&mut self.main, bytes)?;
        if back > self.retained {
            let error = Error::MissingReservoir {
                needed: back,
                available: self.retained,
            };
            self.remember(payload);
            return Err(error);
        }
        if bytes > back.saturating_add(payload.len()) {
            self.remember(payload);
            return Err(Error::Truncated);
        }
        let prefix = bytes.min(back);
        self.main
            .extend_from_slice(&self.history[self.retained - back..self.retained - back + prefix]);
        self.main.extend_from_slice(&payload[..bytes - prefix]);
        self.remember(payload);
        Ok(&self.main)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_payload_is_borrowed_and_retained_for_following_frames() {
        let mut reservoir = Reservoir::new();
        let payload = [0x73, 0x42, 0x98, 0xab];
        let view = reservoir.push_frame(0, &payload, 13).unwrap();
        assert_eq!(view, &payload[..2]);
        assert_eq!(view.as_ptr(), payload.as_ptr());
        assert_eq!(reservoir.retained_bytes(), payload.len());
        assert_eq!(
            reservoir.push_frame(3, &[0xee], 32).unwrap(),
            [0x42, 0x98, 0xab, 0xee]
        );
    }

    #[test]
    fn borrowed_frame_views_preserve_copying_api_values_errors_and_history() {
        let mut copied = Reservoir::new();
        let mut borrowed = Reservoir::new();
        for size in [0, 1, 7, 31, 255, 511, 512, 1024] {
            let payload: Vec<_> = (0..size).map(|n| (n * 37 + size) as u8).collect();
            for back in [0, 1, 7, 255, 511, 512, usize::MAX] {
                for bits in [0, 1, 7, 8, 9, 4095, 16380, 16381, usize::MAX] {
                    let expected = copied.push(back, &payload, bits).map(<[u8]>::to_vec);
                    let actual = borrowed
                        .push_frame(back, &payload, bits)
                        .map(<[u8]>::to_vec);
                    assert_eq!(actual, expected, "size={size} back={back} bits={bits}");
                    assert_eq!(borrowed.retained, copied.retained);
                    assert_eq!(borrowed.history, copied.history);
                }
            }
            if size == 31 || size == 512 {
                copied.clear();
                borrowed.clear();
            }
        }
    }
}
