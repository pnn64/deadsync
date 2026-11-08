//! Frozen direction and INI formatters from 0f7ded20c.
use crate::keymap::{InputBinding, Keymap};
use crate::{ALL_VIRTUAL_ACTIONS, gamepad_code_binding_to_token};
use std::fmt::Write as _;
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

pub fn keymap_ini_lines(keymap: &Keymap) -> Vec<(&'static str, String)> {
    ALL_VIRTUAL_ACTIONS
        .iter()
        .map(|&action| {
            let mut value = String::new();
            write_binding_list(&mut value, keymap.bindings_for_action(action));
            (crate::action_to_ini_key(action), value)
        })
        .collect()
}

pub fn write_binding_list(content: &mut String, bindings: &[InputBinding]) {
    for (index, &binding) in bindings.iter().enumerate() {
        if index != 0 {
            content.push(',');
        }
        match binding {
            InputBinding::Key(code) => write!(content, "KeyCode::{code:?}").unwrap(),
            InputBinding::PadDir(dir) => write!(content, "PadDir::{dir:?}").unwrap(),
            InputBinding::PadDirOn { device, dir } => {
                write!(content, "Pad{device}::Dir::{dir:?}").unwrap()
            }
            InputBinding::GamepadCode(binding) => {
                crate::write_gamepad_code_binding_token(content, binding)
            }
        }
    }
}

pub fn write_keymap_ini_section(content: &mut String, keymap: &Keymap) {
    content.push_str("[Keymaps]\n");
    for action in ALL_VIRTUAL_ACTIONS {
        content.push_str(crate::action_to_ini_key(action));
        content.push('=');
        write_binding_list(content, keymap.bindings_for_action(action));
        content.push('\n');
    }
    content.push('\n');
}
