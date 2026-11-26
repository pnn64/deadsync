use crate::{bitstream::Bits, Error, PictureKind, Result};

#[derive(Clone, Copy)]
pub(crate) struct Layer {
    pub(crate) width: usize,
    pub(crate) height: usize,
    time_bits: u8,
    pub(crate) resync: bool,
}

impl Layer {
    pub(crate) fn parse(payload: &[u8]) -> Result<Self> {
        let mut bits = Bits::new(payload);
        bits.skip(9)?; // Random access flag and video object type.
        let version = if bits.flag()? {
            let version = bits.read(4)?;
            bits.skip(3)?;
            version
        } else {
            1
        };
        if bits.read(4)? == 15 {
            bits.skip(16)?; // Extended pixel aspect ratio.
        }
        if bits.flag()? {
            if bits.read(2)? != 1 {
                return Err(Error::Unsupported("chroma format other than 4:2:0"));
            }
            bits.skip(1)?; // Low-delay flag.
            if bits.flag()? {
                for length in [15, 15, 15, 14, 15] {
                    bits.skip(length)?;
                    bits.marker()?;
                }
            }
        }
        if bits.read(2)? != 0 {
            return Err(Error::Unsupported("nonrectangular video object"));
        }
        bits.marker()?;
        let resolution = bits.read(16)?;
        if resolution == 0 {
            return Err(Error::Invalid("zero time increment resolution"));
        }
        let time_bits = (32 - (resolution - 1).leading_zeros()).max(1) as u8;
        bits.marker()?;
        if bits.flag()? {
            bits.skip(time_bits)?;
        }
        bits.marker()?;
        let width = bits.read(13)? as usize;
        bits.marker()?;
        let height = bits.read(13)? as usize;
        bits.marker()?;
        if width == 0 || height == 0 {
            return Err(Error::Invalid("zero picture dimension"));
        }
        if width > 4096 || height > 4096 {
            return Err(Error::Unsupported("picture dimensions above 4096 × 4096"));
        }
        if bits.flag()? {
            return Err(Error::Unsupported("interlaced video"));
        }
        if !bits.flag()? {
            return Err(Error::Unsupported("overlapped motion compensation"));
        }
        if bits.read(if version == 1 { 1 } else { 2 })? != 0 {
            return Err(Error::Unsupported("sprites or global motion"));
        }
        if bits.flag()? {
            return Err(Error::Unsupported(
                "nonstandard bit depth or quantizer precision",
            ));
        }
        if bits.flag()? {
            return Err(Error::Unsupported("MPEG quantization matrices"));
        }
        if version != 1 && bits.flag()? {
            return Err(Error::Unsupported("quarter-pixel motion"));
        }
        if !bits.flag()? {
            return Err(Error::Unsupported("complexity estimation headers"));
        }
        let resync = !bits.flag()?;
        if bits.flag()? {
            return Err(Error::Unsupported("data partitioning"));
        }
        if version != 1 {
            if bits.flag()? {
                return Err(Error::Unsupported("new prediction"));
            }
            if bits.flag()? {
                return Err(Error::Unsupported("reduced-resolution pictures"));
            }
        }
        if bits.flag()? {
            return Err(Error::Unsupported("scalability"));
        }
        Ok(Self {
            width,
            height,
            time_bits,
            resync,
        })
    }
}

pub(crate) struct Picture {
    pub(crate) kind: PictureKind,
    pub(crate) truncate: bool,
    pub(crate) dc_threshold: i32,
    pub(crate) quantizer: i32,
    pub(crate) f_code: u8,
}

impl Picture {
    pub(crate) fn parse(bits: &mut Bits<'_>, layer: Layer) -> Result<Option<Self>> {
        let kind = match bits.read(2)? {
            0 => PictureKind::I,
            1 => PictureKind::P,
            _ => return Err(Error::Unsupported("B or sprite pictures")),
        };
        while bits.flag()? {} // Modulo time base.
        bits.marker()?;
        bits.skip(layer.time_bits)?;
        bits.marker()?;
        if !bits.flag()? {
            return Ok(None);
        }
        let truncate = kind == PictureKind::P && bits.flag()?;
        let dc_threshold = [32, 13, 15, 17, 19, 21, 23, 0][bits.read(3)? as usize];
        let quantizer = bits.read(5)? as i32;
        if quantizer == 0 {
            return Err(Error::Invalid("zero picture quantizer"));
        }
        let f_code = if kind == PictureKind::P {
            let code = bits.read(3)? as u8;
            if code == 0 {
                return Err(Error::Invalid("zero motion vector f_code"));
            }
            code
        } else {
            1
        };
        Ok(Some(Self {
            kind,
            truncate,
            dc_threshold,
            quantizer,
            f_code,
        }))
    }

    pub(crate) fn packet(
        &mut self,
        bits: &mut Bits<'_>,
        layer: Layer,
        expected: usize,
        macroblocks: usize,
    ) -> Result<bool> {
        let stuffing = bits.alignment_stuffing();
        if bits.peek(stuffing) != (1 << (stuffing - 1)) - 1 {
            return Ok(false);
        }
        let mut probe = *bits;
        let length = if self.kind == PictureKind::I {
            17
        } else {
            16 + self.f_code
        };
        if probe.skip(stuffing).is_err() || probe.peek(length) != 1 {
            return Ok(false);
        }
        probe.skip(length)?;
        let address_bits = (usize::BITS - (macroblocks - 1).leading_zeros()) as u8;
        if probe.read(address_bits)? as usize != expected {
            return Err(Error::Invalid("video packet macroblock address"));
        }
        let quantizer = probe.read(5)? as i32;
        if quantizer == 0 {
            return Err(Error::Invalid("zero video packet quantizer"));
        }
        if probe.flag()? {
            while probe.flag()? {}
            probe.marker()?;
            probe.skip(layer.time_bits)?;
            probe.marker()?;
            probe.skip(5)?; // VOP type and intra DC threshold.
            if self.kind == PictureKind::P {
                probe.skip(3)?; // Repeated forward f_code.
            }
        }
        self.quantizer = quantizer;
        *bits = probe;
        Ok(true)
    }
}
