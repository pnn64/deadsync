use super::{DecodeEvent, DecodeStatus, ImageInfo, ImageKind, StreamDecoder};
use crate::{Animation, DecodeOptions, Error, Frame, Image, Png, Samples, reserve};
use std::io::Read;

fn allocate(info: ImageInfo) -> Result<Image, Error> {
    let count = (info.width as usize)
        .checked_mul(info.height as usize)
        .and_then(|n| n.checked_mul(info.color_type.channels()))
        .ok_or(Error::LimitExceeded)?;
    let samples = if info.bit_depth == 16 {
        let mut v = Vec::new();
        reserve(&mut v, count)?;
        if info.interlaced {
            v.resize(count, 0);
        }
        Samples::U16(v)
    } else {
        let mut v = Vec::new();
        reserve(&mut v, count)?;
        if info.interlaced {
            v.resize(count, 0);
        }
        Samples::U8(v)
    };
    Ok(Image {
        width: info.width,
        height: info.height,
        color_type: info.color_type,
        bit_depth: info.bit_depth,
        samples,
    })
}

pub(super) fn row_image(png: &mut Png, kind: ImageKind) -> Result<&mut Image, Error> {
    match kind {
        ImageKind::Default => Ok(&mut png.image),
        ImageKind::AnimationFrame(index) => png
            .animation
            .as_mut()
            .and_then(|a| a.frames.get_mut(index as usize))
            .and_then(|f| f.image.as_mut())
            .ok_or(Error::Invalid("missing frame image")),
    }
}

fn collect(png: &mut Option<Png>, decoder: &mut StreamDecoder) -> Result<(), Error> {
    let event = decoder
        .event()
        .ok_or(Error::Invalid("missing decode event"))?;
    if let DecodeEvent::Header(info) = event {
        let mut out = Png::new(allocate(info)?);
        out.interlaced = info.interlaced;
        *png = Some(out);
        return Ok(());
    }
    let png = png.as_mut().ok_or(Error::Invalid("missing IHDR"))?;
    match event {
        DecodeEvent::Header(_) => unreachable!(),
        DecodeEvent::Metadata { .. } => {
            reserve(&mut png.ancillary, 1)?;
            png.ancillary.push(
                decoder
                    .owned_ancillary()
                    .ok_or(Error::Invalid("missing decode event"))?,
            );
        }
        DecodeEvent::Animation { plays, .. } => {
            png.animation = Some(Animation {
                plays,
                default_is_frame: false,
                frames: Vec::new(),
            })
        }
        DecodeEvent::AnimationError(error) => {
            png.animation = None;
            png.animation_error = Some(error.clone());
        }
        DecodeEvent::FrameControl {
            index,
            control,
            uses_default_image,
        } => {
            let info = ImageInfo {
                width: control.width,
                height: control.height,
                color_type: png.image.color_type,
                bit_depth: png.image.bit_depth,
                interlaced: png.interlaced,
            };
            let image = if uses_default_image {
                None
            } else {
                Some(allocate(info)?)
            };
            let animation = png
                .animation
                .as_mut()
                .ok_or(Error::Invalid("missing acTL"))?;
            if animation.frames.len() != index as usize {
                return Err(Error::Invalid("frame index"));
            }
            animation.default_is_frame |= uses_default_image;
            reserve(&mut animation.frames, 1)?;
            animation.frames.push(Frame { control, image });
        }
        DecodeEvent::Row(_) => return Err(Error::Invalid("missing image decoder")),
        DecodeEvent::Palette(_) | DecodeEvent::Transparency(_) | DecodeEvent::ImageEnd(_) => (),
    }
    Ok(())
}

fn read(reader: &mut impl Read, bytes: &mut [u8]) -> Result<usize, Error> {
    loop {
        match reader.read(bytes) {
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Ok(n) if n > bytes.len() => {
                return Err(Error::Io("reader returned invalid byte count".into()));
            }
            result => return result.map_err(Into::into),
        }
    }
}

/// Decode a complete PNG from a possibly short-reading source. Retains only
/// final native samples/metadata, a small input buffer, and the row pipeline.
/// Like `decode`, rejects bytes after IEND. Use `StreamDecoder` for prefix parsing
/// or to consume rows without retaining complete images and animation frames.
pub fn decode_reader(mut reader: impl Read, options: &DecodeOptions) -> Result<Png, Error> {
    let mut decoder = StreamDecoder::for_reader(*options);
    let mut png = None;
    let mut input = [0; 8192];
    let (mut pos, mut end) = (0, 0);
    loop {
        let progress = decoder.process_into(&input[pos..end], png.as_mut())?;
        pos += progress.consumed;
        match progress.status {
            DecodeStatus::Event => collect(&mut png, &mut decoder)?,
            DecodeStatus::NeedInput => {
                if pos != end {
                    return Err(Error::Invalid("decoder input progress"));
                }
                end = read(&mut reader, &mut input)?;
                pos = 0;
                if end == 0 {
                    break;
                }
            }
            DecodeStatus::Done => {
                if pos != end || read(&mut reader, &mut [0; 1])? != 0 {
                    return Err(Error::Invalid("data after IEND"));
                }
                break;
            }
        }
    }
    decoder.finish()?;
    let mut png = png.ok_or(Error::Invalid("missing IHDR"))?;
    (png.palette, png.transparency) = decoder.take_palette_and_transparency();
    Ok(png)
}
