mod litboard;
mod snek;

use log::warn;
use std::str::FromStr;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

const PLAYER_COUNT: usize = 2;
const CABINET_COUNT: usize = 6;
const BUTTON_COUNT: usize = 5;
const BLINK_SECONDS: f32 = 0.1;
const SERIAL_PORT_NAME_CAP: usize = 64;

#[cfg(windows)]
pub const DEFAULT_LITBOARD_PORT: &str = "COM54";
#[cfg(not(windows))]
pub const DEFAULT_LITBOARD_PORT: &str = "/dev/ttyUSB0";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SerialPortName {
    bytes: [u8; SERIAL_PORT_NAME_CAP],
    len: u8,
}

impl Default for SerialPortName {
    fn default() -> Self {
        Self::parse(DEFAULT_LITBOARD_PORT, Self::empty())
    }
}

impl SerialPortName {
    const fn empty() -> Self {
        Self {
            bytes: [0; SERIAL_PORT_NAME_CAP],
            len: 0,
        }
    }

    pub fn parse(raw: &str, default: Self) -> Self {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.len() > SERIAL_PORT_NAME_CAP {
            return default;
        }
        let mut out = Self::empty();
        for (i, b) in trimmed.bytes().enumerate() {
            if !b.is_ascii() || b.is_ascii_control() {
                return default;
            }
            out.bytes[i] = b;
            out.len += 1;
        }
        out
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..self.len as usize]).unwrap_or(DEFAULT_LITBOARD_PORT)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DriverKind {
    #[default]
    Off,
    Snek,
    Litboard,
}

impl DriverKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "None",
            Self::Snek => "Snek",
            Self::Litboard => "Litboard",
        }
    }
}

impl std::fmt::Display for DriverKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for DriverKind {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "" | "0" | "false" | "off" | "none" | "disabled" => Ok(Self::Off),
            "snek" | "snekboard" => Ok(Self::Snek),
            "lit" | "litboard" | "win32serial" | "sextetserial" | "sextetstream" => {
                Ok(Self::Litboard)
            }
            _ => Err(()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Attract,
    MenuStartOnly,
    MenuStartAndDirections,
    Gameplay,
    Stage,
    Cleared,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Player {
    P1,
    P2,
}

impl Player {
    const fn ix(self) -> usize {
        match self {
            Self::P1 => 0,
            Self::P2 => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CabinetLight {
    MarqueeUpperLeft,
    MarqueeUpperRight,
    MarqueeLowerLeft,
    MarqueeLowerRight,
    BassLeft,
    BassRight,
}

impl CabinetLight {
    const fn ix(self) -> usize {
        match self {
            Self::MarqueeUpperLeft => 0,
            Self::MarqueeUpperRight => 1,
            Self::MarqueeLowerLeft => 2,
            Self::MarqueeLowerRight => 3,
            Self::BassLeft => 4,
            Self::BassRight => 5,
        }
    }

    const fn is_marquee(self) -> bool {
        matches!(
            self,
            Self::MarqueeUpperLeft
                | Self::MarqueeUpperRight
                | Self::MarqueeLowerLeft
                | Self::MarqueeLowerRight
        )
    }

    const fn is_bass(self) -> bool {
        matches!(self, Self::BassLeft | Self::BassRight)
    }
}

const MARQUEE_LIGHTS: [CabinetLight; 4] = [
    CabinetLight::MarqueeUpperLeft,
    CabinetLight::MarqueeUpperRight,
    CabinetLight::MarqueeLowerLeft,
    CabinetLight::MarqueeLowerRight,
];

const BASS_LIGHTS: [CabinetLight; 2] = [CabinetLight::BassLeft, CabinetLight::BassRight];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonLight {
    Left,
    Down,
    Up,
    Right,
    Start,
}

impl ButtonLight {
    const fn ix(self) -> usize {
        match self {
            Self::Left => 0,
            Self::Down => 1,
            Self::Up => 2,
            Self::Right => 3,
            Self::Start => 4,
        }
    }
}

const DIRECTION_BUTTONS: [ButtonLight; 4] = [
    ButtonLight::Left,
    ButtonLight::Down,
    ButtonLight::Up,
    ButtonLight::Right,
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HideFlags {
    pub all: bool,
    pub marquee: bool,
    pub bass: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct State {
    cabinet: [bool; CABINET_COUNT],
    buttons: [[bool; BUTTON_COUNT]; PLAYER_COUNT],
}

impl State {
    pub const fn cabinet(self, light: CabinetLight) -> bool {
        self.cabinet[light.ix()]
    }

    pub const fn button(self, player: Player, button: ButtonLight) -> bool {
        self.buttons[player.ix()][button.ix()]
    }

    fn set_cabinet(&mut self, light: CabinetLight, on: bool) {
        self.cabinet[light.ix()] = on;
    }

    fn set_button(&mut self, player: Player, button: ButtonLight, on: bool) {
        self.buttons[player.ix()][button.ix()] = on;
    }
}

/// Main-thread lights state owner.
///
/// Owner: app/game logic thread. Thread-safety: single-threaded manager with a
/// bounded-size state snapshot sent to one driver worker by channel. Lifetime:
/// process/session. Capacity: latest `State`; the driver only writes changed
/// snapshots. Warmup: constructed at app startup. Gameplay miss behavior: no
/// disk or GPU work, only timer math and channel send. Eviction/pruning:
/// none. Destruction: `Drop` sends all-off and joins the worker. Worst-case
/// frame cost is O(1) over fixed two-player cabinet/button arrays.
pub struct Manager {
    worker: Option<Worker>,
    driver_kind: DriverKind,
    litboard_port: String,
    mode: Mode,
    joined: [bool; PLAYER_COUNT],
    hide: [HideFlags; PLAYER_COUNT],
    button_pressed: [[bool; BUTTON_COUNT]; PLAYER_COUNT],
    button_blink: [[f32; BUTTON_COUNT]; PLAYER_COUNT],
    cabinet_blink: [f32; CABINET_COUNT],
    last_sent: Option<State>,
}

impl Manager {
    pub fn new(kind: DriverKind, litboard_port: &str) -> Self {
        Self {
            worker: Worker::new(kind, litboard_port),
            driver_kind: kind,
            litboard_port: litboard_port.to_owned(),
            mode: Mode::Attract,
            joined: [false; PLAYER_COUNT],
            hide: [HideFlags::default(); PLAYER_COUNT],
            button_pressed: [[false; BUTTON_COUNT]; PLAYER_COUNT],
            button_blink: [[0.0; BUTTON_COUNT]; PLAYER_COUNT],
            cabinet_blink: [0.0; CABINET_COUNT],
            last_sent: None,
        }
    }

    pub fn set_driver(&mut self, kind: DriverKind, litboard_port: &str) {
        if self.driver_kind == kind && self.litboard_port == litboard_port {
            return;
        }
        if let Some(worker) = self.worker.take() {
            worker.shutdown();
        }
        self.worker = Worker::new(kind, litboard_port);
        self.driver_kind = kind;
        self.litboard_port.clear();
        self.litboard_port.push_str(litboard_port);
        self.last_sent = None;
    }

    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
    }

    pub fn set_joined(&mut self, joined: [bool; PLAYER_COUNT]) {
        self.joined = joined;
    }

    pub fn set_hide_flags(&mut self, hide: [HideFlags; PLAYER_COUNT]) {
        self.hide = hide;
    }

    pub fn set_button_pressed(&mut self, player: Player, button: ButtonLight, pressed: bool) {
        self.button_pressed[player.ix()][button.ix()] = pressed;
    }

    pub fn clear_button_pressed(&mut self) {
        self.button_pressed = [[false; BUTTON_COUNT]; PLAYER_COUNT];
    }

    pub fn blink_cabinet(&mut self, light: CabinetLight) {
        self.cabinet_blink[light.ix()] = BLINK_SECONDS;
    }

    pub fn blink_button(&mut self, player: Player, button: ButtonLight) {
        self.button_blink[player.ix()][button.ix()] = BLINK_SECONDS;
    }

    pub fn tick(&mut self, delta_seconds: f32, elapsed_seconds: f32) {
        let delta = delta_seconds.max(0.0);
        fade_timers(&mut self.cabinet_blink, delta);
        for timers in &mut self.button_blink {
            fade_timers(timers, delta);
        }
        let state = self.build_state(elapsed_seconds);
        self.push_state(state);
    }

    fn build_state(&self, elapsed_seconds: f32) -> State {
        let mut state = State::default();
        match self.mode {
            Mode::Attract => self.build_attract(&mut state, elapsed_seconds),
            Mode::MenuStartOnly => self.build_menu(&mut state, elapsed_seconds, false),
            Mode::MenuStartAndDirections => self.build_menu(&mut state, elapsed_seconds, true),
            Mode::Gameplay => self.build_gameplay(&mut state),
            Mode::Stage | Mode::Cleared => self.build_stage(&mut state, elapsed_seconds),
        }
        self.apply_physical_buttons(&mut state);
        state
    }

    fn build_attract(&self, state: &mut State, elapsed_seconds: f32) {
        let ix = ((elapsed_seconds.max(0.0) as usize) % MARQUEE_LIGHTS.len())
            .min(MARQUEE_LIGHTS.len() - 1);
        state.set_cabinet(MARQUEE_LIGHTS[ix], true);
        state.set_cabinet(CabinetLight::BassLeft, true);
    }

    fn build_menu(&self, state: &mut State, elapsed_seconds: f32, directions: bool) {
        let step = (elapsed_seconds.max(0.0) * 2.0) as usize;
        let marquee = MARQUEE_LIGHTS[step % MARQUEE_LIGHTS.len()];
        let pulse = ((elapsed_seconds.max(0.0) * 2.0).fract()) < 0.5;
        state.set_cabinet(marquee, true);
        for player in [Player::P1, Player::P2] {
            let p = player.ix();
            state.set_button(player, ButtonLight::Start, self.joined[p] || pulse);
            if directions {
                for button in DIRECTION_BUTTONS {
                    state.set_button(player, button, pulse);
                }
            }
        }
    }

    fn build_gameplay(&self, state: &mut State) {
        for light in MARQUEE_LIGHTS {
            if self.cabinet_blink[light.ix()] > 0.0 && !self.hidden(light) {
                state.set_cabinet(light, true);
            }
        }
        for light in BASS_LIGHTS {
            if self.cabinet_blink[light.ix()] > 0.0 && !self.hidden(light) {
                state.set_cabinet(light, true);
            }
        }
        for player in [Player::P1, Player::P2] {
            for button in [
                ButtonLight::Left,
                ButtonLight::Down,
                ButtonLight::Up,
                ButtonLight::Right,
                ButtonLight::Start,
            ] {
                if self.button_blink[player.ix()][button.ix()] > 0.0 {
                    state.set_button(player, button, true);
                }
            }
        }
    }

    fn build_stage(&self, state: &mut State, elapsed_seconds: f32) {
        let pulse = ((elapsed_seconds.max(0.0) * 2.0).fract()) < 0.5;
        for light in MARQUEE_LIGHTS {
            state.set_cabinet(light, true);
        }
        for light in BASS_LIGHTS {
            state.set_cabinet(light, pulse);
        }
        for player in [Player::P1, Player::P2] {
            if self.joined[player.ix()] {
                state.set_button(player, ButtonLight::Start, true);
            }
        }
    }

    fn hidden(&self, light: CabinetLight) -> bool {
        self.joined.iter().zip(self.hide).any(|(joined, hide)| {
            *joined
                && (hide.all
                    || (hide.marquee && light.is_marquee())
                    || (hide.bass && light.is_bass()))
        })
    }

    fn apply_physical_buttons(&self, state: &mut State) {
        for player in [Player::P1, Player::P2] {
            for button in [
                ButtonLight::Left,
                ButtonLight::Down,
                ButtonLight::Up,
                ButtonLight::Right,
                ButtonLight::Start,
            ] {
                if self.button_pressed[player.ix()][button.ix()] {
                    state.set_button(player, button, true);
                }
            }
        }
    }

    fn push_state(&mut self, state: State) {
        if self.last_sent == Some(state) {
            return;
        }
        if let Some(worker) = &self.worker {
            worker.send(Command::Set(state));
        }
        self.last_sent = Some(state);
    }
}

impl Drop for Manager {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            worker.shutdown();
        }
    }
}

fn fade_timers<const N: usize>(timers: &mut [f32; N], delta: f32) {
    for timer in timers {
        *timer = (*timer - delta).max(0.0);
    }
}

struct Worker {
    tx: Sender<Command>,
    join: JoinHandle<()>,
}

impl Worker {
    fn new(kind: DriverKind, litboard_port: &str) -> Option<Self> {
        if kind == DriverKind::Off {
            return None;
        }
        let (tx, rx) = mpsc::channel();
        let litboard_port = litboard_port.to_owned();
        let join = thread::Builder::new()
            .name("deadsync-lights".to_owned())
            .spawn(move || run_worker(kind, litboard_port, rx))
            .ok()?;
        Some(Self { tx, join })
    }

    fn send(&self, cmd: Command) {
        let _ = self.tx.send(cmd);
    }

    fn shutdown(self) {
        let _ = self.tx.send(Command::Set(State::default()));
        let _ = self.tx.send(Command::Shutdown);
        let _ = self.join.join();
    }
}

#[derive(Clone, Copy, Debug)]
enum Command {
    Set(State),
    Shutdown,
}

fn run_worker(kind: DriverKind, litboard_port: String, rx: Receiver<Command>) {
    let Some(mut driver) = Driver::new(kind, litboard_port) else {
        return;
    };
    while let Ok(cmd) = rx.recv() {
        let mut latest = match cmd {
            Command::Set(state) => state,
            Command::Shutdown => break,
        };
        let mut shutdown = false;
        for queued in rx.try_iter() {
            match queued {
                Command::Set(state) => latest = state,
                Command::Shutdown => shutdown = true,
            }
        }
        driver.set(&latest);
        if shutdown {
            break;
        }
    }
}

enum Driver {
    Snek(snek::Driver),
    Litboard(litboard::Driver),
}

impl Driver {
    fn new(kind: DriverKind, litboard_port: String) -> Option<Self> {
        match kind {
            DriverKind::Off => None,
            DriverKind::Snek => Some(Self::Snek(snek::Driver::new())),
            DriverKind::Litboard => Some(Self::Litboard(litboard::Driver::new(litboard_port))),
        }
    }

    fn set(&mut self, state: &State) {
        match self {
            Self::Snek(driver) => driver.set(state),
            Self::Litboard(driver) => driver.set(state),
        }
    }
}

pub fn parse_driver_or_default(raw: &str, default: DriverKind) -> DriverKind {
    DriverKind::from_str(raw).unwrap_or_else(|_| {
        warn!("Ignoring unknown LightsDriver value '{raw}'");
        default
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_board_driver_names() {
        assert_eq!(DriverKind::default().as_str(), "None");
        assert_eq!(DriverKind::from_str("None").unwrap(), DriverKind::Off);
        assert_eq!(DriverKind::from_str("Off").unwrap(), DriverKind::Off);
        assert_eq!(DriverKind::from_str("Snekboard").unwrap(), DriverKind::Snek);
        assert_eq!(
            DriverKind::from_str("Litboard").unwrap(),
            DriverKind::Litboard
        );
        assert_eq!(
            DriverKind::from_str("Win32Serial").unwrap(),
            DriverKind::Litboard
        );
    }

    #[test]
    fn parses_serial_port_names_with_default_fallback() {
        let default = SerialPortName::default();
        assert_eq!(SerialPortName::parse(" COM7 ", default).as_str(), "COM7");
        assert_eq!(SerialPortName::parse("", default), default);
        assert_eq!(SerialPortName::parse("COM\u{1b}", default), default);
    }

    #[test]
    fn hide_flags_apply_only_to_joined_players() {
        let mut lights = Manager::new(DriverKind::Off, DEFAULT_LITBOARD_PORT);
        lights.blink_cabinet(CabinetLight::MarqueeUpperLeft);
        lights.set_hide_flags([
            HideFlags {
                marquee: true,
                ..HideFlags::default()
            },
            HideFlags::default(),
        ]);
        let visible = lights.build_state(0.0);
        assert!(visible.cabinet(CabinetLight::MarqueeUpperLeft));

        lights.set_joined([true, false]);
        let hidden = lights.build_state(0.0);
        assert!(!hidden.cabinet(CabinetLight::MarqueeUpperLeft));
    }
}
