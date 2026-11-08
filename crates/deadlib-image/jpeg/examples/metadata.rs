//! Read/replace ICC metadata for the independent external metadata gate.
use deadlib_image_jpeg::{
    CodingOptions, IccWrite, Limits, MetadataOptions, decode_coefficients,
    encode_coefficients_with_metadata,
};
use std::io::Write;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() < 3 {
        return Err("usage: metadata inspect input prefix | metadata rewrite input output profile|remove|preserve progressive[0/1]".into());
    }
    let input = std::fs::read(&args[1])?;
    let image = decode_coefficients(&input, Limits::default())?;
    let prefix = std::path::Path::new(&args[2]);
    if args[0] == "inspect" {
        let mut markers = std::fs::File::create(prefix.with_extension("markers"))?;
        for s in image.metadata.iter().filter(|s| s.preceding_scans == 0) {
            markers.write_all(&[s.marker])?;
            markers.write_all(&(s.data.len() as u32).to_le_bytes())?;
            markers.write_all(s.data)?;
        }
        let profile = image.icc_profile()?;
        if let Some(profile) = &profile {
            std::fs::write(
                prefix.with_extension("icc"),
                profile.to_vec(Limits::default().max_metadata_bytes)?,
            )?;
        }
        let jfif = image.jfif()?;
        let adobe = image.adobe()?;
        let orientation = image
            .exif()?
            .map(|e| e.orientation())
            .transpose()?
            .flatten();
        let json = format!(
            "{{\"jfif\":{},\"unit\":{},\"x_density\":{},\"y_density\":{},\"icc\":{},\"icc_length\":{},\"adobe\":{},\"orientation\":{},\"color_space\":\"{:?}\"}}\n",
            jfif.is_some(),
            jfif.map_or(0, |j| j.unit as u8),
            jfif.map_or(1, |j| j.x_density),
            jfif.map_or(1, |j| j.y_density),
            profile.is_some(),
            profile.as_ref().map_or(0, |p| p.len()),
            adobe.map_or("null".into(), |a| a.transform.to_string()),
            orientation.map_or("null".into(), |v| v.to_string()),
            image.color_space()
        );
        std::fs::write(prefix.with_extension("json"), json)?;
    } else if args[0] == "rewrite" && args.len() == 5 {
        let profile = if args[3] == "remove" || args[3] == "preserve" {
            Vec::new()
        } else {
            std::fs::read(&args[3])?
        };
        let icc = match args[3].as_str() {
            "remove" => IccWrite::Remove,
            "preserve" => IccWrite::Preserve,
            _ => IccWrite::Replace(&profile),
        };
        let jpeg = encode_coefficients_with_metadata(
            &image,
            CodingOptions {
                progressive: args[4] == "1",
                ..CodingOptions::default()
            },
            MetadataOptions {
                icc,
                ..MetadataOptions::default()
            },
        )?;
        std::fs::write(prefix, jpeg)?;
    } else {
        return Err("invalid metadata command".into());
    }
    Ok(())
}
