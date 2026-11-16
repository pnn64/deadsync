//! Song-owned managers. Tables retained by methods are traced VM captures.
use deadlib_lua::{Error, HostCall, TableId, Value, Vm};
mod catalog;
#[cfg(feature = "test-support")]
pub use catalog::{fixture_path_methods, fixture_single_value_array, fixture_string_method};
pub(super) mod compat;
mod display;
mod fields;
mod files;
pub(super) mod game_state;
mod gameplay_helpers;
mod noteskin;
mod option_rows;
mod player_options;
mod preferences;
mod profiles;
mod scores;
pub(super) mod screen;
mod screen_helpers;
mod sl;
mod song_options;
pub(super) mod statistics;
mod theme;
mod theme_preferences;
mod utility;
mod variables;
pub(super) use game_state::install as install_game_state;
pub(super) use noteskin::install as install_noteskin;
pub(super) use player_options::install_states as install_player_states;

pub(super) fn install(
    vm: &mut Vm,
    context: &crate::SongLuaCompileContext,
    #[cfg(feature = "test-support")] observations: crate::observations::Observations,
) -> Result<deadlib_lua::Root, Error> {
    for (index, name) in ["PLAYER_1", "PLAYER_2"].into_iter().enumerate() {
        let value = vm.try_intern(crate::player_number_name(index).as_bytes())?;
        vm.set_global(name.as_bytes(), value)?;
    }
    preferences::install(vm, context)?;
    display::install(vm, context)?;
    theme::install(vm, context)?;
    theme_preferences::install(vm)?;
    sl::install(vm, context)?;
    utility::install(vm)?;
    files::install(vm, context)?;
    gameplay_helpers::install(vm)?;
    variables::install(vm)?;
    profiles::install(vm)?;
    option_rows::install(vm)?;
    compat::install(vm, context)?;
    screen_helpers::install(vm)?;
    catalog::install(
        vm,
        context,
        #[cfg(feature = "test-support")]
        observations,
    )
}

fn constant(call: &mut HostCall<'_>) -> Result<(), Error> {
    call.push(call.captures()[0])
}

pub(super) fn rebuild_sl(vm: &mut Vm, context: &crate::SongLuaCompileContext) -> Result<(), Error> {
    let state = sl::state(vm, context)?;
    vm.set_global(b"SL", state)
}

fn constant_method(vm: &mut Vm, table: TableId, name: &str, value: Value) -> Result<(), Error> {
    let method = vm.new_host_function(constant, &[value])?;
    let root = vm.pin(method)?;
    let result = super::put(vm, table, name, method);
    vm.unpin(root)?;
    result
}

fn table(call: &HostCall<'_>, value: Value) -> Result<TableId, Error> {
    match value {
        Value::Table(table) => Ok(table),
        _ => Err(call.error(b"manager table expected")),
    }
}

pub(super) fn side_effect(
    call: &mut HostCall<'_>,
    globals: Value,
    tonumber: Value,
) -> Result<(), Error> {
    let key = call.try_intern(crate::SONG_LUA_SIDE_EFFECT_COUNT_KEY.as_bytes())?;
    let count = call.table_get(globals, key)?;
    let count = if count == Value::Nil {
        0
    } else {
        let value = call.call(tonumber, &[count])?[0];
        let Value::Number(value) = value else {
            return Err(call.error(b"side effect count must be a number"));
        };
        value as i64
    };
    call.table_set(globals, key, Value::Number(count.saturating_add(1) as f64))
}

/// Match the host's integer counter conversion without calling authored tonumber.
pub(super) fn note_side_effect(call: &mut HostCall<'_>, globals: Value) -> Result<(), Error> {
    let mut stack = call.native_stack()?;
    let initial = stack.top();
    stack.push(globals)?;
    let globals = stack.top() as isize;
    stack.get_field(globals, crate::SONG_LUA_SIDE_EFFECT_COUNT_KEY.as_bytes())?;
    let count = if stack.value(-1) == Some(Value::Nil) {
        0
    } else {
        let value = stack.number(-1)?;
        if !(i64::MIN as f64..-(i64::MIN as f64)).contains(&value) {
            return Err(stack.error(b"integer out of range"));
        }
        value as i64
    };
    stack.push_bytes(crate::SONG_LUA_SIDE_EFFECT_COUNT_KEY.as_bytes())?;
    stack.push(Value::Number(count.saturating_add(1) as f64))?;
    stack.set_table(globals, false)?;
    stack.set_top(initial)
}
