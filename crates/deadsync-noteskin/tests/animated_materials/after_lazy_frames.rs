// Parent plus first-reference reuse and deferred owning frame storage.
use super::baseline::itg_resolve_relative_or_noteskin_path;
use super::*;

pub(super) fn itg_resolve_animated_texture_ini(
    data: &noteskin_itg::NoteskinData,
    path: &Path,
) -> Option<ItgResolvedModelTexture> {
    let ini = noteskin_itg::IniData::parse_file(path).ok()?;
    let (first_frame_idx, frame) = if let Some(frame) = ini.get("AnimatedTexture", "Frame0000") {
        (0, frame)
    } else {
        (1, ini.get("AnimatedTexture", "Frame0001")?)
    };
    let first_frame = frame;
    let texture_path = itg_resolve_relative_or_noteskin_path(data, path, frame)?;
    let tex_velocity_x = ini
        .get("AnimatedTexture", "TexVelocityX")
        .and_then(noteskin_itg::parse_ini_float)
        .unwrap_or(0.0);
    let tex_velocity_y = ini
        .get("AnimatedTexture", "TexVelocityY")
        .and_then(noteskin_itg::parse_ini_float)
        .unwrap_or(0.0);
    let tex_offset_x = ini
        .get("AnimatedTexture", "TexOffsetX")
        .and_then(noteskin_itg::parse_ini_float)
        .unwrap_or(0.0);
    let tex_offset_y = ini
        .get("AnimatedTexture", "TexOffsetY")
        .and_then(noteskin_itg::parse_ini_float)
        .unwrap_or(0.0);
    let mut cycle_seconds = 0.0f32;
    let mut frames = Vec::new();
    let mut has_distinct_image = false;
    // Keep common prefixes on the stack. Longer single-image sequences still
    // need no heap scratch; a later path change can revisit the parsed INI.
    let mut prefix_delays = [0.0f32; 32];
    for idx in first_frame_idx..1000 {
        let frame_key = itg_animated_texture_key(*b"Frame0000", idx);
        let delay_key = itg_animated_texture_key(*b"Delay0000", idx);
        let Some(frame) = ini.get("AnimatedTexture", itg_animated_texture_key_str(&frame_key))
        else {
            break;
        };
        let Some(delay) = ini
            .get("AnimatedTexture", itg_animated_texture_key_str(&delay_key))
            .and_then(noteskin_itg::parse_ini_float)
        else {
            break;
        };
        if !delay.is_finite() || delay < 0.0 {
            return None;
        }
        if frames.is_empty()
            && let Some(cached) = prefix_delays.get_mut(idx - first_frame_idx)
        {
            *cached = delay;
        }
        // Borrow the first image while frames retain its exact path spelling.
        // Exact repeated references reuse this load's successful resolution.
        let frame_path = if idx == first_frame_idx || frame == first_frame {
            std::borrow::Cow::Borrowed(texture_path.as_path())
        } else {
            std::borrow::Cow::Owned(itg_resolve_relative_or_noteskin_path(data, path, frame)?)
        };
        if frames.is_empty() && frame_path.as_os_str() != texture_path.as_os_str() {
            // Earlier paths all had the first path's exact spelling. Their
            // validated delays are inline, with an INI fallback past 32.
            frames.reserve(idx - first_frame_idx + 1);
            for previous in first_frame_idx..idx {
                let delay = if let Some(cached) = prefix_delays.get(previous - first_frame_idx) {
                    *cached
                } else {
                    let key = itg_animated_texture_key(*b"Delay0000", previous);
                    ini.get("AnimatedTexture", itg_animated_texture_key_str(&key))
                        .and_then(noteskin_itg::parse_ini_float)
                        .expect("preceding frame delays were validated")
                };
                frames.push(ItgTextureFrame {
                    path: texture_path.clone(),
                    delay,
                });
            }
        }
        // Path equality also accepts aliases such as ./frames/a.png. Preserve
        // those spellings in stored frames without creating a duplicate atlas.
        has_distinct_image = has_distinct_image || frame_path.as_ref() != texture_path;
        if !frames.is_empty() {
            frames.push(ItgTextureFrame {
                path: frame_path.into_owned(),
                delay,
            });
        }
        cycle_seconds += delay;
    }
    Some(ItgResolvedModelTexture {
        sphere_mapped: path.to_string_lossy().contains("sphere"),
        // Repeated references to one image need no atlas. Keep its full UV
        // domain for scrolling materials instead of adding duplicate tiles.
        animation: (has_distinct_image
            && cycle_seconds > f32::EPSILON
            && cycle_seconds.is_finite())
        .then(|| ItgTextureAnimation {
            path: path.to_path_buf(),
            frames,
        }),
        texture_path,
        tex: ItgModelTexturePath {
            uv_velocity: [tex_velocity_x, tex_velocity_y],
            uv_offset: [tex_offset_x, tex_offset_y],
            uv_cycle_seconds: (cycle_seconds > f32::EPSILON && cycle_seconds.is_finite())
                .then_some(cycle_seconds),
        },
    })
}
