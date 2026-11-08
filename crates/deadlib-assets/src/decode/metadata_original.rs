//! Frozen dimension and multiframe implementations from 79bc83980.
use super::super::{gif_reader, image_limit};
use std::{io::Read, path::Path};

fn image_signature(path: &Path) -> image::ImageResult<[u8; 8]> {
    let mut signature = [0; 8];
    std::fs::File::open(path)?.read_exact(&mut signature)?;
    Ok(signature)
}

pub fn texture_source_size(path: &Path) -> image::ImageResult<(u32, u32)> {
    let signature = image_signature(path)?;
    if &signature[..6] == b"GIF87a" || &signature[..6] == b"GIF89a" {
        let reader = gif_reader(path)?;
        if reader.width() == 0 || reader.height() == 0 {
            return Err(image_limit());
        }
        return Ok((u32::from(reader.width()), u32::from(reader.height())));
    }
    image::ImageReader::open(path)?
        .with_guessed_format()?
        .into_dimensions()
}

pub fn texture_filename_has_multiframe_hint(filename: &str) -> bool {
    let bytes = filename.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] != b' ' {
            i += 1;
            continue;
        }
        let mut left = i + 1;
        while left < bytes.len() && bytes[left].is_ascii_digit() {
            left += 1;
        }
        if left == i + 1 || left >= bytes.len() || !matches!(bytes[left], b'x' | b'X') {
            i += 1;
            continue;
        }
        let mut right = left + 1;
        while right < bytes.len() && bytes[right].is_ascii_digit() {
            right += 1;
        }
        if right > left + 1 {
            return Path::new(filename)
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.as_bytes().iter().all(u8::is_ascii_alphabetic));
        }
        i = right;
    }
    false
}
