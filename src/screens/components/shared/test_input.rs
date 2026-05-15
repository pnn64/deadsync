use crate::act;
use crate::assets::{FontRole, current_machine_font_key};
pub use crate::engine::input::fsr::{
    BarView as FsrBarView, VIEW_SENSOR_COUNT as FSR_BAR_COUNT, View as FsrView,
};
use crate::engine::input::{
    InputEvent, InputSource, PadDir, PadEvent, RawKeyboardEvent, VirtualAction, with_keymap,
};
use crate::engine::present::actors::Actor;
use crate::engine::present::color;
use crate::engine::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use std::collections::{HashMap, VecDeque};
use std::time::Instant;
use winit::keyboard::KeyCode;

const UNMAPPED_AXIS_HELD_THRESHOLD: f32 = 0.5;
const SORT_MENU_DIM_ALPHA: f32 = 0.875;
const SORT_MENU_CLOSE_HINT: &str = "Press &START; to dismiss.";
const EVENT_RATE_HISTORY: usize = 64;
const MAX_DISPLAY_HZ: u32 = 1000;
const FSR_BAR_WIDTH: f32 = 42.0;
const FSR_BAR_GAP: f32 = 18.0;
const FSR_BAR_HEIGHT: f32 = 160.0;
const FSR_PANEL_BG: [f32; 4] = [0.0, 0.0, 0.0, 0.68];
const FSR_PANEL_BORDER_H: f32 = 3.0;
const FSR_THRESHOLD_STEP: u16 = 5;

#[derive(Clone, Copy, Debug)]
struct FsrTheme {
    frame: [f32; 4],
    track_top: [f32; 4],
    track_active_bottom: [f32; 4],
    track_idle_bottom: [f32; 4],
    fill_top: [f32; 4],
    fill_bottom: [f32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FsrCommand {
    pub sensor_index: usize,
    pub threshold: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FsrEditResult {
    None,
    Selected,
    Threshold,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LogicalButton {
    Up,
    Down,
    Left,
    Right,
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
    fsr_view: Option<FsrView>,
    fsr_selected_bar: usize,
    fsr_pending: Option<FsrCommand>,
}

#[derive(Clone, Debug, Default)]
struct UnmappedTracker {
    held: HashMap<UnmappedKey, bool>,
    axis_value: HashMap<UnmappedKey, f32>,
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

    fn readout(&self) -> Option<(String, u32, u32)> {
        let stream = self.active_stream?;
        let stats = self.stats.get(&stream)?;
        let label = match stream {
            EventStreamKey::Keyboard => "Keyboard".to_owned(),
            EventStreamKey::Pad { dev } => format!("Gamepad {dev}"),
        };
        Some((label, stats.latest_hz, stats.max_hz()))
    }
}

#[inline(always)]
fn format_hz(hz: u32) -> String {
    if hz > MAX_DISPLAY_HZ {
        return format!(">{MAX_DISPLAY_HZ} Hz");
    }
    format!("{hz} Hz")
}

impl UnmappedTracker {
    #[inline(always)]
    fn set(&mut self, key: UnmappedKey, pressed: bool) {
        self.held.insert(key, pressed);
    }

    #[inline(always)]
    fn set_axis(&mut self, key: UnmappedKey, value: f32) {
        self.axis_value.insert(key, value);
        self.held
            .insert(key, value.abs() >= UNMAPPED_AXIS_HELD_THRESHOLD);
    }

    #[inline(always)]
    fn active_lines(&self) -> Vec<String> {
        let mut out = Vec::new();
        for (k, pressed) in &self.held {
            if !*pressed {
                continue;
            }
            let line = match *k {
                UnmappedKey::Dir { dev, dir } => format!("Gamepad {dev}: Dir::{dir:?}"),
                UnmappedKey::RawButton { dev, code_u32 } => {
                    format!("Gamepad {dev}: RawButton [0x{code_u32:08X}]")
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
}

#[inline(always)]
pub fn clear(state: &mut State) {
    *state = State::default();
}

#[inline(always)]
pub fn set_fsr_view(state: &mut State, view: Option<FsrView>) {
    state.fsr_view = view;
    if state.fsr_selected_bar >= FSR_BAR_COUNT {
        state.fsr_selected_bar = 0;
    }
}

#[inline(always)]
pub fn take_fsr_command(state: &mut State) -> Option<FsrCommand> {
    state.fsr_pending.take()
}

const fn player_from_action(act: VirtualAction) -> Option<PlayerSlot> {
    use VirtualAction::{
        p1_down, p1_left, p1_menu_left, p1_menu_right, p1_right, p1_select, p1_start, p1_up,
        p2_down, p2_left, p2_menu_left, p2_menu_right, p2_right, p2_select, p2_start, p2_up,
    };
    match act {
        p1_up | p1_down | p1_left | p1_right | p1_menu_left | p1_menu_right | p1_start
        | p1_select => Some(PlayerSlot::P1),
        p2_up | p2_down | p2_left | p2_right | p2_menu_left | p2_menu_right | p2_start
        | p2_select => Some(PlayerSlot::P2),
        _ => None,
    }
}

const fn logical_button_from_action(act: VirtualAction) -> Option<LogicalButton> {
    use VirtualAction::{
        p1_down, p1_left, p1_menu_left, p1_menu_right, p1_right, p1_select, p1_start, p1_up,
        p2_down, p2_left, p2_menu_left, p2_menu_right, p2_right, p2_select, p2_start, p2_up,
    };
    match act {
        p1_up | p2_up => Some(LogicalButton::Up),
        p1_down | p2_down => Some(LogicalButton::Down),
        p1_left | p2_left => Some(LogicalButton::Left),
        p1_right | p2_right => Some(LogicalButton::Right),
        p1_menu_left | p2_menu_left => Some(LogicalButton::MenuLeft),
        p1_menu_right | p2_menu_right => Some(LogicalButton::MenuRight),
        p1_start | p2_start => Some(LogicalButton::Start),
        p1_select | p2_select => Some(LogicalButton::Select),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FsrUiAction {
    PrevSensor,
    NextSensor,
    RaiseThreshold,
    LowerThreshold,
}

const fn fsr_ui_action(act: VirtualAction) -> Option<FsrUiAction> {
    match act {
        VirtualAction::p1_left
        | VirtualAction::p1_menu_left
        | VirtualAction::p2_left
        | VirtualAction::p2_menu_left => Some(FsrUiAction::PrevSensor),
        VirtualAction::p1_right
        | VirtualAction::p1_menu_right
        | VirtualAction::p2_right
        | VirtualAction::p2_menu_right => Some(FsrUiAction::NextSensor),
        VirtualAction::p1_up | VirtualAction::p2_up => Some(FsrUiAction::RaiseThreshold),
        VirtualAction::p1_down | VirtualAction::p2_down => Some(FsrUiAction::LowerThreshold),
        _ => None,
    }
}

#[inline(always)]
fn selected_fsr_bar(state: &State) -> usize {
    state.fsr_selected_bar.min(FSR_BAR_COUNT.saturating_sub(1))
}

#[inline(always)]
fn current_fsr_threshold(state: &State, sensor_index: usize) -> Option<u16> {
    if let Some(pending) = state.fsr_pending
        && pending.sensor_index == sensor_index
    {
        return Some(pending.threshold);
    }
    state
        .fsr_view
        .as_ref()
        .map(|view| view.bars[sensor_index].raw_threshold)
}

fn adjust_fsr_threshold(state: &mut State, delta: i32) -> FsrEditResult {
    let Some(view) = state.fsr_view.as_ref() else {
        return FsrEditResult::None;
    };
    let sensor_index = selected_fsr_bar(state);
    let Some(current) = current_fsr_threshold(state, sensor_index) else {
        return FsrEditResult::None;
    };
    let bar = &view.bars[sensor_index];
    let next = (i32::from(current) + delta).clamp(
        i32::from(bar.min_raw_threshold),
        i32::from(bar.max_raw_threshold),
    ) as u16;
    if next == current {
        return FsrEditResult::None;
    }
    state.fsr_pending = Some(FsrCommand {
        sensor_index,
        threshold: next,
    });
    FsrEditResult::Threshold
}

pub fn apply_virtual_input(state: &mut State, ev: &InputEvent) -> FsrEditResult {
    if let Some(player) = player_from_action(ev.action)
        && let Some(btn) = logical_button_from_action(ev.action)
    {
        state.buttons_held.insert((player, btn), ev.pressed);
    }
    if !ev.pressed || state.fsr_view.is_none() || ev.source == InputSource::Gamepad {
        return FsrEditResult::None;
    }
    match fsr_ui_action(ev.action) {
        Some(FsrUiAction::PrevSensor) => {
            state.fsr_selected_bar = selected_fsr_bar(state).wrapping_sub(1) % FSR_BAR_COUNT;
            FsrEditResult::Selected
        }
        Some(FsrUiAction::NextSensor) => {
            state.fsr_selected_bar = (selected_fsr_bar(state) + 1) % FSR_BAR_COUNT;
            FsrEditResult::Selected
        }
        Some(FsrUiAction::RaiseThreshold) => {
            adjust_fsr_threshold(state, i32::from(FSR_THRESHOLD_STEP))
        }
        Some(FsrUiAction::LowerThreshold) => {
            adjust_fsr_threshold(state, -i32::from(FSR_THRESHOLD_STEP))
        }
        None => FsrEditResult::None,
    }
}

pub fn apply_raw_pad_event(state: &mut State, pad_event: &PadEvent) {
    use crate::engine::input::PadEvent as PE;

    state.event_rate.record_pad(pad_event);

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
    slot: PlayerSlot,
    pad_x: f32,
    pad_y: f32,
    show_menu_buttons: bool,
    show_player_label: bool,
    z: f32,
) {
    push_pad_scaled(
        actors,
        state,
        slot,
        pad_x,
        pad_y,
        show_menu_buttons,
        show_player_label,
        z,
        1.0,
    );
}

fn push_pad_scaled(
    actors: &mut Vec<Actor>,
    state: &State,
    slot: PlayerSlot,
    pad_x: f32,
    pad_y: f32,
    show_menu_buttons: bool,
    show_player_label: bool,
    z: f32,
    scale: f32,
) {
    let arrow_h_offset = 67.0_f32 * scale;
    let arrow_v_offset = 68.0_f32 * scale;
    let sprite_zoom = 0.8_f32 * scale;
    let buttons_y = pad_y + 160.0 * scale;
    let start_y = pad_y + 146.0 * scale;
    let select_y = pad_y + 175.0 * scale;
    let menu_y = pad_y + 160.0 * scale;
    let menu_x_offset = 37.0_f32 * scale;

    actors.push(act!(sprite("test_input/dance.png"):
        align(0.5, 0.5):
        xy(pad_x, pad_y):
        zoom(sprite_zoom):
        z(z)
    ));

    if show_player_label {
        let label = match slot {
            PlayerSlot::P1 => "Player 1",
            PlayerSlot::P2 => "Player 2",
        };
        actors.push(act!(text:
            align(0.5, 0.5):
            xy(pad_x, pad_y - 130.0 * scale):
            zoom(0.7 * scale):
            font(current_machine_font_key(FontRole::Header)):
            settext(label):
            horizalign(center):
            z(z + 1.0)
        ));
    }

    actors.push(act!(sprite("test_input/highlight.png"):
        align(0.5, 0.5):
        xy(pad_x, pad_y - arrow_v_offset):
        zoom(sprite_zoom):
        diffuse(1.0, 1.0, 1.0, held_alpha(state, slot, LogicalButton::Up)):
        z(z + 1.0)
    ));
    actors.push(act!(sprite("test_input/highlight.png"):
        align(0.5, 0.5):
        xy(pad_x, pad_y + arrow_v_offset):
        zoom(sprite_zoom):
        diffuse(1.0, 1.0, 1.0, held_alpha(state, slot, LogicalButton::Down)):
        z(z + 1.0)
    ));
    actors.push(act!(sprite("test_input/highlight.png"):
        align(0.5, 0.5):
        xy(pad_x - arrow_h_offset, pad_y):
        zoom(sprite_zoom):
        diffuse(1.0, 1.0, 1.0, held_alpha(state, slot, LogicalButton::Left)):
        z(z + 1.0)
    ));
    actors.push(act!(sprite("test_input/highlight.png"):
        align(0.5, 0.5):
        xy(pad_x + arrow_h_offset, pad_y):
        zoom(sprite_zoom):
        diffuse(1.0, 1.0, 1.0, held_alpha(state, slot, LogicalButton::Right)):
        z(z + 1.0)
    ));

    if !show_menu_buttons {
        return;
    }

    let button_zoom = 0.5_f32 * scale;
    actors.push(act!(sprite("test_input/buttons.png"):
        align(0.5, 0.5):
        xy(pad_x, buttons_y):
        zoom(button_zoom):
        z(z)
    ));
    actors.push(act!(sprite("test_input/highlightgreen.png"):
        align(0.5, 0.5):
        xy(pad_x, start_y):
        zoom(button_zoom):
        diffuse(1.0, 1.0, 1.0, held_alpha(state, slot, LogicalButton::Start)):
        z(z + 1.0)
    ));
    actors.push(act!(sprite("test_input/highlightred.png"):
        align(0.5, 0.5):
        xy(pad_x, select_y):
        zoom(button_zoom):
        diffuse(1.0, 1.0, 1.0, held_alpha(state, slot, LogicalButton::Select)):
        z(z + 1.0)
    ));
    actors.push(act!(sprite("test_input/highlightarrow.png"):
        align(0.5, 0.5):
        xy(pad_x - menu_x_offset, menu_y):
        zoom(button_zoom):
        rotationz(180.0):
        diffuse(1.0, 1.0, 1.0, held_alpha(state, slot, LogicalButton::MenuLeft)):
        z(z + 1.0)
    ));
    actors.push(act!(sprite("test_input/highlightarrow.png"):
        align(0.5, 0.5):
        xy(pad_x + menu_x_offset, menu_y):
        zoom(button_zoom):
        diffuse(1.0, 1.0, 1.0, held_alpha(state, slot, LogicalButton::MenuRight)):
        z(z + 1.0)
    ));
}

fn push_polling_readout(actors: &mut Vec<Actor>, state: &State, z: f32) {
    let (rate_source, latest_hz, max_hz) = state
        .event_rate
        .readout()
        .unwrap_or_else(|| ("Waiting for raw input".to_owned(), 0, 0));

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
        settext(format!("{} latest / {} max", format_hz(latest_hz), format_hz(max_hz))):
        align(1.0, 1.0):
        xy(screen_width() - 20.0, screen_height() - 20.0):
        zoom(0.72):
        horizalign(right):
        z(z)
    ));
}

fn push_fsr_quad(actors: &mut Vec<Actor>, x: f32, y: f32, w: f32, h: f32, color: [f32; 4], z: f32) {
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    actors.push(act!(quad:
        align(0.5, 0.0):
        xy(x, y):
        zoomto(w, h):
        diffuse(color[0], color[1], color[2], color[3]):
        z(z)
    ));
}

#[inline(always)]
fn color_with_alpha(mut rgba: [f32; 4], alpha: f32) -> [f32; 4] {
    rgba[3] = alpha;
    rgba
}

#[inline(always)]
fn scale_rgb(mut rgba: [f32; 4], scale: f32, alpha: f32) -> [f32; 4] {
    rgba[0] = (rgba[0] * scale).clamp(0.0, 1.0);
    rgba[1] = (rgba[1] * scale).clamp(0.0, 1.0);
    rgba[2] = (rgba[2] * scale).clamp(0.0, 1.0);
    rgba[3] = alpha;
    rgba
}

#[inline(always)]
fn fsr_theme(active_color_index: i32) -> FsrTheme {
    let fill_bottom = color::decorative_rgba(active_color_index);
    let fill_top = color::lighten_rgba(color::decorative_rgba(active_color_index + 1));
    let track_top = color::decorative_rgba(active_color_index + 4);
    let track_active_bottom = color::decorative_rgba(active_color_index + 2);
    let frame = color::decorative_rgba(active_color_index - 2);
    FsrTheme {
        frame: color_with_alpha(frame, 0.95),
        track_top: color_with_alpha(track_top, 0.95),
        track_active_bottom: color_with_alpha(track_active_bottom, 0.92),
        track_idle_bottom: scale_rgb(track_active_bottom, 0.28, 0.78),
        fill_top: scale_rgb(fill_top, 1.0, 0.98),
        fill_bottom: color_with_alpha(fill_bottom, 0.98),
    }
}

fn push_fsr_frame(
    actors: &mut Vec<Actor>,
    center_x: f32,
    top_y: f32,
    panel_w: f32,
    panel_h: f32,
    frame_color: [f32; 4],
    z: f32,
) {
    let left = center_x - panel_w * 0.5;
    let right = center_x + panel_w * 0.5;
    push_fsr_quad(actors, center_x, top_y, panel_w, panel_h, FSR_PANEL_BG, z);
    push_fsr_quad(
        actors,
        center_x,
        top_y,
        panel_w,
        FSR_PANEL_BORDER_H,
        frame_color,
        z + 1.0,
    );
    push_fsr_quad(
        actors,
        center_x,
        top_y + panel_h - FSR_PANEL_BORDER_H,
        panel_w,
        FSR_PANEL_BORDER_H,
        frame_color,
        z + 1.0,
    );
    actors.push(act!(quad:
        align(0.0, 0.0):
        xy(left, top_y):
        zoomto(FSR_PANEL_BORDER_H, panel_h):
        diffuse(frame_color[0], frame_color[1], frame_color[2], frame_color[3]):
        z(z + 1.0)
    ));
    actors.push(act!(quad:
        align(0.0, 0.0):
        xy(right - FSR_PANEL_BORDER_H, top_y):
        zoomto(FSR_PANEL_BORDER_H, panel_h):
        diffuse(frame_color[0], frame_color[1], frame_color[2], frame_color[3]):
        z(z + 1.0)
    ));
}

fn push_fsr_outline(
    actors: &mut Vec<Actor>,
    center_x: f32,
    top_y: f32,
    width: f32,
    height: f32,
    color: [f32; 4],
    thickness: f32,
    z: f32,
) {
    let left = center_x - width * 0.5;
    let right = center_x + width * 0.5;
    push_fsr_quad(actors, center_x, top_y, width, thickness, color, z);
    push_fsr_quad(
        actors,
        center_x,
        top_y + height - thickness,
        width,
        thickness,
        color,
        z,
    );
    actors.push(act!(quad:
        align(0.0, 0.0):
        xy(left, top_y):
        zoomto(thickness, height):
        diffuse(color[0], color[1], color[2], color[3]):
        z(z)
    ));
    actors.push(act!(quad:
        align(0.0, 0.0):
        xy(right - thickness, top_y):
        zoomto(thickness, height):
        diffuse(color[0], color[1], color[2], color[3]):
        z(z)
    ));
}

fn push_fsr_bar(
    actors: &mut Vec<Actor>,
    bar: &FsrBarView,
    x: f32,
    y: f32,
    scale: f32,
    theme: FsrTheme,
    selected: bool,
    z: f32,
) {
    let bar_w = FSR_BAR_WIDTH * scale;
    let bar_h = FSR_BAR_HEIGHT * scale;
    let value_norm = bar.value_norm.clamp(0.0, 1.0);
    let threshold_norm = bar.threshold_norm.clamp(0.0, 1.0);
    let track_bottom = if bar.active {
        theme.track_active_bottom
    } else {
        theme.track_idle_bottom
    };
    let half_h = bar_h * 0.5;
    push_fsr_quad(actors, x, y, bar_w, half_h, theme.track_top, z);
    push_fsr_quad(
        actors,
        x,
        y + half_h,
        bar_w,
        bar_h - half_h,
        track_bottom,
        z,
    );

    let fill_h = value_norm * bar_h;
    if fill_h > 0.0 {
        let fill_y = y + bar_h - fill_h;
        let fill_top_h = fill_h * 0.45;
        push_fsr_quad(
            actors,
            x,
            fill_y,
            bar_w * 0.5,
            fill_top_h,
            theme.fill_top,
            z + 1.0,
        );
        push_fsr_quad(
            actors,
            x,
            fill_y + fill_top_h,
            bar_w * 0.5,
            fill_h - fill_top_h,
            theme.fill_bottom,
            z + 1.0,
        );
    }

    let threshold_h = (3.0 * scale).max(2.0);
    let threshold_y = y + (1.0 - threshold_norm) * bar_h - threshold_h * 0.5;
    push_fsr_quad(
        actors,
        x,
        threshold_y,
        bar_w,
        threshold_h,
        [1.0, 1.0, 1.0, 1.0],
        z + 2.0,
    );

    if selected {
        push_fsr_outline(
            actors,
            x,
            y - 14.0 * scale,
            bar_w + 10.0 * scale,
            bar_h + 42.0 * scale,
            [1.0, 1.0, 1.0, 1.0],
            (2.0 * scale).max(2.0),
            z + 2.5,
        );
    }

    let text_color = if selected {
        theme.frame
    } else {
        [1.0, 1.0, 1.0, 0.95]
    };

    actors.push(act!(text:
        font("miso"):
        settext(bar.raw_value.to_string()):
        align(0.5, 1.0):
        xy(x, y - 6.0 * scale):
        zoom(0.65 * scale):
        horizalign(center):
        diffuse(text_color[0], text_color[1], text_color[2], text_color[3]):
        z(z + 3.0)
    ));
    actors.push(act!(text:
        font("miso"):
        settext(format!("T{}", bar.raw_threshold)):
        align(0.5, 0.5):
        xy(x, threshold_y - 10.0 * scale):
        zoom(0.48 * scale):
        horizalign(center):
        diffuse(text_color[0], text_color[1], text_color[2], text_color[3]):
        z(z + 3.0)
    ));
    actors.push(act!(text:
        font("miso"):
        settext(bar.label):
        align(0.5, 0.0):
        xy(x, y + bar_h + 6.0 * scale):
        zoom(0.65 * scale):
        horizalign(center):
        diffuse(text_color[0], text_color[1], text_color[2], text_color[3]):
        z(z + 3.0)
    ));
}

fn push_fsr_readout(
    actors: &mut Vec<Actor>,
    state: &State,
    active_color_index: i32,
    panel_x: f32,
    panel_y: f32,
    scale: f32,
    z: f32,
) {
    let Some(fsr) = state.fsr_view.as_ref() else {
        return;
    };

    let selected = selected_fsr_bar(state);
    let selected_bar = &fsr.bars[selected];
    let selected_threshold =
        current_fsr_threshold(state, selected).unwrap_or(selected_bar.raw_threshold);
    let theme = fsr_theme(active_color_index);
    let bar_w = FSR_BAR_WIDTH * scale;
    let bar_gap = FSR_BAR_GAP * scale;
    let bar_h = FSR_BAR_HEIGHT * scale;
    let span = bar_w * FSR_BAR_COUNT as f32 + bar_gap * (FSR_BAR_COUNT - 1) as f32;
    let panel_w = span + 34.0 * scale;
    let panel_h = bar_h + 116.0 * scale;
    push_fsr_frame(actors, panel_x, panel_y, panel_w, panel_h, theme.frame, z);
    actors.push(act!(text:
        font("miso"):
        settext("DIRECT FSR"):
        align(0.5, 0.0):
        xy(panel_x, panel_y + 10.0 * scale):
        zoom(0.82 * scale):
        horizalign(center):
        diffuse(1.0, 1.0, 1.0, 0.95):
        z(z + 2.0)
    ));
    actors.push(act!(text:
        font("miso"):
        settext(fsr.device_name.clone().unwrap_or_else(|| "Analog Dance Pad".to_owned())):
        align(0.5, 0.0):
        xy(panel_x, panel_y + 30.0 * scale):
        zoom(0.58 * scale):
        horizalign(center):
        diffuse(1.0, 1.0, 1.0, 0.8):
        z(z + 2.0)
    ));
    actors.push(act!(text:
        font("miso"):
        settext(format!("{} selected  threshold {}", selected_bar.label, selected_threshold)):
        align(0.5, 0.0):
        xy(panel_x, panel_y + 44.0 * scale):
        zoom(0.55 * scale):
        horizalign(center):
        diffuse(theme.frame[0], theme.frame[1], theme.frame[2], 0.95):
        z(z + 2.0)
    ));

    let track_y = panel_y + 64.0 * scale;
    let left = panel_x - span * 0.5 + bar_w * 0.5;
    for (i, bar) in fsr.bars.iter().enumerate() {
        let x = left + i as f32 * (bar_w + bar_gap);
        push_fsr_bar(
            actors,
            bar,
            x,
            track_y,
            scale,
            theme,
            i == selected,
            z + 1.0,
        );
    }
    actors.push(act!(text:
        font("miso"):
        settext(format!("Keyboard L/R sensor   U/D threshold +/-{}", FSR_THRESHOLD_STEP)):
        align(0.5, 0.0):
        xy(panel_x, panel_y + panel_h - 20.0 * scale):
        zoom(0.5 * scale):
        horizalign(center):
        diffuse(1.0, 1.0, 1.0, 0.85):
        z(z + 2.0)
    ));
}

pub fn build_test_input_screen_content(state: &State, active_color_index: i32) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(96);
    let cx = screen_center_x();
    let cy = screen_center_y() - 20.0;
    let pad_spacing = 150.0;

    push_pad(
        &mut actors,
        state,
        PlayerSlot::P1,
        cx - pad_spacing,
        cy,
        true,
        true,
        20.0,
    );
    push_pad(
        &mut actors,
        state,
        PlayerSlot::P2,
        cx + pad_spacing,
        cy,
        true,
        true,
        20.0,
    );

    push_fsr_readout(
        &mut actors,
        state,
        active_color_index,
        screen_center_x(),
        88.0,
        1.0,
        26.0,
    );

    let lines = state.unmapped.active_lines();
    if !lines.is_empty() {
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

    actors.push(act!(text:
        font("miso"):
        settext("Hold &BACK; to return to Options."):
        align(0.5, 0.0):
        xy(cx, screen_height() - 40.0):
        zoom(0.8):
        horizalign(center):
        z(30)
    ));

    push_polling_readout(&mut actors, state, 30.0);

    actors
}

/// Build a TestInput pad for use inside an evaluation pane (SL ScreenEvaluation Pane6 parity).
///
/// `scale` scales the entire pad uniformly (1.0 = full size; SL Pane6 uses ~0.8).
pub fn build_evaluation_pad(
    state: &State,
    slot: PlayerSlot,
    pad_x: f32,
    pad_y: f32,
    scale: f32,
) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(6);
    push_pad_scaled(
        &mut actors,
        state,
        slot,
        pad_x,
        pad_y,
        false,
        false,
        100.0,
        scale,
    );
    actors
}

/// Approximate visual half-width of a pad rendered by `build_evaluation_pad` at the given scale.
/// Useful for laying out neighboring elements (e.g., gaps between two pads in Double play).
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
    /// otherwise it's left-aligned to TEXT_LEFT_X.
    pub const TITLE_CENTERED: bool = true;

    pub const TITLE_ZOOM: f32 = 1.0889;
    pub const BODY_ZOOM: f32 = 0.7778;

    pub const BODY_LINE_SPACING: i32 = 20;

    /// Pad natural full width at PAD_LOGICAL_SCALE = 1.0, in logical px.
    /// This is `(arrow_h_offset + half_arrow_sprite) * 2` from `push_pad_scaled`.
    pub const PAD_NATURAL_WIDTH: f32 = (67.0 + 27.0) * 2.0;
    /// Pad natural full height at PAD_LOGICAL_SCALE = 1.0, in logical px.
    pub const PAD_NATURAL_HEIGHT: f32 = (68.0 + 27.0) * 2.0;
}

/// Visual size of the unscaled panel in logical pixels (width, height at
/// scale 1.0).
pub fn evaluation_panel_size() -> (f32, f32) {
    (
        eval_panel_layout::PANEL_WIDTH,
        eval_panel_layout::PANEL_HEIGHT,
    )
}

/// Build the TestInput evaluation panel anchored at its **top-left corner**.
///
/// `(anchor_x, anchor_y)` is the screen-space position of the panel's
/// top-left corner. `scale` uniformly scales the entire panel.
pub fn build_evaluation_panel(
    state: &State,
    slot: PlayerSlot,
    anchor_x: f32,
    anchor_y: f32,
    scale: f32,
    title_font: &'static str,
    title: std::sync::Arc<str>,
    body_font: &'static str,
    instructions: std::sync::Arc<str>,
) -> Vec<Actor> {
    use eval_panel_layout::*;
    let mut actors = Vec::with_capacity(10);

    // Convert a panel-local (x_right, y_down) point in logical px to screen-space actor coords.
    let map = |local_x: f32, local_y_from_top: f32| -> (f32, f32) {
        (
            anchor_x + local_x * scale,
            anchor_y + local_y_from_top * scale,
        )
    };

    let (pad_x, pad_y) = {
        // PAD_X/PAD_Y refer to the pad's top-left (panel-local, y-down);
        // convert to the pad's center for push_pad_scaled.
        let pad_box_w = PAD_NATURAL_WIDTH * PAD_LOGICAL_SCALE;
        let pad_box_h = PAD_NATURAL_HEIGHT * PAD_LOGICAL_SCALE;
        let cx_local = PAD_X + pad_box_w * 0.5;
        let cy_local = PAD_Y + pad_box_h * 0.5;
        map(cx_local, cy_local)
    };
    let pad_scale = PAD_LOGICAL_SCALE * scale;
    push_pad_scaled(
        &mut actors,
        state,
        slot,
        pad_x,
        pad_y,
        false,
        false,
        100.0,
        pad_scale,
    );

    let (text_x, title_y) = map(TEXT_LEFT_X, TITLE_TOP_Y);
    let (_, divider_y) = map(TEXT_LEFT_X, TITLE_TOP_Y + DIVIDER_OFFSET);
    let (_, body_y) = map(TEXT_LEFT_X, TITLE_TOP_Y + BODY_OFFSET);
    let block_w = TEXT_BLOCK_WIDTH * scale;
    let title_zoom = TITLE_ZOOM * scale;
    let body_zoom = BODY_ZOOM * scale;

    if TITLE_CENTERED {
        let title_center_x = text_x + block_w * 0.5;
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

    actors
}

pub fn build_select_music_overlay(
    state: &State,
    active_color_index: i32,
    show_p1: bool,
    show_p2: bool,
    pad_spacing: f32,
) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(96);
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
            &mut actors,
            state,
            PlayerSlot::P1,
            cx - pad_spacing,
            cy,
            false,
            false,
            1451.0,
        );
    }
    if show_p2 {
        push_pad(
            &mut actors,
            state,
            PlayerSlot::P2,
            cx + pad_spacing,
            cy,
            false,
            false,
            1451.0,
        );
    }

    let solo_layout = show_p1 ^ show_p2;
    if state.fsr_view.is_some() {
        let (panel_x, panel_y, panel_scale) = if solo_layout {
            let empty_x = if show_p1 {
                cx + pad_spacing
            } else {
                cx - pad_spacing
            };
            (empty_x, 84.0, 1.0)
        } else {
            (cx, 50.0, 0.82)
        };
        push_fsr_readout(
            &mut actors,
            state,
            active_color_index,
            panel_x,
            panel_y,
            panel_scale,
            1452.0,
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

    push_polling_readout(&mut actors, state, 1453.0);

    actors
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::input::{InputEvent, InputSource, PadCode, PadId};
    use std::time::Duration;

    fn test_fsr_view() -> FsrView {
        FsrView {
            device_name: Some("FSR".to_owned()),
            bars: std::array::from_fn(|i| FsrBarView {
                label: match i {
                    0 => "S0",
                    1 => "S1",
                    2 => "S2",
                    _ => "S3",
                },
                raw_value: 0,
                value_norm: 0.0,
                raw_threshold: 100 + i as u16 * 10,
                threshold_norm: 0.0,
                min_raw_threshold: 0,
                max_raw_threshold: 850,
                active: false,
            }),
        }
    }

    fn input_event_from(action: VirtualAction, source: InputSource) -> InputEvent {
        let now = Instant::now();
        InputEvent {
            action,
            input_slot: 0,
            pressed: true,
            source,
            timestamp: now,
            timestamp_host_nanos: 0,
            stored_at: now,
            emitted_at: now,
        }
    }

    fn input_event(action: VirtualAction) -> InputEvent {
        input_event_from(action, InputSource::Keyboard)
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
    fn fsr_virtual_input_selects_and_queues_threshold_changes() {
        let mut state = State::default();
        set_fsr_view(&mut state, Some(test_fsr_view()));

        assert_eq!(
            apply_virtual_input(&mut state, &input_event(VirtualAction::p1_right)),
            FsrEditResult::Selected
        );
        assert_eq!(
            apply_virtual_input(&mut state, &input_event(VirtualAction::p1_up)),
            FsrEditResult::Threshold
        );
        assert_eq!(
            take_fsr_command(&mut state),
            Some(FsrCommand {
                sensor_index: 1,
                threshold: 115,
            })
        );
    }

    #[test]
    fn fsr_gamepad_input_does_not_edit_thresholds() {
        let mut state = State::default();
        set_fsr_view(&mut state, Some(test_fsr_view()));

        assert_eq!(
            apply_virtual_input(
                &mut state,
                &input_event_from(VirtualAction::p1_right, InputSource::Gamepad),
            ),
            FsrEditResult::None
        );
        assert_eq!(
            apply_virtual_input(
                &mut state,
                &input_event_from(VirtualAction::p1_up, InputSource::Gamepad),
            ),
            FsrEditResult::None
        );
        assert_eq!(selected_fsr_bar(&state), 0);
        assert_eq!(take_fsr_command(&mut state), None);
        assert_eq!(
            state
                .buttons_held
                .get(&(PlayerSlot::P1, LogicalButton::Up))
                .copied(),
            Some(true)
        );
    }
}
