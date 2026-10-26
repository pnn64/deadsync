// Frozen bodies from 8139799a1.
use std::path::Path;

pub(super) fn step_type_lanes(step_type: &str) -> usize {
    let step_type = step_type.trim();
    if step_type.eq_ignore_ascii_case("pump-double")
        || step_type.eq_ignore_ascii_case("pump_double")
    {
        10
    } else if step_type.eq_ignore_ascii_case("dance-double")
        || step_type.eq_ignore_ascii_case("dance_double")
    {
        8
    } else if step_type.eq_ignore_ascii_case("pump-single")
        || step_type.eq_ignore_ascii_case("pump_single")
    {
        5
    } else {
        4
    }
}

pub(super) fn is_video_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(is_video_extension)
}

pub(super) fn is_video_extension(extension: &str) -> bool {
    let bytes = extension.as_bytes();
    if !(3..=4).contains(&bytes.len()) {
        return false;
    }

    let mut lowercase = [0_u8; 4];
    for (target, source) in lowercase.iter_mut().zip(bytes) {
        *target = source.to_ascii_lowercase();
    }
    matches!(
        &lowercase[..bytes.len()],
        b"mp4"
            | b"avi"
            | b"f4v"
            | b"flv"
            | b"m4v"
            | b"mov"
            | b"ogv"
            | b"webm"
            | b"mkv"
            | b"mpg"
            | b"mpeg"
            | b"wmv"
    )
}
