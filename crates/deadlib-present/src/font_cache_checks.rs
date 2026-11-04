//! Exact glyph ownership, fallback, cache-state and numeric checks.
use super::*;
use std::io::Write;

#[path = "font_cache_original.rs"]
pub(super) mod original;

pub(super) const NAMES: [&str; 64] = [
    "f0", "f1", "f2", "f3", "f4", "f5", "f6", "f7", "f8", "f9", "f10", "f11", "f12", "f13", "f14",
    "f15", "f16", "f17", "f18", "f19", "f20", "f21", "f22", "f23", "f24", "f25", "f26", "f27",
    "f28", "f29", "f30", "f31", "f32", "f33", "f34", "f35", "f36", "f37", "f38", "f39", "f40",
    "f41", "f42", "f43", "f44", "f45", "f46", "f47", "f48", "f49", "f50", "f51", "f52", "f53",
    "f54", "f55", "f56", "f57", "f58", "f59", "f60", "f61", "f62", "f63",
];

pub(super) fn glyph(id: u32) -> Glyph {
    let value = |offset: u32| f32::from_bits(id.wrapping_mul(0x9e37_79b9).wrapping_add(offset));
    Glyph {
        texture_key: Arc::from(format!("atlas/日本/{id}")),
        stroke_texture_key: (id & 1 != 0).then(|| Arc::from(format!("stroke/{id}"))),
        tex_rect: std::array::from_fn(|i| value(i as u32)),
        uv_scale: [value(10), value(11)],
        uv_offset: [value(20), value(21)],
        size: [value(30), value(31)],
        offset: [value(40), value(41)],
        advance: value(50),
        advance_i32: id.wrapping_mul(31) as i32,
    }
}

pub(super) fn fonts(depth: usize, per_font: usize, unicode: usize, default: bool) -> FontMap {
    let mut fonts = FontMap::default();
    for (index, &name) in NAMES[..depth].iter().enumerate() {
        let mut map = GlyphMap::default();
        for code in 0..per_font {
            map.insert(
                ((code + index * per_font) % 128) as u8 as char,
                glyph((code + index * 256) as u32),
            );
        }
        for code in 0..unicode {
            map.insert(
                char::from_u32(0x100 + code as u32).unwrap(),
                glyph((code + index * 8192) as u32),
            );
        }
        fonts.insert(
            name,
            Font {
                glyph_map: map,
                ascii_glyphs: Box::new(std::array::from_fn(|_| None)),
                default_glyph: (default || index != 0).then(|| glyph((0x8000 + index) as u32)),
                line_spacing: -17,
                height: 41,
                fallback_font_name: NAMES.get(index + 1).copied().filter(|_| index + 1 < depth),
                cache_tag: 0,
                chain_key: 0,
                default_stroke_color: [f32::from_bits(0x7fc0_4321), -0.0, 1.0, f32::INFINITY],
                stroke_texture_map: HashMap::from_iter([("a".into(), "b".into())]),
                texture_hints_map: HashMap::from_iter([("a".into(), "grayscale".into())]),
            },
        );
    }
    fonts
}

fn same_glyph(actual: Option<&Glyph>, expected: Option<&Glyph>, trace: &mut impl Write) {
    let Some(a) = actual else {
        assert!(expected.is_none());
        trace.write_all(&[0]).unwrap();
        return;
    };
    let b = expected.expect("matching glyph presence");
    trace.write_all(&[1]).unwrap();
    assert!(Arc::ptr_eq(&a.texture_key, &b.texture_key));
    assert_eq!(
        a.stroke_texture_key.is_some(),
        b.stroke_texture_key.is_some()
    );
    if let Some(stroke) = &a.stroke_texture_key {
        assert!(Arc::ptr_eq(stroke, b.stroke_texture_key.as_ref().unwrap()));
    }
    let fields = |g: &Glyph| {
        g.tex_rect
            .into_iter()
            .chain(g.uv_scale)
            .chain(g.uv_offset)
            .chain(g.size)
            .chain(g.offset)
            .chain([g.advance])
            .map(f32::to_bits)
            .collect::<Vec<_>>()
    };
    assert_eq!(fields(a), fields(b));
    assert_eq!(a.advance_i32, b.advance_i32);
    for value in fields(a) {
        trace.write_all(&value.to_le_bytes()).unwrap();
    }
    trace.write_all(&a.advance_i32.to_le_bytes()).unwrap();
    for text in [Some(&a.texture_key), a.stroke_texture_key.as_ref()] {
        trace
            .write_all(&(text.map_or(0, |s| s.len()) as u32).to_le_bytes())
            .unwrap();
        if let Some(text) = text {
            trace.write_all(text.as_bytes()).unwrap();
        }
    }
}

pub(super) fn same_table(
    a: &[Option<Glyph>; 128],
    b: &[Option<Glyph>; 128],
    trace: &mut impl Write,
) {
    for (a, b) in a.iter().zip(b) {
        same_glyph(a.as_ref(), b.as_ref(), trace);
    }
}

fn cache_corpus(trace: &mut impl Write) {
    let mut cases = 0;
    for depth in [0, 1, 2, 4, 8] {
        for per in [0, 1, 8, 16, 64, 127, 128] {
            for unicode in [0, 1, 129] {
                for default in [false, true] {
                    let mut map = fonts(depth, per, unicode, default);
                    if let Some(last) = map.get_mut(NAMES[depth.saturating_sub(1)]) {
                        last.fallback_font_name = Some("missing");
                    }
                    for name in ["missing", NAMES[0], NAMES[depth.saturating_sub(1)]] {
                        let old = original::compute_ascii_glyphs(name, &map);
                        let new = compute_ascii_glyphs(name, &map);
                        same_table(&new, &old, trace);
                        cases += 1;
                    }
                }
            }
        }
    }
    for (depth, per) in [(8, 16), (2, 64), (1, 128)] {
        let mut map = fonts(depth, per, 0, true);
        map.get_mut(NAMES[depth - 1]).unwrap().fallback_font_name = Some(NAMES[0]);
        for &name in &NAMES[..depth] {
            same_table(
                &compute_ascii_glyphs(name, &map),
                &original::compute_ascii_glyphs(name, &map),
                trace,
            );
            cases += 1;
        }
    }
    for &(depth, per, unicode, default) in &[
        (1, 128, 5000, true),
        (8, 1, 5000, true),
        (64, 1, 0, true),
        (1, 16, 0, false),
    ] {
        let map = fonts(depth, per, unicode, default);
        same_table(
            &compute_ascii_glyphs("f0", &map),
            &original::compute_ascii_glyphs("f0", &map),
            trace,
        );
        cases += 1;
    }
    println!("cache corpus: {cases} tables");
}

fn refresh_corpus(trace: &mut impl Write) {
    let mut cases = 0;
    for depth in [0, 1, 2, 8, 33, 64] {
        for per in [0, 1, 16, 128] {
            for default in [false, true] {
                let mut old = fonts(depth, per, 0, default);
                for (index, &name) in NAMES[..depth].iter().enumerate() {
                    old.get_mut(name).unwrap().cache_tag = match index % 4 {
                        0 => 0,
                        1 => u64::MAX,
                        2 => u64::MAX - 1,
                        _ => 7,
                    };
                }
                let mut new = old.clone();
                let pointers: Vec<_> = NAMES[..depth]
                    .iter()
                    .map(|name| new[name].ascii_glyphs.as_ptr())
                    .collect();
                for _ in 0..2 {
                    original::refresh_chain_keys(&mut old);
                    refresh_chain_keys(&mut new);
                    for (i, &name) in NAMES[..depth].iter().enumerate() {
                        let a = &new[name];
                        let b = &old[name];
                        assert_eq!(a.ascii_glyphs.as_ptr(), pointers[i]);
                        assert_eq!((a.cache_tag, a.chain_key), (b.cache_tag, b.chain_key));
                        assert_eq!(
                            (a.line_spacing, a.height, a.fallback_font_name),
                            (b.line_spacing, b.height, b.fallback_font_name)
                        );
                        assert_eq!(
                            a.default_stroke_color.map(f32::to_bits),
                            b.default_stroke_color.map(f32::to_bits)
                        );
                        assert_eq!(a.stroke_texture_map, b.stroke_texture_map);
                        assert_eq!(a.texture_hints_map, b.texture_hints_map);
                        for value in [a.cache_tag, a.chain_key] {
                            trace.write_all(&value.to_le_bytes()).unwrap();
                        }
                        same_table(&a.ascii_glyphs, &b.ascii_glyphs, trace);
                    }
                    cases += 1;
                }
            }
        }
    }
    println!("refresh corpus: {cases} complete map refreshes");
}

pub(super) fn round_values() -> Vec<f32> {
    let mut values = Vec::new();
    for integer in -65536..=65536 {
        let tie = integer as f32 + 0.5;
        values.extend([tie.next_down(), tie, tie.next_up()]);
    }
    for exponent in 0..=255 {
        for sign in [0, 1 << 31] {
            for mantissa in [0, 1, 0x3f_ffff, 0x40_0000, 0x7f_ffff] {
                values.push(f32::from_bits(sign | (exponent << 23) | mantissa));
            }
        }
    }
    let mut state = 0x89ab_cdefu32;
    for _ in 0..500_000 {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        values.push(f32::from_bits(state));
    }
    values
}

fn round_corpus(trace: &mut impl Write) {
    let values = round_values();
    for &value in &values {
        let old = original::round_half_to_even_i32(value);
        let new = round_half_to_even_i32(value);
        assert_eq!(new, old, "input {:08x}", value.to_bits());
        trace.write_all(&value.to_bits().to_le_bytes()).unwrap();
        trace.write_all(&new.to_le_bytes()).unwrap();
    }
    println!("round corpus: {} inputs", values.len());
}

#[test]
fn ascii_tables_preserve_fallbacks_glyph_bits_and_arc_identity() {
    cache_corpus(&mut std::io::sink());
}
#[test]
fn complete_refresh_preserves_tags_tables_and_existing_storage() {
    refresh_corpus(&mut std::io::sink());
}
#[test]
fn integer_rounding_matches_half_neighbors_and_arbitrary_float_bits() {
    round_corpus(&mut std::io::sink());
}

#[test]
fn unresolved_cycles_preserve_nontermination() {
    for variant in ["original", "current"] {
        let path = std::env::temp_dir().join(format!(
            "deadsync-font-cycle-{}-{variant}",
            std::process::id()
        ));
        let module = module_path!().split_once("::").unwrap().1;
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                &format!("{module}::unresolved_cycle_probe"),
                "--ignored",
                "--test-threads=1",
            ])
            .env("DEADSYNC_FONT_CYCLE_VARIANT", variant)
            .env("DEADSYNC_FONT_CYCLE_READY", &path)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::fs::read(&path).ok().as_deref() != Some(b"ready") {
            assert!(
                child.try_wait().unwrap().is_none(),
                "cycle probe ended before readiness"
            );
            if std::time::Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("cycle probe readiness timed out");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
        let status = child.try_wait().unwrap();
        if status.is_none() {
            child.kill().unwrap();
        }
        child.wait().unwrap();
        assert!(status.is_none(), "unresolved cycle returned");
        assert_eq!(std::fs::read(&path).unwrap(), b"ready");
        std::fs::remove_file(path).unwrap();
    }
}

#[test]
#[ignore = "child process fixture for unresolved fallback cycles"]
fn unresolved_cycle_probe() {
    let mut map = fonts(2, 1, 0, true);
    map.get_mut("f1").unwrap().fallback_font_name = Some("f0");
    let variant = std::env::var("DEADSYNC_FONT_CYCLE_VARIANT").unwrap();
    let ready = std::env::var_os("DEADSYNC_FONT_CYCLE_READY").unwrap();
    std::fs::write(&ready, b"ready").unwrap();
    let output = if variant == "original" {
        original::compute_ascii_glyphs("f0", &map)
    } else {
        compute_ascii_glyphs("f0", &map)
    };
    drop(std::hint::black_box(output));
    std::fs::write(ready, b"returned").unwrap();
}

#[test]
#[ignore = "exhausts 2,650,800,128 f32 patterns in the biased rounding domain"]
fn exhaustive_rounding_domain() {
    for magnitude in 0..0x4f00_0000 {
        for sign in [0, 1 << 31] {
            let value = f32::from_bits(magnitude | sign);
            assert_eq!(
                round_half_to_even_i32(value),
                original::round_half_to_even_i32(value),
                "{:08x}",
                magnitude | sign
            );
        }
    }
}

#[test]
#[ignore = "writes deterministic font/rounding trace"]
fn dump_font_cache_trace() {
    let path = std::env::var_os("DEADSYNC_FONT_CACHE_TRACE_OUT").unwrap();
    let mut trace =
        std::io::BufWriter::with_capacity(1024 * 1024, std::fs::File::create(path).unwrap());
    cache_corpus(&mut trace);
    refresh_corpus(&mut trace);
    round_corpus(&mut trace);
}
