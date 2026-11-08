use crate::{AncillaryChunk, ChunkPosition, ColorType, Error, Image, Png, Samples, reserve};

impl Png {
    /// Convert straight, interleaved color samples with 1–16 significant bits
    /// per channel into a native PNG image. Channel depths are in the same
    /// order as the samples (gray/alpha or red/green/blue/alpha).
    ///
    /// The smallest legal PNG depth accommodating every channel is selected.
    /// Integer linear scaling rounds to nearest and preserves every original
    /// high bit. An sBIT chunk records the channel depths whenever scaling is
    /// needed. Alpha maps both endpoints exactly; no color-space conversion,
    /// alpha removal, RGB merging or quantization is performed.
    ///
    /// Either sample container is accepted if its values fit the declared
    /// channel depths. Matching native containers are reused in place. Palette
    /// indices are not reference color samples; use `Png::new` for indexed PNGs.
    /// APNG callers can move the converted `image` into a frame; use the same
    /// channel depths for every frame and keep sBIT once on the enclosing PNG.
    ///
    /// ```
    /// use deadlib_image_png::{ColorType, Png, Samples};
    /// # fn main() -> Result<(), deadlib_image_png::Error> {
    /// let png = Png::from_reference_samples(
    ///     1, 1, ColorType::Rgb, &[5, 6, 5], Samples::U8(vec![31, 63, 31]),
    /// )?;
    /// assert_eq!(png.image.bit_depth, 8);
    /// assert_eq!(png.image.samples, Samples::U8(vec![255, 255, 255]));
    /// # Ok(())
    /// # }
    /// ```
    pub fn from_reference_samples(
        width: u32,
        height: u32,
        color_type: ColorType,
        channel_depths: &[u8],
        samples: Samples,
    ) -> Result<Self, Error> {
        if color_type == ColorType::Indexed {
            return Err(Error::Invalid(
                "reference color samples cannot be palette indices",
            ));
        }
        if width == 0 || height == 0 || width > 0x7fff_ffff || height > 0x7fff_ffff {
            return Err(Error::Invalid("reference image dimensions"));
        }
        let channels = color_type.channels();
        if channel_depths.len() != channels || channel_depths.iter().any(|&n| n == 0 || n > 16) {
            return Err(Error::Invalid("reference channel depths"));
        }
        let count = usize::try_from(width)
            .ok()
            .and_then(|w| usize::try_from(height).ok().and_then(|h| w.checked_mul(h)))
            .and_then(|n| n.checked_mul(channels))
            .ok_or(Error::LimitExceeded)?;
        if count != samples.len() {
            return Err(Error::Invalid("reference sample count"));
        }
        let required = *channel_depths.iter().max().unwrap();
        let depth = [1, 2, 4, 8, 16]
            .into_iter()
            .find(|&n| n >= required && color_type.allows_depth(n))
            .unwrap();
        let shifted = channel_depths.iter().any(|&n| n != depth);
        let matching_container = matches!(&samples, Samples::U8(_) if depth <= 8)
            || matches!(&samples, Samples::U16(_) if depth == 16);
        if !shifted && matching_container {
            let image = Image {
                width,
                height,
                color_type,
                bit_depth: depth,
                samples,
            };
            image
                .validate()
                .map_err(|_| Error::Invalid("reference sample range"))?;
            return Ok(Self::new(image));
        }
        let mut maxima = [0; 4];
        for (out, &bits) in maxima.iter_mut().zip(channel_depths) {
            *out = (1u32 << bits) - 1;
        }
        let maximum = (1u32 << depth) - 1;
        let scale = |value: u16, channel: usize| -> Result<u16, Error> {
            let value = u32::from(value);
            let input_maximum = maxima[channel];
            if value > input_maximum {
                return Err(Error::Invalid("reference sample range"));
            }
            // For depths <=16 the numerator is at most 0xfffe8000.
            Ok(((value * maximum + input_maximum / 2) / input_maximum) as u16)
        };
        let samples = match samples {
            Samples::U8(mut values) if depth <= 8 => {
                for pixel in values.chunks_exact_mut(channels) {
                    for (channel, value) in pixel.iter_mut().enumerate() {
                        *value = scale(u16::from(*value), channel)? as u8;
                    }
                }
                Samples::U8(values)
            }
            Samples::U16(mut values) if depth == 16 => {
                for pixel in values.chunks_exact_mut(channels) {
                    for (channel, value) in pixel.iter_mut().enumerate() {
                        *value = scale(*value, channel)?;
                    }
                }
                Samples::U16(values)
            }
            Samples::U8(values) => {
                let mut output = Vec::new();
                reserve(&mut output, count)?;
                for pixel in values.chunks_exact(channels) {
                    for (channel, &value) in pixel.iter().enumerate() {
                        output.push(scale(u16::from(value), channel)?);
                    }
                }
                Samples::U16(output)
            }
            Samples::U16(values) => {
                let mut output = Vec::new();
                reserve(&mut output, count)?;
                for pixel in values.chunks_exact(channels) {
                    for (channel, &value) in pixel.iter().enumerate() {
                        output.push(scale(value, channel)? as u8);
                    }
                }
                Samples::U8(output)
            }
        };
        let mut png = Self::new(Image {
            width,
            height,
            color_type,
            bit_depth: depth,
            samples,
        });
        if shifted {
            let mut data = Vec::new();
            reserve(&mut data, channels)?;
            data.extend_from_slice(channel_depths);
            reserve(&mut png.ancillary, 1)?;
            png.ancillary.push(AncillaryChunk {
                kind: *b"sBIT",
                data,
                position: ChunkPosition::BeforePalette,
            });
        }
        Ok(png)
    }
}
