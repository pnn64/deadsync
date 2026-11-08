//! Same-output native widths/Hound benchmark plus fresh-process physical-file RSS.
#[path = "support/native.rs"]
mod support;
use deadlib_audio_decode_wav::{
    Container, ForwardReader, Reader, SampleFormat, Spec, Wave, WriteOptions, Writer,
};
use std::{
    fs::{self, File},
    hint::black_box,
    io::{BufRead, BufReader, Cursor, Read},
    path::Path,
};
use support::{Numeric, bench, check, memory, period};
#[global_allocator]
static ALLOCATOR: support::Heap = support::Heap;

fn write_native<T: Numeric>(
    output: &mut [u8],
    samples: &[T],
    spec: Spec,
    options: WriteOptions,
    raw: Option<&[u8]>,
) -> usize {
    let mut w = Writer::new(Cursor::new(output), spec, options, &[]).unwrap();
    if let Some(raw) = raw {
        w.write_frames(raw).unwrap();
    } else {
        T::write(&mut w, samples).unwrap();
    }
    w.finish().unwrap().position() as usize
}
fn write_hound<T: Numeric>(
    output: &mut [u8],
    samples: &[T],
    spec: hound::WavSpec,
) -> Result<usize, hound::Error> {
    let mut sink = Cursor::new(output);
    let mut w = hound::WavWriter::new(&mut sink, spec)?;
    T::hound_write(&mut w, samples)?;
    w.finalize()?;
    // Hound leaves the cursor in its patched header, rather than at the end.
    let bytes = sink.into_inner();
    Ok(u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize + 8)
}
fn hound_full<T: Numeric>(encoded: &[u8], out: &mut [T]) -> Result<usize, hound::Error> {
    let mut r = hound::WavReader::new(Cursor::new(encoded))?;
    T::hound(&mut r, out)
}
fn benchmark<T: Numeric>(path: &Path, kind: &str, channels: usize, frames: u64, offset: usize) {
    let file = fs::read(path).unwrap();
    let mut arena = vec![0; offset];
    arena.extend(file);
    let encoded = &arena[offset..];
    let wave = Wave::parse(encoded).unwrap();
    let p = wave.pcm().unwrap();
    let f = wave.format;
    assert_eq!(usize::from(f.channels), channels);
    assert_eq!(p.frames(), frames);
    let count = frames as usize * channels;
    let width = usize::from(f.bits_per_sample / 8);
    let truth = period::<T>(&fs::read(path.with_extension("period")).unwrap());
    assert_eq!(truth.len(), 4096);
    let mut out = vec![T::default(); count];
    assert_eq!(T::pcm(p, 0, &mut out).unwrap(), count);
    check(&out, &truth, 0);
    let direct = match f.sample_format().unwrap() {
        SampleFormat::I16 => p.as_i16().is_some(),
        SampleFormat::I32 => p.as_i32().is_some(),
        SampleFormat::F32 => p.as_f32().is_some(),
        SampleFormat::F64 => p.as_f64().is_some(),
        _ => false,
    };
    let raw = p.as_bytes().unwrap();
    assert_eq!(raw.as_ptr(), wave.data().unwrap().as_ptr());
    assert_eq!(raw.len(), count * width);
    println!(
        "# frames={frames}; samples={count}; channels={channels}; input_wire_bytes={}; caller_output_bytes={}; memory_offset={offset}; direct_typed_available={direct}; raw_view_pointer_exact=true",
        raw.len(),
        count * T::WIDTH
    );
    println!(
        "operation\ttrial\titers\tns_per_op\tthread_cycles_per_op\twire_MiB_per_s\toutput_MiB_per_s\talloc_calls\trequested_bytes\tpeak_requested_live_bytes"
    );
    bench("wav/open_borrowed", 100000, 0, 0, || {
        black_box(Wave::parse(black_box(encoded)).unwrap());
    });
    bench("wav/raw_view_open", 100000, 0, 0, || {
        let w = Wave::parse(black_box(encoded)).unwrap();
        black_box(w.pcm().unwrap().as_bytes().unwrap());
    });
    bench("wav/raw_view_retained", 100000, 0, 0, || {
        black_box(black_box(&p).as_bytes().unwrap());
    });
    bench("wav/chunk_walk_open", 100000, 0, 0, || {
        let w = Wave::parse(black_box(encoded)).unwrap();
        let mut size = 0u64;
        for c in w.chunks().unwrap() {
            size = size.wrapping_add(c.unwrap().data.len() as u64);
        }
        black_box(size);
    });
    bench("wav/full_caller", 16, raw.len(), count * T::WIDTH, || {
        let w = Wave::parse(black_box(encoded)).unwrap();
        assert_eq!(
            T::pcm(w.pcm().unwrap(), 0, black_box(&mut out)).unwrap(),
            count
        );
    });
    check(&out, &truth, 0);
    let mut raw_out = vec![0; raw.len()];
    bench("wav/full_raw", 16, raw.len(), raw.len(), || {
        let w = Wave::parse(black_box(encoded)).unwrap();
        assert_eq!(
            w.pcm()
                .unwrap()
                .read_frames(0, black_box(&mut raw_out))
                .unwrap(),
            frames as usize
        );
    });
    assert_eq!(raw_out, raw);
    let batch = channels * (4096 / channels).max(1);
    let mut stream = vec![T::default(); batch];
    // Verify every chunk against independent truth before timing either I/O API.
    let mut reader = Reader::new(Cursor::new(encoded)).unwrap();
    let mut pos = 0;
    loop {
        let n = T::reader(&mut reader, &mut stream).unwrap();
        check(&stream[..n], &truth, pos);
        pos += n;
        if n == 0 {
            break;
        }
    }
    assert_eq!(pos, count);
    let mut forward = ForwardReader::new(encoded).unwrap();
    let mut pos = 0;
    loop {
        let n = T::forward(&mut forward, &mut stream).unwrap();
        check(&stream[..n], &truth, pos);
        pos += n;
        if n == 0 {
            break;
        }
    }
    forward.finish().unwrap();
    assert_eq!(pos, count);
    bench("wav/stream4096", 8, raw.len(), count * T::WIDTH, || {
        let mut r = Reader::new(Cursor::new(black_box(encoded))).unwrap();
        let mut done = 0;
        loop {
            let n = T::reader(&mut r, black_box(&mut stream)).unwrap();
            done += n;
            if n == 0 {
                break;
            }
        }
        assert_eq!(done, count);
    });
    bench(
        "wav/forward4096_finish",
        8,
        raw.len(),
        count * T::WIDTH,
        || {
            let mut r = ForwardReader::new(black_box(encoded)).unwrap();
            let mut done = 0;
            loop {
                let n = T::forward(&mut r, black_box(&mut stream)).unwrap();
                done += n;
                if n == 0 {
                    break;
                }
            }
            r.finish().unwrap();
            assert_eq!(done, count);
        },
    );
    let range_frames = frames.min(256) as usize;
    let mut range = vec![T::default(); range_frames * channels];
    let mut next = 0u64;
    bench(
        "wav/seek256",
        25000,
        range_frames * channels * width,
        range.len() * T::WIDTH,
        || {
            next = (next + 7919) % (frames - range_frames as u64 + 1);
            reader.seek_frame(black_box(next)).unwrap();
            assert_eq!(
                T::reader(&mut reader, black_box(&mut range)).unwrap(),
                range_frames * channels
            );
        },
    );
    check(&range, &truth, next as usize * channels);
    let hound_exact = match hound_full(encoded, &mut out) {
        Ok(n) if n == count => out
            .iter()
            .enumerate()
            .all(|(i, &v)| v.bits() == truth[i % truth.len()].bits()),
        Ok(_) => false,
        Err(_) => false,
    };
    match hound::WavReader::new(Cursor::new(encoded)) {
        Ok(_) => {
            println!("# hound_open=ok; hound_decode_exact={hound_exact}");
            bench("hound/open", 100000, 0, 0, || {
                black_box(hound::WavReader::new(Cursor::new(black_box(encoded))).unwrap());
            });
        }
        Err(e) => println!("# hound_open=unsupported; reason={e}; hound_decode_exact=false"),
    }
    if hound_exact {
        bench("hound/full_caller", 4, raw.len(), count * T::WIDTH, || {
            assert_eq!(
                hound_full(black_box(encoded), black_box(&mut out)).unwrap(),
                count
            );
        });
        check(&out, &truth, 0);
        let mut r = hound::WavReader::new(Cursor::new(encoded)).unwrap();
        let mut pos = 0;
        loop {
            let n = T::hound(&mut r, &mut stream).unwrap();
            check(&stream[..n], &truth, pos);
            pos += n;
            if n == 0 {
                break;
            }
        }
        assert_eq!(pos, count);
        bench("hound/stream4096", 4, raw.len(), count * T::WIDTH, || {
            let mut r = hound::WavReader::new(Cursor::new(black_box(encoded))).unwrap();
            let mut done = 0;
            loop {
                let n = T::hound(&mut r, black_box(&mut stream)).unwrap();
                done += n;
                if n == 0 {
                    break;
                }
            }
            assert_eq!(done, count);
        });
        let mut next = 0u64;
        bench(
            "hound/seek256",
            25000,
            range_frames * channels * width,
            range.len() * T::WIDTH,
            || {
                next = (next + 7919) % (frames - range_frames as u64 + 1);
                r.seek(black_box(next as u32)).unwrap();
                assert_eq!(
                    T::hound(&mut r, black_box(&mut range)).unwrap(),
                    range_frames * channels
                );
            },
        );
        check(&range, &truth, next as usize * channels);
    }
    // Restore canonical caller samples even if a reference rejected/partially decoded.
    for (i, v) in out.iter_mut().enumerate() {
        *v = truth[i % truth.len()];
    }
    let spec = Spec {
        sample_format: f.sample_format().unwrap(),
        channels: f.channels,
        sample_rate: f.sample_rate,
        valid_bits: f.valid_bits,
        channel_mask: f.channel_mask,
    };
    let options = WriteOptions {
        container: wave.container,
        auto_rf64: false,
        force_extensible: f.tag == 0xfffe,
    };
    let mut output = vec![0; raw.len() + 1024];
    bench("wav/encode_raw", 16, raw.len(), count * T::WIDTH, || {
        black_box(write_native(&mut output, &out, spec, options, Some(raw)));
    });
    assert_eq!(Wave::parse(&output).unwrap().data().unwrap(), raw);
    bench("wav/encode_typed", 8, raw.len(), count * T::WIDTH, || {
        black_box(write_native(&mut output, &out, spec, options, None));
    });
    assert_eq!(Wave::parse(&output).unwrap().data().unwrap(), raw);
    let hs = hound::WavSpec {
        channels: f.channels,
        sample_rate: f.sample_rate,
        bits_per_sample: f.bits_per_sample,
        sample_format: if spec.sample_format.is_float() {
            hound::SampleFormat::Float
        } else {
            hound::SampleFormat::Int
        },
    };
    let he = wave.container == Container::Riff
        && f.valid_bits == f.bits_per_sample
        && write_hound(&mut output, &out, hs).is_ok();
    println!("# hound_encode_comparable={he}");
    if he {
        let n = write_hound(&mut output, &out, hs).unwrap();
        assert_eq!(Wave::parse(&output[..n]).unwrap().data().unwrap(), raw);
        bench(
            "hound/encode_sample",
            4,
            raw.len(),
            count * T::WIDTH,
            || {
                black_box(write_hound(&mut output, &out, hs).unwrap());
            },
        );
        assert_eq!(Wave::parse(&output).unwrap().data().unwrap(), raw);
    }
    if kind == "i16" {
        let values = (0..count)
            .map(|i| truth[i % truth.len()].bits() as u32 as i32 as i16)
            .collect::<Vec<_>>();
        let mut native = vec![0; count];
        bench("wav/full_native_i16", 16, raw.len(), raw.len(), || {
            let w = Wave::parse(black_box(encoded)).unwrap();
            assert_eq!(
                w.pcm()
                    .unwrap()
                    .read_i16(0, black_box(&mut native))
                    .unwrap(),
                count
            );
        });
        assert_eq!(native, values);
        if hound_exact {
            bench("hound/full_native_i16", 4, raw.len(), raw.len(), || {
                let mut r = hound::WavReader::new(Cursor::new(black_box(encoded))).unwrap();
                for (o, v) in black_box(&mut native).iter_mut().zip(r.samples::<i16>()) {
                    *o = v.unwrap();
                }
            });
            assert_eq!(native, values);
        }
        bench("wav/encode_native_i16", 16, raw.len(), raw.len(), || {
            let mut w =
                Writer::new(Cursor::new(black_box(&mut output[..])), spec, options, &[]).unwrap();
            w.write_i16(black_box(&values)).unwrap();
            black_box(w.finish().unwrap());
        });
        assert_eq!(Wave::parse(&output).unwrap().data().unwrap(), raw);
        if he {
            bench("hound/encode_i16_blocks", 4, raw.len(), raw.len(), || {
                let mut sink = Cursor::new(black_box(&mut output[..]));
                let mut w = hound::WavWriter::new(&mut sink, hs).unwrap();
                for b in black_box(&values).chunks(4096) {
                    let mut batch = w.get_i16_writer(b.len() as u32);
                    for &s in b {
                        // SAFETY: exactly the number of samples requested for
                        // this slice is written before flushing the block.
                        unsafe {
                            batch.write_sample_unchecked(s);
                        }
                    }
                    batch.flush().unwrap();
                }
                w.finalize().unwrap();
            });
            assert_eq!(Wave::parse(&output).unwrap().data().unwrap(), raw);
        }
    }
}

fn rss<T: Numeric>(path: &Path, channels: usize, frames: u64, backend: &str) {
    let truth = period::<T>(&fs::read(path.with_extension("period")).unwrap());
    let mut out = vec![T::default(); channels * (4096 / channels).max(1)];
    for o in &mut out {
        *o = truth[0];
    }
    black_box(&mut out);
    let mut raw = vec![0; 65536];
    raw.fill(0x35);
    black_box(&mut raw);
    let mut source = BufReader::with_capacity(65536, File::open(path).unwrap());
    source.fill_buf().unwrap();
    black_box(source.buffer());
    let before = memory();
    let start = std::time::Instant::now();
    let mut samples = 0usize;
    let mut baseline_bytes = 0u64;
    let after = match backend {
        "wav" => {
            let mut r = Reader::new(source).unwrap();
            loop {
                let n = T::reader(&mut r, &mut out).unwrap();
                check(&out[..n], &truth, samples);
                samples += n;
                if n == 0 {
                    break;
                }
            }
            let m = memory();
            black_box(&r);
            m
        }
        "hound" => {
            let mut r = hound::WavReader::new(source).unwrap();
            loop {
                let n = T::hound(&mut r, &mut out).unwrap();
                check(&out[..n], &truth, samples);
                samples += n;
                if n == 0 {
                    break;
                }
            }
            let m = memory();
            black_box(&r);
            m
        }
        "baseline" => {
            let mut source = source;
            loop {
                let n = source.read(&mut raw).unwrap();
                baseline_bytes += n as u64;
                if n == 0 {
                    break;
                }
            }
            let m = memory();
            black_box(&source);
            m
        }
        _ => panic!("unknown RSS backend"),
    };
    if backend != "baseline" {
        assert_eq!(samples, frames as usize * channels);
    } else {
        assert_eq!(baseline_bytes, fs::metadata(path).unwrap().len());
    }
    println!(
        "available\tbefore_working_set\tafter_working_set\tpeak_working_set\tbefore_private\tafter_private\tsamples\tbaseline_bytes\telapsed_ns"
    );
    println!(
        "{}\t{}\t{}\t{}\t{}\t{}\t{samples}\t{baseline_bytes}\t{}",
        cfg!(windows),
        before.working_set,
        after.working_set,
        after.peak_working_set,
        before.private_bytes,
        after.private_bytes,
        start.elapsed().as_nanos()
    );
    black_box((out, raw));
}
fn disk<T: Numeric>(path: &Path, kind: &str, channels: usize, frames: u64, backend: &str) {
    let truth = period::<T>(&fs::read(path.with_extension("period")).unwrap());
    let count = frames as usize * channels;
    let width = match kind {
        "u8" => 1,
        "i16" => 2,
        "i24" => 3,
        "i32" | "f32" => 4,
        "f64" => 8,
        _ => unreachable!(),
    };
    let mut out = vec![T::default(); channels * (4096 / channels).max(1)];
    let open = || BufReader::with_capacity(65536, File::open(path).unwrap());
    if backend == "wav" {
        let mut r = Reader::new(open()).unwrap();
        let mut pos = 0;
        loop {
            let n = T::reader(&mut r, &mut out).unwrap();
            check(&out[..n], &truth, pos);
            pos += n;
            if n == 0 {
                break;
            }
        }
        assert_eq!(pos, count);
        println!(
            "operation\ttrial\titers\tns_per_op\tthread_cycles_per_op\twire_MiB_per_s\toutput_MiB_per_s\talloc_calls\trequested_bytes\tpeak_requested_live_bytes"
        );
        bench(
            "wav/cached_file_stream4096",
            if count * width >= (64 << 20) { 2 } else { 64 },
            count * width,
            count * T::WIDTH,
            || {
                let mut r = Reader::new(open()).unwrap();
                let mut done = 0;
                loop {
                    let n = T::reader(&mut r, black_box(&mut out)).unwrap();
                    done += n;
                    if n == 0 {
                        break;
                    }
                }
                assert_eq!(done, count);
            },
        );
    } else {
        let mut r = hound::WavReader::new(open()).unwrap();
        let mut pos = 0;
        loop {
            let n = T::hound(&mut r, &mut out).unwrap();
            check(&out[..n], &truth, pos);
            pos += n;
            if n == 0 {
                break;
            }
        }
        assert_eq!(pos, count);
        println!(
            "operation\ttrial\titers\tns_per_op\tthread_cycles_per_op\twire_MiB_per_s\toutput_MiB_per_s\talloc_calls\trequested_bytes\tpeak_requested_live_bytes"
        );
        bench(
            "hound/cached_file_stream4096",
            if count * width >= (64 << 20) { 2 } else { 64 },
            count * width,
            count * T::WIDTH,
            || {
                let mut r = hound::WavReader::new(open()).unwrap();
                let mut done = 0;
                loop {
                    let n = T::hound(&mut r, black_box(&mut out)).unwrap();
                    done += n;
                    if n == 0 {
                        break;
                    }
                }
                assert_eq!(done, count);
            },
        );
    }
}
fn main() {
    let a = std::env::args().collect::<Vec<_>>();
    if a.len() == 1 || (a.len() == 2 && a[1] == "--test") {
        eprintln!("native benchmark fixtures: run benches/run_native.py");
        return;
    }
    assert!(
        a.len() >= 7,
        "native bench|rss PATH KIND CHANNELS FRAMES MEMORY_OFFSET [BACKEND]"
    );
    let path = Path::new(&a[2]);
    let channels = a[4].parse().unwrap();
    let frames = a[5].parse().unwrap();
    let offset = a[6].parse().unwrap();
    macro_rules! run {
        ($t:ty) => {
            if a[1] == "rss" {
                rss::<$t>(path, channels, frames, &a[7]);
            } else if a[1] == "disk" {
                disk::<$t>(path, &a[3], channels, frames, &a[7]);
            } else {
                benchmark::<$t>(path, &a[3], channels, frames, offset);
            }
        };
    }
    match a[3].as_str() {
        "f32" => run!(f32),
        "f64" => run!(f64),
        _ => run!(i32),
    }
}
