use deadlib_assets::{TextureHints, decode_texture_image};

#[test]
fn packed_bmp_rows_cross_scratch_boundary_without_changing_pixels() {
    let dir = std::env::temp_dir().join(format!("deadsync-bmp-scratch-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("boundary.bmp");
    for bits in [1usize, 4, 8] {
        for width in [1023usize, 1024, 1025, 2049, 8193] {
            for top_down in [false, true] {
                let count = 1usize << bits;
                let offset = 54 + count * 4;
                let pitch = (width * bits).div_ceil(32) * 4;
                let mut bytes = vec![0; offset + pitch * 3];
                bytes[..2].copy_from_slice(b"BM");
                let len = bytes.len() as u32;
                bytes[2..6].copy_from_slice(&len.to_le_bytes());
                bytes[10..14].copy_from_slice(&(offset as u32).to_le_bytes());
                bytes[14..18].copy_from_slice(&40u32.to_le_bytes());
                bytes[18..22].copy_from_slice(&(width as u32).to_le_bytes());
                bytes[22..26].copy_from_slice(&(if top_down { -3i32 } else { 3 }).to_le_bytes());
                bytes[26..28].copy_from_slice(&1u16.to_le_bytes());
                bytes[28..30].copy_from_slice(&(bits as u16).to_le_bytes());
                for i in 0..count {
                    bytes[54 + i * 4..57 + i * 4].copy_from_slice(&[
                        i as u8 ^ 170,
                        i as u8 ^ 85,
                        i as u8,
                    ]);
                }
                for row in 0..3 {
                    let y = if top_down { row } else { 2 - row };
                    for x in 0..width {
                        let index = ((x + y) % count) as u8;
                        bytes[offset + row * pitch + x * bits / 8] |=
                            index << (8 - bits - x * bits % 8);
                    }
                }
                std::fs::write(&path, bytes).unwrap();
                let image = decode_texture_image(&path, &TextureHints::default()).unwrap();
                assert_eq!(image.dimensions(), (width as u32, 3));
                for (x, y, pixel) in image.enumerate_pixels() {
                    let index = ((x as usize + y as usize) % count) as u8;
                    assert_eq!(pixel.0, [index, index ^ 85, index ^ 170, 255]);
                }
            }
        }
    }
    std::fs::remove_file(path).unwrap();
    std::fs::remove_dir(dir).unwrap();
}
