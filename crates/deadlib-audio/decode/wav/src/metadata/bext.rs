use super::{length, nul_terminated, output};
use crate::{Endian, Error, mux::put16};

/// BWF loudness in integer hundredths. Invalid/unset input remains available
/// raw; semantic accessors ignore values outside EBU Tech 3285's valid range.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Loudness {
    pub integrated: i16,
    pub range: i16,
    pub true_peak: i16,
    pub momentary: i16,
    pub short_term: i16,
}
impl Loudness {
    pub const UNKNOWN: Self = Self {
        integrated: i16::MAX,
        range: i16::MAX,
        true_peak: i16::MAX,
        momentary: i16::MAX,
        short_term: i16::MAX,
    };
    pub fn values(self) -> [Option<i16>; 5] {
        [
            valid(self.integrated, false),
            valid(self.range, true),
            valid(self.true_peak, false),
            valid(self.momentary, false),
            valid(self.short_term, false),
        ]
    }
}
fn valid(v: i16, positive: bool) -> Option<i16> {
    (v >= if positive { 0 } else { -9999 } && v <= 9999).then_some(v)
}

fn digits(b: &[u8]) -> Option<u32> {
    b.iter().try_fold(0u32, |v, &n| {
        n.is_ascii_digit().then(|| v * 10 + u32::from(n - b'0'))
    })
}
fn valid_date(date: &[u8; 10]) -> bool {
    let Some(year) = digits(&date[..4]) else {
        return false;
    };
    let Some(month) = digits(&date[5..7]) else {
        return false;
    };
    let Some(day) = digits(&date[8..]) else {
        return false;
    };
    let max = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if year.is_multiple_of(400) || year.is_multiple_of(4) && !year.is_multiple_of(100) {
                29
            } else {
                28
            }
        }
        _ => 0,
    };
    date.is_ascii() && !date.contains(&0) && day > 0 && day <= max
}
fn valid_time(time: &[u8; 8]) -> bool {
    time.is_ascii()
        && !time.contains(&0)
        && digits(&time[..2]).is_some_and(|v| v < 24)
        && digits(&time[3..5]).is_some_and(|v| v < 60)
        && digits(&time[6..]).is_some_and(|v| v < 60)
}

#[derive(Clone, Copy, Debug)]
pub struct Bext<'a> {
    raw: &'a [u8],
}
impl<'a> Bext<'a> {
    pub fn parse(raw: &'a [u8]) -> Result<Self, Error> {
        if raw.len() < 602 {
            return Err(Error::Truncated);
        }
        Ok(Self { raw })
    }
    pub fn raw(self) -> &'a [u8] {
        self.raw
    }
    pub fn description(self) -> &'a [u8] {
        nul_terminated(&self.raw[..256])
    }
    pub fn originator(self) -> &'a [u8] {
        nul_terminated(&self.raw[256..288])
    }
    pub fn originator_reference(self) -> &'a [u8] {
        nul_terminated(&self.raw[288..320])
    }
    pub fn date(self) -> &'a [u8; 10] {
        self.raw[320..330].try_into().unwrap()
    }
    pub fn time(self) -> &'a [u8; 8] {
        self.raw[330..338].try_into().unwrap()
    }
    pub fn time_reference(self) -> u64 {
        Endian::Little.u64(&self.raw[338..])
    }
    pub fn version(self) -> u16 {
        Endian::Little.u16(&self.raw[346..])
    }
    pub fn umid(self) -> Option<&'a [u8; 64]> {
        (self.version() >= 1).then(|| self.raw[348..412].try_into().unwrap())
    }
    pub fn loudness(self) -> Option<Loudness> {
        (self.version() >= 2).then(|| {
            let e = Endian::Little;
            Loudness {
                integrated: e.u16(&self.raw[412..]) as i16,
                range: e.u16(&self.raw[414..]) as i16,
                true_peak: e.u16(&self.raw[416..]) as i16,
                momentary: e.u16(&self.raw[418..]) as i16,
                short_term: e.u16(&self.raw[420..]) as i16,
            }
        })
    }
    pub fn reserved(self) -> &'a [u8] {
        &self.raw[match self.version() {
            0 => 348,
            1 => 412,
            _ => 422,
        }..602]
    }
    pub fn coding_history(self) -> &'a [u8] {
        &self.raw[602..]
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BextSpec<'a> {
    pub description: &'a [u8],
    pub originator: &'a [u8],
    pub originator_reference: &'a [u8],
    pub date: [u8; 10],
    pub time: [u8; 8],
    pub time_reference: u64,
    pub version: u16,
    pub umid: [u8; 64],
    pub loudness: Loudness,
    pub coding_history: &'a [u8],
}
impl Default for BextSpec<'_> {
    fn default() -> Self {
        Self {
            description: &[],
            originator: &[],
            originator_reference: &[],
            date: *b"0000-01-01",
            time: *b"00:00:00",
            time_reference: 0,
            version: 2,
            umid: [0; 64],
            loudness: Loudness::UNKNOWN,
            coding_history: &[],
        }
    }
}
impl BextSpec<'_> {
    /// Emit BWF v0/v1/v2 with zero reserved fields. No loudness measurement or
    /// date/time inference is performed; the producer supplies those values.
    pub fn write(self, out: &mut [u8]) -> Result<usize, Error> {
        if self.version > 2 {
            return Err(Error::Unsupported("BWF output version"));
        }
        for (text, max) in [
            (self.description, 256),
            (self.originator, 32),
            (self.originator_reference, 32),
        ] {
            if text.len() > max || !text.is_ascii() || text.contains(&0) {
                return Err(Error::Invalid("BWF text field"));
            }
        }
        if !self.date.is_ascii() || !self.time.is_ascii() || !self.coding_history.is_ascii() {
            return Err(Error::Invalid("BWF ASCII fields"));
        }
        if !valid_date(&self.date) || !valid_time(&self.time) {
            return Err(Error::Invalid("BWF date or time"));
        }
        if !self.coding_history.is_empty() && !self.coding_history.ends_with(b"\r\n") {
            return Err(Error::Invalid("BWF coding history termination"));
        }
        if self.version == 0 && self.umid.iter().any(|&v| v != 0) {
            return Err(Error::Invalid("UMID requires BWF v1"));
        }
        if self.version < 2 && self.loudness != Loudness::UNKNOWN {
            return Err(Error::Invalid("loudness requires BWF v2"));
        }
        for (raw, value) in [
            self.loudness.integrated,
            self.loudness.range,
            self.loudness.true_peak,
            self.loudness.momentary,
            self.loudness.short_term,
        ]
        .into_iter()
        .zip(self.loudness.values())
        {
            if raw != i16::MAX && value.is_none() {
                return Err(Error::Invalid("BWF loudness range"));
            }
        }
        let len = length(602, 0, 0, self.coding_history.len())?;
        let b = output(out, len)?;
        b.fill(0);
        b[..self.description.len()].copy_from_slice(self.description);
        b[256..256 + self.originator.len()].copy_from_slice(self.originator);
        b[288..288 + self.originator_reference.len()].copy_from_slice(self.originator_reference);
        b[320..330].copy_from_slice(&self.date);
        b[330..338].copy_from_slice(&self.time);
        b[338..346].copy_from_slice(&self.time_reference.to_le_bytes());
        put16(&mut b[346..], self.version, Endian::Little);
        if self.version >= 1 {
            b[348..412].copy_from_slice(&self.umid);
        }
        if self.version >= 2 {
            for (v, row) in [
                self.loudness.integrated,
                self.loudness.range,
                self.loudness.true_peak,
                self.loudness.momentary,
                self.loudness.short_term,
            ]
            .into_iter()
            .zip(b[412..422].as_chunks_mut::<2>().0)
            {
                put16(row, v as u16, Endian::Little);
            }
        }
        b[602..].copy_from_slice(self.coding_history);
        Ok(len)
    }
}
