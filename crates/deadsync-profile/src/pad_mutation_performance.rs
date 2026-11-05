use super::PadConfigProfile;
use crate::perf;
#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "pad_mutation_original.rs"]
mod original;
use std::hint::black_box as bb;
fn profile(name: String, index: usize) -> PadConfigProfile {
    PadConfigProfile {
        name,
        backend: format!("backend{}", index % 3),
        pad_type: Some("pad".into()),
        serial: Some(format!("serial{index}")),
        default_for_serials: vec!["sensor-a".into(), "sensor-b".into(), "sensor-a".into()],
        global_default: index % 2 == 0,
        settings: vec![("threshold".into(), format!("{index}"))],
    }
}
fn fixture(count: usize) -> Vec<PadConfigProfile> {
    (0..count)
        .map(|i| {
            profile(
                format!("shared-pad-configuration-name-with-a-long-prefix-{i:08}"),
                i,
            )
        })
        .collect()
}
fn capacities(list: &[PadConfigProfile]) -> Vec<Vec<usize>> {
    list.iter()
        .map(|p| {
            let mut v = vec![
                p.name.capacity(),
                p.backend.capacity(),
                p.pad_type.as_ref().map_or(0, String::capacity),
                p.serial.as_ref().map_or(0, String::capacity),
                p.default_for_serials.capacity(),
                p.settings.capacity(),
            ];
            v.extend(p.default_for_serials.iter().map(String::capacity));
            for (k, s) in &p.settings {
                v.extend([k.capacity(), s.capacity()]);
            }
            v
        })
        .collect()
}
fn same(a: &[PadConfigProfile], b: &[PadConfigProfile]) {
    assert_eq!(a, b);
    assert_eq!(capacities(a), capacities(b));
}
fn check(list: Vec<PadConfigProfile>, old: &str, new: &str, serial: &str) {
    for spare in [false, true] {
        let paired = || {
            let (mut a, mut b) = (list.clone(), list.clone());
            if spare {
                for list in [&mut a, &mut b] {
                    list.reserve(7);
                    for p in list {
                        p.name.reserve(17);
                        p.backend.reserve(13);
                        if let Some(s) = &mut p.pad_type {
                            s.reserve(11);
                        }
                        if let Some(s) = &mut p.serial {
                            s.reserve(19);
                        }
                        p.default_for_serials.reserve(3);
                        for s in &mut p.default_for_serials {
                            s.reserve(9);
                        }
                        p.settings.reserve(4);
                        for (k, v) in &mut p.settings {
                            k.reserve(13);
                            v.reserve(17);
                        }
                    }
                }
            }
            (a, b)
        };
        let (mut a, mut b) = paired();
        let (ca, cb) = (a.capacity(), b.capacity());
        assert_eq!(
            original::rename_config(&mut a, old, new),
            super::rename_config(&mut b, old, new)
        );
        same(&a, &b);
        assert_eq!((a.capacity(), b.capacity()), (ca, cb));
        let (mut a, mut b) = paired();
        let (ca, cb) = (a.capacity(), b.capacity());
        assert_eq!(
            original::set_default_config(&mut a, serial, old),
            super::set_default_config(&mut b, serial, old)
        );
        same(&a, &b);
        assert_eq!((a.capacity(), b.capacity()), (ca, cb));
    }
}
#[test]
fn exact_updates_and_names() {
    let names = [
        "",
        " ",
        "A",
        "a",
        "A\0B",
        "\u{2003}A\t",
        "Stra\u{df}e",
        "STRASSE",
        "\u{96ea}",
        "\u{feff}",
    ];
    for owner in names {
        for name in names {
            let a = original::migrated_config_name(owner, name);
            let b = super::migrated_config_name(owner, name);
            assert_eq!(a, b);
            assert_eq!(a.capacity(), b.capacity(), "{owner:?} / {name:?}");
        }
    }
    for a in 0..100 {
        for b in 0..100 {
            for ch in ["a", "\u{96ea}", "\u{2003}"] {
                let owner = ch.repeat(a);
                let name = ch.repeat(b);
                let x = original::migrated_config_name(&owner, &name);
                let y = super::migrated_config_name(&owner, &name);
                assert_eq!(x, y);
                assert_eq!(x.capacity(), y.capacity(), "length {a}/{b}");
            }
        }
    }
    for count in [0, 1, 2, 3, 8, 16, 64] {
        for old in names {
            for new in names {
                let list = (0..count)
                    .map(|i| profile(names[i % names.len()].into(), i))
                    .collect();
                check(list, old, new, "sensor-a");
            }
        }
        for i in 0..count {
            let list = fixture(count);
            let old = list[i].name.clone();
            check(list, &old, "new name", "sensor-b");
        }
    }
    let mut seed = 0x12345678u32;
    for _ in 0..8192 {
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        let count = next() as usize % 24;
        let list = (0..count)
            .map(|i| {
                let mut p = profile(names[next() as usize % names.len()].into(), i);
                p.default_for_serials = (0..next() % 8)
                    .map(|_| names[next() as usize % names.len()].into())
                    .collect();
                p
            })
            .collect();
        check(
            list,
            names[next() as usize % names.len()],
            names[next() as usize % names.len()],
            names[next() as usize % names.len()],
        );
    }
}
fn compare(label: &str, n: usize, mut work: impl FnMut(bool)) {
    for (c, name) in [(false, "original"), (true, "current")] {
        let (_, v) = perf::measure(|| work(c));
        println!("{label}/{name}: allocation {v:?}");
    }
    bench::compare(label, n, work);
}
#[test]
#[ignore]
fn benchmark() {
    for count in [1, 4, 16, 64] {
        for last in [false, true] {
            let index = if last { count - 1 } else { 0 };
            let base = fixture(count);
            let name = base[index].name.clone();
            let new = format!(
                "shared-pad-configuration-name-with-a-long-prefix-{:08}",
                count + 1
            );
            let (mut a, mut b) = (base.clone(), base.clone());
            compare(
                &format!("rename/{count}-{}", if last { "last" } else { "first" }),
                5000,
                |c| {
                    let list = if c { &mut b } else { &mut a };
                    let f = if c {
                        super::rename_config
                    } else {
                        original::rename_config
                    };
                    bb(f(bb(list), bb(&name), bb(&new)));
                    bb(f(bb(list), bb(&new), bb(&name)));
                },
            );
            let (mut a, mut b) = (base.clone(), base);
            compare(
                &format!("default/{count}-{}", if last { "last" } else { "first" }),
                5000,
                |c| {
                    bb(if c {
                        super::set_default_config(bb(&mut b), bb("sensor-a"), bb(&name))
                    } else {
                        original::set_default_config(bb(&mut a), bb("sensor-a"), bb(&name))
                    });
                },
            );
        }
    }
    for (count, label) in [(16, "missing"), (0, "empty")] {
        let (mut a, mut b) = (fixture(count), fixture(count));
        compare(&format!("rename/{label}"), 10000, |c| {
            bb(if c {
                super::rename_config(bb(&mut b), bb("missing"), bb("new"))
            } else {
                original::rename_config(bb(&mut a), bb("missing"), bb("new"))
            });
        });
        compare(&format!("default/{label}"), 10000, |c| {
            bb(if c {
                super::set_default_config(bb(&mut b), bb("sensor-a"), bb("missing"))
            } else {
                original::set_default_config(bb(&mut a), bb("sensor-a"), bb("missing"))
            });
        });
    }
    let (long_owner, long_name) = ("a".repeat(128), "b".repeat(128));
    for (label, owner, name) in [
        ("short", "A", "B"),
        ("normal", "DDR player", "Left pad"),
        ("long", long_owner.as_str(), long_name.as_str()),
        ("empty-owner", "", "name"),
        ("unicode", "\u{2003}\u{96ea}\u{2003}", "\u{e9} pad"),
    ] {
        compare(&format!("name/{label}"), 20000, |c| {
            bb(if c {
                super::migrated_config_name(bb(owner), bb(name))
            } else {
                original::migrated_config_name(bb(owner), bb(name))
            });
        });
    }
}

#[test]
#[ignore = "deterministic full pad mutation record/capacity trace"]
fn dump_trace() {
    use std::io::Write;
    let mut out = std::io::BufWriter::new(
        std::fs::File::create(
            std::env::var_os("DEADSYNC_PAD_MUTATION_TRACE_OUT").expect("trace path"),
        )
        .unwrap(),
    );
    for count in [0, 1, 2, 4, 16, 64] {
        let base = fixture(count);
        for i in 0..count + 1 {
            let old = base.get(i).map_or("missing", |p| p.name.as_str());
            for new in [
                "",
                " new name ",
                old,
                "shared-pad-configuration-name-with-a-long-prefix-00000000",
            ] {
                for spare in [false, true] {
                    let mut renamed = base.clone();
                    if spare {
                        renamed.reserve(7);
                        for p in &mut renamed {
                            p.name.reserve(17);
                        }
                    }
                    let success = super::rename_config(&mut renamed, old, new);
                    let record = format!(
                        "{count}/{i}/{new:?}/{spare}/{success}/{}/{renamed:?}/{:?}",
                        renamed.capacity(),
                        capacities(&renamed)
                    );
                    out.write_all(&(record.len() as u64).to_le_bytes()).unwrap();
                    out.write_all(record.as_bytes()).unwrap();
                }
            }
            for serial in ["sensor-a", "sensor-b", "", "\u{96ea}"] {
                let mut defaults = base.clone();
                let success = super::set_default_config(&mut defaults, serial, old);
                let record = format!(
                    "{count}/{i}/{serial:?}/{success}/{}/{defaults:?}/{:?}",
                    defaults.capacity(),
                    capacities(&defaults)
                );
                out.write_all(&(record.len() as u64).to_le_bytes()).unwrap();
                out.write_all(record.as_bytes()).unwrap();
            }
        }
    }
    for n in [0, 1, 3, 7, 8, 15, 16, 17, 31, 32, 64, 128, 4096] {
        for m in [0, 1, 7, 8, 15, 16, 64, 128] {
            for ch in ["a", "\u{96ea}", "\u{2003}"] {
                let owner = ch.repeat(n);
                let name = ch.repeat(m);
                let result = super::migrated_config_name(&owner, &name);
                let record = format!("{owner:?}/{name:?}/{result:?}/{}", result.capacity());
                out.write_all(&(record.len() as u64).to_le_bytes()).unwrap();
                out.write_all(record.as_bytes()).unwrap();
            }
        }
    }
    out.flush().unwrap();
}
