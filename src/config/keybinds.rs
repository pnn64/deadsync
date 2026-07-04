use super::{SimpleIni, save_without_keymaps};
use deadsync_input::{
    InputBinding, Keymap, VirtualAction, cleared_keymap, get_keymap, load_keymap_from_ini_entries,
    set_keymap, updated_keymap_unique_gamepad, updated_keymap_unique_keyboard,
};
pub(crate) use deadsync_input::{
    editable_key_binding_slot_indices, keycode_to_token, parse_keycode_to_key,
    protected_default_key_for_action,
};
use winit::keyboard::KeyCode;

pub(crate) fn load_keymap_from_ini_local(conf: &SimpleIni) -> Keymap {
    let section = conf
        .get_section("Keymaps")
        .or_else(|| conf.get_section("keymaps"))
        .map(|section| {
            section
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_str()))
        });
    load_keymap_from_ini_entries(section)
}

/// Update a keyboard binding in Primary/Secondary slots, ensuring that the
/// given key code is not used anywhere else in the keymap.
pub fn update_keymap_binding_unique_keyboard(
    action: VirtualAction,
    index: usize,
    keycode: KeyCode,
) {
    let current = get_keymap();
    let new_map = updated_keymap_unique_keyboard(&current, action, index, keycode);
    set_keymap(new_map);
    save_without_keymaps();
}

/// Update a gamepad binding in Primary/Secondary slots, ensuring that the
/// given physical binding is not used anywhere else in the keymap.
pub fn update_keymap_binding_unique_gamepad(
    action: VirtualAction,
    index: usize,
    binding: InputBinding,
) {
    let current = get_keymap();
    let new_map = updated_keymap_unique_gamepad(&current, action, index, binding);
    set_keymap(new_map);
    save_without_keymaps();
}

/// Clear the requested Primary/Secondary binding slot for an action.
/// Returns `true` when a binding was removed.
pub fn clear_keymap_binding(action: VirtualAction, index: usize) -> bool {
    let current = get_keymap();
    let (new_map, changed) = cleared_keymap(&current, action, index);

    if changed {
        set_keymap(new_map);
        save_without_keymaps();
    }
    changed
}
