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
