//! Test-only direct comparisons with unmodified floor-zero backend callbacks.
use super::*;
use crate as codec;
#[path = "../tests/support/floor0_vectors.rs"]
#[allow(dead_code)]
mod vectors;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use vectors::Config;

struct Case {
    long: bool,
    prefix: u8,
    data: Vec<u8>,
}
fn take<'a>(b: &mut &'a [u8], n: usize) -> &'a [u8] {
    let (a, r) = b.split_at(n);
    *b = r;
    a
}
fn u32_in(b: &mut &[u8]) -> u32 {
    u32::from_le_bytes(take(b, 4).try_into().unwrap())
}
fn u64_in(b: &mut &[u8]) -> u64 {
    u64::from_le_bytes(take(b, 8).try_into().unwrap())
}
fn request(c: Config, cases: &[Case]) -> Vec<u8> {
    let mut out = Vec::new();
    for h in c.headers() {
        out.extend((h.len() as u64).to_le_bytes());
        out.extend(h);
    }
    out.extend((cases.len() as u64).to_le_bytes());
    for c in cases {
        out.extend((c.long as u64).to_le_bytes());
        out.extend((c.prefix as u64).to_le_bytes());
        out.extend((c.data.len() as u64).to_le_bytes());
        out.extend(&c.data);
    }
    out
}
fn oracle(label: &str, input: &[u8]) -> Vec<u8> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "deadlib-floor0-component-{}-{}.bin",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&path, input).unwrap();
    let output = Command::new(
        std::env::var_os("DEADLIB_LIBVORBIS_ORACLE").expect("set DEADLIB_LIBVORBIS_ORACLE"),
    )
    .arg("floor0cases")
    .arg(&path)
    .output()
    .unwrap();
    fs::remove_file(path).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    if let Some(root) = std::env::var_os("DEADLIB_VORBIS_FLOOR0_INPUTS") {
        let root = PathBuf::from(root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join(format!("{label}.bin")), input).unwrap();
        fs::write(root.join(format!("{label}.out")), &output.stdout).unwrap();
    }
    output.stdout
}
fn cases(c: Config, compact: bool) -> Vec<Case> {
    let mut seen = BTreeSet::new();
    let selectors = BTreeSet::from([
        0,
        c.books as u32 - 1,
        c.books as u32,
        (1 << ilog(c.books as u32)) - 1,
    ]);
    for long in [false, true] {
        for prefix in if compact { vec![0, 7] } else { vec![0, 3, 7] } {
            for seed in 0..7 {
                for &selector in &selectors {
                    let data = c.floor_packet(prefix, c.amplitude(seed), selector, seed);
                    seen.insert((long, prefix, data.clone()));
                    if !compact && prefix == 3 && selector == 0 && [1, 3].contains(&seed) {
                        for length in 0..data.len() {
                            seen.insert((long, prefix, data[..length].to_vec()));
                        }
                    }
                }
            }
        }
    }
    seen.into_iter()
        .map(|(long, prefix, data)| Case { long, prefix, data })
        .collect()
}
#[derive(Default)]
struct Counts {
    cases: usize,
    finite: usize,
    nonfinite: usize,
    eop: usize,
}
fn compare(mut input: &[u8], mut reference: &[u8]) -> Counts {
    let mut headers = Vec::new();
    for _ in 0..3 {
        let n = u64_in(&mut input) as usize;
        headers.push(take(&mut input, n).to_vec());
    }
    let id = crate::Identification::parse(&headers[0]).unwrap();
    let setup = crate::Setup::parse_with_options(
        &headers[2],
        &id,
        crate::ParseOptions {
            allow_empty_codebooks: true,
            ..crate::ParseOptions::default()
        },
    )
    .unwrap();
    let Floor::Type0(floor) = &setup.floors()[0] else {
        panic!("floor zero")
    };
    let count = u64_in(&mut input);
    assert_eq!(u64_in(&mut reference), count);
    let mut total = Counts::default();
    for case in 0..count {
        let long = u64_in(&mut input);
        let prefix = u64_in(&mut input) as u32;
        let n = u64_in(&mut input) as usize;
        let data = take(&mut input, n);
        let present = take(&mut reference, 1)[0] != 0;
        let position = u64_in(&mut reference) as usize;
        let eop = take(&mut reference, 1)[0] != 0;
        let bins = u32_in(&mut reference) as usize;
        let _amplitude = u32_in(&mut reference);
        assert_eq!(bins, setup.block_sizes()[long as usize] / 2);
        let mapping = BarkMap::new(floor.rate, floor.bark_map_size, Floor0Behavior::Libvorbis);
        for i in 0..bins {
            assert_eq!(
                mapping.index(i, bins),
                u32_in(&mut reference) as usize,
                "case {case}, map {i}, rate {}, size {}",
                floor.rate,
                floor.bark_map_size
            );
        }
        let mut bits = BitReader::new(data);
        let mut output = vec![0.0; bins];
        let actual = bits.read(prefix).and_then(|_| {
            decode0(
                floor,
                setup.codebooks(),
                &mut bits,
                &mut output,
                true,
                Floor0Behavior::Libvorbis,
            )
        });
        let actual = match actual {
            Ok(p) => p,
            Err(Error::EndOfPacket) => false,
            Err(e) => panic!("case {case}: {e}"),
        };
        assert_eq!(actual, present, "case {case}");
        assert_eq!(bits.is_exhausted(), eop, "case {case}");
        // Native position names the last successful read. libogg exposes an
        // endbyte/endbit sentinel after EOF; compare positions before that state.
        if !eop {
            assert_eq!(bits.position(), position, "case {case}");
        } else {
            total.eop += 1;
        }
        let mut peak = 0.0f64;
        let mut error = 0.0f64;
        let mut squares = 0.0;
        let mut finite = 0;
        for actual in output {
            let expected = f32::from_bits(u32_in(&mut reference));
            if expected.is_nan() {
                assert!(actual.is_nan(), "case {case}");
                total.nonfinite += 1;
            } else if expected.is_infinite() {
                assert_eq!(actual, expected, "case {case}");
                total.nonfinite += 1;
            } else {
                assert!(actual.is_finite(), "case {case}");
                let d = (actual as f64 - expected as f64).abs();
                error = error.max(d);
                squares += d * d;
                peak = peak.max((expected as f64).abs());
                finite += 1;
            }
        }
        let scale = peak.max(1.0);
        assert!(
            error <= 5e-5 * scale,
            "case {case}: peak {error}, scale {scale}"
        );
        if finite != 0 {
            assert!(
                (squares / finite as f64).sqrt() <= 3e-6 * scale,
                "case {case}: RMS"
            );
        }
        total.finite += finite;
        total.cases += 1;
    }
    assert!(input.is_empty() && reference.is_empty());
    total
}
fn configurations() -> Vec<Config> {
    let mut out: Vec<_> = (0..=63)
        .map(|width| Config {
            width,
            ..Config::default()
        })
        .collect();
    for order in [1, 2, 3, 7, 8, 31, 64, 127, 254, 255] {
        for dimensions in [1, 2, 3, 4, 8] {
            for sequence in [false, true] {
                out.push(Config {
                    order,
                    dimensions,
                    sequence,
                    ..Config::default()
                });
            }
        }
    }
    for offset in [0, 1, 60, 255] {
        for order in [1, 2, 7, 31, 255] {
            for width in [6, 31, 32, 33, 63] {
                out.push(Config {
                    offset,
                    order,
                    width,
                    dimensions: 3,
                    ..Config::default()
                });
            }
        }
    }
    for books in [1, 2, 3, 4, 7, 8, 15, 16] {
        out.push(Config {
            books,
            order: 3,
            ..Config::default()
        });
    }
    for width in [0, 6, 31, 32, 33, 63] {
        for order in [1, 2, 31, 255] {
            for dimensions in [1, 3, 8] {
                out.push(Config {
                    width,
                    order,
                    dimensions,
                    empty: true,
                    rate: 65535,
                    bark: 65535,
                    ..Config::default()
                });
            }
        }
    }
    out
}
#[test]
#[ignore = "requires external libvorbis floor0 callbacks"]
fn floor_zero_components_match_reference_callbacks() {
    let configs = configurations();
    let mut total = Counts::default();
    for (i, c) in configs.iter().copied().enumerate() {
        let input = request(c, &cases(c, false));
        let reference = oracle(&format!("components-{i}"), &input);
        let n = compare(&input, &reference);
        total.cases += n.cases;
        total.finite += n.finite;
        total.nonfinite += n.nonfinite;
        total.eop += n.eop;
    }
    eprintln!(
        "floor0 components: {} setups, {} cases, {} finite values, {} nonfinite classes, {} EOF cases",
        configs.len(),
        total.cases,
        total.finite,
        total.nonfinite,
        total.eop
    );
}
#[test]
fn floor_zero_components_match_retained_reference() {
    let mut wire = include_bytes!("../tests/fixtures/floor0-components-v1.libvorbis").as_slice();
    assert_eq!(take(&mut wire, 8), b"DVF0CMP1");
    let count = u32_in(&mut wire);
    for _ in 0..count {
        let n = u64_in(&mut wire) as usize;
        let input = take(&mut wire, n);
        let n = u64_in(&mut wire) as usize;
        let output = take(&mut wire, n);
        compare(input, output);
    }
    assert!(wire.is_empty());
}
#[test]
fn floor_zero_bark_maps_match_retained_reference() {
    let mut wire = include_bytes!("../tests/fixtures/floor0-bark-v1.libvorbis").as_slice();
    assert_eq!(take(&mut wire, 8), b"DVF0MAP1");
    let count = u32_in(&mut wire);
    let mut differences = 0;
    for _ in 0..count {
        let rate = u16::from_le_bytes(take(&mut wire, 2).try_into().unwrap());
        let size = u16::from_le_bytes(take(&mut wire, 2).try_into().unwrap());
        let n = u16::from_le_bytes(take(&mut wire, 2).try_into().unwrap()) as usize;
        let map = BarkMap::new(rate, size, Floor0Behavior::Libvorbis);
        let spec = BarkMap::new(rate, size, Floor0Behavior::Vorbis);
        for i in 0..n {
            let expected = u16::from_le_bytes(take(&mut wire, 2).try_into().unwrap()) as usize;
            assert_eq!(
                map.index(i, n),
                expected,
                "rate {rate}, size {size}, n {n}, i {i}"
            );
            differences += usize::from(spec.index(i, n) != expected);
        }
    }
    assert!(wire.is_empty());
    assert!(differences > 0);
    eprintln!("floor0 Bark: {count} maps, {differences} published/reference integer differences");
}
#[test]
#[ignore = "manual golden regeneration; requires external libvorbis floor0 callbacks"]
fn regenerate_floor0_reference_goldens() {
    let mut components = b"DVF0CMP1".to_vec();
    let mut configs: Vec<_> = (0..=63)
        .map(|width| Config {
            width,
            long: 64,
            ..Config::default()
        })
        .collect();
    for (order, dimensions, empty, books, offset) in [
        (2, 1, false, 3, 0),
        (7, 3, false, 16, 255),
        (31, 8, false, 7, 60),
        (255, 3, false, 2, 30),
        (255, 3, true, 1, 30),
    ] {
        configs.push(Config {
            order,
            dimensions,
            empty,
            books,
            offset,
            long: 64,
            ..Config::default()
        });
    }
    components.extend((configs.len() as u32).to_le_bytes());
    for (i, c) in configs.into_iter().enumerate() {
        let input = request(c, &cases(c, true));
        let output = oracle(&format!("component-golden-{i}"), &input);
        components.extend((input.len() as u64).to_le_bytes());
        components.extend(input);
        components.extend((output.len() as u64).to_le_bytes());
        components.extend(output);
    }
    fs::write(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/floor0-components-v1.libvorbis"),
        components,
    )
    .unwrap();
    let mut maps = BTreeMap::new();
    for rate in [
        1, 2, 3, 7, 11, 25, 63, 1000, 8000, 11025, 16000, 22050, 32000, 44100, 48000, 65534, 65535,
    ] {
        for bark in [
            1, 2, 3, 7, 31, 32, 63, 64, 127, 128, 255, 256, 1023, 4095, 16383, 65535,
        ] {
            for long in [64, 256, 2048, 8192] {
                let c = Config {
                    rate,
                    bark,
                    long,
                    ..Config::default()
                };
                let cases = [false, true]
                    .into_iter()
                    .map(|long| Case {
                        long,
                        prefix: 0,
                        data: c.floor_packet(0, 0, 0, 0),
                    })
                    .collect::<Vec<_>>();
                let input = request(c, &cases);
                let output = oracle(&format!("bark-{rate}-{bark}-{long}"), &input);
                let mut out = output.as_slice();
                assert_eq!(u64_in(&mut out), 2);
                for _ in 0..2 {
                    assert_eq!(take(&mut out, 1), [0]);
                    assert_eq!(u64_in(&mut out), 6);
                    assert_eq!(take(&mut out, 1), [0]);
                    let n = u32_in(&mut out) as usize;
                    assert_eq!(u32_in(&mut out), 0);
                    let mut values = Vec::new();
                    for _ in 0..n {
                        let v = u32_in(&mut out);
                        assert!(v < bark as u32);
                        values.push(v as u16);
                    }
                    assert!(take(&mut out, n * 4).iter().all(|&b| b == 0));
                    if let Some(previous) = maps.insert((rate, bark, n), values.clone()) {
                        assert_eq!(previous, values);
                    }
                }
                assert!(out.is_empty());
            }
        }
    }
    let mut out = b"DVF0MAP1".to_vec();
    out.extend((maps.len() as u32).to_le_bytes());
    for ((rate, bark, n), values) in maps {
        out.extend(rate.to_le_bytes());
        out.extend(bark.to_le_bytes());
        out.extend((n as u16).to_le_bytes());
        for v in values {
            out.extend(v.to_le_bytes());
        }
    }
    fs::write(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/floor0-bark-v1.libvorbis"),
        out,
    )
    .unwrap();
}
