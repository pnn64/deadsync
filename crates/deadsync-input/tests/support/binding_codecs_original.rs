//! Frozen production reference from dd91f5bf9.
use deadlib_platform::input::KeyCode;
use deadsync_input::{
    ALL_VIRTUAL_ACTIONS, GamepadCodeBinding, InputBinding, Keymap, VirtualAction, parse_keycode,
    parse_pad_dir_binding, restore_available_default_bindings,
};

const fn default_bindings_for_action(action: VirtualAction) -> &'static [InputBinding] {
    use InputBinding::Key;
    use KeyCode::*;
    use VirtualAction as A;
    match action {
        A::p1_up => &[Key(ArrowUp), Key(KeyE)],
        A::p1_down => &[Key(ArrowDown), Key(KeyQ)],
        A::p1_left => &[Key(ArrowLeft), Key(KeyZ)],
        A::p1_right => &[Key(ArrowRight), Key(KeyC)],
        A::p1_center => &[Key(Space), Key(KeyS)],
        A::p1_select => &[Key(Slash)],
        A::p1_start => &[Key(Enter)],
        A::p1_back => &[Key(Escape)],
        A::p1_operator => &[Key(ScrollLock)],
        A::p1_coin => &[Key(F1)],
        A::p2_up => &[Key(Numpad8)],
        A::p2_down => &[Key(Numpad2)],
        A::p2_left => &[Key(Numpad4)],
        A::p2_right => &[Key(Numpad6)],
        A::p2_center => &[Key(Numpad5)],
        A::p2_select => &[Key(NumpadDecimal)],
        A::p2_start => &[Key(NumpadEnter)],
        A::p2_back => &[Key(Numpad0)],
        A::system_fast_forward => &[Key(Tab)],
        A::system_slow_down => &[Key(Backquote)],
        _ => &[],
    }
}

fn default_keymap() -> Keymap {
    let mut km = Keymap::default();
    for action in ALL_VIRTUAL_ACTIONS {
        let bindings = default_bindings_for_action(action);
        if !bindings.is_empty() {
            km.bind(action, bindings);
        }
    }
    km
}

pub(super) fn load_keymap_from_ini_entries<'a, I>(section: Option<I>) -> Keymap
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
        if let Some(action) = action_from_ini_key(key) {
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

    for action in ALL_VIRTUAL_ACTIONS {
        if seen & action.bit() != 0 {
            continue;
        }
        let bindings = default_bindings_for_action(action);
        if !bindings.is_empty() {
            km.bind(action, bindings);
        }
    }
    restore_available_default_bindings(&mut km);

    km
}

pub(super) fn gamepad_code_binding_to_token(binding: GamepadCodeBinding) -> String {
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
    write_gamepad_code_binding_token(&mut s, binding);
    s
}

fn write_gamepad_code_binding_token(s: &mut String, binding: GamepadCodeBinding) {
    use std::fmt::Write;
    let _ = write!(s, "PadCode[0x{:08X}]", binding.code_u32);
    if let Some(device) = binding.device {
        let _ = write!(s, "@{device}");
    }
    if let Some(uuid) = binding.uuid {
        s.push('#');
        for b in &uuid {
            let _ = write!(s, "{b:02X}");
        }
    }
}

pub(super) fn parse_gamepad_code_binding(t: &str) -> Option<GamepadCodeBinding> {
    let rest = t.strip_prefix("PadCode[")?;
    let end = rest.find(']')?;
    let code_str = &rest[..end];
    let mut tail = &rest[end + 1..];

    let code_u32 = if let Some(hex) = code_str
        .strip_prefix("0x")
        .or_else(|| code_str.strip_prefix("0X"))
    {
        u32::from_str_radix(hex, 16).ok()?
    } else {
        code_str.parse::<u32>().ok()?
    };

    let mut device = None;
    let mut uuid = None;
    loop {
        if let Some(rest) = tail.strip_prefix('@') {
            let digits_len = rest.bytes().take_while(u8::is_ascii_digit).count();
            if digits_len == 0 {
                break;
            }
            if let Ok(dev_idx) = rest[..digits_len].parse::<usize>() {
                device = Some(dev_idx);
            }
            tail = &rest[digits_len..];
            continue;
        }
        if let Some(rest) = tail.strip_prefix('#') {
            let hex_len = rest.bytes().take_while(u8::is_ascii_hexdigit).count();
            if hex_len == 32 {
                let mut bytes = [0u8; 16];
                let mut ok = true;
                for (i, byte) in bytes.iter_mut().enumerate() {
                    let start = i * 2;
                    let end = start + 2;
                    if let Ok(parsed) = u8::from_str_radix(&rest[start..end], 16) {
                        *byte = parsed;
                    } else {
                        ok = false;
                        break;
                    }
                }
                if ok {
                    uuid = Some(bytes);
                }
            }
            tail = &rest[hex_len..];
            continue;
        }
        break;
    }

    Some(GamepadCodeBinding {
        code_u32,
        device,
        uuid,
    })
}

fn parse_pad_code(t: &str) -> Option<InputBinding> {
    parse_gamepad_code_binding(t).map(InputBinding::GamepadCode)
}

fn parse_binding_token(tok: &str) -> Option<InputBinding> {
    let t = tok.trim();
    parse_keycode(t)
        .or_else(|| parse_pad_code(t))
        .or_else(|| parse_pad_dir_binding(t))
}

fn action_from_ini_key_lower(key: &str) -> Option<VirtualAction> {
    use VirtualAction::{
        p1_back, p1_down, p1_left, p1_menu_down, p1_menu_left, p1_menu_right, p1_menu_up,
        p1_operator, p1_restart, p1_right, p1_select, p1_start, p1_up, p2_back, p2_down, p2_left,
        p2_menu_down, p2_menu_left, p2_menu_right, p2_menu_up, p2_operator, p2_restart, p2_right,
        p2_select, p2_start, p2_up,
    };
    match key {
        "p1_up" => Some(p1_up),
        "p1_down" => Some(p1_down),
        "p1_left" => Some(p1_left),
        "p1_right" => Some(p1_right),
        "p1_start" => Some(p1_start),
        "p1_back" => Some(p1_back),
        "p1_menuup" => Some(p1_menu_up),
        "p1_menudown" => Some(p1_menu_down),
        "p1_menuleft" => Some(p1_menu_left),
        "p1_menuright" => Some(p1_menu_right),
        "p1_select" => Some(p1_select),
        "p1_operator" => Some(p1_operator),
        "p1_restart" => Some(p1_restart),
        "p2_up" => Some(p2_up),
        "p2_down" => Some(p2_down),
        "p2_left" => Some(p2_left),
        "p2_right" => Some(p2_right),
        "p2_start" => Some(p2_start),
        "p2_back" => Some(p2_back),
        "p2_menuup" => Some(p2_menu_up),
        "p2_menudown" => Some(p2_menu_down),
        "p2_menuleft" => Some(p2_menu_left),
        "p2_menuright" => Some(p2_menu_right),
        "p2_select" => Some(p2_select),
        "p2_operator" => Some(p2_operator),
        "p2_restart" => Some(p2_restart),
        "p1_center" => Some(VirtualAction::p1_center),
        "p2_center" => Some(VirtualAction::p2_center),
        "p1_coin" => Some(VirtualAction::p1_coin),
        "p2_coin" => Some(VirtualAction::p2_coin),
        "system_fastforward" => Some(VirtualAction::system_fast_forward),
        "system_slowdown" => Some(VirtualAction::system_slow_down),
        _ => None,
    }
}

const INI_ACTION_KEY_MAX_BYTES: usize = "system_fastforward".len();

fn action_from_ini_key(key: &str) -> Option<VirtualAction> {
    if key.len() > INI_ACTION_KEY_MAX_BYTES {
        return None;
    }
    let mut normalized = [0u8; INI_ACTION_KEY_MAX_BYTES];
    normalized[..key.len()].copy_from_slice(key.as_bytes());
    normalized[..key.len()].make_ascii_lowercase();
    let normalized =
        std::str::from_utf8(&normalized[..key.len()]).expect("ASCII case folding preserves UTF-8");
    action_from_ini_key_lower(normalized)
}
