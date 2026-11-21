use deadlib_image_bmp::{Pixels, decode, probe};
use std::path::Path;

#[test]
fn generated_alpha_position_controls_match_every_expected_pixel() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../alpha-controls");
    let manifest = std::fs::read_to_string(directory.join("manifest.tsv")).unwrap();
    let mut checked = 0;
    for line in manifest.lines().skip(1) {
        let columns: Vec<_> = line.split('\t').collect();
        assert_eq!(columns.len(), 5);
        let width = columns[1].parse::<u32>().unwrap();
        let height = columns[2].parse::<u32>().unwrap();
        let top_down = columns[3].parse::<bool>().unwrap();
        let scenario = columns[4];
        let encoded = std::fs::read(directory.join(columns[0])).unwrap();
        let info = probe(&encoded).unwrap();
        assert_eq!(
            (info.width, info.height, info.bit_depth, info.top_down),
            (width, height, 32, top_down)
        );
        let bitmap = decode(&encoded).unwrap();
        assert_eq!((bitmap.width, bitmap.height), (width, height));
        let Pixels::Rgba(samples) = bitmap.pixels else {
            panic!("implicit BGRA must retain RGBA storage");
        };
        let count = width as usize * height as usize;
        assert_eq!(samples.len(), count * 4);
        for (position, &pixel) in samples.as_chunks::<4>().0.iter().enumerate() {
            let alpha = match scenario {
                "zero" => 255,
                "first" if position == 0 => 1,
                "last" if position + 1 == count => 1,
                "first" | "last" => 0,
                _ => panic!("unknown alpha scenario"),
            };
            let index = position as u8;
            assert_eq!(
                pixel,
                [
                    index,
                    index.wrapping_mul(71).wrapping_add(19),
                    index.wrapping_mul(139).wrapping_add(41),
                    alpha,
                ],
                "{} pixel {position}",
                columns[0]
            );
        }
        checked += 1;
    }
    assert_eq!(checked, 18);
}
