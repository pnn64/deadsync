use crate::codec::{Header, be16, be32, write_chunk};
use crate::{ColorType, EncodeOptions, Error, Image, Limits, Png, Samples, scanline, zlib};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum DisposeOp {
    None = 0,
    Background = 1,
    Previous = 2,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum BlendOp {
    Source = 0,
    Over = 1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameControl {
    pub width: u32,
    pub height: u32,
    pub x_offset: u32,
    pub y_offset: u32,
    pub delay_num: u16,
    pub delay_den: u16,
    pub dispose: DisposeOp,
    pub blend: BlendOp,
}
impl FrameControl {
    pub fn delay_seconds(self) -> f64 {
        f64::from(self.delay_num)
            / f64::from(if self.delay_den == 0 {
                100
            } else {
                self.delay_den
            })
    }
    fn validate(self, h: Header, default: bool) -> Result<(), Error> {
        if self.width == 0
            || self.height == 0
            || self.width > h.width
            || self.height > h.height
            || self.x_offset > h.width - self.width
            || self.y_offset > h.height - self.height
            || (default
                && (self.width != h.width
                    || self.height != h.height
                    || self.x_offset != 0
                    || self.y_offset != 0))
        {
            return Err(Error::Invalid("APNG frame rectangle"));
        }
        Ok(())
    }
    fn parse(data: &[u8], h: Header, default: bool) -> Result<Self, Error> {
        if data.len() != 26 {
            return Err(Error::Invalid("fcTL length"));
        }
        let dispose = match data[24] {
            0 => DisposeOp::None,
            1 => DisposeOp::Background,
            2 => DisposeOp::Previous,
            _ => return Err(Error::Invalid("APNG disposal")),
        };
        let blend = match data[25] {
            0 => BlendOp::Source,
            1 => BlendOp::Over,
            _ => return Err(Error::Invalid("APNG blend")),
        };
        let value = Self {
            width: be32(&data[4..]),
            height: be32(&data[8..]),
            x_offset: be32(&data[12..]),
            y_offset: be32(&data[16..]),
            delay_num: be16(&data[20..]),
            delay_den: be16(&data[22..]),
            dispose,
            blend,
        };
        value.validate(h, default)?;
        Ok(value)
    }
    fn wire(self, sequence: u32) -> [u8; 26] {
        let mut out = [0; 26];
        for (n, v) in [
            sequence,
            self.width,
            self.height,
            self.x_offset,
            self.y_offset,
        ]
        .iter()
        .enumerate()
        {
            out[n * 4..n * 4 + 4].copy_from_slice(&v.to_be_bytes());
        }
        out[20..22].copy_from_slice(&self.delay_num.to_be_bytes());
        out[22..24].copy_from_slice(&self.delay_den.to_be_bytes());
        out[24] = self.dispose as u8;
        out[25] = self.blend as u8;
        out
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Frame {
    pub control: FrameControl,
    /// `None` only for the first frame when it uses `Png::image`. This avoids
    /// duplicating the default image's sample allocation.
    pub image: Option<Image>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Animation {
    pub plays: u32,
    pub default_is_frame: bool,
    pub frames: Vec<Frame>,
}

pub(crate) struct Decoder {
    h: Header,
    limits: Limits,
    control: Option<(u32, u32)>,
    sequence: u32,
    default: bool,
    frames: Vec<Frame>,
    pending: Option<(FrameControl, Vec<u8>)>,
    remaining: usize,
}
fn image_bytes(h: Header) -> Result<usize, Error> {
    (h.width as usize)
        .checked_mul(h.height as usize)
        .and_then(|n| n.checked_mul(h.color.channels()))
        .and_then(|n| n.checked_mul(if h.depth == 16 { 2 } else { 1 }))
        .ok_or(Error::LimitExceeded)
}
impl Decoder {
    pub(crate) fn new(h: Header, limits: Limits) -> Result<Self, Error> {
        let remaining = limits
            .max_output_bytes
            .checked_sub(image_bytes(h)?)
            .ok_or(Error::LimitExceeded)?;
        Ok(Self {
            h,
            limits,
            control: None,
            sequence: 0,
            default: false,
            frames: Vec::new(),
            pending: None,
            remaining,
        })
    }
    fn sequence(&mut self, data: &[u8]) -> Result<(), Error> {
        let n = be32(data);
        if n > 0x7fff_ffff || n != self.sequence {
            return Err(Error::Invalid("APNG sequence number"));
        }
        self.sequence += 1;
        Ok(())
    }
    fn flush(&mut self, palette: usize) -> Result<(), Error> {
        if let Some((control, compressed)) = self.pending.take() {
            let h = Header {
                width: control.width,
                height: control.height,
                ..self.h
            };
            let bytes = image_bytes(h)?;
            self.remaining = self
                .remaining
                .checked_sub(bytes)
                .ok_or(Error::LimitExceeded)?;
            let expected =
                scanline::filtered_size(h.width, h.height, h.color, h.depth, h.interlaced)?;
            if expected > self.limits.max_output_bytes {
                return Err(Error::LimitExceeded);
            }
            let (mut raw, _) = zlib::decompress_prefix(&compressed, expected)?;
            if raw.len() != expected {
                return Err(Error::Invalid("APNG frame data length"));
            }
            let image = scanline::decode(
                &mut raw,
                h.width,
                h.height,
                h.color,
                h.depth,
                h.interlaced,
                self.limits.max_output_bytes,
            )?;
            validate_image(&image, self.h, palette)?;
            self.frames.push(Frame {
                control,
                image: Some(image),
            });
        }
        Ok(())
    }
    pub(crate) fn chunk(
        &mut self,
        kind: &[u8; 4],
        data: &[u8],
        seen_data: bool,
        palette: usize,
    ) -> Result<(), Error> {
        match kind {
            b"acTL" => {
                if data.len() != 8 || seen_data || self.control.is_some() {
                    return Err(Error::Invalid("acTL length, order or duplication"));
                }
                let count = be32(data);
                let plays = be32(&data[4..]);
                if count == 0 || count > 0x7fff_ffff || plays > 0x7fff_ffff {
                    return Err(Error::Invalid("APNG animation control"));
                }
                if count as usize > self.limits.max_frames {
                    return Err(Error::LimitExceeded);
                }
                self.control = Some((count, plays));
            }
            b"fcTL" => {
                let (count, _) = self.control.ok_or(Error::Invalid("fcTL without acTL"))?;
                if data.len() != 26 {
                    return Err(Error::Invalid("fcTL length"));
                }
                self.sequence(data)?;
                self.flush(palette)?;
                if self.frames.len() >= count as usize {
                    return Err(Error::Invalid("too many APNG frames"));
                }
                let control = FrameControl::parse(data, self.h, !seen_data)?;
                if !seen_data {
                    if !self.frames.is_empty() {
                        return Err(Error::Invalid("multiple fcTL before IDAT"));
                    }
                    self.default = true;
                    self.frames.push(Frame {
                        control,
                        image: None,
                    });
                } else {
                    self.pending = Some((control, Vec::new()));
                }
            }
            b"fdAT" => {
                if !seen_data || data.len() < 4 {
                    return Err(Error::Invalid("fdAT length or order"));
                }
                self.sequence(data)?;
                let (_, bytes) = self
                    .pending
                    .as_mut()
                    .ok_or(Error::Invalid("fdAT without frame control"))?;
                crate::reserve(bytes, data.len() - 4)?;
                bytes.extend_from_slice(&data[4..]);
            }
            _ => unreachable!(),
        }
        Ok(())
    }
    pub(crate) fn finish(mut self, palette: usize) -> Result<Option<Animation>, Error> {
        self.flush(palette)?;
        if let Some((count, plays)) = self.control {
            if self.frames.len() != count as usize {
                return Err(Error::Invalid("APNG frame count mismatch"));
            }
            Ok(Some(Animation {
                plays,
                default_is_frame: self.default,
                frames: self.frames,
            }))
        } else {
            Ok(None)
        }
    }
}

fn validate_image(image: &Image, h: Header, palette: usize) -> Result<(), Error> {
    image
        .validate()
        .map_err(|_| Error::Invalid("APNG frame samples"))?;
    if image.color_type != h.color || image.bit_depth != h.depth {
        return Err(Error::Invalid("APNG frame format"));
    }
    if h.color == ColorType::Indexed {
        let Samples::U8(values) = &image.samples else {
            return Err(Error::Invalid("indexed APNG storage"));
        };
        if values.iter().any(|&v| v as usize >= palette) {
            return Err(Error::Invalid("APNG palette index"));
        }
    }
    Ok(())
}
pub(crate) fn validate(png: &Png, h: Header) -> Result<(), Error> {
    if let Some(a) = &png.animation {
        if a.frames.is_empty() || a.frames.len() > 0x7fff_ffff || a.plays > 0x7fff_ffff {
            return Err(Error::Invalid("APNG control"));
        }
        for (n, f) in a.frames.iter().enumerate() {
            let default = n == 0 && a.default_is_frame;
            f.control.validate(h, default)?;
            if default {
                if f.image.is_some() {
                    return Err(Error::Invalid("default frame must reference Png image"));
                }
            } else {
                let image = f
                    .image
                    .as_ref()
                    .ok_or(Error::Invalid("missing APNG frame image"))?;
                validate_image(image, h, png.palette.len())?;
                if image.width != f.control.width || image.height != f.control.height {
                    return Err(Error::Invalid("APNG image/control dimensions"));
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn write_control(out: &mut Vec<u8>, png: &Png) -> Result<(), Error> {
    if let Some(a) = &png.animation {
        let mut data = [0; 8];
        data[..4].copy_from_slice(&(a.frames.len() as u32).to_be_bytes());
        data[4..].copy_from_slice(&a.plays.to_be_bytes());
        write_chunk(out, b"acTL", &data)?;
        if a.default_is_frame {
            write_chunk(out, b"fcTL", &a.frames[0].control.wire(0))?;
        }
    }
    Ok(())
}
pub(crate) fn write_frames(
    out: &mut Vec<u8>,
    png: &Png,
    options: &EncodeOptions,
) -> Result<(), Error> {
    if let Some(a) = &png.animation {
        let mut sequence = u32::from(a.default_is_frame);
        for f in &a.frames[usize::from(a.default_is_frame)..] {
            if sequence > 0x7fff_ffff {
                return Err(Error::LimitExceeded);
            }
            write_chunk(out, b"fcTL", &f.control.wire(sequence))?;
            sequence += 1;
            let image = f
                .image
                .as_ref()
                .ok_or(Error::Invalid("missing APNG frame"))?;
            let raw = scanline::encode(image, options.interlaced, options.filter)?;
            let compressed = zlib::compress(&raw, options.compression)?;
            let chunk_size = options.idat_chunk_size.min(0x7fff_fffb);
            let mut payload = Vec::new();
            for part in compressed.chunks(chunk_size) {
                if sequence > 0x7fff_ffff {
                    return Err(Error::LimitExceeded);
                }
                payload.clear();
                crate::reserve(&mut payload, part.len() + 4)?;
                payload.extend_from_slice(&sequence.to_be_bytes());
                payload.extend_from_slice(part);
                write_chunk(out, b"fdAT", &payload)?;
                sequence += 1;
            }
        }
    }
    Ok(())
}
