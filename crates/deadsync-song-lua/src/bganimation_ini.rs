//! ITG 3.95-style `BGAnimation.ini` folders, translated to an actor definition.
//!
//! Mirrors `BGAnimation::AddLayersFromAniDir` and the legacy branch of
//! `LuaHelpers::ParseCommandList`: every `[LayerN]` section becomes one layer
//! frame holding the actor named by `File`, ordered by N, and each
//! `...Command` value becomes a function with lowercased method names.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

pub(crate) const FILE_NAME: &str = "BGAnimation.ini";

pub(crate) fn is_ini(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case(FILE_NAME))
}

struct Layer {
    order: i64,
    keys: Vec<(String, String)>,
}

impl Layer {
    fn get(&self, key: &str) -> Option<&str> {
        self.keys
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(key))
            .map(|(_, value)| value.as_str())
    }
}

fn layers(text: &str) -> Vec<Layer> {
    let mut layers: Vec<Layer> = Vec::new();
    let mut in_layer = false;
    for line in text.trim_start_matches('\u{feff}').lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with([';', '#']) || line.starts_with("//") {
            continue;
        }
        if let Some(section) = line.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
            let section = section.trim();
            in_layer = section
                .get(..5)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("layer"));
            if in_layer {
                // sscanf("Layer%d"): an unnumbered layer sorts as zero.
                let digits = section[5..].trim_start();
                let end = digits
                    .char_indices()
                    .find(|&(index, ch)| !(ch.is_ascii_digit() || (index == 0 && ch == '-')))
                    .map_or(digits.len(), |(index, _)| index);
                layers.push(Layer {
                    order: digits[..end].parse().unwrap_or(0),
                    keys: Vec::new(),
                });
            }
            continue;
        }
        if in_layer
            && let Some((key, value)) = line.split_once('=')
            && let Some(layer) = layers.last_mut()
        {
            layer
                .keys
                .push((key.trim().to_owned(), value.trim().to_owned()));
        }
    }
    layers.sort_by_key(|layer| layer.order);
    layers
}

/// RageFile lookups ignore case and `File` may omit its extension.
fn resolve_file(dir: &Path, file: &str) -> Option<PathBuf> {
    let file = file.replace('\\', "/");
    let exact = dir.join(&file);
    if exact.exists() {
        return Some(exact);
    }
    let parent = exact.parent()?;
    let prefix = exact.file_name()?.to_str()?.to_ascii_lowercase();
    let mut matches = std::fs::read_dir(parent)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.to_ascii_lowercase().starts_with(&prefix))
        })
        .collect::<Vec<_>>();
    matches.sort_unstable();
    matches.into_iter().next()
}

fn quote(out: &mut String, text: &str) {
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' | '\\' => {
                out.push('\\');
                out.push(ch);
            }
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\0' => out.push_str("\\0"),
            _ => out.push(ch),
        }
    }
    out.push('"');
}

fn color(arg: &str) -> Option<[f32; 4]> {
    let hex = arg.strip_prefix('#')?;
    if !matches!(hex.len(), 6 | 8) || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let mut rgba = [1.0; 4];
    for (index, channel) in rgba.iter_mut().enumerate().take(hex.len() / 2) {
        *channel = f32::from(u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).ok()?) / 255.0;
    }
    Some(rgba)
}

fn command_body(out: &mut String, commands: &str) {
    for command in commands.split(';') {
        let mut args = command.split(',').map(str::trim);
        let name = args.next().unwrap_or_default().to_ascii_lowercase();
        if name.is_empty()
            || name.starts_with(|ch: char| ch.is_ascii_digit())
            || !name.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            continue;
        }
        let _ = write!(out, " self:{name}(");
        let mut first = true;
        for arg in args {
            if !first {
                out.push(',');
            }
            first = false;
            let arg = arg.strip_prefix('+').unwrap_or(arg);
            if arg.parse::<f64>().is_ok_and(f64::is_finite) {
                out.push_str(arg);
            } else if let Some(rgba) = color(arg) {
                let _ = write!(out, "{},{},{},{}", rgba[0], rgba[1], rgba[2], rgba[3]);
            } else {
                quote(out, arg);
            }
        }
        out.push(')');
    }
}

/// A 3.95 `.sprite` file: `[Sprite]` with `Texture`, `FrameNNNN`, `DelayNNNN`.
/// Returns the `Texture=` and `Frames=` fields of a `Def.Sprite` table.
fn sprite_file(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let mut texture = None;
    let mut frames: Vec<(u32, Option<u32>, Option<f32>)> = Vec::new();
    for line in text.trim_start_matches('\u{feff}').lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim().trim_end_matches(';').trim();
        if key == "texture" {
            texture = resolve_file(path.parent()?, value);
            continue;
        }
        let (is_frame, digits) = if let Some(digits) = key.strip_prefix("frame") {
            (true, digits)
        } else if let Some(digits) = key.strip_prefix("delay") {
            (false, digits)
        } else {
            continue;
        };
        let Ok(index) = digits.parse::<u32>() else {
            continue;
        };
        let slot = match frames.iter().position(|frame| frame.0 == index) {
            Some(slot) => slot,
            None => {
                frames.push((index, None, None));
                frames.len() - 1
            }
        };
        if is_frame {
            frames[slot].1 = value.parse().ok();
        } else {
            frames[slot].2 = value.parse().ok();
        }
    }
    frames.sort_by_key(|frame| frame.0);
    let mut out = String::from("\t\tTexture=");
    quote(&mut out, &crate::file_path_string(&texture?));
    out.push_str(",\n");
    if frames.iter().any(|frame| frame.1.is_some()) {
        out.push_str("\t\tFrames={");
        for (_, frame, delay) in frames {
            if let Some(frame) = frame {
                let _ = write!(out, "{{Frame={frame},Delay={}}},", delay.unwrap_or(0.0));
            }
        }
        out.push_str("},\n");
    }
    Some(out)
}

pub(crate) fn lua_source(text: &str, dir: &Path) -> String {
    let mut out = String::from("return Def.ActorFrame{\n");
    for layer in layers(text) {
        let kind = layer.get("Type").unwrap_or("sprite").to_ascii_lowercase();
        if !matches!(kind.as_str(), "sprite" | "0" | "1" | "") {
            log::warn!(
                "{}: BGAnimation layer type '{kind}' is not supported; skipping layer",
                dir.display()
            );
            continue;
        }
        let Some(file) = layer.get("File").filter(|file| !file.is_empty()) else {
            continue;
        };
        let Some(path) = resolve_file(dir, file) else {
            log::warn!("{}: BGAnimation layer file '{file}' not found", dir.display());
            continue;
        };
        let stretch = kind == "1"
            || layer
                .get("Stretch")
                .is_some_and(|value| matches!(value.trim(), "1" | "true" | "True" | "TRUE"));
        out.push_str("\tDef.ActorFrame{ ");
        let is_sprite_file = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("sprite"));
        if is_sprite_file {
            let Some(sprite) = sprite_file(&path) else {
                log::warn!("{}: could not read sprite file", path.display());
                out.push_str("},\n");
                continue;
            };
            out.push_str("Def.Sprite{\n");
            out.push_str(&sprite);
        } else {
            out.push_str("LoadActor(");
            quote(&mut out, &crate::file_path_string(&path));
            out.push_str(")..{\n");
        }
        let mut init = String::new();
        if stretch {
            init.push_str(" self:stretchto(0,0,SCREEN_WIDTH,SCREEN_HEIGHT)");
        }
        for (key, value) in &layer.keys {
            let lower = key.to_ascii_lowercase();
            let Some(stem) = lower.strip_suffix("command") else {
                continue;
            };
            // "Command" alone is the 3.9 spelling of OnCommand.
            let stem = if stem.is_empty() { "on" } else { stem };
            if stem == "init" {
                command_body(&mut init, value);
                continue;
            }
            if !stem.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_') {
                continue;
            }
            let mut name = stem.to_owned();
            name[..1].make_ascii_uppercase();
            let _ = write!(out, "\t\t{name}Command=function(self)");
            command_body(&mut out, value);
            out.push_str(" end,\n");
        }
        if !init.is_empty() {
            let _ = writeln!(out, "\t\tInitCommand=function(self){init} end,");
        }
        out.push_str("\t} },\n");
    }
    out.push_str("}\n");
    out
}
