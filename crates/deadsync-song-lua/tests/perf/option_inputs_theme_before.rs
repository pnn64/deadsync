// Frozen availability registrations from af4d42368 create_theme_table.
use mlua::{Lua, MultiValue, Table};
use crate::{SONG_LUA_THEME_NAME, method_arg, read_string};
pub(super) fn create_availability_table(lua: &Lua) -> mlua::Result<Table> {
    let theme = lua.create_table()?;
    for method in ["DoesThemeExist", "IsThemeSelectable"] {
        theme.set(
            method,
            lua.create_function(|_, args: MultiValue| {
                let name = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_string)
                    .unwrap_or_default();
                Ok(name.eq_ignore_ascii_case(SONG_LUA_THEME_NAME))
            })?,
        )?;
    }
    theme.set(
        "DoesLanguageExist",
        lua.create_function(|_, args: MultiValue| {
            let name = method_arg(&args, 0)
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            Ok(matches!(
                name.to_ascii_lowercase().as_str(),
                "en" | "english"
            ))
        })?,
    )?;
    Ok(theme)
}
