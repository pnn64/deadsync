//! Exact queue state and buffer ownership checks with paired public-operation timings.
use super::*;
use std::hint::black_box;
use std::io::Write;
use std::sync::mpsc::{Receiver, TryRecvError, sync_channel};

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "upload_cycle_original.rs"]
mod original;

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Item {
    key: u64,
    kind: u8,
    width: u32,
    height: u32,
    raw: Vec<u8>,
    capacity: Option<usize>,
    shared: Option<usize>,
    strong: usize,
    conversion: Option<[[u32; 4]; 2]>,
    recycle: bool,
    source_size: Option<[u32; 2]>,
    sampler: SamplerDesc,
    bytes: usize,
}
#[derive(Debug, PartialEq, Eq)]
pub(super) struct State {
    order: Vec<u64>,
    order_capacity: usize,
    entries_capacity: usize,
    bytes: usize,
    count: usize,
    items: Vec<Item>,
}

include!("upload_snapshot.rs");

fn record(state: &State, trace: &mut impl Write) {
    for value in [
        state.order_capacity,
        state.entries_capacity,
        state.bytes,
        state.count,
        state.order.len(),
    ] {
        trace.write_all(&(value as u64).to_le_bytes()).unwrap();
    }
    for &key in &state.order {
        trace.write_all(&key.to_le_bytes()).unwrap();
    }
    for item in &state.items {
        for value in [
            item.key,
            u64::from(item.kind),
            u64::from(item.width),
            u64::from(item.height),
            item.capacity.unwrap_or(0) as u64,
            item.strong as u64,
            item.bytes as u64,
            item.raw.len() as u64,
        ] {
            trace.write_all(&value.to_le_bytes()).unwrap();
        }
        trace.write_all(&item.raw).unwrap();
        trace
            .write_all(&[
                u8::from(item.recycle),
                u8::from(item.source_size.is_some()),
                item.sampler.filter as u8,
                item.sampler.wrap as u8,
                u8::from(item.sampler.mipmaps),
            ])
            .unwrap();
        if let Some(size) = item.source_size {
            for value in size {
                trace.write_all(&value.to_le_bytes()).unwrap();
            }
        }
        trace
            .write_all(&[u8::from(item.conversion.is_some())])
            .unwrap();
        if let Some(conversion) = item.conversion {
            for value in conversion.into_iter().flatten() {
                trace.write_all(&value.to_le_bytes()).unwrap();
            }
        }
    }
}

fn same(old: &original::TextureUploadQueue, new: &TextureUploadQueue, trace: &mut impl Write) {
    let expected = original::snapshot(old);
    let actual = snapshot(new);
    assert_eq!(actual, expected);
    record(&actual, trace);
}

fn owned(seed: u8) -> RgbaImage {
    let len = 4 + usize::from(seed % 13);
    let mut raw = Vec::with_capacity(len + 11);
    raw.extend((0..len).map(|i| seed.wrapping_add(i as u8)));
    RgbaImage::from_raw(1, 1, raw).unwrap()
}

fn recycled(old: &Receiver<Vec<u8>>, new: &Receiver<Vec<u8>>, trace: &mut impl Write) {
    loop {
        match (old.try_recv(), new.try_recv()) {
            (Ok(a), Ok(b)) => {
                assert_eq!(a, b);
                assert_eq!(a.capacity(), b.capacity());
                trace.write_all(&(b.len() as u64).to_le_bytes()).unwrap();
                trace
                    .write_all(&(b.capacity() as u64).to_le_bytes())
                    .unwrap();
                trace.write_all(&b).unwrap();
            }
            (Err(a), Err(b)) => {
                assert_eq!(a, b);
                trace
                    .write_all(&[if a == TryRecvError::Empty { 0 } else { 1 }])
                    .unwrap();
                break;
            }
            result => panic!("recycler mismatch: {result:?}"),
        }
    }
}

fn pop(
    old: &mut original::TextureUploadQueue,
    new: &mut TextureUploadQueue,
    budget: [usize; 4],
    trace: &mut impl Write,
) {
    let [max_uploads, max_bytes, drained_uploads, drained_bytes] = budget;
    let a = old.pop_next(
        original::TextureUploadBudget {
            max_uploads,
            max_bytes,
        },
        drained_uploads,
        drained_bytes,
    );
    let b = new.pop_next(
        TextureUploadBudget {
            max_uploads,
            max_bytes,
        },
        drained_uploads,
        drained_bytes,
    );
    assert_eq!(a.is_some(), b.is_some());
    if let (Some((key_a, a)), Some((key_b, b))) = (a, b) {
        assert_eq!(key_a, key_b);
        assert_eq!(a.bytes, b.bytes);
        assert_eq!(a.sampler, b.sampler);
        assert_eq!(a.source_size, b.source_size);
        assert_eq!(
            (a.image().width(), a.image().height(), a.image().byte_len()),
            (b.image().width(), b.image().height(), b.image().byte_len())
        );
        match (a.image(), b.image()) {
            (original::TextureUploadImage::Rgba(a), TextureUploadImage::Rgba(b)) => {
                assert_eq!(a, b);
                assert_eq!(a.as_raw().capacity(), b.as_raw().capacity());
                trace.write_all(b.as_raw()).unwrap();
            }
            (original::TextureUploadImage::Yuv420(a), TextureUploadImage::Yuv420(b)) => {
                assert_eq!(a.as_raw(), b.as_raw());
                assert_eq!(a.conversion(), b.conversion());
                trace.write_all(b.as_raw()).unwrap();
            }
            _ => panic!("image variant differs"),
        }
        trace.write_all(&key_b.to_le_bytes()).unwrap();
    }
}

fn queue_corpus(trace: &mut impl Write) {
    assert_eq!(layout(), original::layout());
    let mut state = 0x3141_5926u32;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        state
    };
    let shared: Vec<_> = (0..8).map(|i| Arc::new(owned(i))).collect();
    let mut mutations = 0;
    for sequence in 0..64 {
        let mut old = original::TextureUploadQueue::default();
        let mut new = TextureUploadQueue::default();
        let (old_tx, old_rx) = sync_channel(sequence % 3);
        let (new_tx, new_rx) = sync_channel(sequence % 3);
        for _ in 0..128 {
            let value = next();
            let key = [0, 7, 11, u64::MAX][(value as usize >> 8) & 3];
            let sampler = SamplerDesc {
                filter: if value & 1 == 0 {
                    deadlib_render_core::SamplerFilter::Linear
                } else {
                    deadlib_render_core::SamplerFilter::Nearest
                },
                wrap: if value & 2 == 0 {
                    deadlib_render_core::SamplerWrap::Clamp
                } else {
                    deadlib_render_core::SamplerWrap::Repeat
                },
                mipmaps: value & 4 != 0,
            };
            match value % 7 {
                0 => {
                    old.push(key, shared[(value as usize >> 16) & 7].clone(), sampler);
                    new.push(key, shared[(value as usize >> 16) & 7].clone(), sampler);
                }
                1 => {
                    let size = [value, value.rotate_left(13)];
                    old.push_prepared(
                        key,
                        shared[(value as usize >> 16) & 7].clone(),
                        sampler,
                        size,
                    );
                    new.push_prepared(
                        key,
                        shared[(value as usize >> 16) & 7].clone(),
                        sampler,
                        size,
                    );
                }
                2 => {
                    old.push_owned(key, owned(value as u8), sampler);
                    new.push_owned(key, owned(value as u8), sampler);
                }
                3 => {
                    old.push_recyclable(key, owned(value as u8), sampler, old_tx.clone());
                    new.push_recyclable(key, owned(value as u8), sampler, new_tx.clone());
                }
                4 => {
                    let bytes = vec![value as u8; 6];
                    old.push_recyclable_yuv420(
                        key,
                        Yuv420Image::from_raw(2, 2, bytes.clone()).unwrap(),
                        sampler,
                        old_tx.clone(),
                    );
                    new.push_recyclable_yuv420(
                        key,
                        Yuv420Image::from_raw(2, 2, bytes).unwrap(),
                        sampler,
                        new_tx.clone(),
                    );
                }
                5 => {
                    old.remove(key);
                    new.remove(key);
                }
                _ => pop(
                    &mut old,
                    &mut new,
                    [
                        (value as usize >> 3) & 3,
                        [0, 1, 16, usize::MAX][(value as usize >> 5) & 3],
                        (value as usize >> 9) & 3,
                        [0, 1, 32, usize::MAX][(value as usize >> 11) & 3],
                    ],
                    trace,
                ),
            }
            same(&old, &new, trace);
            recycled(&old_rx, &new_rx, trace);
            mutations += 1;
        }
        while !new.entries.is_empty() {
            pop(&mut old, &mut new, [usize::MAX, usize::MAX, 0, 0], trace);
            same(&old, &new, trace);
            recycled(&old_rx, &new_rx, trace);
        }
    }
    let mut budgets = 0;
    for image in [
        Arc::new(RgbaImage::new(0, 0)),
        Arc::new(RgbaImage::new(1, 1)),
        Arc::new(RgbaImage::new(4, 4)),
    ] {
        for max_uploads in [0, 1, 2, usize::MAX] {
            for max_bytes in [0, 1, 4, 63, 64, usize::MAX] {
                for drained_uploads in [0, 1, 2, usize::MAX] {
                    for drained_bytes in [0, 1, 4, usize::MAX] {
                        for counter in [0, 1, usize::MAX] {
                            let mut old = original::TextureUploadQueue::default();
                            let mut new = TextureUploadQueue::default();
                            for key in [0, 7, 11] {
                                old.push_prepared(
                                    key,
                                    image.clone(),
                                    SamplerDesc::default(),
                                    [0, u32::MAX],
                                );
                                new.push_prepared(
                                    key,
                                    image.clone(),
                                    SamplerDesc::default(),
                                    [0, u32::MAX],
                                );
                            }
                            old.remove(0);
                            new.remove(0);
                            original::set_bytes(&mut old, counter);
                            set_bytes(&mut new, counter);
                            pop(
                                &mut old,
                                &mut new,
                                [max_uploads, max_bytes, drained_uploads, drained_bytes],
                                trace,
                            );
                            same(&old, &new, trace);
                            budgets += 1;
                        }
                    }
                }
            }
        }
    }
    println!("queue corpus: {mutations} mutations and {budgets} boundary budgets");
}

#[test]
fn queue_state_recycling_and_budget_boundaries_match() {
    queue_corpus(&mut std::io::sink());
}

#[test]
fn recycled_buffers_keep_their_original_allocation() {
    let old_image = owned(9);
    let new_image = owned(9);
    let old_pointer = old_image.as_raw().as_ptr();
    let new_pointer = new_image.as_raw().as_ptr();
    let (a, ar) = sync_channel(1);
    let (b, br) = sync_channel(1);
    let mut old = original::TextureUploadQueue::default();
    let mut new = TextureUploadQueue::default();
    old.push_recyclable(7, old_image, SamplerDesc::default(), a);
    new.push_recyclable(7, new_image, SamplerDesc::default(), b);
    let shared = Arc::new(RgbaImage::new(1, 1));
    old.push_prepared(7, shared.clone(), SamplerDesc::default(), [7, 8]);
    new.push_prepared(7, shared, SamplerDesc::default(), [7, 8]);
    let a = ar.try_recv().unwrap();
    let b = br.try_recv().unwrap();
    assert_eq!(a.as_ptr(), old_pointer);
    assert_eq!(b.as_ptr(), new_pointer);
    assert_eq!(a, b);
    assert_eq!(a.capacity(), b.capacity());
}

#[test]
#[ignore = "writes deterministic upload queue trace"]
fn dump_upload_trace() {
    let path = std::env::var_os("DEADSYNC_UPLOAD_TRACE_OUT").unwrap();
    let mut trace =
        std::io::BufWriter::with_capacity(1024 * 1024, std::fs::File::create(path).unwrap());
    queue_corpus(&mut trace);
}

fn allocation(label: &str, work: impl FnOnce()) {
    let (_, counts) = crate::perf::measure(work);
    println!("{label}: allocation {counts:?}");
}

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_upload_cycle() {
    let image = Arc::new(RgbaImage::new(16, 16));
    for count in [1, 64, 256] {
        let mut old = original::TextureUploadQueue::default();
        let mut new = TextureUploadQueue::default();
        for key in 0..count {
            old.push_prepared(key, image.clone(), SamplerDesc::default(), [7, 8]);
            new.push_prepared(key, image.clone(), SamplerDesc::default(), [7, 8]);
        }
        let label = format!("prepared/replace/{count}");
        allocation(&format!("{label}/original"), || {
            old.push_prepared(0, image.clone(), SamplerDesc::default(), [3, 5])
        });
        allocation(&format!("{label}/current"), || {
            new.push_prepared(0, image.clone(), SamplerDesc::default(), [3, 5])
        });
        let mut counters = [0u64; 2];
        bench::compare(&label, 100000, |current| {
            let i = usize::from(current);
            let key = counters[i] % count;
            counters[i] += 1;
            if current {
                new.push_prepared(
                    black_box(key),
                    black_box(image.clone()),
                    black_box(SamplerDesc::default()),
                    black_box([3, 5]),
                );
            } else {
                old.push_prepared(
                    black_box(key),
                    black_box(image.clone()),
                    black_box(SamplerDesc::default()),
                    black_box([3, 5]),
                );
            }
        });
    }
    for prepared in [false, true] {
        let mut old = original::TextureUploadQueue::default();
        let mut new = TextureUploadQueue::default();
        old.push(7, image.clone(), SamplerDesc::default());
        new.push(7, image.clone(), SamplerDesc::default());
        let label = if prepared {
            "prepared/cycle"
        } else {
            "ordinary/cycle"
        };
        let mut work = |current| {
            if current {
                if prepared {
                    new.push_prepared(7, image.clone(), SamplerDesc::default(), [3, 5]);
                } else {
                    new.push(7, image.clone(), SamplerDesc::default());
                }
                black_box(new.pop_next(
                    TextureUploadBudget {
                        max_uploads: 1,
                        max_bytes: 2048,
                    },
                    0,
                    0,
                ));
            } else {
                if prepared {
                    old.push_prepared(7, image.clone(), SamplerDesc::default(), [3, 5]);
                } else {
                    old.push(7, image.clone(), SamplerDesc::default());
                }
                black_box(old.pop_next(
                    original::TextureUploadBudget {
                        max_uploads: 1,
                        max_bytes: 2048,
                    },
                    0,
                    0,
                ));
            }
        };
        work(false);
        work(true);
        allocation(&format!("{label}/original"), || work(false));
        allocation(&format!("{label}/current"), || work(true));
        bench::compare(label, 100000, &mut work);
    }
}
