//! Direct unmodified floor-one backend comparisons.
use super::*;
use crate as codec;
#[path = "../tests/support/floor1_vectors.rs"]
#[allow(dead_code)]
mod vectors;
use std::{
    fs,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use vectors::Config;
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct Case {
    long: bool,
    prefix: u32,
    exhausted: bool,
    data: Vec<u8>,
}
fn take<'a>(b: &mut &'a [u8], n: usize) -> &'a [u8] {
    let (v, r) = b.split_at(n);
    *b = r;
    v
}
fn u64_in(b: &mut &[u8]) -> u64 {
    u64::from_le_bytes(take(b, 8).try_into().unwrap())
}
fn u32_in(b: &mut &[u8]) -> u32 {
    u32::from_le_bytes(take(b, 4).try_into().unwrap())
}
fn packet(b: &mut Vec<u8>, v: &[u8]) {
    b.extend((v.len() as u64).to_le_bytes());
    b.extend(v);
}
fn request(c: Config, cases: &[Case]) -> Vec<u8> {
    let mut b = Vec::new();
    for h in c.headers() {
        packet(&mut b, &h);
    }
    b.extend((cases.len() as u64).to_le_bytes());
    for c in cases {
        for v in [c.long as u64, c.prefix as u64, c.exhausted as u64] {
            b.extend(v.to_le_bytes());
        }
        packet(&mut b, &c.data);
    }
    b
}
fn oracle(label: &str, input: &[u8]) -> Vec<u8> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "deadlib-floor1-component-{}-{}.bin",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&p, input).unwrap();
    let r = Command::new(std::env::var_os("DEADLIB_LIBVORBIS_ORACLE").expect("set oracle"))
        .arg("floor1cases")
        .arg(&p)
        .output()
        .unwrap();
    fs::remove_file(p).unwrap();
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    if let Some(root) = std::env::var_os("DEADLIB_VORBIS_FLOOR1_INPUTS") {
        fs::create_dir_all(&root).unwrap();
        fs::write(
            std::path::Path::new(&root).join(format!("{label}.bin")),
            input,
        )
        .unwrap();
        fs::write(
            std::path::Path::new(&root).join(format!("{label}.out")),
            &r.stdout,
        )
        .unwrap();
    }
    r.stdout
}
#[derive(Default)]
struct Counts {
    cases: usize,
    values: usize,
    present: usize,
    eof: usize,
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
    let Floor::Type1(floor) = &setup.floors()[0] else {
        panic!()
    };
    let count = u64_in(&mut input);
    assert_eq!(u64_in(&mut reference), count);
    let mut total = Counts::default();
    for i in 0..count {
        let long = u64_in(&mut input) as usize;
        let prefix = u64_in(&mut input) as u32;
        let exhausted = u64_in(&mut input) != 0;
        let n = u64_in(&mut input) as usize;
        let data = take(&mut input, n);
        let present = take(&mut reference, 1)[0] != 0;
        let position = u64_in(&mut reference) as usize;
        let eof = take(&mut reference, 1)[0] != 0;
        let bins = u32_in(&mut reference) as usize;
        assert_eq!(bins, setup.block_sizes()[long] / 2);
        let mut bits = BitReader::new(data);
        bits.read(prefix).unwrap();
        if exhausted {
            bits.mark_exhausted();
        }
        let mut storage = vec![-1234.25; bins + 16];
        let curve = &mut storage[8..8 + bins];
        curve.fill(0.0);
        let actual = match decode1(
            floor,
            setup.codebooks(),
            &mut bits,
            curve,
            Floor1Behavior::Libvorbis,
        ) {
            Ok(x) => x,
            Err(Error::EndOfPacket) => false,
            Err(e) => panic!("{e}"),
        };
        if !actual {
            curve.fill(0.0);
        }
        assert_eq!(actual, present, "case {i}, presence");
        assert_eq!(bits.is_exhausted(), eof, "case {i}, EOF");
        if !eof {
            assert_eq!(bits.position(), position, "case {i}, position");
        } else {
            total.eof += 1;
        }
        for (bin, &value) in curve.iter().enumerate() {
            assert_eq!(
                value.to_bits(),
                u32_in(&mut reference),
                "case {i}, bin {bin}"
            );
            total.values += 1;
        }
        assert_eq!(&storage[..8], &[-1234.25; 8]);
        assert_eq!(&storage[bins + 8..], &[-1234.25; 8]);
        total.cases += 1;
        total.present += usize::from(present);
    }
    assert!(input.is_empty() && reference.is_empty());
    total
}
#[test]
#[ignore = "requires external libvorbis floor-one backend"]
fn floor_one_guard_witness() {
    let c = Config::default();
    let cases = [Case {
        long: false,
        prefix: 0,
        exhausted: false,
        data: c.floor_packet(0, 0, 0, 0, 3),
    }];
    let input = request(c, &cases);
    let output = oracle("floor1-baseline-guard", &input);
    compare(&input, &output);
}

fn configurations() -> Vec<Config> {
    let mut out = Vec::new();
    for multiplier in 1..=4 {
        for (i, high) in [
            1, 63, 64, 85, 86, 127, 128, 255, 256, 511, 32767, 32768, 32769, 65535, 65536,
        ]
        .into_iter()
        .enumerate()
        {
            for posts in [1, 7, 63] {
                out.push(Config {
                    multiplier,
                    high,
                    posts,
                    subclasses: [0, 1, 3][i % 3],
                    ..Config::default()
                });
            }
        }
        for range_bits in [0, 7, 15] {
            out.push(Config {
                multiplier,
                range_bits,
                posts: 0,
                ..Config::default()
            });
        }
        for posts in [1, 7, 63] {
            out.push(Config {
                multiplier,
                posts,
                absent: true,
                ..Config::default()
            });
            for subclasses in [0, 1, 3] {
                out.push(Config {
                    multiplier,
                    posts,
                    subclasses,
                    empty: true,
                    ..Config::default()
                });
            }
            for high in [65535, (1 << 20) - 1] {
                out.push(Config {
                    multiplier,
                    posts,
                    high,
                    range_bits: 15,
                    subclasses: 3,
                    ..Config::default()
                });
            }
        }
    }
    out
}
fn cases(c: Config, compact: bool) -> Vec<Case> {
    let mut out = std::collections::BTreeSet::new();
    let range = [256, 128, 86, 64][c.multiplier as usize - 1];
    let max = (1 << ilog(range - 1)) - 1;
    let pairs: std::collections::BTreeSet<_> = [
        (0, 0),
        (0, max),
        (max, 0),
        (max, max),
        (range - 1, range - 1),
        (1, range / 2),
        (max / 2, max - 1),
    ]
    .into_iter()
    .collect();
    for long in [false, true] {
        for prefix in if compact { vec![0, 7] } else { vec![0, 3, 7] } {
            for &(y0, y1) in &pairs {
                for selection in [0, 3, 7] {
                    for seed in 0..4 {
                        let data = c.floor_packet(prefix, y0, y1, selection, seed);
                        out.insert(Case {
                            long,
                            prefix,
                            exhausted: false,
                            data: data.clone(),
                        });
                        if !compact && y0 == max && y1 == max && selection == 0 && seed == 3 {
                            for n in 0..data.len() {
                                if n * 8 >= prefix as usize {
                                    out.insert(Case {
                                        long,
                                        prefix,
                                        exhausted: false,
                                        data: data[..n].to_vec(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
            out.insert(Case {
                long,
                prefix,
                exhausted: true,
                data: vec![0xff; 4],
            });
            out.insert(Case {
                long,
                prefix,
                exhausted: false,
                data: vec![0; 4],
            });
        }
    }
    out.into_iter().collect()
}
#[test]
#[ignore = "requires external libvorbis floor-one backend"]
fn floor_one_guards_eof_and_prediction_match_reference() {
    let configs = configurations();
    let mut total = Counts::default();
    for (i, c) in configs.iter().copied().enumerate() {
        for (j, cases) in cases(c, false).chunks(128).enumerate() {
            let input = request(c, cases);
            let output = oracle(&format!("floor1-component-{i}-batch-{j}"), &input);
            let n = compare(&input, &output);
            total.cases += n.cases;
            total.values += n.values;
            total.present += n.present;
            total.eof += n.eof;
        }
    }
    eprintln!(
        "floor1 components: {} setups, {} cases, {} exact values, {} present floors, {} sticky EOF cases",
        configs.len(),
        total.cases,
        total.values,
        total.present,
        total.eof
    );
}
fn golden_configs() -> Vec<Config> {
    let configs = configurations();
    [2, 7, 49, 95, 142, 190, 226, 250]
        .into_iter()
        .map(|i| configs[i])
        .collect()
}
#[test]
#[ignore = "manual golden regeneration; requires external libvorbis floor-one backend"]
fn regenerate_floor1_component_golden() {
    let configs = golden_configs();
    let mut wire = b"DF1CMP01".to_vec();
    wire.extend((configs.len() as u32).to_le_bytes());
    for (i, c) in configs.into_iter().enumerate() {
        let cases: Vec<_> = cases(c, true).into_iter().step_by(9).collect();
        let input = request(c, &cases);
        let output = oracle(&format!("floor1-component-golden-{i}"), &input);
        packet(&mut wire, &input);
        packet(&mut wire, &output);
    }
    fs::write(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/floor1-components-v1.libvorbis"),
        wire,
    )
    .unwrap();
}
#[test]
fn floor_one_components_match_retained_reference() {
    let mut wire = include_bytes!("../tests/fixtures/floor1-components-v1.libvorbis").as_slice();
    assert_eq!(take(&mut wire, 8), b"DF1CMP01");
    assert_eq!(u32_in(&mut wire) as usize, golden_configs().len());
    let mut total = 0;
    for c in golden_configs() {
        let cases: Vec<_> = cases(c, true).into_iter().step_by(9).collect();
        let n = u64_in(&mut wire) as usize;
        let input = take(&mut wire, n);
        assert_eq!(input, request(c, &cases));
        let n = u64_in(&mut wire) as usize;
        let output = take(&mut wire, n);
        total += compare(input, output).cases;
    }
    assert!(wire.is_empty());
    assert_eq!(total, 146);
}
#[test]
fn floor_one_published_guard_and_eof_remain_default() {
    assert_eq!(
        crate::DecodeOptions::default().floor1_behavior,
        Floor1Behavior::Vorbis
    );
    assert_eq!(
        crate::OggOptions::default().floor1_behavior,
        Floor1Behavior::Vorbis
    );
    assert_eq!(
        crate::SeekOptions::default().floor1_behavior,
        Floor1Behavior::Vorbis
    );
    let c = Config::default();
    let h = c.headers();
    let id = crate::Identification::parse(&h[0]).unwrap();
    let setup = crate::Setup::parse(&h[2], &id, &crate::Limits::default()).unwrap();
    let Floor::Type1(f) = &setup.floors()[0] else {
        panic!()
    };
    let packet = c.floor_packet(0, 0, 0, 0, 3);
    let mut a = vec![0.0; 32];
    let mut b = a.clone();
    let mut left = BitReader::new(&packet);
    let mut right = left.clone();
    assert!(
        decode1(
            f,
            setup.codebooks(),
            &mut left,
            &mut a,
            Floor1Behavior::Vorbis
        )
        .unwrap()
    );
    assert!(
        decode1(
            f,
            setup.codebooks(),
            &mut right,
            &mut b,
            Floor1Behavior::Libvorbis
        )
        .unwrap()
    );
    assert_ne!(a, b);
    assert_eq!(left.position(), right.position());
    let c = Config { posts: 0, ..c };
    let h = c.headers();
    let setup = crate::Setup::parse(&h[2], &id, &crate::Limits::default()).unwrap();
    let Floor::Type1(f) = &setup.floors()[0] else {
        panic!()
    };
    let packet = [0x80];
    for behavior in [Floor1Behavior::Vorbis, Floor1Behavior::Libvorbis] {
        let mut bits = BitReader::new(&packet);
        bits.read(7).unwrap();
        let mut curve = [0.0; 32];
        let r = decode1(f, setup.codebooks(), &mut bits, &mut curve, behavior);
        assert!(bits.is_exhausted());
        if behavior == Floor1Behavior::Vorbis {
            assert_eq!(r, Err(Error::EndOfPacket));
        } else {
            assert_eq!(r, Ok(true));
            assert!(curve.iter().all(|&v| v == INVERSE_DB[0]));
        }
    }
}
