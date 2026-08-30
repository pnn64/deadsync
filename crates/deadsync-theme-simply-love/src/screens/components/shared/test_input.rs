use crate::act;
use crate::assets::{FontRole, machine_font_key};
use crate::config::MachineFont;
use deadlib_present::actors::{Actor, TextContent};
use deadlib_present::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use deadsync_config::prelude::GameFlag;
use deadsync_input::KeyCode;
use deadsync_input::RawKeyboardEvent;
use deadsync_input::{InputEvent, PadDir, PadEvent, VirtualAction, with_keymap};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

const UNMAPPED_AXIS_HELD_THRESHOLD: f32 = 0.5;
const SORT_MENU_DIM_ALPHA: f32 = 0.875;
const SORT_MENU_CLOSE_HINT: &str = "Press &START; to dismiss.";
const EVENT_RATE_HISTORY: usize = 64;
const MAX_DISPLAY_HZ: u32 = 1000;

/// Process-lifetime table for the seven immutable bundled Test Input textures.
/// It is initialized synchronously on the first game-thread render, never
/// grows or evicts, and releases its entries only at process teardown. Hot
/// frames clone the retained `Arc<str>` handles instead of allocating keys.
struct TestInputTextureKeys {
    dance: Arc<str>,
    pump: Arc<str>,
    highlight: Arc<str>,
    buttons: Arc<str>,
    highlight_green: Arc<str>,
    highlight_red: Arc<str>,
    highlight_arrow: Arc<str>,
}

fn test_input_texture_keys() -> &'static TestInputTextureKeys {
    static KEYS: OnceLock<TestInputTextureKeys> = OnceLock::new();
    KEYS.get_or_init(|| TestInputTextureKeys {
        dance: Arc::from("test_input/dance.png"),
        pump: Arc::from("test_input/pump.png"),
        highlight: Arc::from("test_input/highlight.png"),
        buttons: Arc::from("test_input/buttons.png"),
        highlight_green: Arc::from("test_input/highlightgreen.png"),
        highlight_red: Arc::from("test_input/highlightred.png"),
        highlight_arrow: Arc::from("test_input/highlightarrow.png"),
    })
}

#[inline(always)]
fn texture_key(shared: Option<&Arc<str>>, legacy: &'static str) -> Arc<str> {
    shared.map_or_else(|| Arc::from(legacy), Arc::clone)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LogicalButton {
    Up,
    Down,
    Left,
    Right,
    Center,
    MenuLeft,
    MenuRight,
    Start,
    Select,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlayerSlot {
    P1,
    P2,
}

#[derive(Clone, Debug, Default)]
pub struct State {
    buttons_held: HashMap<(PlayerSlot, LogicalButton), bool>,
    unmapped: UnmappedTracker,
    event_rate: EventRateTracker,
    /// Game-thread-owned one-entry presentations for the two full-screen Test
    /// Input surfaces. Input/readout changes invalidate both; viewport or
    /// layout changes replace the relevant entry. They never evict and drop
    /// with the screen state.
    presentation_revision: u64,
    select_music_presentation: RefCell<Option<SelectMusicPresentation>>,
    screen_presentation: RefCell<Option<TestInputScreenPresentation>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SelectMusicPresentationKey {
    revision: u64,
    game: GameFlag,
    active_color_index: i32,
    show_p1: bool,
    show_p2: bool,
    pad_spacing_bits: u32,
    screen_width_bits: u32,
    screen_height_bits: u32,
}

#[derive(Clone, Debug)]
struct SelectMusicPresentation {
    key: SelectMusicPresentationKey,
    children: Arc<[Actor]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TestInputScreenPresentationKey {
    revision: u64,
    game: GameFlag,
    active_color_index: i32,
    machine_font: MachineFont,
    select_returns: bool,
    screen_width_bits: u32,
    screen_height_bits: u32,
}

#[derive(Clone, Debug)]
struct TestInputScreenPresentation {
    key: TestInputScreenPresentationKey,
    children: Arc<[Actor]>,
}

impl State {
    #[inline(always)]
    fn invalidate_presentations(&mut self) {
        self.presentation_revision = self.presentation_revision.wrapping_add(1);
        self.select_music_presentation.get_mut().take();
        self.screen_presentation.get_mut().take();
    }
}

#[derive(Clone, Debug, Default)]
struct UnmappedTracker {
    held: HashMap<UnmappedKey, bool>,
    axis_value: HashMap<UnmappedKey, f32>,
    revision: u64,
    active_lines_cache: RefCell<Option<(u64, Arc<[Arc<str>]>)>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum UnmappedKey {
    Dir { dev: usize, dir: PadDir },
    RawButton { dev: usize, code_u32: u32 },
    RawAxis { dev: usize, code_u32: u32 },
    Keyboard { code: KeyCode },
}

#[derive(Clone, Debug, Default)]
struct EventRateTracker {
    stats: HashMap<EventStreamKey, EventStreamStats>,
    active_stream: Option<EventStreamKey>,
    source_cache: RefCell<Option<(EventStreamKey, Arc<str>)>>,
    summary_cache: RefCell<Option<((u32, u32), Arc<str>)>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum EventStreamKey {
    Keyboard,
    Pad { dev: usize },
}

#[derive(Clone, Debug, Default)]
struct EventStreamStats {
    prev_time: Option<EventSampleTime>,
    last_sample: Option<EventSampleKey>,
    hz_samples: VecDeque<u32>,
    latest_hz: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EventSampleKey {
    KeyboardHost {
        host_nanos: u64,
        code: KeyCode,
        pressed: bool,
    },
    KeyboardInstant {
        timestamp: Instant,
        code: KeyCode,
        pressed: bool,
    },
    PadHost {
        dev: usize,
        host_nanos: u64,
    },
    PadInstant {
        dev: usize,
        timestamp: Instant,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EventSampleTime {
    HostNanos(u64),
    Local(Instant),
}

impl EventSampleTime {
    fn hz_since(self, prev: Self) -> Option<u32> {
        let delta_ns = match (self, prev) {
            (Self::HostNanos(now), Self::HostNanos(prev)) => now.checked_sub(prev)?,
            (Self::Local(now), Self::Local(prev)) => {
                let nanos = now.checked_duration_since(prev)?.as_nanos();
                u64::try_from(nanos).ok()?
            }
            _ => return None,
        };
        if delta_ns == 0 {
            return None;
        }
        let hz = 1_000_000_000u64 / delta_ns;
        u32::try_from(hz).ok().filter(|hz| *hz != 0)
    }
}

impl EventStreamStats {
    fn record(&mut self, sample: EventSampleKey, time: EventSampleTime) {
        if self.last_sample == Some(sample) {
            return;
        }
        self.last_sample = Some(sample);
        if let Some(prev) = self.prev_time
            && let Some(hz) = time.hz_since(prev)
        {
            self.latest_hz = hz;
            if self.hz_samples.len() == EVENT_RATE_HISTORY {
                self.hz_samples.pop_front();
            }
            self.hz_samples.push_back(hz);
        }
        self.prev_time = Some(time);
    }

    fn max_hz(&self) -> u32 {
        if self.hz_samples.is_empty() {
            return 0;
        }
        self.hz_samples.iter().copied().max().unwrap_or(0)
    }
}

impl EventRateTracker {
    #[inline(always)]
    fn record_key(&mut self, key_event: &RawKeyboardEvent) {
        let key = if key_event.host_nanos != 0 {
            EventSampleKey::KeyboardHost {
                host_nanos: key_event.host_nanos,
                code: key_event.code,
                pressed: key_event.pressed,
            }
        } else {
            EventSampleKey::KeyboardInstant {
                timestamp: key_event.timestamp,
                code: key_event.code,
                pressed: key_event.pressed,
            }
        };
        let time = if key_event.host_nanos != 0 {
            EventSampleTime::HostNanos(key_event.host_nanos)
        } else {
            EventSampleTime::Local(key_event.timestamp)
        };
        self.record_sample(EventStreamKey::Keyboard, key, time);
    }

    #[inline(always)]
    fn record_pad(&mut self, pad_event: &PadEvent) {
        let (dev, timestamp, host_nanos) = match *pad_event {
            PadEvent::Dir {
                id,
                timestamp,
                host_nanos,
                ..
            }
            | PadEvent::RawButton {
                id,
                timestamp,
                host_nanos,
                ..
            }
            | PadEvent::RawAxis {
                id,
                timestamp,
                host_nanos,
                ..
            } => (usize::from(id), timestamp, host_nanos),
        };
        let key = if host_nanos != 0 {
            EventSampleKey::PadHost { dev, host_nanos }
        } else {
            EventSampleKey::PadInstant { dev, timestamp }
        };
        let time = if host_nanos != 0 {
            EventSampleTime::HostNanos(host_nanos)
        } else {
            EventSampleTime::Local(timestamp)
        };
        self.record_sample(EventStreamKey::Pad { dev }, key, time);
    }

    #[inline(always)]
    fn record_sample(
        &mut self,
        stream: EventStreamKey,
        key: EventSampleKey,
        time: EventSampleTime,
    ) {
        self.active_stream = Some(stream);
        self.stats.entry(stream).or_default().record(key, time);
    }

    fn readout_values(&self) -> Option<(EventStreamKey, u32, u32)> {
        let stream = self.active_stream?;
        let stats = self.stats.get(&stream)?;
        Some((stream, stats.latest_hz, stats.max_hz()))
    }

    #[cfg(test)]
    fn readout(&self) -> Option<(String, u32, u32)> {
        let (stream, latest_hz, max_hz) = self.readout_values()?;
        Some((event_source_label(stream), latest_hz, max_hz))
    }

    fn source_text(&self, stream: EventStreamKey) -> Arc<str> {
        if let Some((cached_stream, text)) = self.source_cache.borrow().as_ref()
            && *cached_stream == stream
        {
            return Arc::clone(text);
        }

        let text = Arc::from(event_source_label(stream));
        *self.source_cache.borrow_mut() = Some((stream, Arc::clone(&text)));
        text
    }

    fn summary_text(&self, latest_hz: u32, max_hz: u32) -> Arc<str> {
        let key = (latest_hz, max_hz);
        if let Some((cached_key, text)) = self.summary_cache.borrow().as_ref()
            && *cached_key == key
        {
            return Arc::clone(text);
        }

        let text = Arc::from(format_event_rate_summary(latest_hz, max_hz));
        *self.summary_cache.borrow_mut() = Some((key, Arc::clone(&text)));
        text
    }
}

#[inline(always)]
fn event_source_label(stream: EventStreamKey) -> String {
    match stream {
        EventStreamKey::Keyboard => "Keyboard".to_owned(),
        EventStreamKey::Pad { dev } => format!("Gamepad {dev}"),
    }
}

#[inline(always)]
fn format_hz(hz: u32) -> String {
    if hz > MAX_DISPLAY_HZ {
        return format!(">{MAX_DISPLAY_HZ} Hz");
    }
    format!("{hz} Hz")
}

#[inline(always)]
fn format_event_rate_summary(latest_hz: u32, max_hz: u32) -> String {
    format!(
        "{} latest / {} max",
        format_hz(latest_hz),
        format_hz(max_hz)
    )
}

impl UnmappedTracker {
    #[inline(always)]
    fn set(&mut self, key: UnmappedKey, pressed: bool) {
        let was_pressed = self.held.insert(key, pressed).unwrap_or(false);
        if was_pressed != pressed {
            self.revision = self.revision.wrapping_add(1);
        }
    }

    #[inline(always)]
    fn set_axis(&mut self, key: UnmappedKey, value: f32) {
        let value_changed = self
            .axis_value
            .insert(key, value)
            .is_none_or(|old| old.to_bits() != value.to_bits());
        let pressed = value.abs() >= UNMAPPED_AXIS_HELD_THRESHOLD;
        let was_pressed = self.held.insert(key, pressed).unwrap_or(false);
        if was_pressed != pressed || (pressed && value_changed) {
            self.revision = self.revision.wrapping_add(1);
        }
    }

    #[inline(always)]
    fn active_lines_legacy(&self) -> Vec<String> {
        let mut out = Vec::new();
        for (k, pressed) in &self.held {
            if !*pressed {
                continue;
            }
            let line = match *k {
                UnmappedKey::Dir { dev, dir } => format!("Gamepad {dev}: Dir::{dir:?}"),
                UnmappedKey::RawButton { dev, code_u32 } => {
                    deadsync_input::raw_button_label(dev, code_u32)
                        .unwrap_or_else(|| format!("Gamepad {dev}: RawButton [0x{code_u32:08X}]"))
                }
                UnmappedKey::RawAxis { dev, code_u32 } => {
                    let value = self.axis_value.get(k).copied().unwrap_or(0.0);
                    format!("Gamepad {dev}: RawAxis [0x{code_u32:08X}] ({value:.3})")
                }
                UnmappedKey::Keyboard { code } => format!("Keyboard: KeyCode::{code:?}"),
            };
            out.push(format!("{line} (not mapped)"));
        }
        out.sort();
        out
    }

    fn active_lines(&self) -> Arc<[Arc<str>]> {
        if let Some((revision, lines)) = self.active_lines_cache.borrow().as_ref()
            && *revision == self.revision
        {
            return Arc::clone(lines);
        }

        // One bounded snapshot is retained by `State`; unchanged render frames only clone Arcs.
        let lines: Arc<[Arc<str>]> = self
            .active_lines_legacy()
            .into_iter()
            .map(Arc::from)
            .collect();
        *self.active_lines_cache.borrow_mut() = Some((self.revision, Arc::clone(&lines)));
        lines
    }
}

#[inline(always)]
pub fn clear(state: &mut State) {
    *state = State::default();
}

const fn player_from_action(act: VirtualAction) -> Option<PlayerSlot> {
    use VirtualAction::{
        p1_center, p1_down, p1_left, p1_menu_down, p1_menu_left, p1_menu_right, p1_menu_up,
        p1_right, p1_select, p1_start, p1_up, p2_center, p2_down, p2_left, p2_menu_down,
        p2_menu_left, p2_menu_right, p2_menu_up, p2_right, p2_select, p2_start, p2_up,
    };
    match act {
        p1_up | p1_down | p1_left | p1_right | p1_menu_up | p1_menu_down | p1_menu_left
        | p1_menu_right | p1_start | p1_select | p1_center => Some(PlayerSlot::P1),
        p2_up | p2_down | p2_left | p2_right | p2_menu_up | p2_menu_down | p2_menu_left
        | p2_menu_right | p2_start | p2_select | p2_center => Some(PlayerSlot::P2),
        _ => None,
    }
}

const fn logical_button_from_action(act: VirtualAction) -> Option<LogicalButton> {
    use VirtualAction::{
        p1_center, p1_down, p1_left, p1_menu_down, p1_menu_left, p1_menu_right, p1_menu_up,
        p1_right, p1_select, p1_start, p1_up, p2_center, p2_down, p2_left, p2_menu_down,
        p2_menu_left, p2_menu_right, p2_menu_up, p2_right, p2_select, p2_start, p2_up,
    };
    match act {
        p1_up | p1_menu_up | p2_up | p2_menu_up => Some(LogicalButton::Up),
        p1_down | p1_menu_down | p2_down | p2_menu_down => Some(LogicalButton::Down),
        p1_left | p2_left => Some(LogicalButton::Left),
        p1_right | p2_right => Some(LogicalButton::Right),
        p1_menu_left | p2_menu_left => Some(LogicalButton::MenuLeft),
        p1_menu_right | p2_menu_right => Some(LogicalButton::MenuRight),
        p1_start | p2_start => Some(LogicalButton::Start),
        p1_select | p2_select => Some(LogicalButton::Select),
        p1_center | p2_center => Some(LogicalButton::Center),
        _ => None,
    }
}

/// Track which logical buttons are held, for the test-input button display.
pub fn apply_virtual_input(state: &mut State, ev: &InputEvent) {
    if let Some(player) = player_from_action(ev.action)
        && let Some(btn) = logical_button_from_action(ev.action)
    {
        let changed = state
            .buttons_held
            .insert((player, btn), ev.pressed)
            .is_none_or(|was_pressed| was_pressed != ev.pressed);
        if changed {
            state.invalidate_presentations();
        }
    }
}

pub fn apply_raw_pad_event(state: &mut State, pad_event: &PadEvent) {
    use deadsync_input::PadEvent as PE;

    state.event_rate.record_pad(pad_event);
    state.invalidate_presentations();

    let (key, pressed_opt, axis_value_opt) = match pad_event {
        PE::Dir {
            id, dir, pressed, ..
        } => {
            let dev = usize::from(*id);
            (UnmappedKey::Dir { dev, dir: *dir }, Some(*pressed), None)
        }
        PE::RawButton {
            id, code, pressed, ..
        } => {
            let dev = usize::from(*id);
            (
                UnmappedKey::RawButton {
                    dev,
                    code_u32: code.into_u32(),
                },
                Some(*pressed),
                None,
            )
        }
        PE::RawAxis {
            id, code, value, ..
        } => {
            let dev = usize::from(*id);
            (
                UnmappedKey::RawAxis {
                    dev,
                    code_u32: code.into_u32(),
                },
                None,
                Some(*value),
            )
        }
    };

    let mapped = with_keymap(|km| km.pad_event_mapped(pad_event));
    if mapped {
        return;
    }

    if let Some(pressed) = pressed_opt {
        state.unmapped.set(key, pressed);
        return;
    }
    if let Some(value) = axis_value_opt {
        state.unmapped.set_axis(key, value);
    }
}

pub fn apply_raw_key_event(state: &mut State, key_event: &RawKeyboardEvent) {
    if key_event.repeat {
        return;
    }
    state.event_rate.record_key(key_event);
    state.invalidate_presentations();
    let mapped = with_keymap(|km| km.raw_key_event_mapped(key_event));
    if mapped {
        return;
    }
    state.unmapped.set(
        UnmappedKey::Keyboard {
            code: key_event.code,
        },
        key_event.pressed,
    );
}

#[inline(always)]
fn held_alpha(state: &State, slot: PlayerSlot, button: LogicalButton) -> f32 {
    if *state.buttons_held.get(&(slot, button)).unwrap_or(&false) {
        1.0
    } else {
        0.0
    }
}

fn push_pad(
    actors: &mut Vec<Actor>,
    state: &State,
    game: GameFlag,
    slot: PlayerSlot,
    pad_x: f32,
    pad_y: f32,
    show_menu_buttons: bool,
    show_player_label: bool,
    player_label_font: Option<&'static str>,
    z: f32,
) {
    push_pad_scaled(
        actors,
        state,
        game,
        slot,
        pad_x,
        pad_y,
        show_menu_buttons,
        show_player_label,
        player_label_font,
        z,
        1.0,
    );
}

fn push_pad_scaled(
    actors: &mut Vec<Actor>,
    state: &State,
    game: GameFlag,
    slot: PlayerSlot,
    pad_x: f32,
    pad_y: f32,
    show_menu_buttons: bool,
    show_player_label: bool,
    player_label_font: Option<&'static str>,
    z: f32,
    scale: f32,
) {
    push_pad_scaled_with_texture_policy(
        actors,
        state,
        game,
        slot,
        pad_x,
        pad_y,
        show_menu_buttons,
        show_player_label,
        player_label_font,
        z,
        scale,
        true,
    );
}

#[allow(clippy::too_many_arguments)]
#[inline]
fn push_pad_scaled_with_texture_policy(
    actors: &mut Vec<Actor>,
    state: &State,
    game: GameFlag,
    slot: PlayerSlot,
    pad_x: f32,
    pad_y: f32,
    show_menu_buttons: bool,
    show_player_label: bool,
    player_label_font: Option<&'static str>,
    z: f32,
    scale: f32,
    reuse_texture_keys: bool,
) {
    let textures = reuse_texture_keys.then(test_input_texture_keys);
    let arrow_h_offset = 67.0_f32 * scale;
    let arrow_v_offset = 68.0_f32 * scale;
    let sprite_zoom = 0.8_f32 * scale;
    let buttons_y = 160.0f32.mul_add(scale, pad_y);
    let start_y = 146.0f32.mul_add(scale, pad_y);
    let select_y = 175.0f32.mul_add(scale, pad_y);
    let menu_y = 160.0f32.mul_add(scale, pad_y);
    let menu_x_offset = 37.0_f32 * scale;

    actors.push(match game {
        GameFlag::Dance => {
            act!(sprite(texture_key(textures.map(|keys| &keys.dance), "test_input/dance.png")):
                align(0.5, 0.5):
                xy(pad_x, pad_y):
                zoom(sprite_zoom):
                z(z)
            )
        }
        GameFlag::Pump => {
            act!(sprite(texture_key(textures.map(|keys| &keys.pump), "test_input/pump.png")):
                align(0.5, 0.5):
                xy(pad_x, pad_y):
                zoom(sprite_zoom):
                z(z)
            )
        }
    });

    if show_player_label && let Some(player_label_font) = player_label_font {
        let label = match slot {
            PlayerSlot::P1 => "Player 1",
            PlayerSlot::P2 => "Player 2",
        };
        actors.push(act!(text:
            align(0.5, 0.5):
            xy(pad_x, 130.0f32.mul_add(-scale, pad_y)):
            zoom(0.7 * scale):
            font(player_label_font):
            settext(label):
            horizalign(center):
            z(z + 1.0)
        ));
    }

    let highlights: &[(LogicalButton, f32, f32)] = match game {
        GameFlag::Dance => &[
            (LogicalButton::Up, 0.0, -arrow_v_offset),
            (LogicalButton::Down, 0.0, arrow_v_offset),
            (LogicalButton::Left, -arrow_h_offset, 0.0),
            (LogicalButton::Right, arrow_h_offset, 0.0),
        ],
        // Pump's compact action aliases follow chart lane order:
        // Down=UpLeft, Up=UpRight, Center, Left=DownLeft, Right=DownRight.
        GameFlag::Pump => &[
            (LogicalButton::Down, -arrow_h_offset, -arrow_v_offset),
            (LogicalButton::Up, arrow_h_offset, -arrow_v_offset),
            (LogicalButton::Center, 0.0, 0.0),
            (LogicalButton::Left, -arrow_h_offset, arrow_v_offset),
            (LogicalButton::Right, arrow_h_offset, arrow_v_offset),
        ],
    };
    for &(button, offset_x, offset_y) in highlights {
        actors.push(act!(sprite(texture_key(textures.map(|keys| &keys.highlight), "test_input/highlight.png")):
            align(0.5, 0.5):
            xy(pad_x + offset_x, pad_y + offset_y):
            zoom(sprite_zoom):
            diffuse(1.0, 1.0, 1.0, held_alpha(state, slot, button)):
            z(z + 1.0)
        ));
    }

    if !show_menu_buttons {
        return;
    }

    let button_zoom = 0.5_f32 * scale;
    actors.push(
        act!(sprite(texture_key(textures.map(|keys| &keys.buttons), "test_input/buttons.png")):
            align(0.5, 0.5):
            xy(pad_x, buttons_y):
            zoom(button_zoom):
            z(z)
        ),
    );
    actors.push(act!(sprite(texture_key(textures.map(|keys| &keys.highlight_green), "test_input/highlightgreen.png")):
        align(0.5, 0.5):
        xy(pad_x, start_y):
        zoom(button_zoom):
        diffuse(1.0, 1.0, 1.0, held_alpha(state, slot, LogicalButton::Start)):
        z(z + 1.0)
    ));
    actors.push(act!(sprite(texture_key(textures.map(|keys| &keys.highlight_red), "test_input/highlightred.png")):
        align(0.5, 0.5):
        xy(pad_x, select_y):
        zoom(button_zoom):
        diffuse(1.0, 1.0, 1.0, held_alpha(state, slot, LogicalButton::Select)):
        z(z + 1.0)
    ));
    actors.push(act!(sprite(texture_key(textures.map(|keys| &keys.highlight_arrow), "test_input/highlightarrow.png")):
        align(0.5, 0.5):
        xy(pad_x - menu_x_offset, menu_y):
        zoom(button_zoom):
        rotationz(180.0):
        diffuse(1.0, 1.0, 1.0, held_alpha(state, slot, LogicalButton::MenuLeft)):
        z(z + 1.0)
    ));
    actors.push(act!(sprite(texture_key(textures.map(|keys| &keys.highlight_arrow), "test_input/highlightarrow.png")):
        align(0.5, 0.5):
        xy(pad_x + menu_x_offset, menu_y):
        zoom(button_zoom):
        diffuse(1.0, 1.0, 1.0, held_alpha(state, slot, LogicalButton::MenuRight)):
        z(z + 1.0)
    ));
}

fn push_polling_readout(actors: &mut Vec<Actor>, state: &State, z: f32) {
    let (rate_source, rate_summary) = match state.event_rate.readout_values() {
        Some((stream, latest_hz, max_hz)) => (
            TextContent::Shared(state.event_rate.source_text(stream)),
            TextContent::Shared(state.event_rate.summary_text(latest_hz, max_hz)),
        ),
        None => (
            TextContent::Static("Waiting for raw input"),
            TextContent::Static("0 Hz latest / 0 Hz max"),
        ),
    };

    actors.push(act!(text:
        font("miso"):
        settext("RAW EVENT POLLING"):
        align(1.0, 1.0):
        xy(screen_width() - 20.0, screen_height() - 60.0):
        zoom(0.55):
        horizalign(right):
        diffuse(1.0, 1.0, 1.0, 0.8):
        z(z)
    ));
    actors.push(act!(text:
        font("miso"):
        settext(rate_source):
        align(1.0, 1.0):
        xy(screen_width() - 20.0, screen_height() - 38.0):
        zoom(0.65):
        horizalign(right):
        diffuse(1.0, 1.0, 1.0, 0.9):
        z(z)
    ));
    actors.push(act!(text:
        font("miso"):
        settext(rate_summary):
        align(1.0, 1.0):
        xy(screen_width() - 20.0, screen_height() - 20.0):
        zoom(0.72):
        horizalign(right):
        z(z)
    ));
}

fn push_unmapped_lines(actors: &mut Vec<Actor>, tracker: &UnmappedTracker, cx: f32, cy: f32) {
    let lines = tracker.active_lines();
    if lines.is_empty() {
        return;
    }

    let start_y = cy + 112.0;
    let line_h = 16.0;
    for (i, line) in lines.iter().enumerate() {
        actors.push(act!(text:
            font("miso"):
            settext(Arc::clone(line)):
            align(0.5, 0.0):
            xy(cx, (i as f32).mul_add(line_h, start_y)):
            zoom(0.8):
            horizalign(center):
            z(30)
        ));
    }
}

#[cfg(any(test, feature = "bench-support"))]
fn push_unmapped_lines_legacy(
    actors: &mut Vec<Actor>,
    tracker: &UnmappedTracker,
    cx: f32,
    cy: f32,
) {
    let lines = tracker.active_lines_legacy();
    if lines.is_empty() {
        return;
    }

    let start_y = cy + 112.0;
    let line_h = 16.0;
    for (i, line) in lines.iter().enumerate() {
        actors.push(act!(text:
            font("miso"):
            settext(line.clone()):
            align(0.5, 0.0):
            xy(cx, (i as f32).mul_add(line_h, start_y)):
            zoom(0.8):
            horizalign(center):
            z(30)
        ));
    }
}

pub fn push_test_input_screen_content(
    actors: &mut Vec<Actor>,
    state: &State,
    game: GameFlag,
    active_color_index: i32,
    machine_font: MachineFont,
    select_returns: bool,
) {
    let key = TestInputScreenPresentationKey {
        revision: state.presentation_revision,
        game,
        active_color_index,
        machine_font,
        select_returns,
        screen_width_bits: screen_width().to_bits(),
        screen_height_bits: screen_height().to_bits(),
    };
    let cached = state
        .screen_presentation
        .borrow()
        .as_ref()
        .filter(|presentation| presentation.key == key)
        .map(|presentation| Arc::clone(&presentation.children));
    let children = cached.unwrap_or_else(|| {
        let mut children = Vec::with_capacity(96);
        push_test_input_screen_content_unreserved(
            &mut children,
            state,
            game,
            machine_font,
            select_returns,
        );
        let children = Arc::<[Actor]>::from(children);
        *state.screen_presentation.borrow_mut() = Some(TestInputScreenPresentation {
            key,
            children: Arc::clone(&children),
        });
        children
    });
    crate::screens::components::select_music::push_retained_overlay(actors, children);
}

fn push_test_input_screen_content_unreserved(
    actors: &mut Vec<Actor>,
    state: &State,
    game: GameFlag,
    machine_font: MachineFont,
    select_returns: bool,
) {
    let cx = screen_center_x();
    let cy = screen_center_y() - 20.0;
    let pad_spacing = 150.0;
    let player_label_font = Some(machine_font_key(machine_font, FontRole::Header));

    push_pad(
        actors,
        state,
        game,
        PlayerSlot::P1,
        cx - pad_spacing,
        cy,
        true,
        true,
        player_label_font,
        20.0,
    );
    push_pad(
        actors,
        state,
        game,
        PlayerSlot::P2,
        cx + pad_spacing,
        cy,
        true,
        true,
        player_label_font,
        20.0,
    );

    push_unmapped_lines(actors, &state.unmapped, cx, cy);

    let return_hint = if select_returns {
        "Hold &SELECT; to return to Options."
    } else {
        "Hold &BACK; to return to Options."
    };
    actors.push(act!(text:
        font("miso"):
        settext(return_hint):
        align(0.5, 0.0):
        xy(cx, screen_height() - 40.0):
        zoom(0.8):
        horizalign(center):
        z(30)
    ));

    push_polling_readout(actors, state, 30.0);
}

/// Build a `TestInput` pad for use inside an evaluation pane (SL `ScreenEvaluation` Pane6 parity).
///
/// `scale` scales the entire pad uniformly (1.0 = full size; SL Pane6 uses ~0.8).
#[must_use]
pub fn build_evaluation_pad(
    state: &State,
    game: GameFlag,
    slot: PlayerSlot,
    pad_x: f32,
    pad_y: f32,
    scale: f32,
) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(6);
    push_evaluation_pad(&mut actors, state, game, slot, pad_x, pad_y, scale);
    actors
}

/// Append a `TestInput` evaluation pad directly to an existing actor batch.
pub fn push_evaluation_pad(
    actors: &mut Vec<Actor>,
    state: &State,
    game: GameFlag,
    slot: PlayerSlot,
    pad_x: f32,
    pad_y: f32,
    scale: f32,
) {
    actors.reserve(6);
    push_pad_scaled(
        actors, state, game, slot, pad_x, pad_y, false, false, None, 100.0, scale,
    );
}

/// Approximate visual half-width of a pad rendered by `build_evaluation_pad` at the given scale.
/// Useful for laying out neighboring elements (e.g., gaps between two pads in Double play).
#[must_use]
pub fn evaluation_pad_half_width(scale: f32) -> f32 {
    eval_panel_layout::PAD_NATURAL_WIDTH * 0.5 * scale
}

mod eval_panel_layout {
    // Panel size (logical px).
    pub const PANEL_WIDTH: f32 = 288.889;
    pub const PANEL_HEIGHT: f32 = 177.778;

    // Pad: top-left corner of the pad's bounding box, panel-local, y-down.
    pub const PAD_LOGICAL_SCALE: f32 = 0.8222;
    pub const PAD_X: f32 = 126.667;
    pub const PAD_Y: f32 = -5.111;

    // Text block.
    pub const TEXT_LEFT_X: f32 = 3.111;
    pub const TEXT_BLOCK_WIDTH: f32 = 100.0;
    pub const TITLE_TOP_Y: f32 = 17.778;
    pub const DIVIDER_OFFSET: f32 = 23.111;
    pub const BODY_OFFSET: f32 = 28.889;

    /// If true, the title is horizontally centered within the text block;
    /// otherwise it's left-aligned to `TEXT_LEFT_X`.
    pub const TITLE_CENTERED: bool = true;

    pub const TITLE_ZOOM: f32 = 1.0889;
    pub const BODY_ZOOM: f32 = 0.7778;

    pub const BODY_LINE_SPACING: i32 = 20;

    /// Pad natural full width at `PAD_LOGICAL_SCALE` = 1.0, in logical px.
    /// This is `(arrow_h_offset + half_arrow_sprite) * 2` from `push_pad_scaled`.
    pub const PAD_NATURAL_WIDTH: f32 = (67.0 + 27.0) * 2.0;
    /// Pad natural full height at `PAD_LOGICAL_SCALE` = 1.0, in logical px.
    pub const PAD_NATURAL_HEIGHT: f32 = (68.0 + 27.0) * 2.0;
}

/// Visual size of the unscaled panel in logical pixels (width, height at
/// scale 1.0).
#[must_use]
pub const fn evaluation_panel_size() -> (f32, f32) {
    (
        eval_panel_layout::PANEL_WIDTH,
        eval_panel_layout::PANEL_HEIGHT,
    )
}

/// Build the `TestInput` evaluation panel anchored at its **top-left corner**.
///
/// `(anchor_x, anchor_y)` is the screen-space position of the panel's
/// top-left corner. `scale` uniformly scales the entire panel.
#[must_use]
pub fn build_evaluation_panel(
    state: &State,
    game: GameFlag,
    slot: PlayerSlot,
    anchor_x: f32,
    anchor_y: f32,
    scale: f32,
    title_font: &'static str,
    title: Arc<str>,
    body_font: &'static str,
    instructions: Arc<str>,
) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(10);
    push_evaluation_panel(
        &mut actors,
        state,
        game,
        slot,
        anchor_x,
        anchor_y,
        scale,
        title_font,
        title,
        body_font,
        instructions,
    );
    actors
}

/// Append a complete `TestInput` evaluation panel to an existing actor batch.
#[allow(clippy::too_many_arguments)]
pub fn push_evaluation_panel(
    actors: &mut Vec<Actor>,
    state: &State,
    game: GameFlag,
    slot: PlayerSlot,
    anchor_x: f32,
    anchor_y: f32,
    scale: f32,
    title_font: &'static str,
    title: Arc<str>,
    body_font: &'static str,
    instructions: Arc<str>,
) {
    use eval_panel_layout::*;
    actors.reserve(10);

    // Convert a panel-local (x_right, y_down) point in logical px to screen-space actor coords.
    let map = |local_x: f32, local_y_from_top: f32| -> (f32, f32) {
        (
            local_x.mul_add(scale, anchor_x),
            local_y_from_top.mul_add(scale, anchor_y),
        )
    };

    let (pad_x, pad_y) = {
        // PAD_X/PAD_Y refer to the pad's top-left (panel-local, y-down);
        // convert to the pad's center for push_pad_scaled.
        let pad_box_w = PAD_NATURAL_WIDTH * PAD_LOGICAL_SCALE;
        let pad_box_h = PAD_NATURAL_HEIGHT * PAD_LOGICAL_SCALE;
        let cx_local = pad_box_w.mul_add(0.5, PAD_X);
        let cy_local = pad_box_h.mul_add(0.5, PAD_Y);
        map(cx_local, cy_local)
    };
    let pad_scale = PAD_LOGICAL_SCALE * scale;
    push_pad_scaled(
        actors, state, game, slot, pad_x, pad_y, false, false, None, 100.0, pad_scale,
    );

    let (text_x, title_y) = map(TEXT_LEFT_X, TITLE_TOP_Y);
    let (_, divider_y) = map(TEXT_LEFT_X, TITLE_TOP_Y + DIVIDER_OFFSET);
    let (_, body_y) = map(TEXT_LEFT_X, TITLE_TOP_Y + BODY_OFFSET);
    let block_w = TEXT_BLOCK_WIDTH * scale;
    let title_zoom = TITLE_ZOOM * scale;
    let body_zoom = BODY_ZOOM * scale;

    if TITLE_CENTERED {
        let title_center_x = block_w.mul_add(0.5, text_x);
        actors.push(act!(text:
            font(title_font):
            settext(title):
            align(0.5, 0.0):
            xy(title_center_x, title_y):
            zoom(title_zoom):
            horizalign(center):
            z(100.0)
        ));
    } else {
        actors.push(act!(text:
            font(title_font):
            settext(title):
            align(0.0, 0.0):
            xy(text_x, title_y):
            zoom(title_zoom):
            horizalign(left):
            z(100.0)
        ));
    }
    actors.push(act!(quad:
        align(0.0, 0.0):
        xy(text_x, divider_y):
        zoomto(block_w, 2.0_f32.max(scale * 2.0)):
        diffuse(1.0, 1.0, 1.0, 0.33):
        z(100.0)
    ));
    actors.push(act!(text:
        font(body_font):
        settext(instructions):
        align(0.0, 0.0):
        xy(text_x, body_y):
        zoom(body_zoom):
        horizalign(left):
        wrapwidthpixels(TEXT_BLOCK_WIDTH / BODY_ZOOM):
        vertspacing(BODY_LINE_SPACING):
        z(100.0)
    ));
}

pub fn push_select_music_overlay(
    actors: &mut Vec<Actor>,
    state: &State,
    game: GameFlag,
    active_color_index: i32,
    show_p1: bool,
    show_p2: bool,
    pad_spacing: f32,
) {
    let key = SelectMusicPresentationKey {
        revision: state.presentation_revision,
        game,
        active_color_index,
        show_p1,
        show_p2,
        pad_spacing_bits: pad_spacing.to_bits(),
        screen_width_bits: screen_width().to_bits(),
        screen_height_bits: screen_height().to_bits(),
    };
    let cached = state
        .select_music_presentation
        .borrow()
        .as_ref()
        .filter(|presentation| presentation.key == key)
        .map(|presentation| Arc::clone(&presentation.children));
    let children = cached.unwrap_or_else(|| {
        let mut children = Vec::with_capacity(96);
        push_select_music_overlay_unreserved(
            &mut children,
            state,
            game,
            show_p1,
            show_p2,
            pad_spacing,
        );
        let children = Arc::<[Actor]>::from(children);
        *state.select_music_presentation.borrow_mut() = Some(SelectMusicPresentation {
            key,
            children: Arc::clone(&children),
        });
        children
    });
    crate::screens::components::select_music::push_retained_overlay(actors, children);
}

fn push_select_music_overlay_unreserved(
    actors: &mut Vec<Actor>,
    state: &State,
    game: GameFlag,
    show_p1: bool,
    show_p2: bool,
    pad_spacing: f32,
) {
    let cx = screen_center_x();
    // SL parity: overlay/TestInput.lua places pad AF at y = _screen.cy + 50, then
    // _modules/TestInput Pad/default.lua places the pad art at y = -80 inside that AF.
    // Net visual pad center is _screen.cy - 30.
    let cy = screen_center_y() - 30.0;

    actors.push(act!(quad:
        align(0.0, 0.0):
        xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, SORT_MENU_DIM_ALPHA):
        z(1450)
    ));

    if show_p1 {
        push_pad(
            actors,
            state,
            game,
            PlayerSlot::P1,
            cx - pad_spacing,
            cy,
            false,
            false,
            None,
            1451.0,
        );
    }
    if show_p2 {
        push_pad(
            actors,
            state,
            game,
            PlayerSlot::P2,
            cx + pad_spacing,
            cy,
            false,
            false,
            None,
            1451.0,
        );
    }

    actors.push(act!(text:
        font("miso"):
        settext(SORT_MENU_CLOSE_HINT):
        align(0.5, 0.5):
        xy(screen_center_x(), screen_height() - 120.0):
        zoom(1.1):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(1453):
        horizalign(center)
    ));

    push_polling_readout(actors, state, 1453.0);
}

/// Stable old/new fixture for the two-player Select Music Test Input batch.
#[cfg(any(test, feature = "bench-support"))]
pub struct SelectMusicTestInputAppendBenchmark {
    state: State,
}

#[cfg(any(test, feature = "bench-support"))]
impl SelectMusicTestInputAppendBenchmark {
    #[must_use]
    pub fn new() -> Self {
        let mut state = State::default();
        state
            .buttons_held
            .insert((PlayerSlot::P1, LogicalButton::Left), true);
        state
            .buttons_held
            .insert((PlayerSlot::P2, LogicalButton::Start), true);
        Self { state }
    }

    #[must_use]
    pub fn actor_count(&self) -> usize {
        let mut actors = Vec::with_capacity(96);
        push_select_music_overlay_unreserved(
            &mut actors,
            &self.state,
            GameFlag::Dance,
            true,
            true,
            125.0,
        );
        actors.len()
    }

    #[must_use]
    pub fn legacy_frame(&self, out: &mut Vec<Actor>) -> u64 {
        out.clear();
        push_select_music_overlay_unreserved(out, &self.state, GameFlag::Dance, true, true, 125.0);
        std::hint::black_box(&*out);
        overlay_actor_checksum(out)
    }

    #[must_use]
    pub fn direct_frame(&self, out: &mut Vec<Actor>) -> u64 {
        out.clear();
        push_select_music_overlay(out, &self.state, GameFlag::Dance, 0, true, true, 125.0);
        std::hint::black_box(&*out);
        overlay_actor_checksum(out)
    }

    #[must_use]
    pub fn screen_actor_count(&self) -> usize {
        let mut actors = Vec::with_capacity(96);
        push_test_input_screen_content_unreserved(
            &mut actors,
            &self.state,
            GameFlag::Dance,
            MachineFont::Mega,
            true,
        );
        actors.len()
    }

    #[must_use]
    pub fn legacy_screen_frame(&self, out: &mut Vec<Actor>) -> u64 {
        out.clear();
        push_test_input_screen_content_unreserved(
            out,
            &self.state,
            GameFlag::Dance,
            MachineFont::Mega,
            true,
        );
        std::hint::black_box(&*out);
        overlay_actor_checksum(out)
    }

    #[must_use]
    pub fn direct_screen_frame(&self, out: &mut Vec<Actor>) -> u64 {
        out.clear();
        push_test_input_screen_content(
            out,
            &self.state,
            GameFlag::Dance,
            0,
            MachineFont::Mega,
            true,
        );
        std::hint::black_box(&*out);
        overlay_actor_checksum(out)
    }
}

#[cfg(any(test, feature = "bench-support"))]
impl Default for SelectMusicTestInputAppendBenchmark {
    fn default() -> Self {
        Self::new()
    }
}

/// Stable old/new fixture for Evaluation's live Test Input pane.
#[cfg(any(test, feature = "bench-support"))]
pub struct EvaluationTestInputBenchmark {
    state: State,
    title: Arc<str>,
    instructions: Arc<str>,
}

#[cfg(any(test, feature = "bench-support"))]
impl EvaluationTestInputBenchmark {
    #[must_use]
    pub fn new() -> Self {
        // Warm the bounded static texture table outside measured frames.
        std::hint::black_box(test_input_texture_keys());
        let mut state = State::default();
        state
            .buttons_held
            .insert((PlayerSlot::P1, LogicalButton::Center), true);
        Self {
            state,
            title: Arc::from("Test Input"),
            instructions: Arc::from("Step on the panels to test your input."),
        }
    }

    #[must_use]
    pub fn legacy_texture_frame(&self, out: &mut Vec<Actor>) -> u64 {
        out.clear();
        out.reserve(6);
        push_pad_scaled_with_texture_policy(
            out,
            &self.state,
            GameFlag::Pump,
            PlayerSlot::P1,
            320.0,
            240.0,
            false,
            false,
            None,
            100.0,
            0.75,
            false,
        );
        std::hint::black_box(&*out);
        overlay_actor_checksum(out)
    }

    #[must_use]
    pub fn cached_texture_frame(&self, out: &mut Vec<Actor>) -> u64 {
        out.clear();
        push_evaluation_pad(
            out,
            &self.state,
            GameFlag::Pump,
            PlayerSlot::P1,
            320.0,
            240.0,
            0.75,
        );
        std::hint::black_box(&*out);
        overlay_actor_checksum(out)
    }

    #[must_use]
    pub fn staged_pad_frame(&self, out: &mut Vec<Actor>) -> u64 {
        out.clear();
        out.extend(build_evaluation_pad(
            &self.state,
            GameFlag::Pump,
            PlayerSlot::P1,
            320.0,
            240.0,
            0.75,
        ));
        std::hint::black_box(&*out);
        overlay_actor_checksum(out)
    }

    #[must_use]
    pub fn direct_pad_frame(&self, out: &mut Vec<Actor>) -> u64 {
        out.clear();
        push_evaluation_pad(
            out,
            &self.state,
            GameFlag::Pump,
            PlayerSlot::P1,
            320.0,
            240.0,
            0.75,
        );
        std::hint::black_box(&*out);
        overlay_actor_checksum(out)
    }

    #[must_use]
    pub fn staged_panel_frame(&self, out: &mut Vec<Actor>) -> u64 {
        out.clear();
        out.extend(build_evaluation_panel(
            &self.state,
            GameFlag::Pump,
            PlayerSlot::P1,
            175.0,
            150.0,
            1.0,
            "miso",
            Arc::clone(&self.title),
            "miso",
            Arc::clone(&self.instructions),
        ));
        std::hint::black_box(&*out);
        overlay_actor_checksum(out)
    }

    #[must_use]
    pub fn direct_panel_frame(&self, out: &mut Vec<Actor>) -> u64 {
        out.clear();
        push_evaluation_panel(
            out,
            &self.state,
            GameFlag::Pump,
            PlayerSlot::P1,
            175.0,
            150.0,
            1.0,
            "miso",
            Arc::clone(&self.title),
            "miso",
            Arc::clone(&self.instructions),
        );
        std::hint::black_box(&*out);
        overlay_actor_checksum(out)
    }
}

#[cfg(any(test, feature = "bench-support"))]
impl Default for EvaluationTestInputBenchmark {
    fn default() -> Self {
        Self::new()
    }
}

/// Stable old/new fixture for Test Input's retained diagnostic presentations.
#[cfg(any(test, feature = "bench-support"))]
pub struct TestInputReadoutBenchmark {
    event_rate: EventRateTracker,
    unmapped: UnmappedTracker,
}

#[cfg(any(test, feature = "bench-support"))]
impl TestInputReadoutBenchmark {
    #[must_use]
    pub fn new() -> Self {
        let stream = EventStreamKey::Pad { dev: 7 };
        let mut stats = EventStreamStats {
            latest_hz: 777,
            ..EventStreamStats::default()
        };
        stats.hz_samples.extend([500, 777, 1001]);

        let mut event_rate = EventRateTracker::default();
        event_rate.stats.insert(stream, stats);
        event_rate.active_stream = Some(stream);
        let (_, latest_hz, max_hz) = event_rate
            .readout_values()
            .expect("benchmark event-rate readout");
        std::hint::black_box(event_rate.source_text(stream));
        std::hint::black_box(event_rate.summary_text(latest_hz, max_hz));

        let mut unmapped = UnmappedTracker::default();
        unmapped.set(
            UnmappedKey::Keyboard {
                code: KeyCode::KeyA,
            },
            true,
        );
        unmapped.set(
            UnmappedKey::RawButton {
                dev: 7,
                code_u32: 0xDEAD_BEEF,
            },
            true,
        );
        unmapped.set_axis(
            UnmappedKey::RawAxis {
                dev: 7,
                code_u32: 0x1234_5678,
            },
            0.875,
        );
        std::hint::black_box(unmapped.active_lines());

        Self {
            event_rate,
            unmapped,
        }
    }

    #[must_use]
    pub fn legacy_source_frame(&self) -> u64 {
        let (stream, _, _) = self
            .event_rate
            .readout_values()
            .expect("benchmark event-rate readout");
        let text = event_source_label(std::hint::black_box(stream));
        presentation_text_checksum(std::hint::black_box(&text))
    }

    #[must_use]
    pub fn cached_source_frame(&self) -> u64 {
        let (stream, _, _) = self
            .event_rate
            .readout_values()
            .expect("benchmark event-rate readout");
        let text = self.event_rate.source_text(std::hint::black_box(stream));
        presentation_text_checksum(std::hint::black_box(text.as_ref()))
    }

    #[must_use]
    pub fn legacy_summary_frame(&self) -> u64 {
        let (_, latest_hz, max_hz) = self
            .event_rate
            .readout_values()
            .expect("benchmark event-rate readout");
        let text = format_event_rate_summary(
            std::hint::black_box(latest_hz),
            std::hint::black_box(max_hz),
        );
        presentation_text_checksum(std::hint::black_box(&text))
    }

    #[must_use]
    pub fn cached_summary_frame(&self) -> u64 {
        let (_, latest_hz, max_hz) = self
            .event_rate
            .readout_values()
            .expect("benchmark event-rate readout");
        let text = self.event_rate.summary_text(
            std::hint::black_box(latest_hz),
            std::hint::black_box(max_hz),
        );
        presentation_text_checksum(std::hint::black_box(text.as_ref()))
    }

    #[must_use]
    pub fn legacy_unmapped_frame(&self, out: &mut Vec<Actor>) -> u64 {
        out.clear();
        push_unmapped_lines_legacy(out, &self.unmapped, 320.0, 220.0);
        std::hint::black_box(&*out);
        overlay_actor_checksum(out)
    }

    #[must_use]
    pub fn cached_unmapped_frame(&self, out: &mut Vec<Actor>) -> u64 {
        out.clear();
        push_unmapped_lines(out, &self.unmapped, 320.0, 220.0);
        std::hint::black_box(&*out);
        overlay_actor_checksum(out)
    }
}

#[cfg(any(test, feature = "bench-support"))]
impl Default for TestInputReadoutBenchmark {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(test, feature = "bench-support"))]
fn presentation_text_checksum(text: &str) -> u64 {
    text.bytes().fold(text.len() as u64, |hash, byte| {
        hash.rotate_left(7) ^ u64::from(byte)
    })
}

#[cfg(any(test, feature = "bench-support"))]
fn overlay_actor_checksum(actors: &[Actor]) -> u64 {
    let semantic_actors = match actors {
        [Actor::SharedFrame { children, .. }] => children.as_ref(),
        _ => actors,
    };
    semantic_actors
        .iter()
        .fold(semantic_actors.len() as u64, |checksum, actor| {
            let value = match actor {
                Actor::Sprite {
                    source,
                    tint,
                    offset,
                    z,
                    ..
                } => {
                    let texture = source.texture_key().unwrap_or("");
                    let texture_hash = texture.bytes().fold(texture.len() as u64, |hash, byte| {
                        hash.rotate_left(7) ^ u64::from(byte)
                    });
                    texture_hash
                        ^ u64::from(tint[3].to_bits()).rotate_left(13)
                        ^ u64::from(offset[0].to_bits()).rotate_left(23)
                        ^ u64::from(offset[1].to_bits()).rotate_left(31)
                        ^ u64::from(*z as u16)
                }
                Actor::Text { content, z, .. } => content
                    .as_str()
                    .bytes()
                    .fold(u64::from(*z as u16), |hash, byte| {
                        hash.rotate_left(7) ^ u64::from(byte)
                    }),
                Actor::Frame { children, z, .. } => {
                    overlay_actor_checksum(children) ^ u64::from(*z as u16)
                }
                Actor::SharedFrame { children, z, .. } => {
                    overlay_actor_checksum(children) ^ u64::from(*z as u16)
                }
                _ => 1,
            };
            checksum.rotate_left(11) ^ value
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_input::{PadCode, PadId};
    use std::time::Duration;

    #[test]
    fn direct_select_music_append_matches_legacy_batch() {
        let fixture = SelectMusicTestInputAppendBenchmark::new();
        let mut legacy = Vec::with_capacity(96);
        let mut direct = Vec::with_capacity(96);

        let legacy_checksum = fixture.legacy_frame(&mut legacy);
        let direct_checksum = fixture.direct_frame(&mut direct);

        assert_eq!(legacy_checksum, direct_checksum);
        assert_eq!(legacy.len(), fixture.actor_count());
        let [Actor::SharedFrame { children, .. }] = direct.as_slice() else {
            panic!("expected retained Select Music Test Input actors");
        };
        assert_eq!(format!("{legacy:#?}"), format!("{children:#?}"));

        let legacy_checksum = fixture.legacy_screen_frame(&mut legacy);
        let direct_checksum = fixture.direct_screen_frame(&mut direct);
        assert_eq!(legacy_checksum, direct_checksum);
        assert_eq!(legacy.len(), fixture.screen_actor_count());
        let [Actor::SharedFrame { children, .. }] = direct.as_slice() else {
            panic!("expected retained Test Input screen actors");
        };
        assert_eq!(format!("{legacy:#?}"), format!("{children:#?}"));
    }

    #[test]
    fn test_input_presentations_reuse_and_invalidate_on_input() {
        let mut fixture = SelectMusicTestInputAppendBenchmark::new();
        let mut actors = Vec::with_capacity(96);
        let old_checksum = fixture.direct_frame(&mut actors);
        let [Actor::SharedFrame { children, .. }] = actors.as_slice() else {
            panic!("expected retained Select Music Test Input actors");
        };
        let old = Arc::clone(children);
        let _ = fixture.direct_frame(&mut actors);
        let [
            Actor::SharedFrame {
                children: repeated, ..
            },
        ] = actors.as_slice()
        else {
            panic!("expected retained Select Music Test Input actors");
        };
        assert!(Arc::ptr_eq(&old, repeated));

        let now = Instant::now();
        apply_virtual_input(
            &mut fixture.state,
            &InputEvent::new(
                VirtualAction::p1_left,
                0,
                false,
                deadsync_core::input::InputSource::Keyboard,
                now,
                0,
                now,
                now,
            ),
        );
        let new_checksum = fixture.direct_frame(&mut actors);
        let [Actor::SharedFrame { children, .. }] = actors.as_slice() else {
            panic!("expected retained Select Music Test Input actors");
        };
        assert!(!Arc::ptr_eq(&old, children));
        assert_ne!(old_checksum, new_checksum);

        let _ = fixture.direct_screen_frame(&mut actors);
        let [Actor::SharedFrame { children, .. }] = actors.as_slice() else {
            panic!("expected retained Test Input screen actors");
        };
        let screen = Arc::clone(children);
        let _ = fixture.direct_screen_frame(&mut actors);
        let [Actor::SharedFrame { children, .. }] = actors.as_slice() else {
            panic!("expected retained Test Input screen actors");
        };
        assert!(Arc::ptr_eq(&screen, children));
    }

    #[test]
    fn shared_test_input_texture_keys_match_legacy_actors() {
        let fixture = EvaluationTestInputBenchmark::new();
        let mut legacy = Vec::with_capacity(10);
        let mut cached = Vec::with_capacity(10);

        assert_eq!(
            fixture.legacy_texture_frame(&mut legacy),
            fixture.cached_texture_frame(&mut cached)
        );
        assert_eq!(format!("{legacy:#?}"), format!("{cached:#?}"));
    }

    #[test]
    fn direct_evaluation_pad_append_matches_staged_batch() {
        let fixture = EvaluationTestInputBenchmark::new();
        let mut staged = Vec::with_capacity(10);
        let mut direct = Vec::with_capacity(10);

        assert_eq!(
            fixture.staged_pad_frame(&mut staged),
            fixture.direct_pad_frame(&mut direct)
        );
        assert_eq!(format!("{staged:#?}"), format!("{direct:#?}"));
    }

    #[test]
    fn direct_evaluation_panel_append_matches_staged_batch() {
        let fixture = EvaluationTestInputBenchmark::new();
        let mut staged = Vec::with_capacity(10);
        let mut direct = Vec::with_capacity(10);

        assert_eq!(
            fixture.staged_panel_frame(&mut staged),
            fixture.direct_panel_frame(&mut direct)
        );
        assert_eq!(format!("{staged:#?}"), format!("{direct:#?}"));
    }

    #[test]
    fn test_input_pad_tracks_active_game() {
        let mut state = State::default();
        state
            .buttons_held
            .insert((PlayerSlot::P1, LogicalButton::Center), true);

        let pump = build_evaluation_pad(&state, GameFlag::Pump, PlayerSlot::P1, 100.0, 200.0, 1.0);
        let Actor::Sprite { source, .. } = &pump[0] else {
            panic!("expected Pump pad sprite");
        };
        assert_eq!(source.texture_key(), Some("test_input/pump.png"));
        assert_eq!(pump.len(), 6);
        let offsets: [[f32; 2]; 5] = std::array::from_fn(|idx| {
            let Actor::Sprite { offset, .. } = &pump[idx + 1] else {
                panic!("expected Pump panel highlight");
            };
            *offset
        });
        assert_eq!(
            offsets,
            [
                [33.0, 132.0],
                [167.0, 132.0],
                [100.0, 200.0],
                [33.0, 268.0],
                [167.0, 268.0],
            ]
        );
        let Actor::Sprite { offset, tint, .. } = &pump[3] else {
            panic!("expected Pump center highlight");
        };
        assert_eq!(*offset, [100.0, 200.0]);
        assert_eq!(tint[3], 1.0);

        let dance =
            build_evaluation_pad(&state, GameFlag::Dance, PlayerSlot::P1, 100.0, 200.0, 1.0);
        let Actor::Sprite { source, .. } = &dance[0] else {
            panic!("expected Dance pad sprite");
        };
        assert_eq!(source.texture_key(), Some("test_input/dance.png"));
        assert_eq!(dance.len(), 5);
    }

    #[test]
    fn pump_center_is_tracked_as_a_player_panel() {
        assert_eq!(
            player_from_action(VirtualAction::p1_center),
            Some(PlayerSlot::P1)
        );
        assert_eq!(
            logical_button_from_action(VirtualAction::p2_center),
            Some(LogicalButton::Center)
        );
    }

    #[test]
    fn dedups_pad_events_from_the_same_report() {
        let base = Instant::now();
        let mut tracker = EventRateTracker::default();

        tracker.record_pad(&PadEvent::RawButton {
            id: PadId(0),
            timestamp: base,
            host_nanos: 123,
            code: PadCode(1),
            uuid: [0; 16],
            value: 1.0,
            pressed: true,
        });
        tracker.record_pad(&PadEvent::Dir {
            id: PadId(0),
            timestamp: base,
            host_nanos: 123,
            dir: PadDir::Up,
            pressed: true,
        });

        let (_, latest, max) = tracker.readout().expect("missing readout");
        assert_eq!(latest, 0);
        assert_eq!(max, 0);
    }

    #[test]
    fn reports_latest_and_peak_rate() {
        let base = Instant::now();
        let mut tracker = EventRateTracker::default();

        for (i, host_nanos) in [0u64, 1_000_000, 3_000_000, 4_000_000]
            .into_iter()
            .enumerate()
        {
            tracker.record_key(&RawKeyboardEvent {
                code: KeyCode::KeyA,
                pressed: i % 2 == 0,
                repeat: false,
                timestamp: base + Duration::from_nanos(host_nanos),
                host_nanos,
            });
        }

        let (source, latest, max) = tracker.readout().expect("missing readout");
        assert_eq!(source, "Keyboard");
        assert_eq!(latest, 1000);
        assert_eq!(max, 1000);
    }

    #[test]
    fn keeps_only_the_last_sixty_four_samples_in_the_peak() {
        let base = Instant::now();
        let mut tracker = EventRateTracker::default();
        let mut host_nanos = 0u64;

        for i in 0..66 {
            host_nanos += if i < 2 { 2_000_000 } else { 1_000_000 };
            tracker.record_key(&RawKeyboardEvent {
                code: KeyCode::KeyA,
                pressed: i % 2 == 0,
                repeat: false,
                timestamp: base + Duration::from_nanos(host_nanos),
                host_nanos,
            });
        }

        let (_, latest, max) = tracker.readout().expect("missing readout");
        assert_eq!(latest, 1000);
        assert_eq!(max, 1000);
    }

    #[test]
    fn peak_drops_once_old_spike_leaves_history() {
        let base = Instant::now();
        let mut tracker = EventRateTracker::default();
        let mut host_nanos = 0u64;

        tracker.record_key(&RawKeyboardEvent {
            code: KeyCode::KeyA,
            pressed: true,
            repeat: false,
            timestamp: base,
            host_nanos,
        });
        host_nanos += 500_000;
        tracker.record_key(&RawKeyboardEvent {
            code: KeyCode::KeyA,
            pressed: false,
            repeat: false,
            timestamp: base + Duration::from_nanos(host_nanos),
            host_nanos,
        });

        for i in 0..64 {
            host_nanos += 1_000_000;
            tracker.record_key(&RawKeyboardEvent {
                code: KeyCode::KeyA,
                pressed: i % 2 == 0,
                repeat: false,
                timestamp: base + Duration::from_nanos(host_nanos),
                host_nanos,
            });
        }

        let (_, latest, max) = tracker.readout().expect("missing readout");
        assert_eq!(latest, 1000);
        assert_eq!(max, 1000);
    }

    #[test]
    fn caps_display_above_one_thousand_hz() {
        assert_eq!(format_hz(1000), "1000 Hz");
        assert_eq!(format_hz(1001), ">1000 Hz");
    }

    #[test]
    fn retained_event_source_matches_legacy_and_refreshes_on_stream_change() {
        let pad = EventStreamKey::Pad { dev: 12 };
        let mut tracker = EventRateTracker::default();
        tracker.stats.insert(pad, EventStreamStats::default());
        tracker.active_stream = Some(pad);

        let legacy = tracker.readout().expect("legacy readout").0;
        let cached = tracker.source_text(pad);
        let cached_again = tracker.source_text(pad);
        assert_eq!(cached.as_ref(), legacy);
        assert!(Arc::ptr_eq(&cached, &cached_again));

        tracker
            .stats
            .insert(EventStreamKey::Keyboard, EventStreamStats::default());
        tracker.active_stream = Some(EventStreamKey::Keyboard);
        let refreshed = tracker.source_text(EventStreamKey::Keyboard);
        assert_eq!(refreshed.as_ref(), "Keyboard");
        assert!(!Arc::ptr_eq(&cached, &refreshed));
    }

    #[test]
    fn retained_polling_summary_matches_legacy_and_refreshes_on_rate_change() {
        let tracker = EventRateTracker::default();
        let legacy = format_event_rate_summary(777, 1001);
        let cached = tracker.summary_text(777, 1001);
        let cached_again = tracker.summary_text(777, 1001);
        assert_eq!(cached.as_ref(), legacy);
        assert_eq!(cached.as_ref(), "777 Hz latest / >1000 Hz max");
        assert!(Arc::ptr_eq(&cached, &cached_again));

        let refreshed = tracker.summary_text(500, 1000);
        assert_eq!(refreshed.as_ref(), "500 Hz latest / 1000 Hz max");
        assert!(!Arc::ptr_eq(&cached, &refreshed));
    }

    #[test]
    fn retained_unmapped_lines_match_legacy_sorting_and_invalidate_on_change() {
        let axis = UnmappedKey::RawAxis {
            dev: 3,
            code_u32: 0x1234_5678,
        };
        let mut tracker = UnmappedTracker::default();
        tracker.set(
            UnmappedKey::Keyboard {
                code: KeyCode::KeyA,
            },
            true,
        );
        tracker.set_axis(axis, 0.75);

        let legacy = tracker.active_lines_legacy();
        let cached = tracker.active_lines();
        let cached_again = tracker.active_lines();
        assert_eq!(
            cached.iter().map(AsRef::as_ref).collect::<Vec<_>>(),
            legacy.iter().map(String::as_str).collect::<Vec<_>>()
        );
        assert!(Arc::ptr_eq(&cached, &cached_again));

        tracker.set_axis(axis, -0.875);
        let refreshed = tracker.active_lines();
        assert!(!Arc::ptr_eq(&cached, &refreshed));
        assert!(
            refreshed
                .iter()
                .any(|line| line.contains("(-0.875) (not mapped)"))
        );
    }
}
