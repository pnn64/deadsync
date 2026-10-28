use deadlib_image_gif::*;
use std::{borrow::Cow, hint::black_box, time::Instant};

#[path = "../../../../tests/support/perf.rs"]
#[allow(dead_code)]
mod alloc;
#[path = "../../../../tests/support/kernel_bench.rs"]
#[allow(dead_code)]
mod cpu;

fn fixture(kind: usize) -> Gif {
    let mut g = Gif::new(Header::new(512, 512));
    g.header.global_palette = Some(
        (0..256)
            .map(|n| [n as u8, (n * 73) as u8, (n * 137) as u8])
            .collect(),
    );
    let mut random = 1327u32;
    for n in 0..if kind == 5 { 32 } else { 1 } {
        let (w, h) = if kind == 5 { (128, 128) } else { (512, 512) };
        let data = (0..w as usize * h as usize)
            .map(|i| match kind {
                0 => 0,
                1 => ((i % 512 / 16 + i / 512 / 16) % 16) as u8,
                2 => ((i % 512 + i / 512) / 4) as u8,
                3 | 4 => {
                    random ^= random << 13;
                    random ^= random >> 17;
                    random ^= random << 5;
                    random as u8
                }
                _ => ((i + n * 37) % 256) as u8,
            })
            .collect();
        let mut f = Frame::new(w, h, data);
        f.interlaced = kind == 4;
        if kind == 5 {
            f.left = (n * 11 % 384) as u16;
            f.top = (n * 23 % 384) as u16;
        }
        f.control = Some(GraphicControl {
            disposal: [Disposal::Keep, Disposal::Background, Disposal::Previous][n % 3],
            delay: 3,
            transparent_index: if kind == 5 { Some(0) } else { None },
            user_input: false,
        });
        g.blocks.push(Block::Frame(f));
    }
    g
}
fn reference_encode(g: &Gif) -> Vec<u8> {
    let mut bytes = Vec::new();
    let p: Vec<u8> = g
        .header
        .global_palette
        .as_ref()
        .unwrap()
        .iter()
        .flatten()
        .copied()
        .collect();
    {
        let mut e = gif::Encoder::new(&mut bytes, g.header.width, g.header.height, &p).unwrap();
        for f in g.frames() {
            // Rust gif expects interlaced pixels in transmission order.
            let data = if f.interlaced {
                let mut data = Vec::with_capacity(f.indices.len());
                for (start, step) in [(0, 8), (4, 8), (2, 4), (1, 2)] {
                    for y in (start..f.height as usize).step_by(step) {
                        data.extend_from_slice(
                            &f.indices[y * f.width as usize..(y + 1) * f.width as usize],
                        );
                    }
                }
                Cow::Owned(data)
            } else {
                Cow::Borrowed(f.indices.as_slice())
            };
            let c = f.control.unwrap_or_default();
            e.write_frame(&gif::Frame {
                left: f.left,
                top: f.top,
                width: f.width,
                height: f.height,
                interlaced: f.interlaced,
                delay: c.delay,
                transparent: c.transparent_index,
                dispose: gif::DisposalMethod::from_u8(c.disposal as u8).unwrap(),
                buffer: data,
                ..Default::default()
            })
            .unwrap();
        }
    }
    bytes
}
fn reference_decode(bytes: &[u8]) -> Vec<gif::Frame<'static>> {
    let mut d = gif::DecodeOptions::new().read_info(bytes).unwrap();
    let mut frames = Vec::new();
    while let Some(f) = d.read_next_frame().unwrap() {
        frames.push(f.clone());
    }
    frames
}
fn native_stream(bytes: &[u8], render: bool) -> usize {
    let mut d = Decoder::default();
    let mut c = None;
    let mut at = 0;
    let mut sum = 0;
    while at < bytes.len() {
        let p = d.update(&bytes[at..]).unwrap();
        at += p.consumed;
        match p.event {
            Some(Event::Header) if render => {
                c = Some(
                    Compositor::with_options(
                        d.header().unwrap(),
                        CompositeOptions {
                            background: Background::Transparent,
                            ..Default::default()
                        },
                    )
                    .unwrap(),
                )
            }
            Some(Event::Block(Block::Frame(f))) => {
                if render {
                    sum += black_box(c.as_mut().unwrap().render(&f).unwrap().rgba)[0] as usize;
                }
                sum += black_box(&f.indices).len();
                d.recycle_frame(f);
            }
            _ => {}
        }
    }
    d.finish().unwrap();
    sum
}
fn reference_stream(bytes: &[u8], render: bool) -> usize {
    let mut d = gif::DecodeOptions::new().read_info(bytes).unwrap();
    let mut c = render.then(|| gif_dispose::Screen::new_decoder(&d));
    let mut sum = 0;
    while let Some(f) = d.read_next_frame().unwrap() {
        if let Some(c) = &mut c {
            c.blit_frame(f).unwrap();
            sum += black_box(c.pixels_rgba()).pixels().next().unwrap().r as usize;
        }
        sum += black_box(&f.buffer).len();
    }
    sum
}
fn first_lzw(bytes: &[u8]) -> (u8, Vec<u8>) {
    let mut at = 13
        + if bytes[10] & 128 != 0 {
            3usize << ((bytes[10] & 7) + 1)
        } else {
            0
        };
    loop {
        match bytes[at] {
            0x21 => {
                at += 2;
                loop {
                    let n = bytes[at] as usize;
                    at += 1;
                    if n == 0 {
                        break;
                    }
                    at += n;
                }
            }
            0x2c => {
                let flags = bytes[at + 9];
                at += 10;
                if flags & 128 != 0 {
                    at += 3usize << ((flags & 7) + 1);
                }
                break;
            }
            _ => panic!("unexpected benchmark block"),
        }
    }
    let min = bytes[at];
    at += 1;
    let mut data = Vec::new();
    loop {
        let n = bytes[at] as usize;
        at += 1;
        if n == 0 {
            break;
        }
        data.extend_from_slice(&bytes[at..at + n]);
        at += n;
    }
    (min, data)
}
fn measure<T>(
    mut f: impl FnMut() -> T,
    iters: usize,
    samples: usize,
) -> (u128, u128, u128, u64, alloc::Churn) {
    for _ in 0..3 {
        black_box(f());
    }
    let mut times = Vec::new();
    let mut cycles = Vec::new();
    for _ in 0..samples {
        let start_cycles = cpu::cycles();
        let start = Instant::now();
        for _ in 0..iters {
            black_box(f());
        }
        times.push(start.elapsed().as_nanos() / iters as u128);
        cycles.push((cpu::cycles() - start_cycles) / iters as u64);
    }
    times.sort();
    cycles.sort();
    let (_, churn) = alloc::measure(|| black_box(f()));
    (
        times[times.len() / 2],
        times[0],
        times[times.len() - 1],
        cycles[cycles.len() / 2],
        churn,
    )
}
struct Report<'a> {
    name: &'a str,
    iters: usize,
    samples: usize,
    encoded: usize,
    pixels: usize,
}
impl Report<'_> {
    fn time<T>(&self, codec: &str, operation: &str, bytes: usize, f: impl FnMut() -> T) {
        if std::env::var("DEADLIB_GIF_BENCH_NATIVE_ONLY").is_ok() && codec != "native" {
            return;
        }
        if std::env::var("DEADLIB_GIF_BENCH_CASE").is_ok_and(|s| s != self.name) {
            return;
        }
        if std::env::var("DEADLIB_GIF_BENCH_OPERATION").is_ok_and(|s| s != operation) {
            return;
        }
        let (ns, min, max, cycles, churn) = measure(f, self.iters, self.samples);
        println!(
            "{}\t{codec}\t{operation}\t{}\t{}\t{}\t{}\t{ns}\t{min}\t{max}\t{:.2}\t{cycles}\t{:.6}\t{}\t{}\t{}\t{}",
            self.name,
            self.iters,
            self.samples,
            self.encoded,
            self.pixels,
            bytes as f64 * 1e9 / ns as f64 / 1048576.0,
            bytes as f64 / cycles.max(1) as f64,
            churn.allocs,
            churn.reallocs,
            churn.allocated_bytes,
            churn.peak_bytes,
        );
    }
}
fn main() {
    cpu::pin_thread();
    let iters = std::env::var("DEADLIB_GIF_BENCH_ITERS")
        .ok()
        .map(|s| s.parse::<usize>().unwrap())
        .unwrap_or(20)
        .max(1);
    let samples = std::env::var("DEADLIB_GIF_BENCH_SAMPLES")
        .ok()
        .map(|s| s.parse::<usize>().unwrap())
        .unwrap_or(5)
        .max(1);
    println!(
        "case\tcodec\toperation\titerations\tsamples\tencoded_bytes\tindexed_bytes\tmedian_ns\tmin_ns\tmax_ns\tMiB_per_second\tmedian_cycles\tbytes_per_cycle\tallocations\treallocations\tallocated_bytes\tpeak_bytes"
    );
    for (kind, name) in [
        "solid",
        "tiles16",
        "gradient256",
        "noise256",
        "interlaced-noise",
        "animation32",
        "tiles16-palette",
        "solid2-palette",
    ]
    .into_iter()
    .enumerate()
    {
        let mut g = fixture(match kind {
            6 => 1,
            7 => 0,
            n => n,
        });
        if kind >= 6 {
            g.header
                .global_palette
                .as_mut()
                .unwrap()
                .truncate(if kind == 6 { 16 } else { 2 });
        }
        let bytes = reference_encode(&g);
        let native_bytes = encode(&g, &EncodeOptions::default()).unwrap();
        assert_eq!(
            decode(&bytes).unwrap().frames().collect::<Vec<_>>(),
            g.frames().collect::<Vec<_>>()
        );
        for data in [&bytes, &native_bytes] {
            let actual = reference_decode(data);
            for (a, b) in actual.iter().zip(g.frames()) {
                assert_eq!(a.buffer.as_ref(), b.indices);
            }
            assert_eq!(actual.len(), g.frames().count());
        }
        let frames = reference_decode(&bytes);
        let mut reference = gif::DecodeOptions::new()
            .read_info(bytes.as_slice())
            .unwrap();
        let mut ref_screen = gif_dispose::Screen::new_decoder(&reference);
        let mut compositor = Compositor::with_options(
            &g.header,
            CompositeOptions {
                background: Background::Transparent,
                ..Default::default()
            },
        )
        .unwrap();
        for f in g.frames() {
            let rf = reference.read_next_frame().unwrap().unwrap();
            ref_screen.blit_frame(rf).unwrap();
            let rgba = compositor.render(f).unwrap();
            assert!(
                rgba.rgba
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .zip(ref_screen.pixels_rgba().pixels())
                    .all(|(a, b)| *a == [b.r, b.g, b.b, b.a])
            );
        }
        let pixels: usize = g.frames().map(|f| f.indices.len()).sum();
        let report = Report {
            name,
            iters,
            samples,
            encoded: bytes.len(),
            pixels,
        };
        report.time("native", "decode-retain", pixels, || {
            decode(black_box(&bytes)).unwrap()
        });
        report.time("gif", "decode-retain", pixels, || {
            reference_decode(black_box(&bytes))
        });
        report.time("native", "decode-reuse", pixels, || {
            native_stream(black_box(&bytes), false)
        });
        report.time("gif", "decode-reuse", pixels, || {
            reference_stream(black_box(&bytes), false)
        });
        report.time("native", "decode-render", pixels, || {
            native_stream(black_box(&bytes), true)
        });
        report.time("gif+gif-dispose", "decode-render", pixels, || {
            reference_stream(black_box(&bytes), true)
        });
        let render_bytes =
            g.header.width as usize * g.header.height as usize * 4 * g.frames().count();
        report.time("native", "composite", render_bytes, || {
            let mut c = Compositor::with_options(
                &g.header,
                CompositeOptions {
                    background: Background::Transparent,
                    ..Default::default()
                },
            )
            .unwrap();
            for f in g.frames() {
                black_box(c.render(f).unwrap());
            }
            c
        });
        report.time("gif-dispose", "composite", render_bytes, || {
            let mut c = gif_dispose::Screen::new_decoder(&reference);
            for f in &frames {
                c.blit_frame(black_box(f)).unwrap();
                black_box(c.pixels_rgba());
            }
            c
        });
        let mut native_report = Report {
            encoded: native_bytes.len(),
            ..report
        };
        native_report.time("native", "encode", pixels, || {
            encode(black_box(&g), &EncodeOptions::default()).unwrap()
        });
        native_report.encoded = bytes.len();
        native_report.time("gif", "encode", pixels, || reference_encode(black_box(&g)));
        let (minimum, compressed) = first_lzw(&bytes);
        let n = g.frames().next().unwrap().indices.len();
        report.time("native", "lzw-decode", n, || {
            lzw::decode(black_box(&compressed), minimum, n).unwrap()
        });
        report.time("weezl", "lzw-decode", n, || {
            weezl::decode::Decoder::new(weezl::BitOrder::Lsb, minimum)
                .decode(black_box(&compressed))
                .unwrap()
        });
    }
    // Repeated plain-text graphics reuse the compositor's indices and cell mask.
    // Include small text where a temporary palette allocation is significant.
    struct Font;
    impl TextRasterizer for Font {
        fn rasterize(&mut self, character: u8, _: u8, _: u8, mask: &mut [u8]) -> Result<(), Error> {
            mask.fill(character & 1);
            Ok(())
        }
    }
    for (name, width, height) in [("text-small", 8, 8), ("text-grid", 256, 128)] {
        let mut header = Header::new(width, height);
        header.global_palette = Some(vec![[0, 0, 0], [255, 255, 255]]);
        let text = PlainText {
            left: 0,
            top: 0,
            width,
            height,
            cell_width: 8,
            cell_height: 8,
            foreground_index: 1,
            background_index: 0,
            control: None,
            data: vec![vec![b'A'; width as usize * height as usize / 64]],
        };
        let mut compositor = Compositor::new(&header).unwrap();
        assert!(
            compositor
                .render_text(&text, &mut Font)
                .unwrap()
                .rgba
                .iter()
                .all(|&v| v == 255)
        );
        let pixels = width as usize * height as usize;
        Report {
            name,
            iters: iters * 100,
            samples,
            encoded: 0,
            pixels,
        }
        .time("native", "render-text-reuse", pixels, || {
            black_box(compositor.render_text(black_box(&text), &mut Font).unwrap());
        });
    }
}
