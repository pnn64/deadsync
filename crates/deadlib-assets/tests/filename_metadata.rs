use deadlib_assets::{TextureStore, texture_hint_doubleres, texture_source_frame_dims_from_real};

#[test]
fn doubleres_accepts_every_ascii_case_and_rejects_other_bytes() {
    let target = b"doubleres";
    for casing in 0..512 {
        let word: String = target
            .iter()
            .enumerate()
            .map(|(index, &byte)| {
                char::from(if casing & (1 << index) != 0 {
                    byte.to_ascii_uppercase()
                } else {
                    byte
                })
            })
            .collect();
        for offset in 0..64 {
            assert!(texture_hint_doubleres(&format!(
                "\u{2003}{}{word}\u{3000}",
                "a".repeat(offset)
            )));
        }
    }
    for (index, &expected) in target.iter().enumerate() {
        for byte in 0..=255u8 {
            let word = format!(
                "{}{}{}",
                std::str::from_utf8(&target[..index]).unwrap(),
                char::from(byte),
                std::str::from_utf8(&target[index + 1..]).unwrap()
            );
            assert_eq!(
                texture_hint_doubleres(&word),
                byte.to_ascii_lowercase() == expected,
                "position {index}, byte {byte}"
            );
        }
    }
}

#[test]
fn texture_bindings_keep_resolution_sheet_and_double_resolution_order() {
    let key = "fonts/metadata 4x2 (res 400x200) (doubleres).png";
    let mut store = TextureStore::new();
    store.insert_texture(key.to_owned(), (), 800, 600);
    let bound = store.bind_texture(key).unwrap();
    let dimensions = bound.dimensions.unwrap();
    assert_eq!((dimensions.w, dimensions.h), (200, 100));
    assert_eq!(bound.sheet, (4, 2));
    assert_eq!(texture_source_frame_dims_from_real(key, 800, 600), (50, 50));
}
