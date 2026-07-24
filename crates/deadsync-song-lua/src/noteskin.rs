use mlua::{Function, Lua, MultiValue, Table, Value};
use std::borrow::Cow;
use std::fs;
use std::path::{Path, PathBuf};

use crate::{
    SongLuaCompileContext, SongLuaNoteskinResolver, file_path_string, is_song_lua_image_path,
    song_lua_default_noteskin_name,
};

pub type SongLuaActorFactory = fn(&Lua, &'static str) -> mlua::Result<Table>;

pub fn create_noteskin_table(
    lua: &Lua,
    context: &SongLuaCompileContext,
    resolver: SongLuaNoteskinResolver,
    create_actor: SongLuaActorFactory,
) -> mlua::Result<Table> {
    let noteskin = lua.create_table()?;
    let default_noteskin = song_lua_default_noteskin_name(context);

    let default_metric_skin = default_noteskin.clone();
    noteskin.set(
        "GetMetric",
        lua.create_function(
            move |lua, (_self, element, value): (Table, String, String)| {
                let Some(metric) = resolver.metric(&default_metric_skin, &element, &value) else {
                    return Ok(Value::Nil);
                };
                Ok(Value::String(lua.create_string(&metric)?))
            },
        )?,
    )?;
    noteskin.set(
        "GetMetricForNoteSkin",
        lua.create_function(
            move |lua, (_self, element, value, skin): (Table, String, String, String)| {
                let Some(metric) = resolver.metric(&skin, &element, &value) else {
                    return Ok(Value::Nil);
                };
                Ok(Value::String(lua.create_string(&metric)?))
            },
        )?,
    )?;

    let default_metric_f_skin = default_noteskin.clone();
    noteskin.set(
        "GetMetricF",
        lua.create_function(move |_, (_self, element, value): (Table, String, String)| {
            Ok(resolver
                .metric_f(&default_metric_f_skin, &element, &value)
                .unwrap_or(0.0_f32))
        })?,
    )?;
    noteskin.set(
        "GetMetricFForNoteSkin",
        lua.create_function(
            move |_, (_self, element, value, skin): (Table, String, String, String)| {
                Ok(resolver
                    .metric_f(&skin, &element, &value)
                    .unwrap_or(0.0_f32))
            },
        )?,
    )?;

    let default_metric_i_skin = default_noteskin.clone();
    noteskin.set(
        "GetMetricI",
        lua.create_function(move |_, (_self, element, value): (Table, String, String)| {
            Ok(resolver.metric_i(&default_metric_i_skin, &element, &value))
        })?,
    )?;
    noteskin.set(
        "GetMetricIForNoteSkin",
        lua.create_function(
            move |_, (_self, element, value, skin): (Table, String, String, String)| {
                Ok(resolver.metric_i(&skin, &element, &value))
            },
        )?,
    )?;

    let default_metric_b_skin = default_noteskin.clone();
    noteskin.set(
        "GetMetricB",
        lua.create_function(move |_, (_self, element, value): (Table, String, String)| {
            Ok(resolver
                .metric_b(&default_metric_b_skin, &element, &value)
                .unwrap_or(false))
        })?,
    )?;
    noteskin.set(
        "GetMetricBForNoteSkin",
        lua.create_function(
            move |_, (_self, element, value, skin): (Table, String, String, String)| {
                Ok(resolver.metric_b(&skin, &element, &value).unwrap_or(false))
            },
        )?,
    )?;

    noteskin.set(
        "GetMetricA",
        lua.create_function(
            move |lua, (_self, _element, _value): (Table, String, String)| {
                song_lua_noteskin_metric_a(lua)
            },
        )?,
    )?;
    noteskin.set(
        "GetMetricAForNoteSkin",
        lua.create_function(
            move |lua, (_self, _element, _value, _skin): (Table, String, String, String)| {
                song_lua_noteskin_metric_a(lua)
            },
        )?,
    )?;

    let default_path_skin = default_noteskin.clone();
    noteskin.set(
        "GetPath",
        lua.create_function(
            move |lua, (_self, button, element): (Table, String, String)| {
                let path = resolver.path_string(&default_path_skin, &button, &element);
                Ok(Value::String(lua.create_string(&path)?))
            },
        )?,
    )?;
    noteskin.set(
        "GetPathForNoteSkin",
        lua.create_function(
            move |lua, (_self, button, element, skin): (Table, String, String, String)| {
                let path = resolver.path_string(&skin, &button, &element);
                Ok(Value::String(lua.create_string(&path)?))
            },
        )?,
    )?;

    let default_load_skin = default_noteskin.clone();
    noteskin.set(
        "LoadActor",
        lua.create_function(
            move |lua, (_self, button, element): (Table, String, String)| {
                song_lua_noteskin_actor(
                    lua,
                    resolver,
                    create_actor,
                    &default_load_skin,
                    &button,
                    &element,
                )
            },
        )?,
    )?;
    noteskin.set(
        "LoadActorForNoteSkin",
        lua.create_function(
            move |lua, (_self, button, element, skin): (Table, String, String, String)| {
                song_lua_noteskin_actor(lua, resolver, create_actor, &skin, &button, &element)
            },
        )?,
    )?;

    noteskin.set(
        "DoesNoteSkinExist",
        lua.create_function(move |_, (_self, skin): (Table, String)| Ok(resolver.exists(&skin)))?,
    )?;
    noteskin.set(
        "GetNoteSkinNames",
        lua.create_function(move |lua, _args: MultiValue| {
            let names = resolver.names();
            let table = lua.create_table()?;
            for (idx, name) in names.into_iter().enumerate() {
                table.raw_set(idx + 1, name)?;
            }
            Ok(table)
        })?,
    )?;
    noteskin.set(
        "HasVariants",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    noteskin.set(
        "IsNoteSkinVariant",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    noteskin.set(
        "GetVariantNamesForNoteSkin",
        lua.create_function(|lua, _args: MultiValue| lua.create_table())?,
    )?;
    Ok(noteskin)
}

fn song_lua_noteskin_metric_a(lua: &Lua) -> mlua::Result<Function> {
    lua.create_function(|_, actor: Table| Ok(Value::Table(actor)))
}

fn song_lua_noteskin_actor(
    lua: &Lua,
    resolver: SongLuaNoteskinResolver,
    create_actor: SongLuaActorFactory,
    skin: &str,
    button: &str,
    element: &str,
) -> mlua::Result<Table> {
    let resolved = resolver.resolve_path(skin, button, element);
    let model_path = resolved
        .as_ref()
        .and_then(|path| song_lua_noteskin_model_template_path(resolver, skin, path));
    let sprite_path = resolved
        .as_ref()
        .filter(|_| model_path.is_none())
        .filter(|path| is_song_lua_image_path(path));
    let actor = create_actor(
        lua,
        if model_path.is_some() {
            "Model"
        } else if sprite_path.is_some() {
            "Sprite"
        } else {
            "Actor"
        },
    )?;
    tag_song_lua_noteskin_actor(&actor, skin, button, element)?;
    if let Some(path) = sprite_path {
        actor.set("Texture", file_path_string(path.as_path()))?;
    }
    if let Some(path) = model_path {
        let path = file_path_string(path.as_path());
        actor.set("Meshes", path.clone())?;
        actor.set("Materials", path.clone())?;
        actor.set("Bones", path)?;
    }
    Ok(actor)
}

fn song_lua_noteskin_model_template_path(
    resolver: SongLuaNoteskinResolver,
    skin: &str,
    template_path: &Path,
) -> Option<PathBuf> {
    if !template_path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("lua"))
    {
        return None;
    }
    let script = fs::read_to_string(template_path).ok()?;
    if !script.to_ascii_lowercase().contains("def.model") {
        return None;
    }
    if let Some((button, element)) = first_noteskin_get_path_args(&script) {
        return resolver.resolve_path(skin, &button, &element);
    }
    let raw = first_lua_field_string(&script, "Meshes")
        .or_else(|| first_lua_field_string(&script, "Materials"))
        .or_else(|| first_lua_field_string(&script, "Bones"))?;
    resolve_noteskin_template_relative_path(template_path, &raw)
}

#[inline]
fn find_ascii_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    let haystack = haystack.as_bytes();
    let needle = needle.as_bytes();
    if needle.is_empty() {
        return Some(0);
    }
    if needle.len() > haystack.len() {
        return None;
    }

    let anchor_index = needle
        .iter()
        .position(|byte| !byte.is_ascii_alphanumeric())
        .unwrap_or(0);
    let anchor_lower = needle[anchor_index].to_ascii_lowercase();
    let anchor_upper = needle[anchor_index].to_ascii_uppercase();
    let suffix_len = needle.len() - anchor_index - 1;
    let candidate_bytes = &haystack[anchor_index..haystack.len() - suffix_len];
    if anchor_lower == anchor_upper {
        memchr::memchr_iter(anchor_lower, candidate_bytes)
            .find(|&index| haystack[index..index + needle.len()].eq_ignore_ascii_case(needle))
    } else {
        memchr::memchr2_iter(anchor_lower, anchor_upper, candidate_bytes)
            .find(|&index| haystack[index..index + needle.len()].eq_ignore_ascii_case(needle))
    }
}

fn first_noteskin_get_path_args(script: &str) -> Option<(String, String)> {
    if let Some(start) = find_ascii_case_insensitive(script, "noteskin:getpath") {
        let open = script[start..].find('(').map(|idx| start + idx + 1)?;
        let (button, next) = parse_lua_string(script, open)?;
        let comma = script[next..].find(',').map(|idx| next + idx + 1)?;
        let (element, _) = parse_lua_string(script, comma)?;
        return Some((button, element));
    }
    None
}

fn first_lua_field_string(script: &str, field: &str) -> Option<String> {
    let start = find_ascii_case_insensitive(script, field)?;
    let eq = script[start + field.len()..]
        .find('=')
        .map(|idx| start + field.len() + idx + 1)?;
    parse_lua_string(script, eq).map(|(value, _)| value)
}

#[cfg(any(test, feature = "bench-support"))]
fn first_noteskin_get_path_args_legacy(script: &str) -> Option<(String, String)> {
    let lower = script.to_ascii_lowercase();
    if let Some(start) = lower.find("noteskin:getpath") {
        let open = script[start..].find('(').map(|idx| start + idx + 1)?;
        let (button, next) = parse_lua_string(script, open)?;
        let comma = script[next..].find(',').map(|idx| next + idx + 1)?;
        let (element, _) = parse_lua_string(script, comma)?;
        return Some((button, element));
    }
    None
}

#[cfg(any(test, feature = "bench-support"))]
fn first_lua_field_string_legacy(script: &str, field: &str) -> Option<String> {
    let lower = script.to_ascii_lowercase();
    let needle = field.to_ascii_lowercase();
    let start = lower.find(&needle)?;
    let eq = script[start + field.len()..]
        .find('=')
        .map(|idx| start + field.len() + idx + 1)?;
    parse_lua_string(script, eq).map(|(value, _)| value)
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn noteskin_get_path_args_for_bench(script: &str) -> Option<(String, String)> {
    first_noteskin_get_path_args(script)
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn noteskin_get_path_args_legacy_for_bench(script: &str) -> Option<(String, String)> {
    first_noteskin_get_path_args_legacy(script)
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn noteskin_model_field_for_bench(script: &str) -> Option<String> {
    first_lua_field_string(script, "Meshes")
        .or_else(|| first_lua_field_string(script, "Materials"))
        .or_else(|| first_lua_field_string(script, "Bones"))
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn noteskin_model_field_legacy_for_bench(script: &str) -> Option<String> {
    first_lua_field_string_legacy(script, "Meshes")
        .or_else(|| first_lua_field_string_legacy(script, "Materials"))
        .or_else(|| first_lua_field_string_legacy(script, "Bones"))
}

#[inline(always)]
fn normalize_noteskin_template_path(raw: &str) -> Cow<'_, str> {
    let bytes = raw.as_bytes();
    let Some(first_separator) = memchr::memchr(b'\\', bytes) else {
        return Cow::Borrowed(raw);
    };

    let mut normalized = String::with_capacity(raw.len());
    normalized.push_str(&raw[..first_separator]);
    normalized.push('/');
    let mut copied_through = first_separator + 1;
    let search_start = copied_through;
    for relative_index in memchr::memchr_iter(b'\\', &bytes[search_start..]) {
        let separator = search_start + relative_index;
        normalized.push_str(&raw[copied_through..separator]);
        normalized.push('/');
        copied_through = separator + 1;
    }
    normalized.push_str(&raw[copied_through..]);
    Cow::Owned(normalized)
}

#[cfg(any(test, feature = "bench-support"))]
fn normalize_noteskin_template_path_legacy(raw: &str) -> Cow<'_, str> {
    Cow::Owned(raw.replace('\\', "/"))
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn normalize_noteskin_template_path_for_bench(raw: &str) -> Cow<'_, str> {
    normalize_noteskin_template_path(raw)
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn normalize_noteskin_template_path_legacy_for_bench(raw: &str) -> Cow<'_, str> {
    normalize_noteskin_template_path_legacy(raw)
}

fn parse_lua_string(script: &str, mut cursor: usize) -> Option<(String, usize)> {
    let bytes = script.as_bytes();
    while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
        cursor += 1;
    }
    let quote = *bytes.get(cursor)?;
    if quote != b'\'' && quote != b'"' {
        return None;
    }
    cursor += 1;
    let mut out = String::new();
    while cursor < bytes.len() {
        let byte = bytes[cursor];
        cursor += 1;
        if byte == quote {
            return Some((out, cursor));
        }
        if byte == b'\\' {
            if let Some(next) = bytes.get(cursor).copied() {
                cursor += 1;
                out.push(next as char);
            }
        } else {
            out.push(byte as char);
        }
    }
    None
}

fn resolve_noteskin_template_relative_path(template_path: &Path, raw: &str) -> Option<PathBuf> {
    let normalized = normalize_noteskin_template_path(raw);
    let raw_path = Path::new(normalized.trim());
    if raw_path.is_absolute() && raw_path.is_file() {
        return Some(raw_path.to_path_buf());
    }
    let candidate = template_path.parent()?.join(raw_path);
    candidate.is_file().then_some(candidate)
}

fn tag_song_lua_noteskin_actor(
    actor: &Table,
    skin: &str,
    button: &str,
    element: &str,
) -> mlua::Result<()> {
    actor.set("__songlua_noteskin_name", skin.trim().to_ascii_lowercase())?;
    actor.set("__songlua_noteskin_button", button)?;
    actor.set("__songlua_noteskin_element", element)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_case_insensitive_find_matches_lowercase_search() {
        let haystacks = [
            "",
            "Def.Model",
            "prefix NOTESKIN:GetPath suffix",
            "M\u{e9}shes = 'unicode remains byte-exact'",
            "no matching model constructor here",
            "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxDEF.MODEL",
        ];
        let needles = [
            "",
            "def.model",
            "noteskin:getpath",
            "M\u{e9}shes",
            "MODEL",
            "missing",
        ];

        for haystack in haystacks {
            for needle in needles {
                assert_eq!(
                    find_ascii_case_insensitive(haystack, needle),
                    haystack
                        .to_ascii_lowercase()
                        .find(&needle.to_ascii_lowercase()),
                    "haystack={haystack:?}, needle={needle:?}"
                );
            }
        }
    }

    #[test]
    fn noteskin_get_path_extraction_matches_legacy() {
        for script in [
            "",
            "return NOTESKIN:GetPath('Down', 'Tap Note')",
            "local path = noteskin:getpath ( \"Left\", \"Hold Head Active\" )",
            "local path = NoTeSkIn:GeTpAtH('Up\\\\Arrow', 'Mine')",
            "return NOTESKIN:GetPath(unquoted, 'Mine')",
            "-- noteskin:getpath without arguments",
        ] {
            assert_eq!(
                first_noteskin_get_path_args(script),
                first_noteskin_get_path_args_legacy(script),
                "script={script:?}"
            );
        }
    }

    #[test]
    fn noteskin_model_field_extraction_matches_legacy() {
        for script in [
            "",
            "return Def.Model { Meshes = 'arrow.txt' }",
            "return Def.Model { MATERIALS = \"arrow material.txt\" }",
            "return Def.Model { bones = 'arrow\\\\bones.txt' }",
            "return Def.Model { NotMeshes = 'legacy substring behavior' }",
            "return Def.Model { Meshes = unquoted }",
        ] {
            for field in ["Meshes", "Materials", "Bones", "M\u{e9}shes"] {
                assert_eq!(
                    first_lua_field_string(script, field),
                    first_lua_field_string_legacy(script, field),
                    "script={script:?}, field={field:?}"
                );
            }
        }
    }

    #[test]
    fn noteskin_template_path_normalization_matches_legacy() {
        for raw in [
            "",
            "arrow.txt",
            "models/arrow model.txt",
            r"models\arrow model.txt",
            r"mixed\path/arrow.txt",
            r"nested\models\materials\arrow model.txt",
            " M\u{e9}shes/arrow.txt ",
        ] {
            let current = normalize_noteskin_template_path(raw);
            let legacy = normalize_noteskin_template_path_legacy(raw);
            assert_eq!(current, legacy, "raw={raw:?}");
            assert_eq!(
                matches!(current, Cow::Borrowed(_)),
                !raw.as_bytes().contains(&b'\\')
            );
        }
    }
}
