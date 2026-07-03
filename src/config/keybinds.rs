use super::{SimpleIni, save_without_keymaps};
pub(crate) use deadsync_input::{
    ALL_VIRTUAL_ACTIONS, action_to_ini_key, binding_to_token, editable_key_binding_slot_indices,
    keycode_to_token, parse_keycode_to_key, protected_default_key_for_action,
};
use deadsync_input::{
    InputBinding, Keymap, VirtualAction, action_from_ini_key_lower, cleared_keymap,
    default_keymap as default_keymap_local, get_keymap, parse_binding_token,
    restore_available_default_bindings, set_keymap, updated_keymap_unique_gamepad,
    updated_keymap_unique_keyboard,
};
use winit::keyboard::KeyCode;

pub(crate) fn load_keymap_from_ini_local(conf: &SimpleIni) -> Keymap {
    // When [Keymaps] is present, start from explicit user entries and then fill
    // in any completely missing actions from built-in defaults. When the whole
    // section is absent, fall back to defaults entirely.
    if let Some(section) = conf
        .get_section("Keymaps")
        .or_else(|| conf.get_section("keymaps"))
    {
        let mut km = Keymap::default();
        let mut seen: Vec<VirtualAction> = Vec::new();

        for (k, v) in section {
            let key = k.to_ascii_lowercase();
            if let Some(action) = action_from_ini_key_lower(&key) {
                let mut bindings = Vec::new();
                for tok in v.split(',') {
                    if let Some(b) = parse_binding_token(tok) {
                        bindings.push(b);
                    }
                }
                km.bind(action, &bindings);
                seen.push(action);
            }
        }

        let defaults = default_keymap_local();
        for act in ALL_VIRTUAL_ACTIONS {
            if !seen.contains(&act) {
                let mut bindings = Vec::new();
                let mut i = 0;
                while let Some(b) = defaults.binding_at(act, i) {
                    bindings.push(b);
                    i += 1;
                }
                if !bindings.is_empty() {
                    km.bind(act, &bindings);
                }
            }
        }
        restore_available_default_bindings(&mut km);

        km
    } else {
        default_keymap_local()
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_operator_ini_restores_scroll_lock_default() {
        let mut conf = SimpleIni::new();
        conf.load_str("[Keymaps]\nP1_Operator=\n");
        let keymap = load_keymap_from_ini_local(&conf);

        assert_eq!(
            keymap.binding_at(VirtualAction::p1_operator, 0),
            Some(InputBinding::Key(KeyCode::ScrollLock))
        );
    }

    #[test]
    fn system_acceleration_keys_round_trip_through_ini() {
        let mut conf = SimpleIni::new();
        conf.load_str(
            "[Keymaps]\nSystem_FastForward=KeyCode::Backquote\nSystem_SlowDown=KeyCode::Tab\n",
        );
        let keymap = load_keymap_from_ini_local(&conf);

        assert_eq!(
            keymap.binding_at(VirtualAction::system_fast_forward, 0),
            Some(InputBinding::Key(KeyCode::Backquote))
        );
        assert_eq!(
            keymap.binding_at(VirtualAction::system_slow_down, 0),
            Some(InputBinding::Key(KeyCode::Tab))
        );
    }
}
