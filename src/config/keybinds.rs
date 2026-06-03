use super::{SimpleIni, save_without_keymaps};
use crate::engine::input::{InputBinding, Keymap};
pub(crate) use deadsync_input::{ALL_VIRTUAL_ACTIONS, action_to_ini_key, parse_pad_dir};
use deadsync_input::{
    VirtualAction, action_from_ini_key_lower, gamepad_code_binding_to_token,
    parse_gamepad_code_binding,
};
use winit::keyboard::KeyCode;

fn default_keymap_local() -> Keymap {
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
    // Leave dedicated menu buttons, P2 operator, and restart unbound by default for now.
    km
}

#[inline(always)]
const fn default_key_for_action(action: VirtualAction) -> Option<KeyCode> {
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
        _ => None,
    }
}

#[inline(always)]
fn default_binding_for_action(action: VirtualAction) -> Option<InputBinding> {
    default_key_for_action(action).map(InputBinding::Key)
}

#[inline(always)]
pub(crate) fn binding_to_token(binding: InputBinding) -> String {
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
pub(crate) fn parse_keycode(t: &str) -> Option<InputBinding> {
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
pub(crate) fn keycode_to_token(code: KeyCode) -> String {
    binding_to_token(InputBinding::Key(code))
}

/// Parse a `KeyCode::Name` INI token into a bare `KeyCode`, ignoring any
/// non-keyboard binding tokens (pad/gamepad).
#[inline(always)]
pub(crate) fn parse_keycode_to_key(t: &str) -> Option<KeyCode> {
    match parse_keycode(t)? {
        InputBinding::Key(code) => Some(code),
        _ => None,
    }
}

#[inline(always)]
pub(crate) fn parse_pad_code(t: &str) -> Option<InputBinding> {
    parse_gamepad_code_binding(t).map(InputBinding::GamepadCode)
}

#[inline(always)]
pub(crate) fn parse_pad_device_binding(t: &str) -> Option<InputBinding> {
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
pub(crate) fn parse_pad_dir_binding(t: &str) -> Option<InputBinding> {
    t.strip_prefix("PadDir::")
        .and_then(parse_pad_dir)
        .map(InputBinding::PadDir)
        .or_else(|| parse_pad_device_binding(t))
}

#[inline(always)]
pub(crate) fn parse_binding_token(tok: &str) -> Option<InputBinding> {
    let t = tok.trim();
    parse_keycode(t)
        .or_else(|| parse_pad_code(t))
        .or_else(|| parse_pad_dir_binding(t))
}

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
pub(crate) fn editable_key_binding_slot_indices(
    keymap: &Keymap,
    action: VirtualAction,
) -> (usize, usize) {
    if keymap.binding_at(action, 0) == default_binding_for_action(action) {
        (1, 2)
    } else {
        (0, 1)
    }
}

#[inline(always)]
pub(crate) fn protected_default_key_for_action(
    keymap: &Keymap,
    action: VirtualAction,
) -> Option<KeyCode> {
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
fn restore_available_default_bindings(keymap: &mut Keymap) {
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

fn updated_keymap_unique_keyboard(
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

fn updated_keymap_unique_gamepad(
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

fn cleared_keymap(current: &Keymap, action: VirtualAction, index: usize) -> (Keymap, bool) {
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

/// Update a keyboard binding in Primary/Secondary slots, ensuring that the
/// given key code is not used anywhere else in the keymap.
pub fn update_keymap_binding_unique_keyboard(
    action: VirtualAction,
    index: usize,
    keycode: KeyCode,
) {
    let current = crate::engine::input::get_keymap();
    let new_map = updated_keymap_unique_keyboard(&current, action, index, keycode);
    crate::engine::input::set_keymap(new_map);
    save_without_keymaps();
}

/// Update a gamepad binding in Primary/Secondary slots, ensuring that the
/// given physical binding is not used anywhere else in the keymap.
pub fn update_keymap_binding_unique_gamepad(
    action: VirtualAction,
    index: usize,
    binding: InputBinding,
) {
    let current = crate::engine::input::get_keymap();
    let new_map = updated_keymap_unique_gamepad(&current, action, index, binding);
    crate::engine::input::set_keymap(new_map);
    save_without_keymaps();
}

/// Clear the requested Primary/Secondary binding slot for an action.
/// Returns `true` when a binding was removed.
pub fn clear_keymap_binding(action: VirtualAction, index: usize) -> bool {
    let current = crate::engine::input::get_keymap();
    let (new_map, changed) = cleared_keymap(&current, action, index);

    if changed {
        crate::engine::input::set_keymap(new_map);
        save_without_keymaps();
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_operator_key_is_scroll_lock() {
        let defaults = default_keymap_local();

        assert_eq!(
            defaults.binding_at(VirtualAction::p1_operator, 0),
            Some(InputBinding::Key(KeyCode::ScrollLock))
        );
        assert_eq!(defaults.binding_at(VirtualAction::p2_operator, 0), None);
    }

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
    fn replacing_stolen_default_restores_original_action() {
        let moved = updated_keymap_unique_keyboard(
            &default_keymap_local(),
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
            &default_keymap_local(),
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
            &default_keymap_local(),
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
            &default_keymap_local(),
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
            &default_keymap_local(),
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
}
