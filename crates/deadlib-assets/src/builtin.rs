use image::RgbaImage;

pub const WHITE_TEXTURE_KEY: &str = "__white";
pub const BLACK_TEXTURE_KEY: &str = "__black";

pub struct BuiltinTextureImage {
    pub key: &'static str,
    pub image: RgbaImage,
}

pub fn solid_texture_image(key: &'static str, rgba: [u8; 4]) -> BuiltinTextureImage {
    BuiltinTextureImage {
        key,
        image: RgbaImage::from_raw(1, 1, rgba.to_vec()).expect("solid texture image"),
    }
}

pub fn white_texture_image() -> BuiltinTextureImage {
    solid_texture_image(WHITE_TEXTURE_KEY, [255, 255, 255, 255])
}

pub fn black_texture_image() -> BuiltinTextureImage {
    solid_texture_image(BLACK_TEXTURE_KEY, [0, 0, 0, 255])
}

pub fn fallback_texture_image() -> RgbaImage {
    let data: [u8; 16] = [
        255, 0, 255, 255, 128, 128, 128, 255, 128, 128, 128, 255, 255, 0, 255, 255,
    ];
    RgbaImage::from_raw(2, 2, data.to_vec()).expect("fallback texture image")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_solid_textures_have_expected_keys_and_size() {
        let white = white_texture_image();
        assert_eq!(white.key, WHITE_TEXTURE_KEY);
        assert_eq!((white.image.width(), white.image.height()), (1, 1));

        let black = black_texture_image();
        assert_eq!(black.key, BLACK_TEXTURE_KEY);
        assert_eq!((black.image.width(), black.image.height()), (1, 1));
    }

    #[test]
    fn fallback_texture_is_two_by_two() {
        let image = fallback_texture_image();
        assert_eq!((image.width(), image.height()), (2, 2));
    }
}
