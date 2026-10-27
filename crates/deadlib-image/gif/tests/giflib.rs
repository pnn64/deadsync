use deadlib_image_gif::*;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn number(input: &mut &[u8]) -> u32 {
    let (a, b) = input.split_at(4);
    *input = b;
    u32::from_le_bytes(a.try_into().unwrap())
}
fn palette(input: &mut &[u8], expected: Option<&[[u8; 3]]>, sorted: Option<bool>) {
    let len = number(input) as usize;
    let sort = number(input) != 0;
    assert_eq!(len, expected.map_or(0, |p| p.len()));
    if let Some(p) = expected {
        if let Some(sorted) = sorted {
            assert_eq!(sort, sorted);
        }
        let (bytes, rest) = input.split_at(len * 3);
        *input = rest;
        assert_eq!(bytes, p.iter().flatten().copied().collect::<Vec<_>>());
    }
}
fn compare(input: &[u8], g: &Gif) {
    let mut s = input;
    for value in [
        g.header.width as u32,
        g.header.height as u32,
        g.header.color_resolution as u32,
        g.header.background_index as u32,
        g.header.pixel_aspect_ratio as u32,
        g.frames().count() as u32,
    ] {
        assert_eq!(number(&mut s), value);
    }
    palette(
        &mut s,
        g.header.global_palette.as_deref(),
        Some(g.header.sorted),
    );
    for f in g.frames() {
        for value in [
            f.left as u32,
            f.top as u32,
            f.width as u32,
            f.height as u32,
            u32::from(f.interlaced),
        ] {
            assert_eq!(number(&mut s), value);
        }
        // giflib 5.2.2 does not expose the image descriptor's sort bit.
        palette(&mut s, f.local_palette.as_deref(), None);
        let c = f.control.unwrap_or_default();
        for value in [
            c.disposal as u32,
            u32::from(c.user_input),
            c.delay as u32,
            c.transparent_index.map_or(u32::MAX, u32::from),
        ] {
            assert_eq!(number(&mut s), value);
        }
        let (data, rest) = s.split_at(f.indices.len());
        assert_eq!(data, f.indices);
        s = rest;
    }
    assert!(s.is_empty());
}
fn run(oracle: &Path, path: &Path, g: &Gif, rewrite: Option<&Path>) {
    let mut cmd = Command::new(oracle);
    cmd.arg(path);
    if let Some(p) = rewrite {
        cmd.arg(p);
    }
    let result = cmd.output().unwrap();
    assert!(
        result.status.success(),
        "{}: {}",
        path.display(),
        String::from_utf8_lossy(&result.stderr)
    );
    compare(&result.stdout, g);
    if let Some(p) = rewrite {
        let actual = decode(&fs::read(p).unwrap()).unwrap();
        assert_eq!(actual.header.width, g.header.width);
        assert_eq!(actual.header.height, g.header.height);
        let expected: Vec<_> = g
            .frames()
            .cloned()
            .map(|mut f| {
                f.sorted = false;
                f
            })
            .collect();
        assert_eq!(actual.frames().cloned().collect::<Vec<_>>(), expected);
    }
}
#[test]
#[ignore = "requires DEADLIB_GIFLIB_ORACLE; see tests/oracle/README.md"]
fn giflib_native_indices_palettes_controls_and_encoder() {
    let oracle = PathBuf::from(
        std::env::var_os("DEADLIB_GIFLIB_ORACLE").expect("DEADLIB_GIFLIB_ORACLE required"),
    );
    let dir = std::env::temp_dir().join(format!("deadlib-gif-oracle-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let input = dir.join("input.gif");
    let rewritten = dir.join("rewritten.gif");
    let mut count = 0;
    let samples = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/image-gif/samples");
    for item in fs::read_dir(samples).unwrap() {
        let path = item.unwrap().path();
        if path.extension().is_none_or(|x| x != "gif") {
            continue;
        }
        let bytes = fs::read(&path).unwrap();
        // giflib 5.2.2 rejects this valid empty-comment sample. Rust gif and
        // the independent empty-comment fixture cover it in ordinary tests.
        if path.file_name().unwrap() == "bufo-thinks-about-fishsticks.gif" {
            continue;
        }
        if let Ok(g) = decode(&bytes) {
            run(&oracle, &path, &g, None);
            count += 1;
        }
    }
    assert_eq!(count, 9);
    for colors in [2, 4, 8, 16, 32, 64, 128, 256] {
        for (w, h) in [(1, 1), (3, 2), (17, 13), (257, 73)] {
            for interlaced in [false, true] {
                let p: Palette = (0..colors)
                    .map(|n| [n as u8, (n * 73) as u8, (n * 197) as u8])
                    .collect();
                let mut header = Header::new(w, h);
                header.global_palette = Some(p.clone());
                header.background_index = (colors - 1) as u8;
                header.pixel_aspect_ratio = 49;
                let mut g = Gif::new(header);
                g.blocks
                    .push(Block::Extension(Extension::looping(LoopCount::Infinite)));
                for (n, disposal) in [
                    Disposal::Unspecified,
                    Disposal::Keep,
                    Disposal::Background,
                    Disposal::Previous,
                ]
                .into_iter()
                .enumerate()
                {
                    let mut f = Frame::new(
                        w,
                        h,
                        (0..w as usize * h as usize)
                            .map(|i| ((i * 71 + i / 7 + n) % colors) as u8)
                            .collect(),
                    );
                    f.interlaced = interlaced;
                    f.control = Some(GraphicControl {
                        disposal,
                        delay: if n == 0 { 0 } else { 65535 },
                        transparent_index: Some((n % colors) as u8),
                        user_input: n % 2 == 0,
                    });
                    // Alternate global and local palettes, including sorted local tables.
                    if n % 2 != 0 {
                        f.local_palette = Some(p.iter().map(|c| [c[2], c[1], c[0]]).collect());
                        f.sorted = true;
                    }
                    g.blocks.push(Block::Frame(f));
                }
                fs::write(&input, encode(&g, &EncodeOptions::default()).unwrap()).unwrap();
                run(&oracle, &input, &g, Some(&rewritten));
                count += 1;
            }
        }
    }
    fs::remove_file(input).unwrap();
    fs::remove_file(rewritten).unwrap();
    fs::remove_dir(dir).unwrap();
    println!("giflib cases: {count}; 64 independent encoder rewrites, 256 generated frames");
}
