// Frozen from 12a1090ed (0.5.1228); function bodies unchanged.
use super::*;

pub(super) fn thumbnail(path: &Path) -> Result<RgbaImage, Error> {
    let mut reader = ImageReader::open(path)?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let mut image = reader
        .decode()
        .map_err(|e| Error::Invalid(e.to_string()))?
        .into_rgba8();
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_lowercase();
    // Logical resolution hints are not sprite-sheet dimensions.
    let name = if let Some(start) = name.find("(res ") {
        let end = name[start..]
            .find(')')
            .map_or(name.len(), |n| start + n + 1);
        format!("{}{}", &name[..start], &name[end..])
    } else {
        name
    };
    for (pos, _) in name.match_indices('x') {
        let left: String = name[..pos]
            .chars()
            .rev()
            .take_while(char::is_ascii_digit)
            .collect();
        let left: String = left.chars().rev().collect();
        let right: String = name[pos + 1..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        if let (Ok(cols), Ok(rows)) = (left.parse::<u32>(), right.parse::<u32>()) {
            if cols == 0 || rows == 0 || cols > image.width() || rows > image.height() {
                return Err(Error::Invalid(format!("invalid sprite sheet: {name}")));
            }
            image = imageops::crop_imm(&image, 0, 0, image.width() / cols, image.height() / rows)
                .to_image();
            break;
        }
    }
    Ok(imageops::thumbnail(&image, CELL - 4, CELL - 4))
}
