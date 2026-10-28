//! Indexed/gapless validation CLI. PCM caching is confined to optional seek audit.
use deadlib_audio_decode_mp3::*;
use std::io::Write;
struct AuditChunk {
    position: u64,
    header: FrameHeader,
    planes: Vec<Vec<f32>>,
}
fn write_pcm(output: &mut impl Write, header: FrameHeader, pcm: Pcm<'_>) -> std::io::Result<()> {
    for value in [
        header.sample_rate(),
        pcm.channels() as u32,
        pcm.samples() as u32,
    ] {
        output.write_all(&value.to_le_bytes())?;
    }
    for n in 0..pcm.samples() {
        for plane in pcm.planes() {
            output.write_all(&plane[n].to_le_bytes())?;
        }
    }
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert!(
        args.len() >= 3,
        "file INPUT OUTPUT raw|gapless [audit | SAMPLE LENGTH]"
    );
    let mode = match args[2].as_str() {
        "raw" => SampleMode::Raw,
        "gapless" => SampleMode::Gapless,
        _ => return Err("sample mode".into()),
    };
    let audit = args.get(3).is_some_and(|s| s == "audit");
    let mut file = IndexedDecoder::with_options(
        std::fs::File::open(&args[0])?,
        FileOptions {
            samples: mode,
            stream: StreamOptions {
                recovery: if args.iter().any(|s| s == "recover") {
                    Recovery::Conceal
                } else {
                    Recovery::Strict
                },
                truncated: if args.iter().any(|s| s == "discard") {
                    TruncatedPolicy::Discard
                } else {
                    TruncatedPolicy::Error
                },
                ..Default::default()
            },
        },
    )?;
    let range = file.sample_range();
    let length = file.samples();
    let seek = if !audit {
        args.get(3)
            .map(|s| s.parse::<u64>())
            .transpose()?
            .unwrap_or(0)
    } else {
        0
    };
    let count = if !audit {
        args.get(4)
            .map(|s| s.parse::<u64>())
            .transpose()?
            .unwrap_or(length - seek.min(length))
    } else {
        length
    };
    file.seek_sample(seek)?;
    let mut left = count;
    let mut linear = Vec::<AuditChunk>::new();
    let mut output = std::io::BufWriter::new(std::fs::File::create(&args[1])?);
    output.write_all(b"DLMP3FI1")?;
    while left != 0 {
        let Some(frame) = file.next_frame()? else {
            break;
        };
        let count = (left.min(frame.pcm.samples() as u64)) as usize;
        let pcm = frame.pcm.slice(0..count).unwrap();
        write_pcm(&mut output, frame.header, pcm)?;
        left -= count as u64;
        if audit {
            linear.push(AuditChunk {
                position: frame.position,
                header: frame.header,
                planes: pcm.planes().map(<[f32]>::to_vec).collect(),
            });
        }
    }
    output.flush()?;
    println!(
        "range\t{}\t{}\t{}\t{}\t{}",
        range.raw_start(),
        range.raw_end(),
        length,
        file.index().raw_samples(),
        file.index().frames().len()
    );
    if audit {
        let mut points = vec![0, length];
        for p in [
            1,
            31,
            32,
            528,
            529,
            575,
            576,
            577,
            1151,
            1152,
            1153,
            length / 2,
            length.saturating_sub(1),
        ] {
            if p <= length {
                points.push(p);
            }
        }
        for entry in file.index().frames() {
            if let Some(p) = range.from_raw(entry.raw_position()) {
                points.push(p);
                if p > 0 {
                    points.push(p - 1);
                }
                if p < length {
                    points.push(p + 1);
                }
            }
        }
        let mut random = 0x6577833au64;
        for _ in 0..64 {
            random ^= random << 13;
            random ^= random >> 7;
            random ^= random << 17;
            points.push(random % (length + 1));
        }
        points.reverse();
        points.extend([length, 0, length / 2, 0, length]);
        let mut samples = 0usize;
        for &point in &points {
            file.seek_sample(point)?;
            assert_eq!(file.position(), point);
            if point == length {
                assert!(file.next_frame()?.is_none());
                continue;
            }
            let mut offset = point as usize;
            for _ in 0..2 {
                let Some(out) = file.next_frame()? else {
                    break;
                };
                assert_eq!(out.position, offset as u64);
                let index = linear.partition_point(|c| c.position <= offset as u64) - 1;
                let expected = &linear[index];
                let within = offset - expected.position as usize;
                assert_eq!(out.header, expected.header);
                for (c, plane) in out.pcm.planes().enumerate() {
                    assert!(
                        plane
                            .iter()
                            .zip(&expected.planes[c][within..within + plane.len()])
                            .all(|(a, b)| a.to_bits() == b.to_bits()),
                        "seek PCM differs at {point}, channel {c}"
                    );
                }
                offset += out.pcm.samples();
                samples += out.pcm.samples() * out.pcm.channels();
            }
        }
        println!("seeks\t{}\t{}\tbit-identical", points.len(), samples);
    }
    Ok(())
}
