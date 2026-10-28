//! Validation driver: canonical native decode, exact range access, typed encode.
use deadlib_audio_decode_wav::{
    DataLength, Endian, ForwardReader, RawChunk, Reader, SampleFormat, Spec, Wave, WavlWriter,
    WriteOptions, Writer,
};
use std::io::{Cursor, Read, Write};
struct Input(std::fs::File);
impl Read for Input {
    fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
        let n = b.len().min(7);
        self.0.read(&mut b[..n])
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    if !(3..=6).contains(&args.len()) {
        return Err("pcm_reference INPUT describe|borrowed|reader|forward|encode|encode-known|encode-wavl|encode-wavl-known [START [FRAMES [BUFFER_FRAMES]]]".into());
    }
    let bytes = std::fs::read(&args[1])?;
    let wave = Wave::parse(&bytes)?;
    let pcm = wave.pcm()?;
    let format = wave.format;
    let sample = format.sample_format()?;
    let channels = usize::from(format.channels);
    let bits = sample.bits();
    if args[2] == "describe" {
        println!(
            "{{\"container\":\"{:?}\",\"channels\":{},\"sample_rate\":{},\"bits\":{},\"valid_bits\":{},\"mask\":{},\"frames\":{},\"block_align\":{},\"byte_rate\":{}}}",
            wave.container,
            format.channels,
            format.sample_rate,
            bits,
            format.valid_bits,
            format.channel_mask,
            pcm.frames(),
            format.block_align,
            format.byte_rate
        );
        return Ok(());
    }
    if args[2].starts_with("encode") {
        let spec = Spec {
            sample_format: sample,
            channels: format.channels,
            sample_rate: format.sample_rate,
            valid_bits: format.valid_bits,
            channel_mask: format.channel_mask,
        };
        let options = WriteOptions {
            container: wave.container,
            auto_rf64: false,
            force_extensible: format.tag == 0xfffe,
        };
        let chunks = wave
            .chunks()?
            .map(Result::unwrap)
            .filter(|c| {
                c.offset != wave.audio_chunk().offset
                    && ![*b"fmt ", *b"data", *b"fact", *b"ds64"].contains(&c.id)
            })
            .map(|c| RawChunk {
                id: c.id,
                data: c.data,
            })
            .collect::<Vec<_>>();
        let output = if args[2] == "encode-wavl" || args[2] == "encode-wavl-known" {
            let children = wave.wavl_chunks().ok_or("input is not wavl")?;
            let mut writer = if args[2] == "encode-wavl-known" {
                WavlWriter::with_lengths(
                    Cursor::new(Vec::new()),
                    spec,
                    options,
                    &chunks,
                    DataLength {
                        bytes: wave.audio_chunk().data.len() as u64,
                        frames: pcm.frames(),
                    },
                )?
            } else {
                WavlWriter::new(Cursor::new(Vec::new()), spec, options, &chunks)?
            };
            for c in children {
                match &c.id {
                    b"data" => writer.write_frames(c.data)?,
                    b"slnt" => {
                        let b: [u8; 4] = c.data.try_into()?;
                        let frames = if format.endian == Endian::Little {
                            u32::from_le_bytes(b)
                        } else {
                            u32::from_be_bytes(b)
                        };
                        writer.write_silence(u64::from(frames))?;
                    }
                    _ => writer.write_chunk(RawChunk {
                        id: c.id,
                        data: c.data,
                    })?,
                }
            }
            if args[2] == "encode-wavl-known" {
                writer.finish_stream()?
            } else {
                writer.finish()?
            }
            .into_inner()
        } else {
            let known = args[2] == "encode-known";
            if !known && args[2] != "encode" {
                return Err("unknown mode".into());
            }
            let mut writer = if known {
                Writer::with_frames(
                    Cursor::new(Vec::new()),
                    spec,
                    options,
                    &chunks,
                    pcm.frames(),
                )?
            } else {
                Writer::new(Cursor::new(Vec::new()), spec, options, &chunks)?
            };
            let mut start = 0;
            let mut integers = vec![0i32; 13 * channels];
            let mut floats = vec![0f32; 13 * channels];
            let mut doubles = vec![0f64; 13 * channels];
            let mut shorts = vec![0i16; 13 * channels];
            while start < pcm.frames() {
                let n = match sample {
                    SampleFormat::F32 => {
                        let n = pcm.read_f32(start, &mut floats)?;
                        writer.write_f32(&floats[..n])?;
                        n
                    }
                    SampleFormat::F64 => {
                        let n = pcm.read_f64(start, &mut doubles)?;
                        writer.write_f64(&doubles[..n])?;
                        n
                    }
                    SampleFormat::I16 => {
                        let n = pcm.read_i16(start, &mut shorts)?;
                        writer.write_i16(&shorts[..n])?;
                        n
                    }
                    _ => {
                        let n = pcm.read_i32(start, &mut integers)?;
                        if sample == SampleFormat::I32 {
                            writer.write_i32(&integers[..n])?;
                        } else {
                            writer.write_integers(&integers[..n])?;
                        }
                        n
                    }
                };
                start += (n / channels) as u64;
            }
            if known {
                writer.finish_stream()?
            } else {
                writer.finish()?
            }
            .into_inner()
        };
        let encoded = Wave::parse(&output)?;
        let output_pcm = encoded.pcm()?;
        assert_eq!(output_pcm.frames(), pcm.frames());
        let align = usize::from(format.block_align);
        let mut a = vec![0; 17 * align];
        let mut b = vec![0; a.len()];
        let mut position = 0;
        while position < pcm.frames() {
            let n = pcm.read_frames(position, &mut a)?;
            assert_eq!(output_pcm.read_frames(position, &mut b)?, n);
            assert_eq!(&a[..n * align], &b[..n * align]);
            position += n as u64;
        }
        if let (Some(source), Some(target)) = (wave.wavl_chunks(), encoded.wavl_chunks()) {
            assert_eq!(
                source.map(|c| (c.id, c.data)).collect::<Vec<_>>(),
                target.map(|c| (c.id, c.data)).collect::<Vec<_>>()
            );
        }
        assert_eq!(encoded.format.valid_bits, format.valid_bits);
        assert_eq!(encoded.format.channel_mask, format.channel_mask);
        let retained = encoded
            .chunks()?
            .map(Result::unwrap)
            .filter(|c| {
                c.offset != encoded.audio_chunk().offset
                    && ![*b"fmt ", *b"data", *b"fact", *b"ds64"].contains(&c.id)
            })
            .map(|c| (c.id, c.data))
            .collect::<Vec<_>>();
        assert_eq!(
            retained,
            chunks.iter().map(|c| (c.id, c.data)).collect::<Vec<_>>()
        );
        std::io::stdout().lock().write_all(&output)?;
        return Ok(());
    }
    let start = args.get(3).map_or(Ok(0u64), |v| v.parse())?;
    let count = args
        .get(4)
        .map_or(Ok(pcm.frames().saturating_sub(start)), |v| v.parse::<u64>())?;
    let capacity = args.get(5).map_or(Ok(13usize), |v| v.parse())?;
    if capacity == 0 || capacity > 65536 || start > pcm.frames() {
        return Err("range/capacity".into());
    }
    let mut remaining = count.min(pcm.frames() - start);
    let mut position = start;
    let mut file = if args[2] == "reader" {
        let mut r = Reader::new(std::fs::File::open(&args[1])?)?;
        r.seek_frame(start)?;
        Some(r)
    } else {
        None
    };
    let mut forward = if args[2] == "forward" {
        let mut r = ForwardReader::new(Input(std::fs::File::open(&args[1])?))?;
        let mut skip = start;
        let align = usize::from(format.block_align);
        let mut scratch = vec![0; 4096usize.max(align)];
        while skip != 0 {
            let capacity = skip.min((scratch.len() / align) as u64) as usize * align;
            let n = r.read_frames(&mut scratch[..capacity])?;
            if n == 0 {
                return Err("early skip EOF".into());
            }
            skip -= n as u64;
        }
        Some(r)
    } else {
        None
    };
    if args[2] != "borrowed" && file.is_none() && forward.is_none() {
        return Err("unknown decode mode".into());
    }
    let mut integers = vec![0i32; capacity * channels];
    let mut floats = vec![0f32; capacity * channels];
    let mut doubles = vec![0f64; capacity * channels];
    let mut sink = std::io::BufWriter::new(std::io::stdout().lock());
    while remaining != 0 {
        let cap = remaining.min(capacity as u64) as usize * channels;
        let n = match sample {
            SampleFormat::F32 => {
                let n = if let Some(r) = &mut file {
                    r.read_f32(&mut floats[..cap])?
                } else if let Some(r) = &mut forward {
                    r.read_f32(&mut floats[..cap])?
                } else {
                    pcm.read_f32(position, &mut floats[..cap])?
                };
                for v in &floats[..n] {
                    sink.write_all(&v.to_bits().to_le_bytes())?;
                }
                n
            }
            SampleFormat::F64 => {
                let n = if let Some(r) = &mut file {
                    r.read_f64(&mut doubles[..cap])?
                } else if let Some(r) = &mut forward {
                    r.read_f64(&mut doubles[..cap])?
                } else {
                    pcm.read_f64(position, &mut doubles[..cap])?
                };
                for v in &doubles[..n] {
                    sink.write_all(&v.to_bits().to_le_bytes())?;
                }
                n
            }
            _ => {
                let n = if let Some(r) = &mut file {
                    r.read_integers(&mut integers[..cap])?
                } else if let Some(r) = &mut forward {
                    r.read_integers(&mut integers[..cap])?
                } else {
                    pcm.read_i32(position, &mut integers[..cap])?
                };
                for v in &integers[..n] {
                    sink.write_all(&v.wrapping_shl(u32::from(32 - bits)).to_le_bytes())?;
                }
                n
            }
        };
        if n == 0 {
            return Err("early decode EOF".into());
        }
        let frames = (n / channels) as u64;
        position += frames;
        remaining -= frames;
    }
    if let Some(r) = forward {
        r.finish()?;
    }
    sink.flush()?;
    Ok(())
}
