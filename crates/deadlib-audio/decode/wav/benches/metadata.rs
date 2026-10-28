//! Resident leading RIFF cue/INFO metadata. Parse+walk and retained views have
//! distinct rows; neither row reads audio. See run_metadata.py for fixture truth.
mod support;
use deadlib_audio_decode_wav::{
    Limits, Wave,
    metadata::{Cues, List, Metadata},
};
use std::{env, fs, hint::black_box};

fn checksum(cues: Cues<'_>, info: List<'_>) -> u64 {
    let mut sum = 0u64;
    for c in cues.iter() {
        sum = sum.wrapping_add(
            u64::from(c.id)
                + u64::from(c.position)
                + u64::from(c.chunk_start)
                + u64::from(c.block_start)
                + u64::from(c.sample_offset),
        );
        for b in c.data_chunk {
            sum = sum.wrapping_add(u64::from(b));
        }
    }
    for text in info.entries() {
        for &b in text.unwrap().text_bytes() {
            sum = sum.wrapping_add(u64::from(b));
        }
    }
    sum
}
fn views<'a>(wave: &Wave<'a>) -> (Cues<'a>, List<'a>) {
    let mut cues = None;
    let mut info = None;
    for c in wave.chunks().unwrap() {
        match c
            .unwrap()
            .metadata(wave.format.endian, Limits::default())
            .unwrap()
        {
            Metadata::Cues(v) => cues = Some(v),
            Metadata::List(v) => info = Some(v),
            _ => {}
        }
    }
    (cues.unwrap(), info.unwrap())
}
fn main() {
    let args = env::args().skip(1).collect::<Vec<_>>();
    assert_eq!(args.len(), 3, "metadata FILE ITERATIONS EXPECTED_CHECKSUM");
    let bytes = fs::read(&args[0]).unwrap();
    let iters = args[1].parse().unwrap();
    let expected = args[2].parse::<u64>().unwrap();
    let wave = Wave::parse(&bytes).unwrap();
    let (cues, info) = views(&wave);
    assert_eq!(checksum(cues, info), expected);
    println!(
        "operation\ttrial\titers\tns_per_op\tthread_cycles_per_op\talloc_calls\trequested_bytes\tpeak_requested_live_bytes"
    );
    support::bench("wav/open_headers", iters, || {
        black_box(Wave::parse(black_box(&bytes)).unwrap());
    });
    support::bench("wav/open_walk_metadata", iters, || {
        let wave = Wave::parse(black_box(&bytes)).unwrap();
        let (cues, info) = views(&wave);
        assert_eq!(black_box(checksum(cues, info)), expected);
    });
    support::bench("wav/walk_retained_views", iters, || {
        assert_eq!(
            black_box(checksum(black_box(cues), black_box(info))),
            expected
        );
    });
}
