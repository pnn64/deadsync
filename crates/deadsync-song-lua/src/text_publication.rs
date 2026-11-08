//! Publish immutable text payloads only when their authored values change.
use std::sync::Arc;

use deadlib_present::actors::TextAttribute;
use mlua::{Table, Value};

pub(crate) fn sync_text(actor: &Table, text: &mut Arc<str>) -> mlua::Result<()> {
    // Keep the normal lookup: Text may be supplied by an authored __index.
    match actor.get::<Value>("Text")? {
        Value::String(value) => {
            let value = value.to_str()?;
            if text.as_ref() != value.as_ref() {
                *text = Arc::from(value.as_ref());
            }
        }
        value => {
            let value = crate::lua_util::lua_text_value(value)?;
            if text.as_ref() != value {
                *text = value.into();
            }
        }
    }
    Ok(())
}

pub(crate) fn sync_attributes(
    actor: &Table,
    attributes: &mut Arc<[TextAttribute]>,
) -> Result<(), String> {
    let source = actor
        .get::<Option<Table>>("__songlua_text_attributes")
        .map_err(|error| error.to_string())?;
    let mut replacement: Option<Vec<TextAttribute>> = None;
    let mut index = 0;
    if let Some(source) = source {
        for entry in source.sequence_values::<Table>() {
            let entry = entry.map_err(|error| error.to_string())?;
            let Some(value) = crate::lua_util::read_bitmap_text_attribute(&entry)? else {
                continue;
            };
            if let Some(values) = &mut replacement {
                values.push(value);
            } else if attributes
                .get(index)
                .is_none_or(|old| !same_attribute(old, &value))
            {
                let mut values = Vec::with_capacity(attributes.len().max(index + 1));
                values.extend_from_slice(&attributes[..index]);
                values.push(value);
                replacement = Some(values);
            }
            index += 1;
        }
    }
    // Decode once, including in-place table edits. Publish only after success;
    // retained draw snapshots continue owning their original immutable array.
    if let Some(values) = replacement {
        *attributes = values.into();
    } else if index != attributes.len() {
        *attributes = Arc::from(&attributes[..index]);
    }
    Ok(())
}

fn same_attribute(a: &TextAttribute, b: &TextAttribute) -> bool {
    a.start == b.start
        && a.length == b.length
        && a.color.map(f32::to_bits) == b.color.map(f32::to_bits)
        && a.vertex_colors
            .map(|colors| colors.map(|color| color.map(f32::to_bits)))
            == b.vertex_colors
                .map(|colors| colors.map(|color| color.map(f32::to_bits)))
        && a.glow.map(|color| color.map(f32::to_bits))
            == b.glow.map(|color| color.map(f32::to_bits))
}

#[cfg(test)]
#[path = "../tests/unit/text_publication.rs"]
mod tests;
