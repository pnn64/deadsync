//! Isolated heap accounting / uninstrumented process-memory worker.
#[path = "support/alloc.rs"]
mod allocation;
#[path = "support/io.rs"]
mod io_support;
#[path = "support/seek.rs"]
mod seek_support;
use deadlib_audio_decode_vorbis::{OggEvent, OggOptions, OggReader};
use io_support::{IoStats, Source};
use seek_support::{Geometry, Session, Workload};
use std::{hint::black_box, path::Path, sync::Arc};
use symphonia::core::{
    audio::{Audio, GenericAudioBufferRef},
    codecs::audio::AudioDecoderOptions,
    formats::{FormatOptions, TrackType, probe::Hint},
    io::MediaSourceStream,
    meta::MetadataOptions,
};

#[cfg(feature = "bench-alloc")]
#[global_allocator]
static ALLOCATOR: allocation::Counting = allocation::Counting;

fn forward(
    engine: &str,
    phase: &str,
    source: Source,
    geometry: Geometry,
) -> (u64, Option<allocation::Stats>) {
    let mut samples = 0;
    let mut first = None;
    if engine == "native" {
        let mut reader = OggReader::with_options(source, OggOptions::default());
        let mut starts = 0;
        while reader
            .read_next(|event| {
                if let OggEvent::StreamStart(info) = &event {
                    starts += 1;
                    assert_eq!(starts, 1);
                    assert_eq!(info.identification.channels as usize, geometry.channels);
                    assert_eq!(info.identification.sample_rate, geometry.rate);
                }
                if let OggEvent::Pcm(pcm) = event {
                    if first.is_some() {
                        return;
                    }
                    samples += (pcm.pcm.frames * pcm.pcm.channels) as u64;
                    for plane in pcm.pcm.planes() {
                        black_box(plane);
                    }
                    if phase == "startup" && pcm.pcm.frames != 0 {
                        first = Some(allocation::end());
                    }
                }
            })
            .unwrap()
        {
            if first.is_some() {
                break;
            }
        }
    } else {
        assert_eq!(engine, "symphonia");
        let mut hint = Hint::new();
        hint.with_extension("ogg");
        let mut format = symphonia::default::get_probe()
            .probe(
                &hint,
                MediaSourceStream::new(Box::new(source), Default::default()),
                FormatOptions::default(),
                MetadataOptions::default(),
            )
            .unwrap();
        let track = format.default_track(TrackType::Audio).unwrap();
        let track_id = track.id;
        let parameters = track.codec_params.as_ref().unwrap().audio().unwrap();
        assert_eq!(parameters.sample_rate, Some(geometry.rate));
        let mut decoder = symphonia::default::get_codecs()
            .make_audio_decoder(parameters, &AudioDecoderOptions::default())
            .unwrap();
        while let Some(packet) = format.next_packet().unwrap() {
            assert_eq!(packet.track_id, track_id);
            let GenericAudioBufferRef::F32(audio) = decoder.decode(&packet).unwrap() else {
                panic!("expected f32")
            };
            assert_eq!(audio.spec().channels().count(), geometry.channels);
            samples += (audio.frames() * geometry.channels) as u64;
            for channel in 0..geometry.channels {
                black_box(audio.plane(channel).unwrap());
            }
            if phase == "startup" && audio.frames() != 0 {
                first = Some(allocation::end());
                break;
            }
        }
    }
    (samples, first)
}

#[cfg(windows)]
fn process_peak() -> Option<usize> {
    use std::ffi::c_void;
    #[repr(C)]
    struct Counters {
        cb: u32,
        faults: u32,
        peak_working_set: usize,
        working_set: usize,
        peak_paged_pool: usize,
        paged_pool: usize,
        peak_nonpaged_pool: usize,
        nonpaged_pool: usize,
        pagefile: usize,
        peak_pagefile: usize,
        private: usize,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut c_void;
    }
    #[link(name = "psapi")]
    unsafe extern "system" {
        fn GetProcessMemoryInfo(handle: *mut c_void, counters: *mut Counters, size: u32) -> i32;
    }
    let mut counters: Counters = unsafe { std::mem::zeroed() };
    counters.cb = std::mem::size_of::<Counters>() as u32;
    let ok = unsafe {
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut counters,
            std::mem::size_of::<Counters>() as u32,
        )
    };
    (ok != 0).then_some(counters.peak_working_set)
}
#[cfg(not(windows))]
fn process_peak() -> Option<usize> {
    None
}

fn main() {
    let args: Vec<_> = std::env::args()
        .skip(1)
        .filter(|a| a != "--bench")
        .collect();
    if args == ["--selftest"] {
        if !cfg!(feature = "bench-alloc") {
            panic!("selftest requires --features bench-alloc");
        }
        allocation::self_test();
        println!("allocator-selftest:ok");
        return;
    }
    assert_eq!(args.len(), 4, "ENGINE PHASE FILE REQUESTS");
    let engine = &args[0];
    let phase = &args[1];
    assert!(["full", "startup", "seek_open", "seek_batch"].contains(&phase.as_str()));
    let workload = Workload::read(Path::new(&args[3]), phase == "seek_batch");
    let data: Arc<[u8]> = std::fs::read(&args[2]).unwrap().into();
    let geometry = workload.geometry;
    let stats = Arc::new(IoStats::default());
    let counted = cfg!(feature = "bench-alloc");
    // Counter windows exclude process-wide registry initialization. The RSS
    // worker is built without this allocator and skips this warm-up for forward
    // decode/open, retaining an actual fresh-process working-set high water.
    if counted && phase != "seek_batch" {
        allocation::begin();
        let source = Source::new(&data, usize::MAX, false, &stats);
        let (samples, _) = forward(engine, "full", source, geometry);
        allocation::end();
        assert_eq!(samples, geometry.frames * geometry.channels as u64);
    }
    let exposed;
    let memory;
    if phase == "seek_batch" {
        let mut session = Session::open(
            engine,
            Source::new(&data, usize::MAX, true, &stats),
            geometry,
        );
        session.warm(geometry);
        session.batch(&workload, true);
        stats.reset();
        allocation::begin();
        exposed = session.batch(&workload, false);
        assert!(exposed >= workload.expected_samples());
        memory = allocation::end();
    } else if phase == "seek_open" {
        stats.reset();
        allocation::begin();
        let session = Session::open(
            engine,
            Source::new(&data, usize::MAX, true, &stats),
            geometry,
        );
        black_box(&session);
        memory = allocation::end();
        exposed = 0;
    } else {
        stats.reset();
        allocation::begin();
        let (samples, first) = forward(
            engine,
            phase,
            Source::new(&data, usize::MAX, false, &stats),
            geometry,
        );
        if phase == "full" {
            assert_eq!(samples, geometry.frames * geometry.channels as u64);
        } else {
            assert!(samples > 0);
        }
        memory = first.unwrap_or_else(allocation::end);
        exposed = samples;
        if counted && phase == "full" {
            assert_eq!(memory.retained_delta_bytes, 0);
        }
    }
    let [reads, bytes, seeks, tells] = stats.values();
    let (operations, requested, peak, retained) = if counted {
        (
            memory.operations.to_string(),
            memory.requested_bytes.to_string(),
            memory.peak_extra_bytes.to_string(),
            memory.retained_delta_bytes.to_string(),
        )
    } else {
        ("null".into(), "null".into(), "null".into(), "null".into())
    };
    let working_set = if counted {
        "null".into()
    } else {
        process_peak().map_or_else(|| "null".into(), |v| v.to_string())
    };
    println!(
        "{{\"engine\":\"{engine}\",\"phase\":\"{phase}\",\"instrumented\":{counted},\"operations\":{operations},\"requested_bytes\":{requested},\"peak_extra_bytes\":{peak},\"retained_delta_bytes\":{retained},\"process_peak_working_set_bytes\":{working_set},\"reads\":{reads},\"bytes_read\":{bytes},\"seeks\":{seeks},\"tells\":{tells},\"api_exposed_samples\":{exposed}}}"
    );
}
