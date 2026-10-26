use deadlib_image_gif::*;
use std::{fs, path::PathBuf};
fn main() {
    let root = PathBuf::from(std::env::args_os().nth(1).expect("seed directory"));
    fs::create_dir_all(root.join("decode")).unwrap();
    fs::create_dir_all(root.join("lzw")).unwrap();
    for min in 2..=8u8 {
        for (n, (w, h)) in [(1, 1), (17, 13), (128, 64)].into_iter().enumerate() {
            let colors = 1usize << min;
            let p = (0..colors)
                .map(|i| [i as u8, (i * 73) as u8, (i * 137) as u8])
                .collect();
            let mut g = Gif::new(Header::new(w, h));
            g.header.global_palette = Some(p);
            g.blocks
                .push(Block::Extension(Extension::looping(LoopCount::Infinite)));
            g.blocks.push(Block::Extension(Extension::Comment(vec![
                vec![1, 2, 3],
                vec![4, 5],
            ])));
            let mut seed = 1327u32;
            for (i, d) in [Disposal::Keep, Disposal::Background, Disposal::Previous]
                .into_iter()
                .enumerate()
            {
                let data = (0..w as usize * h as usize)
                    .map(|_| {
                        seed ^= seed << 13;
                        seed ^= seed >> 17;
                        seed ^= seed << 5;
                        (seed as usize % colors) as u8
                    })
                    .collect();
                let mut f = Frame::new(w, h, data);
                f.interlaced = i % 2 != 0;
                f.control = Some(GraphicControl {
                    disposal: d,
                    transparent_index: Some(0),
                    ..Default::default()
                });
                g.blocks.push(Block::Frame(f));
            }
            fs::write(
                root.join("decode").join(format!("{min}-{n}.gif")),
                encode(
                    &g,
                    &EncodeOptions {
                        image_sub_block_size: 7,
                    },
                )
                .unwrap(),
            )
            .unwrap();
            let mut extensions = g.clone();
            extensions.blocks.push(Block::Extension(Extension::Unknown {
                label: 0x42,
                control: Some(GraphicControl {
                    delay: 19,
                    ..Default::default()
                }),
                data: vec![vec![1], vec![2, 3]],
            }));
            extensions
                .blocks
                .push(Block::Extension(Extension::PlainText(PlainText {
                    left: 0,
                    top: 0,
                    width: w,
                    height: h,
                    cell_width: 2,
                    cell_height: 3,
                    foreground_index: 1,
                    background_index: 0,
                    data: vec![b"GIF text".to_vec(), vec![0, 255]],
                    control: Some(GraphicControl {
                        disposal: Disposal::Previous,
                        transparent_index: Some(0),
                        delay: 0,
                        user_input: true,
                    }),
                })));
            fs::write(
                root.join("decode")
                    .join(format!("extensions-{min}-{n}.gif")),
                encode(&extensions, &EncodeOptions::default()).unwrap(),
            )
            .unwrap();
            let data = &g.frames().next().unwrap().indices;
            let mut seed = vec![min - 2, data.len() as u8, (data.len() >> 8) as u8];
            seed.extend(lzw::encode(data, min).unwrap());
            fs::write(root.join("lzw").join(format!("{min}-{n}.lzw")), seed).unwrap();
        }
    }
}
