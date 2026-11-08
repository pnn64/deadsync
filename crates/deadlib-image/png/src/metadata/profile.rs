use crate::{ColorType, Error, codec::be32, reserve};

/// Check the ICC container and the PNG-specific data color space. Tag payloads
/// stay opaque; this does not validate or apply a profile's color transforms.
pub(super) fn icc(data: &[u8], color: Option<ColorType>) -> Result<(), Error> {
    if data.len() < 132 || be32(data) as usize != data.len() || &data[36..40] != b"acsp" {
        return Err(Error::Invalid("ICC size or signature"));
    }
    let space = &data[16..20];
    if !matches!(space, b"GRAY" | b"RGB ") {
        return Err(Error::Invalid("ICC data color space"));
    }
    if let Some(color) = color {
        let gray = matches!(color, ColorType::Grayscale | ColorType::GrayscaleAlpha);
        if gray != (space == b"GRAY") {
            return Err(Error::Invalid("ICC color space does not match PNG"));
        }
    }
    if !matches!(&data[20..24], b"XYZ " | b"Lab ")
        || matches!(&data[12..16], b"link" | b"abst")
        || be32(&data[64..68]) > 3
        || (data[8] >= 4 && !data.len().is_multiple_of(4))
    {
        return Err(Error::Invalid(
            "ICC connection space, class, intent or alignment",
        ));
    }
    let count = be32(&data[128..132]) as usize;
    let table_end = count
        .checked_mul(12)
        .and_then(|n| n.checked_add(132))
        .filter(|&n| n <= data.len())
        .ok_or(Error::Invalid("ICC tag table length"))?;
    // ICC.1:2022 added tag uniqueness, contiguity and partial-overlap rules
    // for v4.4. Earlier profiles may reuse storage inside a larger tag.
    let strict_table = data[8] == 4 && data[9] >> 4 >= 4;
    let mut tags = Vec::new();
    if strict_table {
        reserve(&mut tags, count)?;
    }
    for tag in data[132..table_end].as_chunks::<12>().0 {
        let start = be32(&tag[4..8]) as usize;
        let length = be32(&tag[8..12]) as usize;
        if start < table_end
            || !start.is_multiple_of(4)
            || start > data.len()
            || length > data.len() - start
        {
            return Err(Error::Invalid("ICC tag bounds or alignment"));
        }
        if strict_table {
            tags.push((be32(tag), start as u32, length as u32));
        }
    }
    tags.sort_unstable_by_key(|tag| tag.0);
    if tags.windows(2).any(|v| v[0].0 == v[1].0) {
        return Err(Error::Invalid("duplicate ICC tag signature"));
    }
    tags.sort_unstable_by_key(|tag| (tag.1, tag.2));
    if strict_table {
        let mut cursor = table_end;
        let mut previous = None;
        for (_, start, length) in tags {
            if previous == Some((start, length)) {
                continue;
            } // Shared element.
            if previous.is_some_and(|(offset, _)| offset == start) {
                return Err(Error::Invalid("ICC shared tag sizes differ"));
            }
            if start as usize != cursor {
                return Err(Error::Invalid("ICC tag overlap or gap"));
            }
            cursor = (start as usize + length as usize)
                .checked_add(3)
                .map(|n| n & !3)
                .ok_or(Error::Invalid("ICC tag padding"))?;
            previous = Some((start, length));
        }
        if cursor != data.len() {
            return Err(Error::Invalid("ICC final tag padding or gap"));
        }
    }
    Ok(())
}

/// TIFF header and first-directory framing only. Exif tag meanings, thumbnails,
/// additional directories and conflicts with PNG metadata remain opaque.
pub(super) fn exif(data: &[u8]) -> Result<(), Error> {
    let little = match data.get(..4) {
        Some(b"II\x2a\0") => true,
        Some(b"MM\0\x2a") => false,
        _ => return Err(Error::Invalid("Exif TIFF signature")),
    };
    let header: [u8; 4] = data
        .get(4..8)
        .ok_or(Error::Invalid("Exif TIFF header"))?
        .try_into()
        .unwrap();
    let offset = if little {
        u32::from_le_bytes(header)
    } else {
        u32::from_be_bytes(header)
    } as usize;
    if offset < 8 || !offset.is_multiple_of(2) {
        return Err(Error::Invalid("Exif first IFD offset"));
    }
    let count: [u8; 2] = data
        .get(offset..)
        .and_then(|v| v.get(..2))
        .ok_or(Error::Invalid("Exif first IFD count"))?
        .try_into()
        .unwrap();
    let count = if little {
        u16::from_le_bytes(count)
    } else {
        u16::from_be_bytes(count)
    } as usize;
    if offset
        .checked_add(6 + count * 12)
        .is_none_or(|end| end > data.len())
    {
        return Err(Error::Invalid("Exif first IFD length"));
    }
    Ok(())
}
