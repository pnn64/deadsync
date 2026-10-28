use deadlib_audio_decode_wav::{
    Container, DataLength, Endian, Limits, RawChunk, WavlMuxer, WriteOptions, metadata::*,
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    hint::black_box,
    io::Cursor,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed},
};
static TRACK: AtomicBool = AtomicBool::new(false);
static CALLS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
struct Heap;
fn allocated(n: usize) {
    if TRACK.load(Relaxed) {
        CALLS.fetch_add(1, Relaxed);
        BYTES.fetch_add(n, Relaxed);
    }
}
// SAFETY: every allocation contract and pointer is forwarded unchanged to System.
// Counter operations are atomic, perform no allocation, and cannot unwind.
unsafe impl GlobalAlloc for Heap {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(l) };
        if !p.is_null() {
            allocated(l.size());
        }
        p
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(l) };
        if !p.is_null() {
            allocated(l.size());
        }
        p
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        let result = unsafe { System.realloc(p, l, n) };
        if !result.is_null() {
            allocated(n);
        }
        result
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
}
#[global_allocator]
static ALLOCATOR: Heap = Heap;

#[test]
fn caller_owned_metadata_and_wavl_muxing_allocate_no_heap() {
    let mut out = vec![0u8; 2048];
    TRACK.store(true, Relaxed);
    let result = (|| -> Result<(), deadlib_audio_decode_wav::Error> {
        let n = write_list(
            *b"INFO",
            &[RawChunk {
                id: *b"INAM",
                data: b"title\0",
            }],
            Endian::Little,
            black_box(&mut out),
        )?;
        for e in List::parse(black_box(&out[..n]), Endian::Little, Limits::default())?.entries() {
            black_box(e?.text_bytes());
        }
        let point = CuePoint {
            id: 1,
            position: 0,
            data_chunk: *b"data",
            chunk_start: 0,
            block_start: 0,
            sample_offset: 0,
        };
        let n = write_cues(&[point], Endian::Little, &mut out)?;
        for p in Cues::parse(&out[..n], Endian::Little, Limits::default())?.iter() {
            black_box(p);
        }
        let p = SampleLoop {
            cue_id: 1,
            kind: 0,
            start: 0,
            end: 1,
            fraction: 0,
            play_count: 0,
        };
        let n = write_sampler(
            SamplerHeader::default(),
            &[p],
            &[1, 2],
            Endian::Little,
            &mut out,
        )?;
        for p in Sampler::parse(&out[..n], Endian::Little, Limits::default())?.loops() {
            black_box(p);
        }
        let n = BextSpec::default().write(&mut out)?;
        black_box(Bext::parse(&out[..n])?.time_reference());
        let id = AudioId {
            track_index: 1,
            uid: b"ATU_00000001",
            track_ref: b"AC_00010001_00",
            pack_ref: b"AP_00010002",
            padding: 0,
        };
        let n = write_chna(1, &[id], 4, &mut out)?;
        let chna = Chna::parse(&out[..n], Limits::default())?;
        chna.validate_assignments(1, &mut [0; 1])?;
        for id in chna.ids() {
            black_box(id);
        }
        let n = write_bxml(0, b"<x/>", &mut out)?;
        black_box(Bxml::parse(&out[..n])?.data());
        let n = write_sxml(0, &[(4, b"<x/>")], &[], &mut out)?;
        for p in Sxml::parse(&out[..n], Limits::default())?.subchunks() {
            black_box(p);
        }
        let marker = Marker {
            flags: 1,
            sample_offset: 0,
            byte_offset: 0,
            intra_sample_words: [0; 2],
            label: &[0; 256],
            label_chunk_id: 0,
            vendor_product_guid: &[0; 16],
            user_data: [0; 4],
        };
        let n = write_markers(&[marker], &mut out)?;
        for p in Markers::parse(&out[..n], Limits::default())?.iter() {
            black_box(p);
        }
        for container in [
            Container::Riff,
            Container::Rifx,
            Container::Rf64,
            Container::Bw64,
        ] {
            let format = if container == Container::Rifx {
                [
                    0x12, 0x34, 0, 1, 0, 0, 0x1f, 0x40, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0,
                ]
            } else {
                [
                    0x34, 0x12, 1, 0, 0x40, 0x1f, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0,
                ]
            };
            let options = WriteOptions {
                container,
                ..WriteOptions::default()
            };
            let mut mux =
                WavlMuxer::new(Cursor::new(black_box(&mut out[..])), &format, options, &[])?;
            mux.write_data(&[1, 2, 3], 7)?;
            black_box(mux.finish()?);
            let mut mux = WavlMuxer::with_lengths(
                Cursor::new(black_box(&mut out[..])),
                &format,
                options,
                &[],
                DataLength {
                    bytes: 16,
                    frames: 7,
                },
            )?;
            mux.write_data(&[1, 2, 3], 7)?;
            black_box(mux.finish_stream()?);
        }
        Ok(())
    })();
    TRACK.store(false, Relaxed);
    result.unwrap();
    assert_eq!(CALLS.load(Relaxed), 0);
    assert_eq!(BYTES.load(Relaxed), 0);
}
