use std::fmt::Write as _;

use winit::keyboard::KeyCode;

use crate::{
    ALL_VIRTUAL_ACTIONS, InputBinding, Keymap, VirtualAction, gamepad_code_binding_to_token,
    parse_gamepad_code_binding, parse_pad_dir,
};

pub fn default_keymap() -> Keymap {
    use VirtualAction as A;
    let mut km = Keymap::default();
    // Player 1 defaults (WASD + arrows, Enter/Escape).
    km.bind(
        A::p1_up,
        &[
            InputBinding::Key(KeyCode::ArrowUp),
            InputBinding::Key(KeyCode::KeyW),
        ],
    );
    km.bind(
        A::p1_down,
        &[
            InputBinding::Key(KeyCode::ArrowDown),
            InputBinding::Key(KeyCode::KeyS),
        ],
    );
    km.bind(
        A::p1_left,
        &[
            InputBinding::Key(KeyCode::ArrowLeft),
            InputBinding::Key(KeyCode::KeyA),
        ],
    );
    km.bind(
        A::p1_right,
        &[
            InputBinding::Key(KeyCode::ArrowRight),
            InputBinding::Key(KeyCode::KeyD),
        ],
    );
    km.bind(A::p1_select, &[InputBinding::Key(KeyCode::Slash)]);
    km.bind(A::p1_start, &[InputBinding::Key(KeyCode::Enter)]);
    km.bind(A::p1_back, &[InputBinding::Key(KeyCode::Escape)]);
    // Player 2 defaults (numpad directions + Start on NumpadEnter).
    km.bind(A::p2_up, &[InputBinding::Key(KeyCode::Numpad8)]);
    km.bind(A::p2_down, &[InputBinding::Key(KeyCode::Numpad2)]);
    km.bind(A::p2_left, &[InputBinding::Key(KeyCode::Numpad4)]);
    km.bind(A::p2_right, &[InputBinding::Key(KeyCode::Numpad6)]);
    km.bind(A::p2_select, &[InputBinding::Key(KeyCode::NumpadDecimal)]);
    km.bind(A::p2_start, &[InputBinding::Key(KeyCode::NumpadEnter)]);
    km.bind(A::p2_back, &[InputBinding::Key(KeyCode::Numpad0)]);
    km.bind(A::p1_operator, &[InputBinding::Key(KeyCode::ScrollLock)]);
    km.bind(A::system_fast_forward, &[InputBinding::Key(KeyCode::Tab)]);
    km.bind(
        A::system_slow_down,
        &[InputBinding::Key(KeyCode::Backquote)],
    );
    // Leave dedicated menu buttons, P2 operator, and restart unbound by default for now.
    km
}

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
    let mut seen: Vec<VirtualAction> = Vec::new();

    for (key, value) in section {
        let key = key.to_ascii_lowercase();
        if let Some(action) = crate::action_from_ini_key_lower(&key) {
            let mut bindings = Vec::new();
            for tok in value.split(',') {
                if let Some(binding) = parse_binding_token(tok) {
                    bindings.push(binding);
                }
            }
            km.bind(action, &bindings);
            seen.push(action);
        }
    }

    let defaults = default_keymap();
    for action in ALL_VIRTUAL_ACTIONS {
        if seen.contains(&action) {
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

pub const DEFAULT_KEYMAP_INI_LINES: [(&str, &str); 28] = [
    ("P1_Back", "KeyCode::Escape"),
    ("P1_Down", "KeyCode::ArrowDown,KeyCode::KeyS"),
    ("P1_Left", "KeyCode::ArrowLeft,KeyCode::KeyA"),
    ("P1_MenuDown", ""),
    ("P1_MenuLeft", ""),
    ("P1_MenuRight", ""),
    ("P1_MenuUp", ""),
    ("P1_Operator", "KeyCode::ScrollLock"),
    ("P1_Restart", ""),
    ("P1_Right", "KeyCode::ArrowRight,KeyCode::KeyD"),
    ("P1_Select", "KeyCode::Slash"),
    ("P1_Start", "KeyCode::Enter"),
    ("P1_Up", "KeyCode::ArrowUp,KeyCode::KeyW"),
    ("P2_Back", "KeyCode::Numpad0"),
    ("P2_Down", "KeyCode::Numpad2"),
    ("P2_Left", "KeyCode::Numpad4"),
    ("P2_MenuDown", ""),
    ("P2_MenuLeft", ""),
    ("P2_MenuRight", ""),
    ("P2_MenuUp", ""),
    ("P2_Operator", ""),
    ("P2_Restart", ""),
    ("P2_Right", "KeyCode::Numpad6"),
    ("P2_Select", "KeyCode::NumpadDecimal"),
    ("P2_Start", "KeyCode::NumpadEnter"),
    ("P2_Up", "KeyCode::Numpad8"),
    ("System_FastForward", "KeyCode::Tab"),
    ("System_SlowDown", "KeyCode::Backquote"),
];

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

#[inline(always)]
pub const fn default_key_for_action(action: VirtualAction) -> Option<KeyCode> {
    use VirtualAction as A;
    match action {
        A::p1_up => Some(KeyCode::ArrowUp),
        A::p1_down => Some(KeyCode::ArrowDown),
        A::p1_left => Some(KeyCode::ArrowLeft),
        A::p1_right => Some(KeyCode::ArrowRight),
        A::p1_select => Some(KeyCode::Slash),
        A::p1_start => Some(KeyCode::Enter),
        A::p1_back => Some(KeyCode::Escape),
        A::p1_operator => Some(KeyCode::ScrollLock),
        A::p2_up => Some(KeyCode::Numpad8),
        A::p2_down => Some(KeyCode::Numpad2),
        A::p2_left => Some(KeyCode::Numpad4),
        A::p2_right => Some(KeyCode::Numpad6),
        A::p2_select => Some(KeyCode::NumpadDecimal),
        A::p2_start => Some(KeyCode::NumpadEnter),
        A::p2_back => Some(KeyCode::Numpad0),
        // System (non-player) tier: Tab acceleration fast-forward / slow-down.
        A::system_fast_forward => Some(KeyCode::Tab),
        A::system_slow_down => Some(KeyCode::Backquote),
        _ => None,
    }
}

#[inline(always)]
pub fn default_binding_for_action(action: VirtualAction) -> Option<InputBinding> {
    default_key_for_action(action).map(InputBinding::Key)
}

#[inline(always)]
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

#[inline(always)]
pub fn parse_keycode(t: &str) -> Option<InputBinding> {
    let name = t.strip_prefix("KeyCode::")?;
    macro_rules! keycode_match {
        ($input:expr, $( $name:ident ),* $(,)?) => {
            match $input {
                $( stringify!($name) => Some(KeyCode::$name), )*
                _ => None,
            }
        };
    }
    keycode_match!(
        name,
        Backquote,
        Backslash,
        BracketLeft,
        BracketRight,
        Comma,
        Digit0,
        Digit1,
        Digit2,
        Digit3,
        Digit4,
        Digit5,
        Digit6,
        Digit7,
        Digit8,
        Digit9,
        Equal,
        IntlBackslash,
        IntlRo,
        IntlYen,
        KeyA,
        KeyB,
        KeyC,
        KeyD,
        KeyE,
        KeyF,
        KeyG,
        KeyH,
        KeyI,
        KeyJ,
        KeyK,
        KeyL,
        KeyM,
        KeyN,
        KeyO,
        KeyP,
        KeyQ,
        KeyR,
        KeyS,
        KeyT,
        KeyU,
        KeyV,
        KeyW,
        KeyX,
        KeyY,
        KeyZ,
        Minus,
        Period,
        Quote,
        Semicolon,
        Slash,
        AltLeft,
        AltRight,
        Backspace,
        CapsLock,
        ContextMenu,
        ControlLeft,
        ControlRight,
        Enter,
        SuperLeft,
        SuperRight,
        ShiftLeft,
        ShiftRight,
        Space,
        Tab,
        Convert,
        KanaMode,
        Lang1,
        Lang2,
        Lang3,
        Lang4,
        Lang5,
        NonConvert,
        Delete,
        End,
        Help,
        Home,
        Insert,
        PageDown,
        PageUp,
        ArrowDown,
        ArrowLeft,
        ArrowRight,
        ArrowUp,
        NumLock,
        Numpad0,
        Numpad1,
        Numpad2,
        Numpad3,
        Numpad4,
        Numpad5,
        Numpad6,
        Numpad7,
        Numpad8,
        Numpad9,
        NumpadAdd,
        NumpadBackspace,
        NumpadClear,
        NumpadClearEntry,
        NumpadComma,
        NumpadDecimal,
        NumpadDivide,
        NumpadEnter,
        NumpadEqual,
        NumpadHash,
        NumpadMemoryAdd,
        NumpadMemoryClear,
        NumpadMemoryRecall,
        NumpadMemoryStore,
        NumpadMemorySubtract,
        NumpadMultiply,
        NumpadParenLeft,
        NumpadParenRight,
        NumpadStar,
        NumpadSubtract,
        Escape,
        Fn,
        FnLock,
        PrintScreen,
        ScrollLock,
        Pause,
        BrowserBack,
        BrowserFavorites,
        BrowserForward,
        BrowserHome,
        BrowserRefresh,
        BrowserSearch,
        BrowserStop,
        Eject,
        LaunchApp1,
        LaunchApp2,
        LaunchMail,
        MediaPlayPause,
        MediaSelect,
        MediaStop,
        MediaTrackNext,
        MediaTrackPrevious,
        Power,
        Sleep,
        AudioVolumeDown,
        AudioVolumeMute,
        AudioVolumeUp,
        WakeUp,
        Meta,
        Hyper,
        Turbo,
        Abort,
        Resume,
        Suspend,
        Again,
        Copy,
        Cut,
        Find,
        Open,
        Paste,
        Props,
        Select,
        Undo,
        Hiragana,
        Katakana,
        F1,
        F2,
        F3,
        F4,
        F5,
        F6,
        F7,
        F8,
        F9,
        F10,
        F11,
        F12,
        F13,
        F14,
        F15,
        F16,
        F17,
        F18,
        F19,
        F20,
        F21,
        F22,
        F23,
        F24,
        F25,
        F26,
        F27,
        F28,
        F29,
        F30,
        F31,
        F32,
        F33,
        F34,
        F35,
    )
    .map(InputBinding::Key)
}

/// Serialize a single `KeyCode` to its `KeyCode::Name` INI token.
#[inline(always)]
pub fn keycode_to_token(code: KeyCode) -> String {
    binding_to_token(InputBinding::Key(code))
}

/// Parse a `KeyCode::Name` INI token into a bare `KeyCode`, ignoring any
/// non-keyboard binding tokens (pad/gamepad).
#[inline(always)]
pub fn parse_keycode_to_key(t: &str) -> Option<KeyCode> {
    match parse_keycode(t)? {
        InputBinding::Key(code) => Some(code),
        _ => None,
    }
}

#[inline(always)]
pub fn parse_pad_code(t: &str) -> Option<InputBinding> {
    parse_gamepad_code_binding(t).map(InputBinding::GamepadCode)
}

#[inline(always)]
pub fn parse_pad_device_binding(t: &str) -> Option<InputBinding> {
    let mut parts = t.split("::");
    let pad = parts.next()?;
    let kind = parts.next()?;
    let name = parts.next()?;
    if parts.next().is_some() || kind != "Dir" {
        return None;
    }

    let dev = pad.strip_prefix("Pad")?;
    let dir = parse_pad_dir(name)?;
    if dev.is_empty() {
        return Some(InputBinding::PadDir(dir));
    }
    Some(InputBinding::PadDirOn {
        device: dev.parse::<usize>().ok()?,
        dir,
    })
}

#[inline(always)]
pub fn parse_pad_dir_binding(t: &str) -> Option<InputBinding> {
    t.strip_prefix("PadDir::")
        .and_then(parse_pad_dir)
        .map(InputBinding::PadDir)
        .or_else(|| parse_pad_device_binding(t))
}

#[inline(always)]
pub fn parse_binding_token(tok: &str) -> Option<InputBinding> {
    let t = tok.trim();
    parse_keycode(t)
        .or_else(|| parse_pad_code(t))
        .or_else(|| parse_pad_dir_binding(t))
}

#[inline(always)]
fn bindings_start_with_default(action: VirtualAction, bindings: &[InputBinding]) -> bool {
    matches!(
        (default_binding_for_action(action), bindings.first()),
        (Some(default_binding), Some(first_binding)) if default_binding == *first_binding
    )
}

#[inline(always)]
fn first_editable_binding_slot(action: VirtualAction, bindings: &[InputBinding]) -> usize {
    if bindings_start_with_default(action, bindings) {
        1
    } else {
        0
    }
}

#[inline(always)]
fn requested_to_actual_binding_slot(requested_index: usize, first_editable: usize) -> usize {
    if first_editable == 0 {
        requested_index.saturating_sub(1)
    } else {
        requested_index
    }
}

#[inline(always)]
pub fn editable_key_binding_slot_indices(keymap: &Keymap, action: VirtualAction) -> (usize, usize) {
    if keymap.binding_at(action, 0) == default_binding_for_action(action) {
        (1, 2)
    } else {
        (0, 1)
    }
}

#[inline(always)]
pub fn protected_default_key_for_action(keymap: &Keymap, action: VirtualAction) -> Option<KeyCode> {
    let default_key = default_key_for_action(action)?;
    if keymap.binding_at(action, 0) == Some(InputBinding::Key(default_key)) {
        Some(default_key)
    } else {
        None
    }
}

#[inline(always)]
fn load_action_bindings(keymap: &Keymap, action: VirtualAction) -> Vec<InputBinding> {
    let mut bindings = Vec::new();
    let mut i = 0;
    while let Some(binding) = keymap.binding_at(action, i) {
        bindings.push(binding);
        i += 1;
    }
    bindings
}

#[inline(always)]
fn remove_matching_keyboard_binding(
    bindings: &mut Vec<InputBinding>,
    keycode: KeyCode,
    keep_first: bool,
) {
    let mut slot = 0;
    bindings.retain(|binding| {
        let keep = (keep_first && slot == 0)
            || !matches!(binding, InputBinding::Key(code) if *code == keycode);
        slot += 1;
        keep
    });
}

#[inline(always)]
fn remove_matching_input_binding(bindings: &mut Vec<InputBinding>, binding: InputBinding) {
    bindings.retain(|existing| *existing != binding);
}

#[inline(always)]
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

#[inline(always)]
fn set_binding_at_slot(bindings: &mut Vec<InputBinding>, slot_index: usize, binding: InputBinding) {
    let slot_index = slot_index.min(bindings.len());
    if bindings.len() <= slot_index {
        bindings.push(binding);
    } else {
        bindings[slot_index] = binding;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GamepadCodeBinding, PadDir, parse_pad_dir};

    #[test]
    fn default_operator_key_is_scroll_lock() {
        let defaults = default_keymap();

        assert_eq!(
            defaults.binding_at(VirtualAction::p1_operator, 0),
            Some(InputBinding::Key(KeyCode::ScrollLock))
        );
        assert_eq!(defaults.binding_at(VirtualAction::p2_operator, 0), None);
    }

    #[test]
    fn replacing_stolen_default_restores_original_action() {
        let moved = updated_keymap_unique_keyboard(
            &default_keymap(),
            VirtualAction::p1_start,
            1,
            KeyCode::Slash,
        );
        assert_eq!(
            moved.binding_at(VirtualAction::p1_start, 1),
            Some(InputBinding::Key(KeyCode::Slash))
        );
        assert_eq!(moved.binding_at(VirtualAction::p1_select, 0), None);

        let restored =
            updated_keymap_unique_keyboard(&moved, VirtualAction::p1_start, 1, KeyCode::KeyZ);
        assert_eq!(
            restored.binding_at(VirtualAction::p1_select, 0),
            Some(InputBinding::Key(KeyCode::Slash))
        );
        assert_eq!(
            restored.binding_at(VirtualAction::p1_start, 1),
            Some(InputBinding::Key(KeyCode::KeyZ))
        );
    }

    #[test]
    fn clearing_stolen_default_restores_original_action() {
        let moved = updated_keymap_unique_keyboard(
            &default_keymap(),
            VirtualAction::p1_start,
            1,
            KeyCode::Slash,
        );
        let (restored, changed) = cleared_keymap(&moved, VirtualAction::p1_start, 1);

        assert!(changed);
        assert_eq!(
            restored.binding_at(VirtualAction::p1_select, 0),
            Some(InputBinding::Key(KeyCode::Slash))
        );
        assert_eq!(
            restored.binding_at(VirtualAction::p1_start, 0),
            Some(InputBinding::Key(KeyCode::Enter))
        );
        assert_eq!(restored.binding_at(VirtualAction::p1_start, 1), None);
    }

    #[test]
    fn rebinding_protected_default_does_not_skip_slots() {
        let rebound = updated_keymap_unique_keyboard(
            &default_keymap(),
            VirtualAction::p1_start,
            1,
            KeyCode::Enter,
        );

        assert_eq!(
            rebound.binding_at(VirtualAction::p1_start, 0),
            Some(InputBinding::Key(KeyCode::Enter))
        );
        assert_eq!(rebound.binding_at(VirtualAction::p1_start, 1), None);
    }

    #[test]
    fn rebinding_protected_default_clears_editable_slot() {
        let with_primary = updated_keymap_unique_keyboard(
            &default_keymap(),
            VirtualAction::p1_start,
            1,
            KeyCode::KeyZ,
        );
        let rebound = updated_keymap_unique_keyboard(
            &with_primary,
            VirtualAction::p1_start,
            1,
            KeyCode::Enter,
        );

        assert_eq!(
            rebound.binding_at(VirtualAction::p1_start, 0),
            Some(InputBinding::Key(KeyCode::Enter))
        );
        assert_eq!(rebound.binding_at(VirtualAction::p1_start, 1), None);
    }

    #[test]
    fn no_default_action_replaces_primary_slot() {
        let mapped = updated_keymap_unique_keyboard(
            &default_keymap(),
            VirtualAction::p1_menu_up,
            1,
            KeyCode::KeyI,
        );
        let remapped =
            updated_keymap_unique_keyboard(&mapped, VirtualAction::p1_menu_up, 1, KeyCode::KeyO);

        assert_eq!(
            remapped.binding_at(VirtualAction::p1_menu_up, 0),
            Some(InputBinding::Key(KeyCode::KeyO))
        );
        assert_eq!(remapped.binding_at(VirtualAction::p1_menu_up, 1), None);
        assert_eq!(
            editable_key_binding_slot_indices(&remapped, VirtualAction::p1_menu_up),
            (0, 1)
        );
        assert_eq!(
            protected_default_key_for_action(&remapped, VirtualAction::p1_menu_up),
            None
        );
    }

    #[test]
    fn default_keymap_binds_system_acceleration_keys() {
        let defaults = default_keymap();

        assert_eq!(
            defaults.binding_at(VirtualAction::system_fast_forward, 0),
            Some(InputBinding::Key(KeyCode::Tab))
        );
        assert_eq!(
            defaults.binding_at(VirtualAction::system_slow_down, 0),
            Some(InputBinding::Key(KeyCode::Backquote))
        );
    }

    #[test]
    fn empty_operator_ini_restores_scroll_lock_default() {
        let keymap = load_keymap_from_ini_entries(Some([("P1_Operator", "")]));

        assert_eq!(
            keymap.binding_at(VirtualAction::p1_operator, 0),
            Some(InputBinding::Key(KeyCode::ScrollLock))
        );
    }

    #[test]
    fn system_acceleration_keys_round_trip_through_ini() {
        let keymap = load_keymap_from_ini_entries(Some([
            ("System_FastForward", "KeyCode::Backquote"),
            ("System_SlowDown", "KeyCode::Tab"),
        ]));

        assert_eq!(
            keymap.binding_at(VirtualAction::system_fast_forward, 0),
            Some(InputBinding::Key(KeyCode::Backquote))
        );
        assert_eq!(
            keymap.binding_at(VirtualAction::system_slow_down, 0),
            Some(InputBinding::Key(KeyCode::Tab))
        );
    }

    #[test]
    fn missing_keymap_section_uses_defaults() {
        let keymap = load_keymap_from_ini_entries::<[(&str, &str); 0]>(None);

        assert_eq!(
            keymap.binding_at(VirtualAction::p1_start, 0),
            Some(InputBinding::Key(KeyCode::Enter))
        );
    }

    #[test]
    fn default_keymap_ini_lines_match_default_keymap() {
        let actual = keymap_ini_lines(&default_keymap());
        let expected: Vec<(&str, String)> = DEFAULT_KEYMAP_INI_LINES
            .iter()
            .map(|(key, value)| (*key, (*value).to_string()))
            .collect();

        assert_eq!(actual, expected);
    }

    #[test]
    fn keymap_ini_section_writes_header_and_blank_tail() {
        let mut content = String::new();
        write_keymap_ini_section(&mut content, &default_keymap());

        assert!(content.starts_with("[Keymaps]\nP1_Back=KeyCode::Escape\n"));
        assert!(content.ends_with("System_SlowDown=KeyCode::Backquote\n\n"));
    }

    #[test]
    fn default_keymap_ini_section_uses_static_defaults() {
        let mut content = String::new();
        write_default_keymap_ini_section(&mut content);

        assert!(content.starts_with("[Keymaps]\nP1_Back=KeyCode::Escape\n"));
        assert!(content.contains("P1_MenuDown=\n"));
        assert!(content.ends_with("System_SlowDown=KeyCode::Backquote\n\n"));
    }

    #[test]
    fn parse_keycode_common_keys() {
        let cases = [
            ("KeyCode::Enter", KeyCode::Enter),
            ("KeyCode::Escape", KeyCode::Escape),
            ("KeyCode::ArrowUp", KeyCode::ArrowUp),
            ("KeyCode::ArrowDown", KeyCode::ArrowDown),
            ("KeyCode::ArrowLeft", KeyCode::ArrowLeft),
            ("KeyCode::ArrowRight", KeyCode::ArrowRight),
            ("KeyCode::Slash", KeyCode::Slash),
            ("KeyCode::KeyA", KeyCode::KeyA),
            ("KeyCode::KeyZ", KeyCode::KeyZ),
            ("KeyCode::Numpad0", KeyCode::Numpad0),
            ("KeyCode::Numpad9", KeyCode::Numpad9),
            ("KeyCode::NumpadEnter", KeyCode::NumpadEnter),
            ("KeyCode::NumpadDecimal", KeyCode::NumpadDecimal),
        ];
        for (token, expected) in cases {
            assert_eq!(
                parse_keycode(token),
                Some(InputBinding::Key(expected)),
                "failed for {token}"
            );
        }
    }

    #[test]
    fn parse_keycode_previously_missing_keys() {
        let cases = [
            ("KeyCode::Period", KeyCode::Period),
            ("KeyCode::AltLeft", KeyCode::AltLeft),
            ("KeyCode::AltRight", KeyCode::AltRight),
            ("KeyCode::ControlLeft", KeyCode::ControlLeft),
            ("KeyCode::ControlRight", KeyCode::ControlRight),
            ("KeyCode::ShiftLeft", KeyCode::ShiftLeft),
            ("KeyCode::ShiftRight", KeyCode::ShiftRight),
            ("KeyCode::Space", KeyCode::Space),
            ("KeyCode::Tab", KeyCode::Tab),
            ("KeyCode::Backspace", KeyCode::Backspace),
            ("KeyCode::CapsLock", KeyCode::CapsLock),
            ("KeyCode::Delete", KeyCode::Delete),
            ("KeyCode::Home", KeyCode::Home),
            ("KeyCode::End", KeyCode::End),
            ("KeyCode::PageUp", KeyCode::PageUp),
            ("KeyCode::PageDown", KeyCode::PageDown),
            ("KeyCode::Insert", KeyCode::Insert),
            ("KeyCode::F1", KeyCode::F1),
            ("KeyCode::F12", KeyCode::F12),
            ("KeyCode::PrintScreen", KeyCode::PrintScreen),
            ("KeyCode::Comma", KeyCode::Comma),
            ("KeyCode::Minus", KeyCode::Minus),
            ("KeyCode::Equal", KeyCode::Equal),
            ("KeyCode::BracketLeft", KeyCode::BracketLeft),
            ("KeyCode::Backquote", KeyCode::Backquote),
            ("KeyCode::Digit0", KeyCode::Digit0),
            ("KeyCode::Digit9", KeyCode::Digit9),
            ("KeyCode::NumLock", KeyCode::NumLock),
            ("KeyCode::ScrollLock", KeyCode::ScrollLock),
            ("KeyCode::Pause", KeyCode::Pause),
            ("KeyCode::ContextMenu", KeyCode::ContextMenu),
            ("KeyCode::SuperLeft", KeyCode::SuperLeft),
            ("KeyCode::AudioVolumeMute", KeyCode::AudioVolumeMute),
            ("KeyCode::F35", KeyCode::F35),
        ];
        for (token, expected) in cases {
            assert_eq!(
                parse_keycode(token),
                Some(InputBinding::Key(expected)),
                "failed for {token}"
            );
        }
    }

    #[test]
    fn parse_keycode_rejects_invalid() {
        assert_eq!(parse_keycode("KeyCode::NotAKey"), None);
        assert_eq!(parse_keycode("KeyCode::"), None);
        assert_eq!(parse_keycode("NotKeyCode::Enter"), None);
        assert_eq!(parse_keycode("Enter"), None);
        assert_eq!(parse_keycode(""), None);
    }

    #[test]
    fn parse_pad_dir_valid() {
        assert_eq!(parse_pad_dir("Up"), Some(PadDir::Up));
        assert_eq!(parse_pad_dir("Down"), Some(PadDir::Down));
        assert_eq!(parse_pad_dir("Left"), Some(PadDir::Left));
        assert_eq!(parse_pad_dir("Right"), Some(PadDir::Right));
    }

    #[test]
    fn parse_pad_dir_invalid() {
        assert_eq!(parse_pad_dir("up"), None);
        assert_eq!(parse_pad_dir(""), None);
        assert_eq!(parse_pad_dir("UpDown"), None);
    }

    #[test]
    fn parse_pad_dir_binding_short_form() {
        assert_eq!(
            parse_pad_dir_binding("PadDir::Up"),
            Some(InputBinding::PadDir(PadDir::Up))
        );
        assert_eq!(
            parse_pad_dir_binding("PadDir::Right"),
            Some(InputBinding::PadDir(PadDir::Right))
        );
    }

    #[test]
    fn parse_pad_device_binding_any_pad_long_form() {
        assert_eq!(
            parse_pad_device_binding("Pad::Dir::Down"),
            Some(InputBinding::PadDir(PadDir::Down))
        );
    }

    #[test]
    fn parse_pad_device_binding_device_specific() {
        assert_eq!(
            parse_pad_device_binding("Pad0::Dir::Up"),
            Some(InputBinding::PadDirOn {
                device: 0,
                dir: PadDir::Up,
            })
        );
        assert_eq!(
            parse_pad_device_binding("Pad3::Dir::Left"),
            Some(InputBinding::PadDirOn {
                device: 3,
                dir: PadDir::Left,
            })
        );
    }

    #[test]
    fn parse_pad_dir_binding_rejects_invalid() {
        assert_eq!(parse_pad_dir_binding("PadDir::Diagonal"), None);
        assert_eq!(parse_pad_dir_binding("Pad0::Btn::A"), None);
        assert_eq!(parse_pad_dir_binding("Pad0::Dir"), None);
        assert_eq!(parse_pad_dir_binding("NotPad::Dir::Up"), None);
    }

    #[test]
    fn parse_pad_code_hex_only() {
        assert_eq!(
            parse_pad_code("PadCode[0xDEADBEEF]"),
            Some(InputBinding::GamepadCode(GamepadCodeBinding {
                code_u32: 0xDEADBEEF,
                device: None,
                uuid: None,
            }))
        );
    }

    #[test]
    fn parse_pad_code_decimal() {
        assert_eq!(
            parse_pad_code("PadCode[42]"),
            Some(InputBinding::GamepadCode(GamepadCodeBinding {
                code_u32: 42,
                device: None,
                uuid: None,
            }))
        );
    }

    #[test]
    fn parse_pad_code_with_device() {
        assert_eq!(
            parse_pad_code("PadCode[0x00000001]@2"),
            Some(InputBinding::GamepadCode(GamepadCodeBinding {
                code_u32: 1,
                device: Some(2),
                uuid: None,
            }))
        );
    }

    #[test]
    fn parse_pad_code_with_uuid() {
        let uuid_hex = "00112233AABBCCDDEEFF001122334455";
        let token = format!("PadCode[0xFF]#{uuid_hex}");
        let expected_uuid = [
            0x00, 0x11, 0x22, 0x33, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x00, 0x11, 0x22, 0x33,
            0x44, 0x55,
        ];
        assert_eq!(
            parse_pad_code(&token),
            Some(InputBinding::GamepadCode(GamepadCodeBinding {
                code_u32: 0xFF,
                device: None,
                uuid: Some(expected_uuid),
            }))
        );
    }

    #[test]
    fn parse_pad_code_with_device_and_uuid() {
        let token = "PadCode[0xDEADBEEF]@0#00112233AABBCCDDEEFF001122334455";
        let Some(InputBinding::GamepadCode(binding)) = parse_pad_code(token) else {
            panic!("expected gamepad code binding");
        };
        assert_eq!(binding.code_u32, 0xDEADBEEF);
        assert_eq!(binding.device, Some(0));
        assert!(binding.uuid.is_some());
    }

    #[test]
    fn parse_pad_code_rejects_invalid() {
        assert_eq!(parse_pad_code("PadCode[]"), None);
        assert_eq!(parse_pad_code("PadCode[xyz]"), None);
        assert_eq!(parse_pad_code("NotPadCode[0x01]"), None);
        assert_eq!(parse_pad_code(""), None);
    }

    #[test]
    fn parse_binding_token_dispatches_keycode() {
        assert_eq!(
            parse_binding_token("KeyCode::Period"),
            Some(InputBinding::Key(KeyCode::Period))
        );
    }

    #[test]
    fn parse_binding_token_dispatches_pad_dir() {
        assert_eq!(
            parse_binding_token("PadDir::Up"),
            Some(InputBinding::PadDir(PadDir::Up))
        );
    }

    #[test]
    fn parse_binding_token_dispatches_pad_device() {
        assert_eq!(
            parse_binding_token("Pad0::Dir::Left"),
            Some(InputBinding::PadDirOn {
                device: 0,
                dir: PadDir::Left,
            })
        );
    }

    #[test]
    fn parse_binding_token_dispatches_pad_code() {
        assert_eq!(
            parse_binding_token("PadCode[0x42]"),
            Some(InputBinding::GamepadCode(GamepadCodeBinding {
                code_u32: 0x42,
                device: None,
                uuid: None,
            }))
        );
    }

    #[test]
    fn parse_binding_token_trims_whitespace() {
        assert_eq!(
            parse_binding_token("  KeyCode::Enter  "),
            Some(InputBinding::Key(KeyCode::Enter))
        );
    }

    #[test]
    fn parse_binding_token_rejects_garbage() {
        assert_eq!(parse_binding_token("garbage"), None);
        assert_eq!(parse_binding_token(""), None);
    }

    #[test]
    fn round_trip_keyboard_bindings() {
        let keys = [
            KeyCode::Enter,
            KeyCode::Escape,
            KeyCode::Period,
            KeyCode::AltLeft,
            KeyCode::AltRight,
            KeyCode::Space,
            KeyCode::Tab,
            KeyCode::Backspace,
            KeyCode::ArrowUp,
            KeyCode::KeyA,
            KeyCode::KeyZ,
            KeyCode::Digit0,
            KeyCode::Digit9,
            KeyCode::Numpad0,
            KeyCode::Numpad2,
            KeyCode::NumpadEnter,
            KeyCode::NumpadDecimal,
            KeyCode::F1,
            KeyCode::F12,
            KeyCode::F35,
            KeyCode::ControlLeft,
            KeyCode::ShiftRight,
            KeyCode::SuperLeft,
            KeyCode::PrintScreen,
            KeyCode::Comma,
            KeyCode::Minus,
            KeyCode::Slash,
            KeyCode::Backquote,
            KeyCode::BracketLeft,
            KeyCode::AudioVolumeMute,
        ];
        for key in keys {
            let binding = InputBinding::Key(key);
            let token = binding_to_token(binding);
            assert_eq!(
                parse_binding_token(&token),
                Some(binding),
                "round-trip failed for {key:?}: token was {token:?}"
            );
        }
    }

    #[test]
    fn round_trip_pad_dir() {
        for dir in [PadDir::Up, PadDir::Down, PadDir::Left, PadDir::Right] {
            let binding = InputBinding::PadDir(dir);
            let token = binding_to_token(binding);
            assert_eq!(
                parse_binding_token(&token),
                Some(binding),
                "round-trip failed for {dir:?}"
            );
        }
    }

    #[test]
    fn round_trip_pad_dir_on() {
        for device in [0, 1, 5] {
            for dir in [PadDir::Up, PadDir::Down, PadDir::Left, PadDir::Right] {
                let binding = InputBinding::PadDirOn { device, dir };
                let token = binding_to_token(binding);
                assert_eq!(
                    parse_binding_token(&token),
                    Some(binding),
                    "round-trip failed for device={device}, dir={dir:?}"
                );
            }
        }
    }

    #[test]
    fn round_trip_gamepad_code() {
        let cases = [
            GamepadCodeBinding {
                code_u32: 0xDEADBEEF,
                device: None,
                uuid: None,
            },
            GamepadCodeBinding {
                code_u32: 42,
                device: Some(0),
                uuid: None,
            },
            GamepadCodeBinding {
                code_u32: 0xFF,
                device: None,
                uuid: Some([
                    0x00, 0x11, 0x22, 0x33, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x00, 0x11, 0x22,
                    0x33, 0x44, 0x55,
                ]),
            },
            GamepadCodeBinding {
                code_u32: 0x01,
                device: Some(3),
                uuid: Some([0xAB; 16]),
            },
        ];
        for binding in cases {
            let input = InputBinding::GamepadCode(binding);
            let token = binding_to_token(input);
            assert_eq!(
                parse_binding_token(&token),
                Some(input),
                "round-trip failed for {binding:?}: token was {token:?}"
            );
        }
    }

    #[test]
    fn gamepad_binding_token_round_trips() {
        let binding = InputBinding::GamepadCode(GamepadCodeBinding {
            code_u32: 0x01,
            device: Some(3),
            uuid: Some([0xAB; 16]),
        });
        let token = binding_to_token(binding);
        assert_eq!(parse_binding_token(&token), Some(binding));
    }

    #[test]
    fn pad_dir_binding_token_round_trips() {
        let binding = InputBinding::PadDirOn {
            device: 2,
            dir: PadDir::Left,
        };
        let token = binding_to_token(binding);
        assert_eq!(parse_binding_token(&token), Some(binding));
    }
}
