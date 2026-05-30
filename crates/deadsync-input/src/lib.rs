use std::time::Instant;

pub use deadsync_core::input::{InputSource, Lane};
use deadsync_core::song_time::SongTimeNs;

pub mod debounce;

pub const INPUT_SLOT_INVALID: u32 = u32::MAX;
pub const INPUT_DEBOUNCE_MIN_SECONDS: f32 = 0.0;
pub const INPUT_DEBOUNCE_MAX_SECONDS: f32 = 0.2;

#[inline(always)]
pub fn clamp_input_debounce_seconds(seconds: f32) -> f32 {
    seconds.clamp(INPUT_DEBOUNCE_MIN_SECONDS, INPUT_DEBOUNCE_MAX_SECONDS)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PadId(pub u32);

impl From<PadId> for usize {
    #[inline(always)]
    fn from(value: PadId) -> Self {
        value.0 as Self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PadCode(pub u32);

impl PadCode {
    #[inline(always)]
    pub const fn into_u32(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PadDir {
    Up,
    Down,
    Left,
    Right,
}

impl PadDir {
    #[inline(always)]
    pub const fn ix(self) -> usize {
        match self {
            Self::Up => 0,
            Self::Down => 1,
            Self::Left => 2,
            Self::Right => 3,
        }
    }
}

#[inline(always)]
pub fn parse_pad_dir(name: &str) -> Option<PadDir> {
    match name {
        "Up" => Some(PadDir::Up),
        "Down" => Some(PadDir::Down),
        "Left" => Some(PadDir::Left),
        "Right" => Some(PadDir::Right),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug)]
pub enum PadEvent {
    Dir {
        id: PadId,
        timestamp: Instant,
        host_nanos: u64,
        dir: PadDir,
        pressed: bool,
    },
    /// Raw low-level button event with platform-specific code and device UUID.
    RawButton {
        id: PadId,
        timestamp: Instant,
        host_nanos: u64,
        code: PadCode,
        uuid: [u8; 16],
        value: f32,
        pressed: bool,
    },
    /// Raw low-level axis event with platform-specific code and device UUID.
    #[cfg_attr(windows, allow(dead_code))]
    RawAxis {
        id: PadId,
        timestamp: Instant,
        host_nanos: u64,
        code: PadCode,
        uuid: [u8; 16],
        value: f32,
    },
}

/// Low-level gamepad binding to a platform-specific element code.
///
/// - `code_u32` is the emitted `PadCode(u32)` from `PadEvent::RawButton`.
/// - `device` is an optional runtime `PadId` index from `usize::from(id)`.
/// - `uuid` is an optional per-device stable identifier produced by the backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GamepadCodeBinding {
    pub code_u32: u32,
    pub device: Option<usize>,
    pub uuid: Option<[u8; 16]>,
}

pub fn gamepad_code_binding_to_token(binding: GamepadCodeBinding) -> String {
    let mut s = String::new();
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

pub fn parse_gamepad_code_binding(t: &str) -> Option<GamepadCodeBinding> {
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

#[derive(Clone, Copy, Debug)]
pub struct InputEdge {
    pub lane: Lane,
    pub input_slot: u32,
    pub pressed: bool,
    pub source: InputSource,
    pub record_replay: bool,
    // Real-time timestamps for latency tracing. Filled in by gameplay when the
    // edge is accepted for lane processing.
    pub captured_at: Instant,
    pub captured_host_nanos: u64,
    pub stored_at: Instant,
    pub emitted_at: Instant,
    pub queued_at: Instant,
    // Integer song time for this edge, in nanoseconds. Live input may leave this
    // invalid until gameplay resolves the physical timestamp against the frame's
    // song-clock snapshot.
    pub event_music_time_ns: SongTimeNs,
}

#[allow(non_camel_case_types)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum VirtualAction {
    p1_up,
    p1_down,
    p1_left,
    p1_right,
    p1_start,
    p1_back,
    p1_menu_up,
    p1_menu_down,
    p1_menu_left,
    p1_menu_right,
    p1_select,
    p1_operator,
    p1_restart,
    p2_up,
    p2_down,
    p2_left,
    p2_right,
    p2_start,
    p2_back,
    p2_menu_up,
    p2_menu_down,
    p2_menu_left,
    p2_menu_right,
    p2_select,
    p2_operator,
    p2_restart,
}

impl VirtualAction {
    pub const COUNT: usize = Self::p2_restart as usize + 1;

    #[inline(always)]
    pub const fn from_ix(ix: usize) -> Option<Self> {
        match ix {
            0 => Some(Self::p1_up),
            1 => Some(Self::p1_down),
            2 => Some(Self::p1_left),
            3 => Some(Self::p1_right),
            4 => Some(Self::p1_start),
            5 => Some(Self::p1_back),
            6 => Some(Self::p1_menu_up),
            7 => Some(Self::p1_menu_down),
            8 => Some(Self::p1_menu_left),
            9 => Some(Self::p1_menu_right),
            10 => Some(Self::p1_select),
            11 => Some(Self::p1_operator),
            12 => Some(Self::p1_restart),
            13 => Some(Self::p2_up),
            14 => Some(Self::p2_down),
            15 => Some(Self::p2_left),
            16 => Some(Self::p2_right),
            17 => Some(Self::p2_start),
            18 => Some(Self::p2_back),
            19 => Some(Self::p2_menu_up),
            20 => Some(Self::p2_menu_down),
            21 => Some(Self::p2_menu_left),
            22 => Some(Self::p2_menu_right),
            23 => Some(Self::p2_select),
            24 => Some(Self::p2_operator),
            25 => Some(Self::p2_restart),
            _ => None,
        }
    }

    #[inline(always)]
    pub const fn ix(self) -> usize {
        self as usize
    }

    #[inline(always)]
    pub const fn bit(self) -> u32 {
        1u32 << (self.ix() as u32)
    }

    #[inline(always)]
    pub const fn is_gameplay_arrow(self) -> bool {
        matches!(
            self,
            Self::p1_up
                | Self::p1_down
                | Self::p1_left
                | Self::p1_right
                | Self::p2_up
                | Self::p2_down
                | Self::p2_left
                | Self::p2_right
        )
    }

    #[inline(always)]
    pub const fn secondary_menu(self) -> Option<Self> {
        match self {
            Self::p1_up => Some(Self::p1_menu_up),
            Self::p1_down => Some(Self::p1_menu_down),
            Self::p1_left => Some(Self::p1_menu_left),
            Self::p1_right => Some(Self::p1_menu_right),
            Self::p2_up => Some(Self::p2_menu_up),
            Self::p2_down => Some(Self::p2_menu_down),
            Self::p2_left => Some(Self::p2_menu_left),
            Self::p2_right => Some(Self::p2_menu_right),
            _ => None,
        }
    }

    #[inline(always)]
    pub const fn primary_from_menu_alias(self) -> Option<Self> {
        match self {
            Self::p1_menu_up => Some(Self::p1_up),
            Self::p1_menu_down => Some(Self::p1_down),
            Self::p1_menu_left => Some(Self::p1_left),
            Self::p1_menu_right => Some(Self::p1_right),
            Self::p2_menu_up => Some(Self::p2_up),
            Self::p2_menu_down => Some(Self::p2_down),
            Self::p2_menu_left => Some(Self::p2_left),
            Self::p2_menu_right => Some(Self::p2_right),
            _ => None,
        }
    }
}

pub const ALL_VIRTUAL_ACTIONS: [VirtualAction; VirtualAction::COUNT] = [
    VirtualAction::p1_back,
    VirtualAction::p1_down,
    VirtualAction::p1_left,
    VirtualAction::p1_menu_down,
    VirtualAction::p1_menu_left,
    VirtualAction::p1_menu_right,
    VirtualAction::p1_menu_up,
    VirtualAction::p1_operator,
    VirtualAction::p1_restart,
    VirtualAction::p1_right,
    VirtualAction::p1_select,
    VirtualAction::p1_start,
    VirtualAction::p1_up,
    VirtualAction::p2_back,
    VirtualAction::p2_down,
    VirtualAction::p2_left,
    VirtualAction::p2_menu_down,
    VirtualAction::p2_menu_left,
    VirtualAction::p2_menu_right,
    VirtualAction::p2_menu_up,
    VirtualAction::p2_operator,
    VirtualAction::p2_restart,
    VirtualAction::p2_right,
    VirtualAction::p2_select,
    VirtualAction::p2_start,
    VirtualAction::p2_up,
];

#[inline(always)]
pub fn action_from_ini_key_lower(key: &str) -> Option<VirtualAction> {
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
        _ => None,
    }
}

#[inline(always)]
pub const fn action_to_ini_key(action: VirtualAction) -> &'static str {
    use VirtualAction::{
        p1_back, p1_down, p1_left, p1_menu_down, p1_menu_left, p1_menu_right, p1_menu_up,
        p1_operator, p1_restart, p1_right, p1_select, p1_start, p1_up, p2_back, p2_down, p2_left,
        p2_menu_down, p2_menu_left, p2_menu_right, p2_menu_up, p2_operator, p2_restart, p2_right,
        p2_select, p2_start, p2_up,
    };
    match action {
        p1_up => "P1_Up",
        p1_down => "P1_Down",
        p1_left => "P1_Left",
        p1_right => "P1_Right",
        p1_start => "P1_Start",
        p1_back => "P1_Back",
        p1_menu_up => "P1_MenuUp",
        p1_menu_down => "P1_MenuDown",
        p1_menu_left => "P1_MenuLeft",
        p1_menu_right => "P1_MenuRight",
        p1_select => "P1_Select",
        p1_operator => "P1_Operator",
        p1_restart => "P1_Restart",
        p2_up => "P2_Up",
        p2_down => "P2_Down",
        p2_left => "P2_Left",
        p2_right => "P2_Right",
        p2_start => "P2_Start",
        p2_back => "P2_Back",
        p2_menu_up => "P2_MenuUp",
        p2_menu_down => "P2_MenuDown",
        p2_menu_left => "P2_MenuLeft",
        p2_menu_right => "P2_MenuRight",
        p2_select => "P2_Select",
        p2_operator => "P2_Operator",
        p2_restart => "P2_Restart",
    }
}

#[inline(always)]
pub fn for_each_action(mut mask: u32, mut f: impl FnMut(VirtualAction)) {
    while mask != 0 {
        let ix = mask.trailing_zeros() as usize;
        if let Some(action) = VirtualAction::from_ix(ix) {
            f(action);
        }
        mask &= mask - 1;
    }
}

#[inline(always)]
pub fn secondary_menu_mask(mask: u32) -> u32 {
    let mut out = 0;
    for_each_action(mask, |action| {
        if let Some(menu_action) = action.secondary_menu() {
            out |= menu_action.bit();
        }
    });
    out
}

#[inline(always)]
fn emit_normalized_action(
    action: VirtualAction,
    pressed: bool,
    direct_mask: u32,
    emitted: &mut u32,
    emit: &mut impl FnMut(VirtualAction, bool),
) {
    if pressed
        && let Some(primary) = action.primary_from_menu_alias()
        && (direct_mask & primary.bit()) != 0
    {
        return;
    }
    let bit = action.bit();
    if (*emitted & bit) != 0 {
        return;
    }
    *emitted |= bit;
    emit(action, pressed);
}

#[inline(always)]
pub fn emit_normalized_actions(
    direct_mask: u32,
    pressed: bool,
    only_dedicated_menu_buttons: bool,
    mut emit: impl FnMut(VirtualAction, bool),
) {
    if direct_mask == 0 {
        return;
    }
    let mut emitted = 0;
    for_each_action(direct_mask, |action| {
        emit_normalized_action(action, pressed, direct_mask, &mut emitted, &mut emit)
    });
    if only_dedicated_menu_buttons && pressed {
        return;
    }
    for_each_action(secondary_menu_mask(direct_mask), |action| {
        emit_normalized_action(action, pressed, direct_mask, &mut emitted, &mut emit)
    });
}

#[inline(always)]
pub const fn lane_from_action(action: VirtualAction) -> Option<Lane> {
    match action {
        VirtualAction::p1_left => Some(Lane::Left),
        VirtualAction::p1_down => Some(Lane::Down),
        VirtualAction::p1_up => Some(Lane::Up),
        VirtualAction::p1_right => Some(Lane::Right),
        VirtualAction::p2_left => Some(Lane::P2Left),
        VirtualAction::p2_down => Some(Lane::P2Down),
        VirtualAction::p2_up => Some(Lane::P2Up),
        VirtualAction::p2_right => Some(Lane::P2Right),
        _ => None,
    }
}

#[inline(always)]
pub const fn lane_from_column(column: usize) -> Option<Lane> {
    match column {
        0 => Some(Lane::Left),
        1 => Some(Lane::Down),
        2 => Some(Lane::Up),
        3 => Some(Lane::Right),
        4 => Some(Lane::P2Left),
        5 => Some(Lane::P2Down),
        6 => Some(Lane::P2Up),
        7 => Some(Lane::P2Right),
        _ => None,
    }
}

#[inline(always)]
pub const fn pad_dir_from_action(action: VirtualAction) -> Option<PadDir> {
    match action {
        VirtualAction::p1_left | VirtualAction::p2_left => Some(PadDir::Left),
        VirtualAction::p1_right | VirtualAction::p2_right => Some(PadDir::Right),
        VirtualAction::p1_up | VirtualAction::p2_up => Some(PadDir::Up),
        VirtualAction::p1_down | VirtualAction::p2_down => Some(PadDir::Down),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug)]
pub struct InputEvent {
    pub action: VirtualAction,
    pub input_slot: u32,
    pub pressed: bool,
    pub source: InputSource,
    // Timestamp of the raw input edge before debounce filtering.
    pub timestamp: Instant,
    // Host/QPC clock for `timestamp` when the backend can provide one; 0 means
    // the event only has a local `Instant` anchor.
    pub timestamp_host_nanos: u64,
    // Timestamp at which the edge entered the debounce store on the main input path.
    pub stored_at: Instant,
    // Timestamp at which the debounced/normalized input event was emitted.
    pub emitted_at: Instant,
}

impl InputEvent {
    #[inline(always)]
    pub fn new(
        action: VirtualAction,
        input_slot: u32,
        pressed: bool,
        source: InputSource,
        timestamp: Instant,
        timestamp_host_nanos: u64,
        stored_at: Instant,
        emitted_at: Instant,
    ) -> Self {
        Self {
            action,
            input_slot,
            pressed,
            source,
            timestamp,
            timestamp_host_nanos,
            stored_at,
            emitted_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ALL_VIRTUAL_ACTIONS, GamepadCodeBinding, Lane, PadCode, PadDir, PadEvent, PadId,
        VirtualAction, action_from_ini_key_lower, action_to_ini_key, clamp_input_debounce_seconds,
        emit_normalized_actions, gamepad_code_binding_to_token, lane_from_action, lane_from_column,
        pad_dir_from_action, parse_gamepad_code_binding, parse_pad_dir, secondary_menu_mask,
    };
    use std::time::Instant;

    fn normalized(mask: u32, pressed: bool, dedicated_only: bool) -> Vec<(VirtualAction, bool)> {
        let mut out = Vec::new();
        emit_normalized_actions(mask, pressed, dedicated_only, |action, pressed| {
            out.push((action, pressed));
        });
        out
    }

    #[test]
    fn lane_indices_are_stable() {
        assert_eq!(Lane::Left.index(), 0);
        assert_eq!(Lane::P2Right.index(), 7);
    }

    #[test]
    fn pad_dir_indices_are_stable() {
        assert_eq!(PadDir::Up.ix(), 0);
        assert_eq!(PadDir::Down.ix(), 1);
        assert_eq!(PadDir::Left.ix(), 2);
        assert_eq!(PadDir::Right.ix(), 3);
    }

    #[test]
    fn pad_dir_names_match_config_tokens() {
        assert_eq!(parse_pad_dir("Up"), Some(PadDir::Up));
        assert_eq!(parse_pad_dir("Down"), Some(PadDir::Down));
        assert_eq!(parse_pad_dir("Left"), Some(PadDir::Left));
        assert_eq!(parse_pad_dir("Right"), Some(PadDir::Right));
        assert_eq!(parse_pad_dir("up"), None);
        assert_eq!(parse_pad_dir(""), None);
    }

    #[test]
    fn pad_physical_ids_are_plain_numeric_wrappers() {
        assert_eq!(usize::from(PadId(7)), 7);
        assert_eq!(PadCode(0xDEAD_BEEF).into_u32(), 0xDEAD_BEEF);
    }

    #[test]
    fn input_debounce_clamp_matches_config_range() {
        assert_eq!(clamp_input_debounce_seconds(-1.0), 0.0);
        assert_eq!(clamp_input_debounce_seconds(0.1), 0.1);
        assert_eq!(clamp_input_debounce_seconds(1.0), 0.2);
    }

    #[test]
    fn pad_event_carries_physical_button_data() {
        let timestamp = Instant::now();
        let event = PadEvent::RawButton {
            id: PadId(2),
            timestamp,
            host_nanos: 99,
            code: PadCode(12),
            uuid: [7; 16],
            value: 1.0,
            pressed: true,
        };
        let PadEvent::RawButton {
            id,
            timestamp: event_time,
            host_nanos,
            code,
            uuid,
            value,
            pressed,
        } = event
        else {
            panic!("expected raw button event");
        };
        assert_eq!(id, PadId(2));
        assert_eq!(event_time, timestamp);
        assert_eq!(host_nanos, 99);
        assert_eq!(code.into_u32(), 12);
        assert_eq!(uuid, [7; 16]);
        assert_eq!(value, 1.0);
        assert!(pressed);
    }

    #[test]
    fn gamepad_code_binding_keeps_optional_device_filters() {
        let binding = GamepadCodeBinding {
            code_u32: 42,
            device: Some(3),
            uuid: Some([1; 16]),
        };
        assert_eq!(binding.code_u32, 42);
        assert_eq!(binding.device, Some(3));
        assert_eq!(binding.uuid, Some([1; 16]));
    }

    #[test]
    fn gamepad_code_bindings_parse_config_tokens() {
        assert_eq!(
            parse_gamepad_code_binding("PadCode[42]"),
            Some(GamepadCodeBinding {
                code_u32: 42,
                device: None,
                uuid: None,
            })
        );
        assert_eq!(
            parse_gamepad_code_binding("PadCode[0x00000001]@2"),
            Some(GamepadCodeBinding {
                code_u32: 1,
                device: Some(2),
                uuid: None,
            })
        );
        assert_eq!(
            parse_gamepad_code_binding("PadCode[0xFF]#00112233AABBCCDDEEFF001122334455"),
            Some(GamepadCodeBinding {
                code_u32: 0xFF,
                device: None,
                uuid: Some([
                    0x00, 0x11, 0x22, 0x33, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x00, 0x11, 0x22,
                    0x33, 0x44, 0x55,
                ]),
            })
        );
        assert_eq!(parse_gamepad_code_binding("PadCode[]"), None);
        assert_eq!(parse_gamepad_code_binding("PadCode[xyz]"), None);
        assert_eq!(parse_gamepad_code_binding("NotPadCode[0x01]"), None);
    }

    #[test]
    fn gamepad_code_bindings_round_trip_config_tokens() {
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
        ];
        for binding in cases {
            let token = gamepad_code_binding_to_token(binding);
            assert_eq!(parse_gamepad_code_binding(&token), Some(binding));
        }
    }

    #[test]
    fn gameplay_arrow_and_menu_aliases_match() {
        assert_eq!(ALL_VIRTUAL_ACTIONS.len(), VirtualAction::COUNT);
        assert!(VirtualAction::p1_left.is_gameplay_arrow());
        assert!(!VirtualAction::p1_start.is_gameplay_arrow());
        assert_eq!(VirtualAction::from_ix(0), Some(VirtualAction::p1_up));
        assert_eq!(VirtualAction::from_ix(VirtualAction::COUNT), None);
        assert_eq!(VirtualAction::p2_restart.ix(), VirtualAction::COUNT - 1);
        assert_eq!(
            VirtualAction::p1_left.bit(),
            1 << VirtualAction::p1_left.ix()
        );
        assert_eq!(
            VirtualAction::p2_right.secondary_menu(),
            Some(VirtualAction::p2_menu_right)
        );
        assert_eq!(
            VirtualAction::p2_menu_right.primary_from_menu_alias(),
            Some(VirtualAction::p2_right)
        );
        assert_eq!(VirtualAction::p2_start.secondary_menu(), None);
        assert_eq!(VirtualAction::p2_start.primary_from_menu_alias(), None);
        assert_eq!(
            secondary_menu_mask(VirtualAction::p1_left.bit() | VirtualAction::p2_right.bit()),
            VirtualAction::p1_menu_left.bit() | VirtualAction::p2_menu_right.bit()
        );
    }

    #[test]
    fn virtual_actions_round_trip_through_ini_names() {
        for action in ALL_VIRTUAL_ACTIONS {
            let key = action_to_ini_key(action).to_ascii_lowercase();
            assert_eq!(action_from_ini_key_lower(&key), Some(action));
        }
        assert_eq!(action_from_ini_key_lower("p1_menu_up"), None);
        assert_eq!(action_from_ini_key_lower("p1_coin"), None);
    }

    #[test]
    fn gameplay_actions_map_to_stable_lanes() {
        assert_eq!(lane_from_action(VirtualAction::p1_left), Some(Lane::Left));
        assert_eq!(lane_from_action(VirtualAction::p1_down), Some(Lane::Down));
        assert_eq!(lane_from_action(VirtualAction::p1_up), Some(Lane::Up));
        assert_eq!(lane_from_action(VirtualAction::p1_right), Some(Lane::Right));
        assert_eq!(lane_from_action(VirtualAction::p2_left), Some(Lane::P2Left));
        assert_eq!(lane_from_action(VirtualAction::p2_down), Some(Lane::P2Down));
        assert_eq!(lane_from_action(VirtualAction::p2_up), Some(Lane::P2Up));
        assert_eq!(
            lane_from_action(VirtualAction::p2_right),
            Some(Lane::P2Right)
        );
        assert_eq!(lane_from_action(VirtualAction::p1_start), None);
    }

    #[test]
    fn gameplay_columns_map_to_stable_lanes() {
        assert_eq!(lane_from_column(0), Some(Lane::Left));
        assert_eq!(lane_from_column(1), Some(Lane::Down));
        assert_eq!(lane_from_column(2), Some(Lane::Up));
        assert_eq!(lane_from_column(3), Some(Lane::Right));
        assert_eq!(lane_from_column(4), Some(Lane::P2Left));
        assert_eq!(lane_from_column(5), Some(Lane::P2Down));
        assert_eq!(lane_from_column(6), Some(Lane::P2Up));
        assert_eq!(lane_from_column(7), Some(Lane::P2Right));
        assert_eq!(lane_from_column(8), None);
    }

    #[test]
    fn gameplay_actions_map_to_pad_dirs() {
        assert_eq!(
            pad_dir_from_action(VirtualAction::p1_left),
            Some(PadDir::Left)
        );
        assert_eq!(
            pad_dir_from_action(VirtualAction::p2_left),
            Some(PadDir::Left)
        );
        assert_eq!(
            pad_dir_from_action(VirtualAction::p1_right),
            Some(PadDir::Right)
        );
        assert_eq!(
            pad_dir_from_action(VirtualAction::p2_right),
            Some(PadDir::Right)
        );
        assert_eq!(pad_dir_from_action(VirtualAction::p1_up), Some(PadDir::Up));
        assert_eq!(pad_dir_from_action(VirtualAction::p2_up), Some(PadDir::Up));
        assert_eq!(
            pad_dir_from_action(VirtualAction::p1_down),
            Some(PadDir::Down)
        );
        assert_eq!(
            pad_dir_from_action(VirtualAction::p2_down),
            Some(PadDir::Down)
        );
        assert_eq!(pad_dir_from_action(VirtualAction::p1_start), None);
        assert_eq!(pad_dir_from_action(VirtualAction::p1_menu_left), None);
    }

    #[test]
    fn normalized_actions_emit_menu_aliases_like_engine_input() {
        assert_eq!(
            normalized(VirtualAction::p1_left.bit(), true, false),
            vec![(VirtualAction::p1_left, true)]
        );
        assert_eq!(
            normalized(
                VirtualAction::p1_left.bit() | VirtualAction::p1_menu_left.bit(),
                true,
                false,
            ),
            vec![(VirtualAction::p1_left, true)]
        );
        assert_eq!(
            normalized(VirtualAction::p1_left.bit(), true, true),
            vec![(VirtualAction::p1_left, true)]
        );
        assert_eq!(
            normalized(VirtualAction::p1_left.bit(), false, true),
            vec![
                (VirtualAction::p1_left, false),
                (VirtualAction::p1_menu_left, false),
            ]
        );
    }
}
