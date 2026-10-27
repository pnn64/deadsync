//! Independent unmodified blockin/track-only/read state comparisons.
use super::*;
use crate::reference_vectors::{eos_vectors, floor0_vectors, floor1_vectors, synthetic};
use crate::{
    CodebookBehavior, Floor0Behavior, Floor1Behavior, Identification, Limits, ParseOptions,
    Residue2Layout, ResidueBehavior,
};
use std::{
    fs,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Clone)]
enum Op {
    Reset,
    Fresh,
    Packet {
        track: bool,
        data: Vec<u8>,
        timing: PacketTiming,
    },
    Read(u32),
    Inject {
        gp: i64,
        samples: i64,
        sequence: i64,
    },
    Drain,
    Direct {
        large: bool,
        timing: PacketTiming,
        words: Vec<u32>,
    },
}
fn take<'a>(w: &mut &'a [u8], n: usize) -> &'a [u8] {
    let (a, b) = w.split_at(n);
    *w = b;
    a
}
fn u32_in(w: &mut &[u8]) -> u32 {
    u32::from_le_bytes(take(w, 4).try_into().unwrap())
}
fn u64_in(w: &mut &[u8]) -> u64 {
    u64::from_le_bytes(take(w, 8).try_into().unwrap())
}
fn blob(w: &mut &[u8]) -> Vec<u8> {
    let n = u64_in(w) as usize;
    take(w, n).to_vec()
}
fn put_blob(w: &mut Vec<u8>, b: &[u8]) {
    w.extend((b.len() as u64).to_le_bytes());
    w.extend(b);
}
fn put_timing(w: &mut Vec<u8>, t: PacketTiming) {
    for v in [
        t.packet_number,
        t.granule_position.unwrap_or(-1) as u64,
        t.end_of_stream as u64,
    ] {
        w.extend(v.to_le_bytes());
    }
}
fn request(headers: &[Vec<u8>], cases: &[Vec<Op>]) -> Vec<u8> {
    let mut w = Vec::new();
    for h in headers {
        put_blob(&mut w, h);
    }
    w.extend((cases.len() as u64).to_le_bytes());
    for c in cases {
        w.extend((c.len() as u64).to_le_bytes());
        for op in c {
            match op {
                Op::Reset => w.extend(0u64.to_le_bytes()),
                Op::Fresh => w.extend(7u64.to_le_bytes()),
                Op::Packet {
                    track,
                    data,
                    timing,
                } => {
                    w.extend(if *track { 2u64 } else { 1 }.to_le_bytes());
                    put_timing(&mut w, *timing);
                    put_blob(&mut w, data);
                }
                Op::Read(n) => {
                    w.extend(3u64.to_le_bytes());
                    w.extend((*n as u64).to_le_bytes());
                }
                Op::Inject {
                    gp,
                    samples,
                    sequence,
                } => {
                    w.extend(4u64.to_le_bytes());
                    for v in [*gp, *samples, *sequence] {
                        w.extend((v as u64).to_le_bytes());
                    }
                }
                Op::Drain => w.extend(5u64.to_le_bytes()),
                Op::Direct {
                    large,
                    timing,
                    words,
                } => {
                    w.extend(6u64.to_le_bytes());
                    w.extend((*large as u64).to_le_bytes());
                    put_timing(&mut w, *timing);
                    for &v in words {
                        w.extend((v as u64).to_le_bytes());
                    }
                }
            }
        }
    }
    w
}
fn oracle(label: &str, input: &[u8]) -> Vec<u8> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "deadlib-synthesis-{}-{}.bin",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&p, input).unwrap();
    let r = Command::new(std::env::var_os("DEADLIB_LIBVORBIS_ORACLE").expect("set oracle"))
        .arg("synthesis-ops")
        .arg(&p)
        .output()
        .unwrap();
    fs::remove_file(p).unwrap();
    assert!(
        r.status.success(),
        "{label}: {}",
        String::from_utf8_lossy(&r.stderr)
    );
    if let Some(root) = std::env::var_os("DEADLIB_VORBIS_SYNTHESIS_INPUTS") {
        fs::create_dir_all(&root).unwrap();
        for (ext, data) in [("bin", input), ("out", r.stdout.as_slice())] {
            fs::write(
                std::path::Path::new(&root).join(format!("{label}.{ext}")),
                data,
            )
            .unwrap();
        }
    }
    r.stdout
}
fn decoder(headers: &[Vec<u8>]) -> SynthesisDecoder {
    let id = Identification::parse(&headers[0]).unwrap();
    let setup = Setup::parse_with_options(
        &headers[2],
        &id,
        ParseOptions {
            allow_empty_codebooks: true,
            codebook_behavior: CodebookBehavior::Libvorbis,
            residue_behavior: ResidueBehavior::Libvorbis,
            ..ParseOptions::default()
        },
    )
    .unwrap();
    SynthesisDecoder::with_options(
        setup,
        DecodeOptions {
            allow_nonfinite: true,
            floor0_behavior: Floor0Behavior::Libvorbis,
            floor1_behavior: Floor1Behavior::Libvorbis,
            residue2_layout: Residue2Layout::Libvorbis,
            ..DecodeOptions::default()
        },
    )
    .unwrap()
}
#[derive(Default)]
struct Counts {
    ops: u64,
    accepted: u64,
    discarded: u64,
    busy: u64,
    reads: u64,
    read_errors: u64,
    finite: u64,
    nonfinite: u64,
    peak: f64,
    energy: f64,
    signal: f64,
}
impl Counts {
    fn value(&mut self, a: f32, b: f32) {
        if b.is_finite() {
            assert!(a.is_finite());
            let e = a as f64 - b as f64;
            self.finite += 1;
            self.peak = self.peak.max(e.abs());
            self.energy += e * e;
            self.signal = self.signal.max((b as f64).abs());
        } else {
            self.nonfinite += 1;
            if b.is_nan() {
                assert!(a.is_nan());
            } else {
                assert_eq!(a, b);
            }
        }
    }
    fn check(&self, label: &str, floor0: bool) {
        let rms = (self.energy / self.finite.max(1) as f64).sqrt();
        eprintln!(
            "{label}: {} ops, {} accepted, {} discarded, {} busy, {} reads, {} read errors, {} finite values, {} nonfinite classes, peak {:.9e}, RMS {:.9e}, signal {:.9e}",
            self.ops,
            self.accepted,
            self.discarded,
            self.busy,
            self.reads,
            self.read_errors,
            self.finite,
            self.nonfinite,
            self.peak,
            rms,
            self.signal
        );
        let (p, r) = if floor0 { (5e-5, 3e-6) } else { (3e-6, 5e-7) };
        assert!(self.peak <= p * self.signal.max(1.0) && rms <= r * self.signal.max(1.0));
    }
}
fn compare(label: &str, mut input: &[u8], mut reference: &[u8], floor0: bool) -> Counts {
    let h: Vec<_> = (0..3).map(|_| blob(&mut input)).collect();
    let mut d = decoder(&h);
    let channels = d.setup().channels() as usize;
    let blocks = d.setup().block_sizes();
    assert_eq!(u32_in(&mut reference) as usize, channels);
    let cases = u64_in(&mut input);
    assert_eq!(u64_in(&mut reference), cases);
    let mut counts = Counts::default();
    for case in 0..cases {
        d.reset();
        let mut expected_position = 0u64;
        let mut reference_frames = 0usize;
        for operation in 0..u64_in(&mut input) {
            let kind = u64_in(&mut input);
            let mut accepted = None;
            let mut read_count = None;
            let result = match kind {
                0 => {
                    d.reset();
                    expected_position = 0;
                    Ok(())
                }
                7 => {
                    d = decoder(&h);
                    expected_position = 0;
                    Ok(())
                }
                1 | 2 => {
                    let timing = PacketTiming {
                        packet_number: u64_in(&mut input),
                        granule_position: Some(u64_in(&mut input) as i64),
                        end_of_stream: u64_in(&mut input) != 0,
                    };
                    let data = blob(&mut input);
                    let r = if kind == 1 {
                        d.submit(&data, timing)
                    } else {
                        d.track(&data, timing)
                    };
                    match r {
                        Ok(a) => {
                            accepted = Some(a);
                            Ok(())
                        }
                        Err(e) => {
                            accepted = Some(true);
                            Err(e)
                        }
                    }
                }
                3 => {
                    let n = u64_in(&mut input) as usize;
                    read_count = Some(n);
                    d.consume(n)
                }
                4 => {
                    d.granule = u64_in(&mut input) as i64;
                    d.samples = u64_in(&mut input) as i64;
                    d.sequence = u64_in(&mut input) as i64;
                    Ok(())
                }
                5 => {
                    let n = d.pcm().frames;
                    read_count = Some(reference_frames);
                    d.consume(n)
                }
                6 => {
                    let large = u64_in(&mut input) as usize;
                    let timing = PacketTiming {
                        packet_number: u64_in(&mut input),
                        granule_position: Some(u64_in(&mut input) as i64),
                        end_of_stream: u64_in(&mut input) != 0,
                    };
                    let n = blocks[large];
                    for ch in 0..channels {
                        for k in 0..n {
                            d.decoder.current[ch * d.size + k] =
                                f32::from_bits(u64_in(&mut input) as u32);
                        }
                    }
                    accepted = Some(true);
                    d.insert(n, timing, true, false)
                }
                _ => panic!(),
            };
            let decoded = u32_in(&mut reference) as i32;
            let blockin = u32_in(&mut reference) as i32;
            if let Some(a) = accepted {
                assert_eq!(a, decoded == 0, "{label}/{case}/{operation}, decode");
                if a {
                    counts.accepted += 1;
                } else {
                    counts.discarded += 1;
                }
            }
            assert_eq!(
                result.is_ok(),
                blockin == 0,
                "{label}/{case}/{operation}, {result:?}, C {decoded}/{blockin}"
            );
            if result.is_err() {
                if kind == 3 || kind == 5 {
                    counts.read_errors += 1;
                } else {
                    counts.busy += 1;
                    assert_eq!(result, Err(Error::Invalid("unconsumed synthesis PCM")));
                }
            }
            if kind == 3 || kind == 5 {
                counts.reads += 1;
            }
            assert_eq!(
                d.sequence,
                u64_in(&mut reference) as i64,
                "{label}/{case}/{operation}, sequence"
            );
            assert_eq!(
                d.granule,
                u64_in(&mut reference) as i64,
                "{label}/{case}/{operation}, granule"
            );
            assert_eq!(
                d.samples,
                u64_in(&mut reference) as i64,
                "{label}/{case}/{operation}, samples"
            );
            assert_eq!(d.previous_size, u32_in(&mut reference) as usize);
            assert_eq!(d.center, u32_in(&mut reference) as usize);
            assert_eq!(
                d.returned,
                u32_in(&mut reference) as i32,
                "{label}/{case}/{operation}, returned"
            );
            assert_eq!(
                d.current,
                u32_in(&mut reference) as i32,
                "{label}/{case}/{operation}, current"
            );
            reference_frames = u32_in(&mut reference) as usize;
            assert_eq!(d.pcm().frames, reference_frames);
            if blockin == 0
                && let Some(n) = read_count
            {
                expected_position += n as u64;
            }
            assert_eq!(
                d.position(),
                expected_position,
                "{label}/{case}/{operation}, consumed position"
            );
            assert_eq!(d.eos, take(&mut reference, 1)[0] != 0);
            let state = d.state();
            assert_eq!(state.queued_frames, d.pcm().frames);
            assert_eq!(d.pcm().start_frame, d.position());
            let mut local = Counts::default();
            for (index, &a) in d.pcm.iter().enumerate() {
                let b = f32::from_bits(u32_in(&mut reference));
                let slot = index % d.size;
                if d.returned >= 0
                    && slot >= d.returned as usize
                    && slot < (d.returned as usize + d.frames())
                {
                    let exposed =
                        d.pcm().plane(index / d.size).unwrap()[slot - d.returned as usize];
                    assert_eq!(
                        exposed.to_bits(),
                        a.to_bits(),
                        "{label}/{case}/{operation}, borrowed plane"
                    );
                    local.value(exposed, b);
                } else {
                    local.value(a, b);
                }
                counts.value(a, b);
            }
            let (p, r) = if floor0 { (5e-5, 3e-6) } else { (3e-6, 5e-7) };
            assert!(
                local.peak <= p * local.signal.max(1.0)
                    && (local.energy / local.finite.max(1) as f64).sqrt()
                        <= r * local.signal.max(1.0),
                "{label}/{case}/{operation}, PCM bounds, peak {}",
                local.peak
            );
            counts.ops += 1;
        }
    }
    assert!(input.is_empty() && reference.is_empty());
    counts.check(label, floor0);
    counts
}
fn packet(data: &[u8], number: u64, gp: Option<i64>, eos: bool, track: bool) -> Op {
    Op::Packet {
        track,
        data: data.to_vec(),
        timing: PacketTiming {
            packet_number: number,
            granule_position: gp,
            end_of_stream: eos,
        },
    }
}
fn schedules(packets: &[Vec<u8>]) -> Vec<Vec<Op>> {
    let mut all = Vec::new();
    for tracks in 0..=packets.len() {
        for gp in [None, Some(0), Some(1), Some(37), Some(900), Some(i64::MAX)] {
            for eos in [false, true] {
                let mut ops = Vec::new();
                for (i, p) in packets.iter().enumerate() {
                    ops.push(packet(
                        p,
                        i as u64,
                        gp.filter(|_| i + 1 == packets.len()),
                        eos && i + 1 == packets.len(),
                        i < tracks,
                    ));
                    ops.push(Op::Read(0));
                    ops.push(Op::Read(1));
                    ops.push(Op::Drain);
                }
                all.push(ops);
            }
        }
    }
    for seed in 0..32u64 {
        let mut rng = seed + 1;
        let mut ops = Vec::new();
        let mut number = if seed & 1 != 0 { u64::MAX - 3 } else { 3 };
        for i in 0..48 {
            rng = rng
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let choice = (rng >> 32) as usize;
            match choice % 9 {
                0 => ops.push(Op::Reset),
                1 => ops.push(Op::Read(
                    [0, 1, 16, 32, 64, 128, 1024, i32::MAX as u32][choice % 8],
                )),
                2 => ops.push(Op::Drain),
                _ => {
                    let p = if i % 7 == 0 {
                        if seed & 1 == 0 { vec![] } else { vec![1] }
                    } else {
                        let p = &packets[choice % packets.len()];
                        p[..if i % 11 == 0 { p.len() / 2 } else { p.len() }].to_vec()
                    };
                    let gps = [
                        None,
                        Some(0),
                        Some(1),
                        Some(-2),
                        Some(37),
                        Some(i64::MIN),
                        Some(i64::MAX),
                        Some(1i64 << 32),
                    ];
                    ops.push(packet(
                        &p,
                        number,
                        gps[(choice / 8) % gps.len()],
                        choice & 32 != 0,
                        choice & 16 != 0,
                    ));
                    number = number.wrapping_add(if choice & 128 != 0 { 2 } else { 1 });
                }
            }
        }
        all.push(ops);
    }
    all
}
#[test]
#[ignore = "requires unmodified libvorbis synthesis/track/read oracle"]
fn synthesis_queue_schedule_witness() {
    let p = eos_vectors::packets(eos_vectors::FILES[0]);
    let cases = schedules(&p[3..]);
    let q = request(&p[..3], &cases[..16]);
    compare(
        "synthesis-witness",
        &q,
        &oracle("synthesis-witness", &q),
        false,
    );
}

fn edge_schedules(packets: &[Vec<u8>]) -> Vec<Vec<Op>> {
    let mut cases = Vec::new();
    for &(gp, samples) in &[
        (-1, -1),
        (-1, 0),
        (-1, 1),
        (-1, i64::MAX),
        (-1, i64::MIN),
        (0, i64::MAX),
        (i64::MAX, 0),
        (i64::MAX - 10, i64::MAX - 10),
        (1i64 << 31, 1i64 << 31),
        (1i64 << 32, 1i64 << 32),
    ] {
        for packet_gp in [-1, 0, 1, 37, i64::MIN, i64::MAX] {
            for eos in [false, true] {
                for track in [false, true] {
                    let mut ops = vec![
                        packet(&packets[0], 3, None, false, false),
                        Op::Drain,
                        packet(&packets[1 % packets.len()], 4, None, false, false),
                        Op::Drain,
                    ];
                    ops.push(Op::Inject {
                        gp,
                        samples,
                        sequence: 4,
                    });
                    ops.push(packet(
                        &packets[2 % packets.len()],
                        5,
                        Some(packet_gp),
                        eos,
                        track,
                    ));
                    ops.push(Op::Read(1));
                    ops.push(Op::Read(i32::MAX as u32));
                    ops.push(Op::Drain);
                    ops.push(packet(&packets[0], u64::MAX, Some(0), false, false));
                    ops.push(Op::Drain);
                    ops.push(packet(
                        &packets[1 % packets.len()],
                        0,
                        Some(1),
                        false,
                        false,
                    ));
                    ops.push(Op::Drain);
                    cases.push(ops);
                }
            }
        }
    }
    cases
}
fn direct_schedules(headers: &[Vec<u8>], nonfinite: bool) -> Vec<Vec<Op>> {
    let id = Identification::parse(&headers[0]).unwrap();
    let [small, large] = id.block_sizes;
    let mut all = Vec::new();
    for seed in 0..if nonfinite { 8 } else { 4 } {
        let mut ops = Vec::new();
        let values = [
            0u32, 0x3f800000, 0xbf800000, 0x3f000000, 1, 0x007fffff, 0x7f000000, 0x7f666666,
            0x7fc00000, 0x7f800000, 0xff800000,
        ];
        for (i, is_long) in [false, true, true, false, false, true, false, true]
            .into_iter()
            .enumerate()
        {
            let n = if is_long { large } else { small };
            let choices = if nonfinite { values.len() } else { 8 };
            let words = (0..id.channels as usize * n)
                .map(|k| values[(k * 7 + seed + i) % choices])
                .collect();
            ops.push(Op::Direct {
                large: is_long,
                timing: PacketTiming {
                    packet_number: i as u64,
                    granule_position: if i % 3 == 2 {
                        Some((i * n / 4) as i64)
                    } else {
                        None
                    },
                    end_of_stream: i == 7,
                },
                words,
            });
            if i % 4 == 1 {
                ops.push(Op::Read(1));
                ops.push(Op::Reset);
            } else {
                ops.push(Op::Read(0));
                ops.push(Op::Drain);
            }
        }
        all.push(ops);
    }
    all
}
fn check_batches(label: &str, headers: &[Vec<u8>], cases: &[Vec<Op>], floor0: bool) {
    if std::env::var("DEADLIB_VORBIS_SYNTHESIS_FILTER").is_ok_and(|f| !label.contains(&f)) {
        return;
    }
    let id = Identification::parse(&headers[0]).unwrap();
    let width = id.channels as usize * id.block_sizes[1];
    let batch = if width > 4096 { 1 } else { 8 };
    for (i, c) in cases.chunks(batch).enumerate() {
        let label = format!("{label}-batch-{i}");
        let q = request(headers, c);
        let out = oracle(&label, &q);
        compare(&label, &q, &out, floor0);
    }
}
#[test]
#[ignore = "requires independent libvorbis synthesis state oracle"]
fn synthesis_packet_schedules_track_read_and_restart_match_reference() {
    for (i, bytes) in eos_vectors::FILES.iter().enumerate() {
        let p = eos_vectors::packets(bytes);
        check_batches(
            &format!("synthesis-fixture-{i}"),
            &p[..3],
            &schedules(&p[3..]),
            false,
        );
    }
    for channels in [1, 3, 8] {
        for multiplier in 1..=4 {
            for kind in 0..3 {
                let c = floor1_vectors::Config {
                    channels,
                    multiplier,
                    kind,
                    submaps: 1 + kind,
                    posts: if kind == 2 { 63 } else { 8 },
                    high: if kind == 2 { 65536 } else { 64 },
                    coupling: channels > 1,
                    ..floor1_vectors::Config::default()
                };
                let p: Vec<_> = (0..6)
                    .map(|i| c.audio(i % 3 != 0, (i % 4) as u32, i))
                    .collect();
                check_batches(
                    &format!("synthesis-floor1-ch-{channels}-mult-{multiplier}-kind-{kind}"),
                    &c.headers(),
                    &schedules(&p),
                    false,
                );
            }
        }
    }
    for channels in [1, 2, 8] {
        for width in [0, 6, 32, 33] {
            let c = floor0_vectors::Config {
                channels,
                width,
                coupling: channels > 1,
                ..floor0_vectors::Config::default()
            };
            let p: Vec<_> = (0..6)
                .map(|i| {
                    c.audio(
                        i % 3 != 0,
                        (i % 4) as u32,
                        if width == 32 {
                            0x7fffffff
                        } else if width == 0 {
                            0
                        } else {
                            7
                        },
                        0,
                        i,
                    )
                })
                .collect();
            check_batches(
                &format!("synthesis-floor0-ch-{channels}-width-{width}"),
                &c.headers(),
                &schedules(&p),
                true,
            );
        }
    }
    for modes in [1, 2, 3, 16, 33, 64] {
        for first_long in [false, true] {
            let c = synthetic::Config {
                modes,
                first_long,
                ..synthetic::Config::default()
            };
            let mut p: Vec<_> = (0..4)
                .map(|i| c.audio((i % modes as usize) as u8, (i % 4) as u32, i as u32, None))
                .collect();
            p.extend([vec![], vec![1], vec![0x7e], vec![0]]);
            check_batches(
                &format!("synthesis-modes-{modes}-long-{first_long}"),
                &c.headers(),
                &schedules(&p),
                false,
            );
        }
    }
}
#[test]
#[ignore = "requires independent libvorbis overlap and counter oracle"]
fn synthesis_direct_windows_counter_wrap_and_ieee_match_reference() {
    for channels in [1, 2, 8, 255] {
        for pair in [[64, 64], [64, 256], [64, 8192], [8192, 8192]] {
            if channels as usize * pair[1] > 65536 {
                continue;
            }
            let mut h = floor0_vectors::Config {
                channels,
                long: pair[1],
                ..floor0_vectors::Config::default()
            }
            .headers();
            let mut id = Identification::parse(&h[0]).unwrap();
            id.block_sizes = pair;
            h[0] = id.encode().unwrap();
            check_batches(
                &format!(
                    "synthesis-window-ch-{channels}-blocks-{}-{}",
                    pair[0], pair[1]
                ),
                &h,
                &direct_schedules(&h, true),
                false,
            );
        }
    }
    let c = floor1_vectors::Config::default();
    let p: Vec<_> = (0..3).map(|i| c.audio(i != 0, 3, i)).collect();
    check_batches(
        "synthesis-counter",
        &c.headers(),
        &edge_schedules(&p),
        false,
    );
}
#[test]
fn synthesis_memory_budget_and_finite_failure_are_transactional() {
    let h = floor1_vectors::Config::default().headers();
    let parse = || {
        let id = Identification::parse(&h[0]).unwrap();
        Setup::parse_with_options(
            &h[2],
            &id,
            ParseOptions {
                residue_behavior: ResidueBehavior::Libvorbis,
                ..ParseOptions::default()
            },
        )
        .unwrap()
    };
    let required = SynthesisDecoder::required_storage(&parse());
    assert!(matches!(
        SynthesisDecoder::with_options(
            parse(),
            DecodeOptions {
                limits: Limits {
                    max_decode_memory: required - 1,
                    ..Limits::default()
                },
                ..DecodeOptions::default()
            }
        ),
        Err(Error::LimitExceeded("decode memory"))
    ));
    let mut d = SynthesisDecoder::with_options(
        parse(),
        DecodeOptions {
            limits: Limits {
                max_decode_memory: required,
                ..Limits::default()
            },
            ..DecodeOptions::default()
        },
    )
    .unwrap();
    assert_eq!(d.storage_bytes(), required);
    d.previous_size = d.size;
    d.pcm.fill(f32::MAX * 0.9);
    d.decoder.current.fill(f32::MAX * 0.9);
    let old = d.pcm.clone();
    let state = d.state();
    assert_eq!(
        d.insert(d.size, PacketTiming::default(), true, false),
        Err(Error::Invalid("nonfinite overlap output"))
    );
    assert_eq!(d.state(), state);
    assert_eq!(d.pcm, old);
    assert!(d.consume(usize::MAX).is_err());
    assert_eq!(d.state(), state);
}

fn portable_schedules(packets: &[Vec<u8>]) -> Vec<Vec<Op>> {
    let mut cases = Vec::new();
    for tracks in 0..=3 {
        let mut ops = Vec::new();
        for (i, p) in packets.iter().take(3).enumerate() {
            ops.push(packet(p, i as u64, None, false, i < tracks));
            ops.push(Op::Read(1));
            ops.push(packet(&packets[0], i as u64 + 17, None, false, true));
            ops.push(Op::Drain);
        }
        ops.push(Op::Reset);
        ops.push(packet(&packets[0], 0, Some(0), false, true));
        ops.push(Op::Read(1));
        ops.push(Op::Drain);
        ops.push(Op::Fresh);
        ops.push(packet(&packets[1], u64::MAX, None, false, false));
        ops.push(Op::Drain);
        ops.push(packet(&packets[2], 0, Some(1), true, false));
        ops.push(Op::Drain);
        cases.push(ops);
    }
    cases
}
fn golden_inputs() -> Vec<(bool, Vec<u8>)> {
    let mut all = Vec::new();
    let p = eos_vectors::packets(eos_vectors::FILES[0]);
    all.push((false, request(&p[..3], &portable_schedules(&p[3..]))));
    let c = floor1_vectors::Config {
        high: 65536,
        posts: 63,
        ..floor1_vectors::Config::default()
    };
    let p: Vec<_> = (0..3).map(|i| c.audio(i != 0, 3, i)).collect();
    all.push((false, request(&c.headers(), &portable_schedules(&p))));
    let c = floor0_vectors::Config {
        width: 32,
        ..floor0_vectors::Config::default()
    };
    let p: Vec<_> = (0..3)
        .map(|i| c.audio(i != 0, 3, 0x7fffffff, 0, i))
        .collect();
    all.push((true, request(&c.headers(), &portable_schedules(&p))));
    let c = synthetic::Config {
        modes: 64,
        first_long: true,
        ..synthetic::Config::default()
    };
    let p = vec![c.audio(0, 3, 1, None), vec![0], c.audio(63, 0, 3, None)];
    all.push((false, request(&c.headers(), &portable_schedules(&p))));
    let h = floor0_vectors::Config {
        channels: 1,
        ..floor0_vectors::Config::default()
    }
    .headers();
    all.push((false, request(&h, &direct_schedules(&h, true)[..2])));
    let p = eos_vectors::packets(eos_vectors::FILES[0]);
    let actual = vec![
        Op::Fresh,
        packet(&p[3], 0, Some(0), false, false),
        packet(&p[4], 2, Some(1), true, false),
        Op::Read(576),
        Op::Reset,
        Op::Read(0),
        packet(&p[4], 1, Some(1), true, false),
        Op::Read(0),
    ];
    all.push((false, request(&p[..3], &[actual])));
    all
}
#[test]
#[ignore = "manual external-oracle golden regeneration"]
fn regenerate_synthesis_state_golden() {
    let all = golden_inputs();
    let mut out = b"DSYNOP01".to_vec();
    out.extend((all.len() as u32).to_le_bytes());
    for (i, (floor0, q)) in all.into_iter().enumerate() {
        let r = oracle(&format!("synthesis-golden-{i}"), &q);
        compare(&format!("synthesis-golden-{i}"), &q, &r, floor0);
        out.push(floor0 as u8);
        put_blob(&mut out, &q);
        put_blob(&mut out, &r);
    }
    fs::write(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/synthesis-state-v1.libvorbis"),
        out,
    )
    .unwrap();
}
#[test]
fn synthesis_state_retained_reference_golden() {
    let mut w = include_bytes!("../tests/fixtures/synthesis-state-v1.libvorbis").as_slice();
    assert_eq!(take(&mut w, 8), b"DSYNOP01");
    assert_eq!(u32_in(&mut w), 6);
    for i in 0..6 {
        let floor0 = take(&mut w, 1)[0] != 0;
        let q = blob(&mut w);
        let r = blob(&mut w);
        compare(&format!("synthesis-state-golden-{i}"), &q, &r, floor0);
    }
    assert!(w.is_empty());
}
#[test]
#[ignore = "requires captured reference vorbisfile DSP schedules"]
fn synthesis_captured_seek_schedules_match_reference() {
    let root = std::path::PathBuf::from(
        std::env::var_os("DEADLIB_VORBIS_SYNTHESIS_TRACES").expect("set captured trace root"),
    );
    let mut dirs: Vec<_> = fs::read_dir(root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_dir() && p.join("schedule.bin").exists())
        .collect();
    dirs.sort();
    assert!(!dirs.is_empty());
    for p in &dirs {
        let label = format!(
            "synthesis-seek-trace-{}",
            p.file_name().unwrap().to_string_lossy()
        );
        let q = fs::read(p.join("schedule.bin")).unwrap();
        let r = fs::read(p.join("schedule.out")).unwrap();
        let bounds = fs::read_to_string(p.join("bounds.txt")).unwrap();
        compare(&label, &q, &r, bounds.trim() == "floor0");
    }
    eprintln!("Captured synthesis seek schedules: {}", dirs.len());
}
