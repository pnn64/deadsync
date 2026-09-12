// Frozen from 9b46fda7b / 0.5.1151; unchanged helpers and data types are shared.
use super::super::*;
use crate::GamepadCodeBinding;

pub fn load_keymap_from_ini_entries<'a, I>(section: Option<I>) -> Keymap
where
    I: IntoIterator<Item = (&'a str, &'a str)>,
{
    // When [Keymaps] is present, start from explicit user entries and then fill
    // in any completely missing actions from built-in defaults. When the whole
    // section is absent, fall back to defaults entirely.
    let Some(section) = section else {
        return default_keymap();
    };

    let mut km = Keymap::default();
    let mut seen = 0u32;

    for (key, value) in section {
        if let Some(action) = crate::action_from_ini_key(key) {
            let mut bindings = Vec::new();
            for tok in value.split(',') {
                if let Some(binding) = parse_binding_token(tok) {
                    bindings.push(binding);
                }
            }
            km.bind(action, &bindings);
            seen |= action.bit();
        }
    }

    let defaults = default_keymap();
    for action in ALL_VIRTUAL_ACTIONS {
        if seen & action.bit() != 0 {
            continue;
        }
        let mut bindings = Vec::new();
        let mut i = 0;
        while let Some(binding) = defaults.binding_at(action, i) {
            bindings.push(binding);
            i += 1;
        }
        if !bindings.is_empty() {
            km.bind(action, &bindings);
        }
    }
    restore_available_default_bindings(&mut km);

    km
}

pub fn keymap_ini_lines(keymap: &Keymap) -> Vec<(&'static str, String)> {
    let mut lines = Vec::with_capacity(ALL_VIRTUAL_ACTIONS.len());
    for action in ALL_VIRTUAL_ACTIONS {
        let key_name = crate::action_to_ini_key(action);
        let mut tokens: Vec<String> = Vec::new();
        let mut i = 0;
        while let Some(binding) = keymap.binding_at(action, i) {
            tokens.push(binding_to_token(binding));
            i += 1;
        }
        lines.push((key_name, tokens.join(",")));
    }
    lines
}

pub fn write_default_keymap_ini_section(content: &mut String) {
    content.push_str("[Keymaps]\n");
    for (key, value) in DEFAULT_KEYMAP_INI_LINES {
        writeln!(content, "{key}={value}").expect("writing into String cannot fail");
    }
    content.push('\n');
}

pub fn write_keymap_ini_section(content: &mut String, keymap: &Keymap) {
    content.push_str("[Keymaps]\n");
    for (key, value) in keymap_ini_lines(keymap) {
        writeln!(content, "{key}={value}").expect("writing into String cannot fail");
    }
    content.push('\n');
}

fn load_action_bindings(keymap: &Keymap, action: VirtualAction) -> Vec<InputBinding> {
    let mut bindings = Vec::new();
    let mut i = 0;
    while let Some(binding) = keymap.binding_at(action, i) {
        bindings.push(binding);
        i += 1;
    }
    bindings
}

fn keymap_contains_binding(keymap: &Keymap, binding: InputBinding) -> bool {
    for act in ALL_VIRTUAL_ACTIONS {
        let mut i = 0;
        while let Some(existing) = keymap.binding_at(act, i) {
            if existing == binding {
                return true;
            }
            i += 1;
        }
    }
    false
}

#[inline(always)]
pub fn restore_available_default_bindings(keymap: &mut Keymap) {
    for act in ALL_VIRTUAL_ACTIONS {
        let Some(default_binding) = default_binding_for_action(act) else {
            continue;
        };
        let mut bindings = load_action_bindings(keymap, act);
        if let Some(slot) = bindings
            .iter()
            .position(|binding| *binding == default_binding)
        {
            if slot != 0 {
                bindings.remove(slot);
                bindings.insert(0, default_binding);
                keymap.bind(act, &bindings);
            }
            continue;
        }
        if keymap_contains_binding(keymap, default_binding) {
            continue;
        }
        bindings.insert(0, default_binding);
        keymap.bind(act, &bindings);
    }
}

pub fn updated_keymap_unique_keyboard(
    current: &Keymap,
    action: VirtualAction,
    index: usize,
    keycode: KeyCode,
) -> Keymap {
    let mut new_map = Keymap::default();

    for act in ALL_VIRTUAL_ACTIONS {
        let mut bindings = load_action_bindings(current, act);
        let binding = InputBinding::Key(keycode);
        let keep_default = act == action
            && bindings_start_with_default(act, &bindings)
            && bindings.first() == Some(&binding);

        // Remove this key from every slot so one physical key cannot fan out
        // to multiple actions.
        remove_matching_keyboard_binding(&mut bindings, keycode, keep_default);

        if act == action {
            let first_editable = first_editable_binding_slot(act, &bindings);
            let mut effective_index = requested_to_actual_binding_slot(index, first_editable);
            // If Secondary requested but there is no Primary yet, collapse to
            // the first editable slot.
            if effective_index > first_editable && bindings.len() <= first_editable {
                effective_index = first_editable;
            }
            if keep_default {
                if effective_index >= first_editable && effective_index < bindings.len() {
                    bindings.remove(effective_index);
                }
            } else {
                set_binding_at_slot(&mut bindings, effective_index, binding);
            }
        }

        new_map.bind(act, &bindings);
    }

    restore_available_default_bindings(&mut new_map);
    new_map
}

#[must_use]
pub fn updated_keymap_unique_gamepad(
    current: &Keymap,
    action: VirtualAction,
    index: usize,
    binding: InputBinding,
) -> Keymap {
    let mut new_map = Keymap::default();

    for act in ALL_VIRTUAL_ACTIONS {
        let mut bindings = load_action_bindings(current, act);

        // Remove this binding from every slot so one physical control cannot
        // remain assigned elsewhere.
        remove_matching_input_binding(&mut bindings, binding);

        if act == action {
            let first_editable = first_editable_binding_slot(act, &bindings);
            let mut effective_index = requested_to_actual_binding_slot(index, first_editable);
            // If Secondary requested but there is no Primary yet, collapse to
            // the first editable slot.
            if effective_index > first_editable && bindings.len() <= first_editable {
                effective_index = first_editable;
            }
            set_binding_at_slot(&mut bindings, effective_index, binding);
        }

        new_map.bind(act, &bindings);
    }

    restore_available_default_bindings(&mut new_map);
    new_map
}

#[must_use]
pub fn cleared_keymap(current: &Keymap, action: VirtualAction, index: usize) -> (Keymap, bool) {
    let mut new_map = Keymap::default();
    let mut changed = false;

    for act in ALL_VIRTUAL_ACTIONS {
        let mut bindings = load_action_bindings(current, act);
        if act == action {
            let first_editable = first_editable_binding_slot(act, &bindings);
            let effective_index = requested_to_actual_binding_slot(index, first_editable);
            if effective_index < bindings.len() {
                bindings.remove(effective_index);
                changed = true;
            }
        }
        new_map.bind(act, &bindings);
    }

    if changed {
        restore_available_default_bindings(&mut new_map);
    }
    (new_map, changed)
}

pub fn binding_to_token(binding: InputBinding) -> String {
    match binding {
        InputBinding::Key(code) => format!("KeyCode::{code:?}"),
        InputBinding::PadDir(dir) => format!("PadDir::{dir:?}"),
        InputBinding::PadDirOn { device, dir } => {
            format!("Pad{device}::Dir::{dir:?}")
        }
        InputBinding::GamepadCode(binding) => gamepad_code_binding_to_token(binding),
    }
}

pub fn gamepad_code_binding_to_token(binding: GamepadCodeBinding) -> String {
    const BASE_LEN: usize = "PadCode[0x00000000]".len();
    const UUID_LEN: usize = 33;

    let device_len = binding.device.map_or(0, |mut device| {
        let mut digits = 1;
        while device >= 10 {
            device /= 10;
            digits += 1;
        }
        digits + 1
    });
    let capacity = BASE_LEN + device_len + if binding.uuid.is_some() { UUID_LEN } else { 0 };
    let mut s = String::with_capacity(capacity);
    use std::fmt::Write;
    let _ = write!(&mut s, "PadCode[0x{:08X}]", binding.code_u32);
    if let Some(device) = binding.device {
        let _ = write!(&mut s, "@{device}");
    }
    if let Some(uuid) = binding.uuid {
        s.push('#');
        for b in &uuid {
            let _ = write!(&mut s, "{b:02X}");
        }
    }
    s
}

pub fn has_same_bindings(left: &Keymap, right: &Keymap) -> bool {
    keymap_ini_lines(left) == keymap_ini_lines(right)
}
