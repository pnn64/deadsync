use crate::ini::SimpleIni;
use crate::runtime::save_without_keymaps;
use deadsync_input::{
    InputBinding, Keymap, VirtualAction, cleared_keymap, get_keymap, keymap_ini_lines,
    load_keymap_from_ini_entries, set_keymap, updated_keymap_unique_gamepad,
    updated_keymap_unique_keyboard,
};
pub use deadsync_input::{editable_key_binding_slot_indices, protected_default_key_for_action};
use winit::keyboard::KeyCode;

pub fn load_keymap_from_ini(conf: &SimpleIni) -> Keymap {
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

pub fn publish_keymap_from_ini(conf: &SimpleIni) {
    set_keymap(load_keymap_from_ini(conf));
}

pub fn update_keymap_binding_unique_keyboard(
    action: VirtualAction,
    index: usize,
    keycode: KeyCode,
) -> bool {
    let current = get_keymap();
    let new_map = updated_keymap_unique_keyboard(&current, action, index, keycode);
    set_keymap_if_changed(&current, new_map)
}

pub fn update_keymap_binding_unique_keyboard_saved(
    action: VirtualAction,
    index: usize,
    keycode: KeyCode,
) {
    if update_keymap_binding_unique_keyboard(action, index, keycode) {
        save_without_keymaps();
    }
}

pub fn update_keymap_binding_unique_gamepad(
    action: VirtualAction,
    index: usize,
    binding: InputBinding,
) -> bool {
    let current = get_keymap();
    let new_map = updated_keymap_unique_gamepad(&current, action, index, binding);
    set_keymap_if_changed(&current, new_map)
}

pub fn update_keymap_binding_unique_gamepad_saved(
    action: VirtualAction,
    index: usize,
    binding: InputBinding,
) {
    if update_keymap_binding_unique_gamepad(action, index, binding) {
        save_without_keymaps();
    }
}

pub fn clear_keymap_binding(action: VirtualAction, index: usize) -> bool {
    let current = get_keymap();
    let (new_map, changed) = cleared_keymap(&current, action, index);
    if changed {
        set_keymap(new_map);
    }
    changed
}

pub fn clear_keymap_binding_saved(action: VirtualAction, index: usize) -> bool {
    if clear_keymap_binding(action, index) {
        save_without_keymaps();
        true
    } else {
        false
    }
}

fn set_keymap_if_changed(current: &Keymap, new_map: Keymap) -> bool {
    let changed = keymap_ini_lines(current) != keymap_ini_lines(&new_map);
    if changed {
        set_keymap(new_map);
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keymap_change_detection_uses_serialized_bindings() {
        let current = Keymap::default();
        assert!(!set_keymap_if_changed(&current, Keymap::default()));

        let mut changed = Keymap::default();
        changed.bind(
            VirtualAction::p1_start,
            &[InputBinding::Key(KeyCode::Enter)],
        );

        assert!(keymap_ini_lines(&current) != keymap_ini_lines(&changed));
    }
}
