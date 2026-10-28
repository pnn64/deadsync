//! Independent packet clock and bounded EOS comparisons with libvorbis.
use super::*;
use crate::reference_vectors::eos_vectors as vectors;
use std::{
    fs,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
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
fn packet(w: &mut &[u8]) -> Vec<u8> {
    let n = u64_in(w) as usize;
    take(w, n).to_vec()
}
fn put_packet(w: &mut Vec<u8>, p: &[u8]) {
    w.extend((p.len() as u64).to_le_bytes());
    w.extend(p);
}
fn oracle(mode: &str, label: &str, input: &[u8]) -> Vec<u8> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "deadlib-eos-{}-{}.bin",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&p, input).unwrap();
    let r = Command::new(std::env::var_os("DEADLIB_LIBVORBIS_ORACLE").expect("set oracle"))
        .arg(mode)
        .arg(&p)
        .output()
        .unwrap();
    fs::remove_file(p).unwrap();
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    if let Some(root) = std::env::var_os("DEADLIB_VORBIS_EOS_INPUTS") {
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
#[derive(Default)]
struct Difference {
    n: u64,
    peak: f64,
    energy: f64,
    signal: f64,
}
impl Difference {
    fn add(&mut self, a: f32, b: f32) {
        assert!(a.is_finite() && b.is_finite());
        let e = a as f64 - b as f64;
        self.n += 1;
        self.peak = self.peak.max(e.abs());
        self.energy += e * e;
        self.signal = self.signal.max((b as f64).abs());
    }
    fn check(&self, label: &str) {
        let rms = (self.energy / self.n.max(1) as f64).sqrt();
        eprintln!(
            "{label}: {} values, peak {:.9e}, RMS {:.9e}",
            self.n, self.peak, rms
        );
        assert!(self.peak <= 3e-6 * self.signal.max(1.0) && rms <= 5e-7 * self.signal.max(1.0));
    }
}
fn decoder(h: &[Vec<u8>]) -> PacketDecoder {
    let id = Identification::parse(&h[0]).unwrap();
    PacketDecoder::new(Setup::parse(&h[2], &id, &Limits::default()).unwrap()).unwrap()
}
fn compare_timed(mut input: &[u8], mut reference: &[u8]) {
    let h: Vec<_> = (0..3).map(|_| packet(&mut input)).collect();
    let mut d = decoder(&h);
    let ch = u32_in(&mut reference) as usize;
    let count = u64_in(&mut input);
    assert_eq!(u64_in(&mut reference), count);
    let mut difference = Difference::default();
    let mut decisions = 0;
    let mut discarded = 0;
    for _ in 0..count {
        d.reset();
        let mut t = Timing::with_options(false, EosBehavior::Libvorbis);
        for _ in 0..u64_in(&mut input) {
            let gp = u64_in(&mut input);
            let eos = u64_in(&mut input) != 0;
            let p = packet(&mut input);
            let marker = take(&mut reference, 1)[0];
            let Some(b) = d.decode(&p).unwrap() else {
                assert_eq!(marker, b'S');
                t.discard_packet();
                discarded += 1;
                continue;
            };
            assert_eq!(marker, b'P');
            assert_eq!(u32_in(&mut reference) as usize, b.block_size);
            let n = u32_in(&mut reference) as usize;
            let cgp = u64_in(&mut reference);
            let crop = t
                .advance(
                    b.start_frame,
                    b.frames,
                    gp != u64::MAX,
                    PageHeader {
                        flags: if eos { container::EOS } else { 0 },
                        granule_position: gp,
                        serial: 193,
                        sequence: 0,
                    },
                    true,
                )
                .unwrap();
            let (skip, keep) = crop.map_or((0, 0), |x| (x.skip, x.keep));
            assert_eq!(keep - skip, n);
            assert_eq!(t.reference_gp.map_or(u64::MAX, |x| x as u64), cgp);
            for channel in 0..ch {
                for &x in &b.plane(channel).unwrap()[skip..keep] {
                    difference.add(x, f32::from_bits(u32_in(&mut reference)));
                }
            }
            decisions += 1;
        }
    }
    assert!(input.is_empty() && reference.is_empty());
    difference.check("EOS timed packets");
    eprintln!("EOS timed: {count} cases, {decisions} accepted, {discarded} discarded");
}
fn state_request(boundaries: bool) -> Vec<u8> {
    let p = vectors::packets(vectors::FILES[0]);
    let mut out = Vec::new();
    for p in &p[..6] {
        put_packet(&mut out, p);
    }
    let mut cases = Vec::new();
    for known in [0u64, 1] {
        for excess in [
            0,
            1,
            1023,
            1024,
            1025,
            (1u64 << 31) - 1,
            1u64 << 31,
            (1u64 << 31) + 1,
            (1u64 << 32) - 1,
            1u64 << 32,
            (1u64 << 32) + 1,
            (1u64 << 32) + 1023,
            (1u64 << 32) + 1024,
            (1u64 << 63) - 16384,
        ] {
            let gp = 10000;
            if boundaries || excess <= 1025 {
                cases.push((known, gp + excess - 1024, gp));
            }
        }
    }
    cases.extend([(0, u64::MAX, 0), (0, u64::MAX, 1), (1, 0, 1), (0, 0, 1)]);
    for known in [0, 1] {
        for state in [
            i64::MAX as u64 - 1024,
            i64::MAX as u64 - 1023,
            i64::MAX as u64 - 10,
            i64::MAX as u64,
        ] {
            if !boundaries && state < i64::MAX as u64 - 10 {
                continue;
            }
            for gp in [0, 1, 10000, i64::MAX as u64] {
                cases.push((known, state, gp));
            }
        }
    }
    out.extend((cases.len() as u64).to_le_bytes());
    for (k, s, g) in cases {
        out.extend(k.to_le_bytes());
        out.extend(s.to_le_bytes());
        out.extend(g.to_le_bytes());
    }
    out
}
fn compare_states(mut input: &[u8], mut reference: &[u8]) {
    let p: Vec<_> = (0..6).map(|_| packet(&mut input)).collect();
    let mut d = decoder(&p[..3]);
    let count = u64_in(&mut input);
    let width = u32_in(&mut reference) as usize;
    assert!([4, 8].contains(&width));
    if count >= 32 {
        assert_eq!(width, core::mem::size_of::<core::ffi::c_long>());
    }
    let ch = u32_in(&mut reference) as usize;
    assert_eq!(u64_in(&mut reference), count);
    let mut diff = Difference::default();
    for _ in 0..count {
        let known = u64_in(&mut input) != 0;
        let state = u64_in(&mut input);
        let gp = u64_in(&mut input);
        d.reset();
        for p in &p[3..5] {
            d.decode(p).unwrap().unwrap();
        }
        let b = d.decode(&p[5]).unwrap().unwrap();
        assert_eq!(b.frames, 1024);
        let mut t = Timing::with_options(false, EosBehavior::Libvorbis);
        t.reference_gp = known.then_some(state as i64 as i128);
        t.reference_samples = (state != u64::MAX).then_some(state);
        let padding = t.reference_padding(b.frames, Some(gp), true).unwrap();
        assert_eq!(u32_in(&mut reference) as usize, b.block_size);
        let n = u32_in(&mut reference) as usize;
        assert_eq!(n, b.frames - padding);
        assert_eq!(u64_in(&mut reference), gp);
        assert_eq!(u64_in(&mut reference), t.reference_samples.unwrap());
        for channel in 0..ch {
            for &x in &b.plane(channel).unwrap()[..n] {
                diff.add(x, f32::from_bits(u32_in(&mut reference)));
            }
        }
    }
    assert!(input.is_empty() && reference.is_empty());
    diff.check("EOS counter boundaries");
    eprintln!("EOS counter states: {count}");
}
#[test]
#[ignore = "requires external unmodified libvorbis synthesis"]
fn timed_eos_packets_and_counter_boundaries_match_reference() {
    let all = vectors::configurations();
    for fixture in 0..vectors::FILES.len() {
        let cases: Vec<_> = all
            .iter()
            .filter(|c| c.fixture == fixture)
            .map(|c| vectors::timed(&c.bytes()).1)
            .collect();
        let h = vectors::packets(vectors::FILES[fixture]);
        let q = vectors::request(&h[..3], &cases);
        compare_timed(
            &q,
            &oracle("pcm-timed-cases", &format!("eos-timed-{fixture}"), &q),
        );
    }
    let q = state_request(true);
    compare_states(&q, &oracle("eos-state-cases", "eos-states", &q));
}
#[test]
#[ignore = "manual external-oracle golden regeneration"]
fn regenerate_eos_packet_goldens() {
    let mut out = b"DEOSPK01".to_vec();
    out.extend(2u32.to_le_bytes());
    let h = vectors::packets(vectors::FILES[0]);
    let cases: Vec<_> = vectors::configurations()
        .into_iter()
        .filter(|c| c.fixture == 0 && c.count == 6 && c.group == 1)
        .map(|c| vectors::timed(&c.bytes()).1)
        .collect();
    for (mode, q) in [
        ("pcm-timed-cases", vectors::request(&h[..3], &cases)),
        ("eos-state-cases", state_request(false)),
    ] {
        let r = oracle(mode, mode, &q);
        put_packet(&mut out, &q);
        put_packet(&mut out, &r);
    }
    fs::write(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/eos-packets-v1.libvorbis"),
        out,
    )
    .unwrap();
}
#[test]
fn eos_packet_and_counter_goldens() {
    let mut w = include_bytes!("../tests/fixtures/eos-packets-v1.libvorbis").as_slice();
    assert_eq!(take(&mut w, 8), b"DEOSPK01");
    assert_eq!(u32_in(&mut w), 2);
    let q = packet(&mut w);
    let r = packet(&mut w);
    compare_timed(&q, &r);
    let q = packet(&mut w);
    let r = packet(&mut w);
    compare_states(&q, &r);
    assert!(w.is_empty());
}

#[test]
fn metadata_block_query_does_not_require_synthesis_window_flags() {
    let mut modes = vec![
        crate::Mode {
            long_block: true,
            mapping: 0
        };
        64
    ];
    for (i, mode) in modes.iter_mut().enumerate() {
        mode.long_block = i % 2 == 1;
    }
    for i in 0..64 {
        let p = [(i as u8) << 1];
        assert_eq!(
            crate::decoder::declared_block_size(&modes, [64, 8192], &p),
            Some(if i % 2 == 1 { 8192 } else { 64 })
        );
    }
    assert_eq!(
        crate::decoder::declared_block_size(&modes, [64, 8192], &[]),
        None
    );
    assert_eq!(
        crate::decoder::declared_block_size(&modes, [64, 8192], &[1]),
        None
    );
    assert_eq!(
        crate::decoder::declared_block_size(&modes[..33], [64, 8192], &[126]),
        None
    );
    assert_eq!(
        crate::decoder::declared_block_size(&[], [64, 8192], &[0]),
        None
    );
}
