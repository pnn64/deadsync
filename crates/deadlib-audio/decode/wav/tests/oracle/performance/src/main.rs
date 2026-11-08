//! Reference-only same-output WAV measurements; never a production dependency.
#[path = "../../../../benches/support/affinity.rs"]
mod affinity;
mod measure;
use deadlib_audio_decode_wav::{AdpcmData, AdpcmReader, Reader, Wave};
use std::{
    fs::{self, File},
    hint::black_box,
    io::{BufRead, BufReader, Cursor, Read, Seek, SeekFrom},
    path::Path,
    sync::Arc,
};
use symphonia::core::{
    audio::{Audio, GenericAudioBufferRef},
    codecs::audio::{AudioDecoder, AudioDecoderOptions},
    formats::{FormatOptions, FormatReader, SeekMode, SeekTo, TrackType, probe::Hint},
    io::{MediaSource, MediaSourceStream},
    meta::MetadataOptions,
    units::Timestamp,
};

#[global_allocator]
static ALLOCATOR: measure::Heap = measure::Heap;
type Result<T> = std::result::Result<T, String>;
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}

struct Config {
    compressed: bool,
    channels: usize,
    frames: u64,
    bits: u32,
    offset: u64,
}
impl Config {
    fn samples(&self) -> usize {
        self.frames as usize * self.channels
    }
}
enum Native<R> {
    Pcm(Reader<R>),
    Adpcm(AdpcmReader<R>),
}
impl<R: Read + Seek> Native<R> {
    fn new(source: R, cfg: &Config) -> Result<Self> {
        let r = if cfg.compressed {
            Self::Adpcm(AdpcmReader::new(source).map_err(error)?)
        } else {
            Self::Pcm(Reader::new(source).map_err(error)?)
        };
        let frames = match &r {
            Self::Adpcm(r) => r.frames(),
            Self::Pcm(r) => r.frames().map_err(error)?,
        };
        if frames != cfg.frames {
            return Err(format!("native duration {frames} != {}", cfg.frames));
        }
        Ok(r)
    }
    fn seek(&mut self, frame: u64) -> Result<()> {
        match self {
            Self::Pcm(r) => r.seek_frame(frame).map_err(error),
            Self::Adpcm(r) => r.seek_frame(frame).map_err(error),
        }
    }
}
struct Reference {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track: u32,
}
impl Reference {
    fn new(source: Box<dyn MediaSource>) -> Result<Self> {
        let mut hint = Hint::new();
        hint.with_extension("wav");
        let format = symphonia::default::get_probe()
            .probe(
                &hint,
                MediaSourceStream::new(source, Default::default()),
                FormatOptions::default(),
                MetadataOptions::default(),
            )
            .map_err(error)?;
        let track = format
            .default_track(TrackType::Audio)
            .ok_or("no audio track")?;
        let id = track.id;
        let params = track
            .codec_params
            .as_ref()
            .and_then(|p| p.audio())
            .ok_or("no audio parameters")?;
        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(params, &AudioDecoderOptions::default())
            .map_err(error)?;
        Ok(Self {
            format,
            decoder,
            track: id,
        })
    }
    fn drain<T: Numeric>(
        &mut self,
        cfg: &Config,
        out: &mut [T],
        stream: bool,
        mut consume: impl FnMut(&[T], usize) -> Result<()>,
    ) -> Result<usize> {
        let mut done = 0;
        while let Some(packet) = self.format.next_packet().map_err(error)? {
            if packet.track_id != self.track {
                continue;
            }
            let audio = self.decoder.decode(&packet).map_err(error)?;
            let frames = audio.frames();
            if audio.num_planes() != cfg.channels {
                return Err("reference channel count".into());
            }
            let mut at = 0;
            while at < frames {
                let base = if stream { 0 } else { done };
                let n = (frames - at).min((out.len() - base) / cfg.channels);
                if n == 0 || done + n * cfg.channels > cfg.samples() {
                    return Err("reference output exceeds expected duration".into());
                }
                let dst = &mut out[base..base + n * cfg.channels];
                T::copy(&audio, at, dst, cfg.bits)?;
                consume(dst, done)?;
                done += dst.len();
                at += n;
            }
        }
        black_box(self.decoder.finalize());
        if done != cfg.samples() {
            return Err(format!("reference samples {done} != {}", cfg.samples()));
        }
        Ok(done)
    }
    fn range<T: Numeric>(&mut self, cfg: &Config, start: u64, out: &mut [T]) -> Result<()> {
        let result = self
            .format
            .seek(
                SeekMode::Accurate,
                SeekTo::Timestamp {
                    ts: Timestamp::new(start as i64),
                    track_id: self.track,
                },
            )
            .map_err(error)?;
        if result.actual_ts.get() < 0 || result.actual_ts.get() as u64 > start {
            return Err("reference accurate seek passed target".into());
        }
        self.decoder.reset();
        let mut done = 0;
        while done < out.len() {
            let packet = self
                .format
                .next_packet()
                .map_err(error)?
                .ok_or("reference seek reached EOF")?;
            if packet.track_id != self.track {
                continue;
            }
            let position = packet.pts.get();
            if position < 0 || position as u64 > start + (done / cfg.channels) as u64 {
                return Err("reference packet timestamp passed target".into());
            }
            let audio = self.decoder.decode(&packet).map_err(error)?;
            if audio.num_planes() != cfg.channels {
                return Err("reference seek channel count".into());
            }
            let skip = (start + (done / cfg.channels) as u64 - position as u64) as usize;
            if skip >= audio.frames() {
                continue;
            }
            let frames = (audio.frames() - skip).min((out.len() - done) / cfg.channels);
            T::copy(
                &audio,
                skip,
                &mut out[done..done + frames * cfg.channels],
                cfg.bits,
            )?;
            done += frames * cfg.channels;
        }
        Ok(())
    }
}

trait Numeric: Copy + Default {
    fn bits(self) -> u64;
    fn word(b: &[u8]) -> Self;
    fn canonical(self, _: u32) -> Self {
        self
    }
    fn read<R: Read + Seek>(r: &mut Native<R>, out: &mut [Self], bits: u32) -> Result<usize>;
    fn borrowed(w: &Wave<'_>, out: &mut [Self], scratch: &mut [i16], bits: u32) -> Result<usize>;
    fn copy(
        audio: &GenericAudioBufferRef<'_>,
        start: usize,
        out: &mut [Self],
        bits: u32,
    ) -> Result<()>;
}
impl Numeric for i32 {
    fn canonical(self, bits: u32) -> Self {
        self << (32 - bits)
    }
    fn bits(self) -> u64 {
        self as u32 as u64
    }
    fn word(b: &[u8]) -> Self {
        Self::from_le_bytes(b.try_into().unwrap())
    }
    fn read<R: Read + Seek>(r: &mut Native<R>, out: &mut [Self], bits: u32) -> Result<usize> {
        let Native::Pcm(r) = r else {
            return Err("integer PCM expected".into());
        };
        let n = r.read_integers(out).map_err(error)?;
        if bits != 32 {
            for v in &mut out[..n] {
                *v <<= 32 - bits;
            }
        }
        Ok(n)
    }
    fn borrowed(w: &Wave<'_>, out: &mut [Self], _: &mut [i16], bits: u32) -> Result<usize> {
        let n = w.pcm().map_err(error)?.read_i32(0, out).map_err(error)?;
        if bits != 32 {
            for v in &mut out[..n] {
                *v <<= 32 - bits;
            }
        }
        Ok(n)
    }
    fn copy(
        audio: &GenericAudioBufferRef<'_>,
        start: usize,
        out: &mut [Self],
        _: u32,
    ) -> Result<()> {
        let frames = out.len() / audio.num_planes();
        match audio {
            GenericAudioBufferRef::U8(a) => a
                .slice(start..start + frames)
                .copy_to_slice_interleaved(out),
            GenericAudioBufferRef::U16(a) => a
                .slice(start..start + frames)
                .copy_to_slice_interleaved(out),
            GenericAudioBufferRef::U24(a) => a
                .slice(start..start + frames)
                .copy_to_slice_interleaved(out),
            GenericAudioBufferRef::U32(a) => a
                .slice(start..start + frames)
                .copy_to_slice_interleaved(out),
            GenericAudioBufferRef::S8(a) => a
                .slice(start..start + frames)
                .copy_to_slice_interleaved(out),
            GenericAudioBufferRef::S16(a) => a
                .slice(start..start + frames)
                .copy_to_slice_interleaved(out),
            GenericAudioBufferRef::S24(a) => a
                .slice(start..start + frames)
                .copy_to_slice_interleaved(out),
            GenericAudioBufferRef::S32(a) => a
                .slice(start..start + frames)
                .copy_to_slice_interleaved(out),
            _ => return Err("reference integer representation".into()),
        }
        Ok(())
    }
}
impl Numeric for i16 {
    fn bits(self) -> u64 {
        self as u16 as u64
    }
    fn word(b: &[u8]) -> Self {
        Self::from_le_bytes(b.try_into().unwrap())
    }
    fn read<R: Read + Seek>(r: &mut Native<R>, out: &mut [Self], _: u32) -> Result<usize> {
        let Native::Adpcm(r) = r else {
            return Err("ADPCM expected".into());
        };
        r.read_i16(out).map_err(error)
    }
    fn borrowed(w: &Wave<'_>, out: &mut [Self], scratch: &mut [i16], _: u32) -> Result<usize> {
        AdpcmData::new(w)
            .map_err(error)?
            .read_frames(0, out, scratch)
            .map_err(error)
    }
    fn copy(
        audio: &GenericAudioBufferRef<'_>,
        start: usize,
        out: &mut [Self],
        _: u32,
    ) -> Result<()> {
        let GenericAudioBufferRef::S32(a) = audio else {
            return Err("reference ADPCM representation".into());
        };
        let frames = out.len() / a.num_planes();
        a.slice(start..start + frames)
            .copy_to_slice_interleaved(out);
        Ok(())
    }
}
macro_rules! floating {
    ($t:ty,$int:ty,$variant:ident,$method:ident) => {
        impl Numeric for $t {
            fn bits(self) -> u64 {
                self.to_bits() as u64
            }
            fn word(b: &[u8]) -> Self {
                Self::from_bits(<$int>::from_le_bytes(b.try_into().unwrap()))
            }
            fn read<R: Read + Seek>(r: &mut Native<R>, out: &mut [Self], _: u32) -> Result<usize> {
                let Native::Pcm(r) = r else {
                    return Err("float PCM expected".into());
                };
                r.$method(out).map_err(error)
            }
            fn borrowed(w: &Wave<'_>, out: &mut [Self], _: &mut [i16], _: u32) -> Result<usize> {
                w.pcm().map_err(error)?.$method(0, out).map_err(error)
            }
            fn copy(
                audio: &GenericAudioBufferRef<'_>,
                start: usize,
                out: &mut [Self],
                _: u32,
            ) -> Result<()> {
                let GenericAudioBufferRef::$variant(a) = audio else {
                    return Err("reference float representation".into());
                };
                let frames = out.len() / a.num_planes();
                a.slice(start..start + frames)
                    .copy_to_slice_interleaved(out);
                Ok(())
            }
        }
    };
}
floating!(f32, u32, F32, read_f32);
floating!(f64, u64, F64, read_f64);

fn check<T: Numeric>(out: &[T], truth: &[T], start: usize) -> Result<()> {
    for (i, value) in out.iter().enumerate() {
        let expected = truth[(start + i) % truth.len()].bits();
        if value.bits() != expected {
            return Err(format!(
                "sample {}: {:x} != {:x}",
                start + i,
                value.bits(),
                expected
            ));
        }
    }
    Ok(())
}
fn resident(input: &Arc<[u8]>, cfg: &Config) -> Cursor<Arc<[u8]>> {
    let mut source = Cursor::new(Arc::clone(input));
    source.set_position(cfg.offset);
    source
}
fn native_drain<T: Numeric, R: Read + Seek>(
    r: &mut Native<R>,
    cfg: &Config,
    out: &mut [T],
    stream: bool,
    mut consume: impl FnMut(&[T], usize) -> Result<()>,
) -> Result<usize> {
    let mut done = 0;
    loop {
        let base = if stream { 0 } else { done };
        let n = T::read(r, &mut out[base..], cfg.bits)?;
        consume(&out[base..base + n], done)?;
        done += n;
        if n == 0 {
            break;
        }
    }
    if done != cfg.samples() {
        return Err("native sample count".into());
    }
    Ok(done)
}
fn gate(name: &str, result: &Result<()>) {
    match result {
        Ok(()) => println!("# gate\t{name}\texact"),
        Err(e) => println!("# gate\t{name}\terror\t{e:?}"),
    }
}
fn benchmark<T: Numeric>(input: Arc<[u8]>, cfg: &Config, truth: &[T], wire: usize) -> Result<()> {
    let mut full = vec![T::default(); cfg.samples()];
    let mut stream = vec![T::default(); (4096 / cfg.channels).max(1) * cfg.channels];
    let wave = Wave::parse(&input[cfg.offset as usize..]).map_err(error)?;
    let mut scratch = if cfg.compressed {
        vec![
            0;
            AdpcmData::new(&wave)
                .map_err(error)?
                .codec()
                .scratch_samples()
        ]
    } else {
        Vec::new()
    };
    let n = T::borrowed(&wave, &mut full, &mut scratch, cfg.bits)?;
    if n != cfg.samples() {
        return Err("borrowed sample count".into());
    }
    check(&full, truth, 0)?;
    let mut native = Native::new(resident(&input, cfg), cfg)?;
    native_drain(&mut native, cfg, &mut full, false, |b, p| {
        check(b, truth, p)
    })?;
    let mut native = Native::new(resident(&input, cfg), cfg)?;
    native_drain(&mut native, cfg, &mut stream, true, |b, p| {
        check(b, truth, p)
    })?;
    gate("native/full_stream", &Ok(()));
    let sym_full = Reference::new(Box::new(resident(&input, cfg))).and_then(|mut r| {
        r.drain(cfg, &mut full, false, |b, p| check(b, truth, p))
            .map(|_| ())
    });
    gate("symphonia/full", &sym_full);
    let sym_stream = if sym_full.is_ok() {
        Reference::new(Box::new(resident(&input, cfg))).and_then(|mut r| {
            r.drain(cfg, &mut stream, true, |b, p| check(b, truth, p))
                .map(|_| ())
        })
    } else {
        Err("full gate did not pass".into())
    };
    gate("symphonia/stream", &sym_stream);
    println!(
        "operation\ttrial\titers\tns_per_op\tthread_cycles_per_op\twire_MiB_per_s\toutput_MiB_per_s\talloc_calls\trequested_bytes\tpeak_requested_live_bytes"
    );
    let output = cfg.samples() * std::mem::size_of::<T>();
    let iters = if output > 8 << 20 { 4 } else { 8 };
    measure::bench("native/open", 20000, 0, 0, || {
        black_box(Native::new(resident(&input, cfg), cfg).unwrap());
    });
    measure::bench("native/borrowed_complete", iters, wire, output, || {
        let w = Wave::parse(black_box(&input[cfg.offset as usize..])).unwrap();
        assert_eq!(
            T::borrowed(&w, black_box(&mut full), &mut scratch, cfg.bits).unwrap(),
            cfg.samples()
        );
    });
    measure::bench("native/full", iters, wire, output, || {
        let mut r = Native::new(resident(&input, cfg), cfg).unwrap();
        native_drain(&mut r, cfg, black_box(&mut full), false, |b, _| {
            black_box(b);
            Ok(())
        })
        .unwrap();
    });
    measure::bench("native/stream4096", iters, wire, output, || {
        let mut r = Native::new(resident(&input, cfg), cfg).unwrap();
        native_drain(&mut r, cfg, black_box(&mut stream), true, |b, _| {
            black_box(b);
            Ok(())
        })
        .unwrap();
    });
    if sym_full.is_ok() {
        measure::bench("symphonia/open", 20000, 0, 0, || {
            black_box(Reference::new(Box::new(resident(&input, cfg))).unwrap());
        });
        measure::bench("symphonia/full", iters, wire, output, || {
            let mut r = Reference::new(Box::new(resident(&input, cfg))).unwrap();
            r.drain(cfg, black_box(&mut full), false, |b, _| {
                black_box(b);
                Ok(())
            })
            .unwrap();
        });
    }
    if sym_stream.is_ok() {
        measure::bench("symphonia/stream4096", iters, wire, output, || {
            let mut r = Reference::new(Box::new(resident(&input, cfg))).unwrap();
            r.drain(cfg, black_box(&mut stream), true, |b, _| {
                black_box(b);
                Ok(())
            })
            .unwrap();
        });
    }
    let positions: Vec<_> = (0..128)
        .map(|i| (i * 7919 + 17) % (cfg.frames - 32))
        .collect();
    let mut short = vec![T::default(); 32 * cfg.channels];
    let mut native = Native::new(resident(&input, cfg), cfg)?;
    for &at in &positions {
        native.seek(at)?;
        if T::read(&mut native, &mut short, cfg.bits)? != short.len() {
            return Err("native seek count".into());
        }
        check(&short, truth, at as usize * cfg.channels)?;
    }
    gate("native/seek128", &Ok(()));
    measure::bench("native/seek128_retained", 16, 0, 0, || {
        for &at in &positions {
            native.seek(at).unwrap();
            assert_eq!(
                T::read(&mut native, black_box(&mut short), cfg.bits).unwrap(),
                short.len()
            );
        }
    });
    if sym_full.is_ok() {
        let mut r = Reference::new(Box::new(resident(&input, cfg)))?;
        let result = positions.iter().try_for_each(|&at| {
            r.range(cfg, at, &mut short)
                .map_err(|e| format!("frame {at}: {e}"))?;
            check(&short, truth, at as usize * cfg.channels)
        });
        gate("symphonia/seek128", &result);
        if result.is_ok() {
            measure::bench("symphonia/seek128_retained", 16, 0, 0, || {
                for &at in &positions {
                    r.range(cfg, at, black_box(&mut short)).unwrap();
                }
            });
        }
    }
    Ok(())
}

struct FileSource {
    inner: BufReader<File>,
    bytes: u64,
}
impl Read for FileSource {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        self.inner.read(out)
    }
}
impl Seek for FileSource {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(position)
    }
}
impl MediaSource for FileSource {
    fn is_seekable(&self) -> bool {
        true
    }
    fn byte_len(&self) -> Option<u64> {
        Some(self.bytes)
    }
}
fn open_file(path: &Path) -> Result<FileSource> {
    let file = File::open(path).map_err(error)?;
    let bytes = file.metadata().map_err(error)?.len();
    Ok(FileSource {
        inner: BufReader::with_capacity(65536, file),
        bytes,
    })
}
fn file_work<T: Numeric>(
    source: FileSource,
    cfg: &Config,
    backend: &str,
    observe: bool,
    out: &mut [T],
    mut consume: impl FnMut(&[T], usize) -> Result<()>,
) -> Result<measure::Memory> {
    match backend {
        "native" => {
            let mut r = Native::new(source, cfg)?;
            native_drain(&mut r, cfg, out, true, &mut consume)?;
            let memory = if observe {
                measure::memory()
            } else {
                measure::Memory::default()
            };
            black_box(&r);
            Ok(memory)
        }
        "symphonia" => {
            let mut r = Reference::new(Box::new(source))?;
            r.drain(cfg, out, true, &mut consume)?;
            let memory = if observe {
                measure::memory()
            } else {
                measure::Memory::default()
            };
            black_box(&r);
            Ok(memory)
        }
        _ => Err("unknown file backend".into()),
    }
}
fn rss<T: Numeric>(path: &Path, cfg: &Config, truth: &[T], backend: &str) -> Result<()> {
    let mut out = vec![truth[0]; (4096 / cfg.channels).max(1) * cfg.channels];
    let mut raw = vec![0x35; 65536];
    black_box((&mut out, &mut raw));
    let mut source = open_file(path)?;
    source.inner.fill_buf().map_err(error)?;
    black_box(source.inner.buffer());
    let before = measure::memory();
    let start = std::time::Instant::now();
    let mut baseline_bytes = 0;
    let after = if backend == "baseline" {
        loop {
            let n = source.read(&mut raw).map_err(error)?;
            baseline_bytes += n as u64;
            if n == 0 {
                break;
            }
        }
        if baseline_bytes != source.bytes {
            return Err("baseline file byte count".into());
        }
        let memory = measure::memory();
        black_box(&source);
        memory
    } else {
        file_work(source, cfg, backend, true, &mut out, |b, p| {
            check(b, truth, p)
        })?
    };
    println!(
        "available\tbefore_working_set\tafter_working_set\tpeak_working_set\tbefore_private\tafter_private\tsamples\tbaseline_bytes\telapsed_ns"
    );
    println!(
        "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{baseline_bytes}\t{}",
        cfg!(windows),
        before.working_set,
        after.working_set,
        after.peak_working_set,
        before.private_bytes,
        after.private_bytes,
        if backend == "baseline" {
            0
        } else {
            cfg.samples()
        },
        start.elapsed().as_nanos()
    );
    black_box((out, raw));
    Ok(())
}
fn disk<T: Numeric>(
    path: &Path,
    cfg: &Config,
    truth: &[T],
    wire: usize,
    backend: &str,
) -> Result<()> {
    let mut out = vec![T::default(); (4096 / cfg.channels).max(1) * cfg.channels];
    file_work(open_file(path)?, cfg, backend, false, &mut out, |b, p| {
        check(b, truth, p)
    })?;
    println!(
        "operation\ttrial\titers\tns_per_op\tthread_cycles_per_op\twire_MiB_per_s\toutput_MiB_per_s\talloc_calls\trequested_bytes\tpeak_requested_live_bytes"
    );
    measure::bench(
        &format!("{backend}/cached_file_stream4096"),
        if wire >= 64 << 20 { 2 } else { 16 },
        wire,
        cfg.samples() * std::mem::size_of::<T>(),
        || {
            file_work(
                open_file(path).unwrap(),
                cfg,
                backend,
                false,
                black_box(&mut out),
                |b, _| {
                    black_box(b);
                    Ok(())
                },
            )
            .unwrap();
        },
    );
    Ok(())
}
fn run<T: Numeric>(
    mode: &str,
    path: &Path,
    cfg: &Config,
    wire: usize,
    backend: &str,
) -> Result<()> {
    let raw = fs::read(path.with_extension("period")).map_err(error)?;
    if !raw.len().is_multiple_of(std::mem::size_of::<T>()) {
        return Err("truth word width".into());
    }
    let truth: Vec<T> = raw
        .chunks_exact(std::mem::size_of::<T>())
        .map(|b| T::word(b).canonical(cfg.bits))
        .collect();
    if mode == "bench" {
        let mut input = vec![0; cfg.offset as usize];
        input.extend(fs::read(path).map_err(error)?);
        benchmark(Arc::from(input), cfg, &truth, wire)
    } else if cfg.offset != 0 {
        Err("file mode requires zero memory offset".into())
    } else if mode == "rss" {
        rss(path, cfg, &truth, backend)
    } else if mode == "disk" {
        disk(path, cfg, &truth, wire, backend)
    } else {
        Err("unknown mode".into())
    }
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 10 {
        return Err("MODE PATH KIND CHANNELS FRAMES BITS MEMORY_OFFSET WIRE_BYTES BACKEND".into());
    }
    if args[1] != "rss" {
        println!(
            "# thread_affinity_mask={:#x}; frequency uncontrolled",
            affinity::pin_thread()
        );
    }
    let kind = &args[3];
    let cfg = Config {
        compressed: kind.starts_with("ms") || kind.starts_with("ima"),
        channels: args[4].parse().unwrap(),
        frames: args[5].parse().unwrap(),
        bits: args[6].parse().unwrap(),
        offset: args[7].parse().unwrap(),
    };
    let path = Path::new(&args[2]);
    let wire = args[8].parse().unwrap();
    let mode = &args[1];
    let backend = &args[9];
    match kind.as_str() {
        "f32" => run::<f32>(mode, path, &cfg, wire, backend),
        "f64" => run::<f64>(mode, path, &cfg, wire, backend),
        _ if cfg.compressed => run::<i16>(mode, path, &cfg, wire, backend),
        _ => run::<i32>(mode, path, &cfg, wire, backend),
    }
}
