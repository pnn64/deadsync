use super::{GpSystemEvent, PadBackend, PadCode, PadDir, PadEvent, PadId, uuid_from_bytes};
use std::collections::HashMap;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use windows::Foundation::EventHandler;
use windows::Gaming::Input::{
    GameControllerSwitchPosition, Gamepad as WgiGamepad, GamepadButtons, RawGameController,
};
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};

// NOTE: WGI (Windows.Gaming.Input) does not provide per-input events; it only exposes
// added/removed events and a read-based state snapshot API. This backend therefore polls
// GetCurrentReading() and emits only diffs/edges.

// Standard mapped Gamepad button codes (small + stable; not tied to HID usages).
const CODE_BTN_A: u32 = 0x0001;
const CODE_BTN_B: u32 = 0x0002;
const CODE_BTN_X: u32 = 0x0003;
const CODE_BTN_Y: u32 = 0x0004;
const CODE_BTN_LB: u32 = 0x0005;
const CODE_BTN_RB: u32 = 0x0006;
const CODE_BTN_VIEW: u32 = 0x0007;
const CODE_BTN_MENU: u32 = 0x0008;
const CODE_BTN_LS: u32 = 0x0009;
const CODE_BTN_RS: u32 = 0x000A;
const CODE_BTN_LT2: u32 = 0x000B;
const CODE_BTN_RT2: u32 = 0x000C;

// Standard mapped Gamepad axis codes.
const CODE_AXIS_LT: u32 = 0x0100;
const CODE_AXIS_RT: u32 = 0x0101;
const CODE_AXIS_LX: u32 = 0x0102;
const CODE_AXIS_LY: u32 = 0x0103;
const CODE_AXIS_RX: u32 = 0x0104;
const CODE_AXIS_RY: u32 = 0x0105;

// Analog-to-digital thresholds (WGI values are normalized; we scale to i16).
const STICK_DIGITAL_THRESH: i16 = 16_000;
const TRIGGER_DIGITAL_THRESH: i16 = 16_000;

// RawGameController indices (fallback for non-Gamepad devices).
const RAW_BTN_BASE: u32 = 0x1000;
const RAW_AXIS_BASE: u32 = 0x2000;

const BTN_MAP: [(GamepadButtons, u32); 10] = [
    (GamepadButtons::Menu, CODE_BTN_MENU),
    (GamepadButtons::View, CODE_BTN_VIEW),
    (GamepadButtons::LeftThumbstick, CODE_BTN_LS),
    (GamepadButtons::RightThumbstick, CODE_BTN_RS),
    (GamepadButtons::LeftShoulder, CODE_BTN_LB),
    (GamepadButtons::RightShoulder, CODE_BTN_RB),
    (GamepadButtons::A, CODE_BTN_A),
    (GamepadButtons::B, CODE_BTN_B),
    (GamepadButtons::X, CODE_BTN_X),
    (GamepadButtons::Y, CODE_BTN_Y),
];

#[inline(always)]
fn pressed(btns: GamepadButtons, m: GamepadButtons) -> bool {
    (btns & m) != GamepadButtons::None
}

#[inline(always)]
fn scale_axis(v: f64) -> i16 {
    let v = if v.is_finite() { v } else { 0.0 };
    let v = v.clamp(-1.0, 1.0);
    let x = (v * 32767.0) as i32;
    x.clamp(i16::MIN as i32, i16::MAX as i32) as i16
}

#[inline(always)]
fn scale_trigger(v: f64) -> i16 {
    let v = if v.is_finite() { v } else { 0.0 };
    let v = v.clamp(0.0, 1.0);
    let x = (v * 32767.0) as i32;
    x.clamp(0, i16::MAX as i32) as i16
}

#[inline(always)]
fn dir_xy_from_switch(pos: GameControllerSwitchPosition) -> (i32, i32) {
    match pos {
        GameControllerSwitchPosition::Up => (0, 1),
        GameControllerSwitchPosition::Down => (0, -1),
        GameControllerSwitchPosition::Right => (1, 0),
        GameControllerSwitchPosition::Left => (-1, 0),
        GameControllerSwitchPosition::UpLeft => (-1, 1),
        GameControllerSwitchPosition::UpRight => (1, 1),
        GameControllerSwitchPosition::DownLeft => (-1, -1),
        GameControllerSwitchPosition::DownRight => (1, -1),
        _ => (0, 0),
    }
}

#[inline(always)]
fn uuid_from_non_roamable_id(c: &RawGameController) -> Option<[u8; 16]> {
    let id = c.NonRoamableId().ok()?;
    let s = id.to_string_lossy();
    Some(uuid_from_bytes(s.as_bytes()))
}

enum Msg {
    Added(RawGameController),
    Removed(RawGameController),
}

struct GamepadState {
    pad: WgiGamepad,
    last_time: u64,
    buttons_prev: GamepadButtons,
    axes_prev: [i16; 6],
    dir: [bool; 4],
}

struct RawState {
    last_time: u64,
    buttons_prev: Vec<bool>,
    buttons_now: Vec<bool>,
    switches: Vec<GameControllerSwitchPosition>,
    axes: Vec<f64>,
    axes_prev: Vec<i16>,
    dir: [bool; 4],
}

enum Kind {
    Gamepad(GamepadState),
    Raw(RawState),
}

struct Dev {
    id: PadId,
    name: String,
    vendor_id: Option<u16>,
    product_id: Option<u16>,
    uuid: [u8; 16],
    controller: RawGameController,
    kind: Kind,
}

struct Ctx {
    emit_pad: Box<dyn FnMut(PadEvent) + Send>,
    emit_sys: Box<dyn FnMut(GpSystemEvent) + Send>,
    devs: Vec<Dev>,
    idx_by_uuid: HashMap<[u8; 16], usize>,
    id_by_uuid: HashMap<[u8; 16], PadId>,
    next_id: u32,
    startup_grace_until: Instant,
}

impl Ctx {
    #[inline(always)]
    fn emit_disconnected(&mut self, dev: &Dev) {
        let initial = Instant::now() < self.startup_grace_until;
        (self.emit_sys)(GpSystemEvent::Disconnected {
            name: dev.name.clone(),
            id: dev.id,
            backend: PadBackend::WindowsWgi,
            initial,
        });
    }
}

fn add_controller(ctx: &mut Ctx, controller: RawGameController) {
    let Some(uuid) = uuid_from_non_roamable_id(&controller) else {
        return;
    };
    if ctx.idx_by_uuid.contains_key(&uuid) {
        return;
    }

    let id = ctx.id_by_uuid.get(&uuid).copied().unwrap_or_else(|| {
        let id = PadId(ctx.next_id);
        ctx.next_id += 1;
        ctx.id_by_uuid.insert(uuid, id);
        id
    });

    let name = controller
        .DisplayName()
        .map(|s| s.to_string_lossy())
        .unwrap_or_else(|_| "WGI".to_string());
    let vendor_id = controller.HardwareVendorId().ok();
    let product_id = controller.HardwareProductId().ok();

    let kind = if let Ok(pad) = WgiGamepad::FromGameController(&controller) {
        let (last_time, buttons_prev, axes_prev, want) =
            if let Ok(reading) = pad.GetCurrentReading() {
                let buttons_prev = reading.Buttons;
                let axes_prev = [
                    scale_trigger(reading.LeftTrigger),
                    scale_trigger(reading.RightTrigger),
                    scale_axis(reading.LeftThumbstickX),
                    scale_axis(reading.LeftThumbstickY),
                    scale_axis(reading.RightThumbstickX),
                    scale_axis(reading.RightThumbstickY),
                ];
                let dpad = [
                    pressed(buttons_prev, GamepadButtons::DPadUp),
                    pressed(buttons_prev, GamepadButtons::DPadDown),
                    pressed(buttons_prev, GamepadButtons::DPadLeft),
                    pressed(buttons_prev, GamepadButtons::DPadRight),
                ];
                let stick = [
                    axes_prev[3] >= STICK_DIGITAL_THRESH,
                    axes_prev[3] <= -STICK_DIGITAL_THRESH,
                    axes_prev[2] <= -STICK_DIGITAL_THRESH,
                    axes_prev[2] >= STICK_DIGITAL_THRESH,
                ];
                let want = [
                    dpad[0] || stick[0],
                    dpad[1] || stick[1],
                    dpad[2] || stick[2],
                    dpad[3] || stick[3],
                ];
                (reading.Timestamp, buttons_prev, axes_prev, want)
            } else {
                (0, GamepadButtons::None, [0i16; 6], [false; 4])
            };
        Kind::Gamepad(GamepadState {
            pad,
            last_time,
            buttons_prev,
            axes_prev,
            dir: want,
        })
    } else {
        let axis_count = controller.AxisCount().ok().unwrap_or(0) as usize;
        let button_count = controller.ButtonCount().ok().unwrap_or(0) as usize;
        let switch_count = controller.SwitchCount().ok().unwrap_or(0) as usize;

        let mut buttons_prev = vec![false; button_count];
        let buttons_now = vec![false; button_count];
        let mut switches = vec![GameControllerSwitchPosition::default(); switch_count];
        let mut axes = vec![0.0; axis_count];
        let last_time = controller
            .GetCurrentReading(&mut buttons_prev, &mut switches, &mut axes)
            .unwrap_or(0);
        let mut axes_prev = vec![0i16; axis_count];
        for i in 0..axis_count {
            axes_prev[i] = scale_axis(axes[i]);
        }
        let mut want = [false; 4];
        for s in &switches {
            let (x, y) = dir_xy_from_switch(*s);
            want[0] |= y > 0;
            want[1] |= y < 0;
            want[2] |= x < 0;
            want[3] |= x > 0;
        }
        Kind::Raw(RawState {
            last_time,
            buttons_prev,
            buttons_now,
            switches,
            axes,
            axes_prev,
            dir: want,
        })
    };

    let dev = Dev {
        id,
        name,
        vendor_id,
        product_id,
        uuid,
        controller,
        kind,
    };

    ctx.idx_by_uuid.insert(uuid, ctx.devs.len());
    ctx.devs.push(dev);
    let dev = ctx.devs.last().unwrap();
    let name = dev.name.clone();
    let id = dev.id;
    let vendor_id = dev.vendor_id;
    let product_id = dev.product_id;
    let initial = Instant::now() < ctx.startup_grace_until;
    (ctx.emit_sys)(GpSystemEvent::Connected {
        name,
        id,
        vendor_id,
        product_id,
        backend: PadBackend::WindowsWgi,
        initial,
    });
}

fn remove_controller(ctx: &mut Ctx, controller: RawGameController) {
    let Some(uuid) = uuid_from_non_roamable_id(&controller) else {
        return;
    };
    let Some(idx) = ctx.idx_by_uuid.remove(&uuid) else {
        return;
    };
    let dev = ctx.devs.swap_remove(idx);
    ctx.emit_disconnected(&dev);
    if idx < ctx.devs.len() {
        let uuid2 = ctx.devs[idx].uuid;
        ctx.idx_by_uuid.insert(uuid2, idx);
    }
}

#[inline(always)]
fn emit_dir_edges(
    emit_pad: &mut (dyn FnMut(PadEvent) + Send),
    id: PadId,
    dir_state: &mut [bool; 4],
    timestamp: Instant,
    want: [bool; 4],
) -> bool {
    let mut changed = false;
    let dirs = [PadDir::Up, PadDir::Down, PadDir::Left, PadDir::Right];
    for i in 0..4 {
        if dir_state[i] == want[i] {
            continue;
        }
        dir_state[i] = want[i];
        changed = true;
        (emit_pad)(PadEvent::Dir {
            id,
            timestamp,
            dir: dirs[i],
            pressed: want[i],
        });
    }
    changed
}

fn pump_gamepad(
    emit_pad: &mut (dyn FnMut(PadEvent) + Send),
    id: PadId,
    uuid: [u8; 16],
    st: &mut GamepadState,
) -> bool {
    let Ok(reading) = st.pad.GetCurrentReading() else {
        return false;
    };
    let timestamp = Instant::now();

    let old_lt = st.axes_prev[0];
    let old_rt = st.axes_prev[1];

    let lt = scale_trigger(reading.LeftTrigger);
    let rt = scale_trigger(reading.RightTrigger);
    let lx = scale_axis(reading.LeftThumbstickX);
    let ly = scale_axis(reading.LeftThumbstickY);
    let rx = scale_axis(reading.RightThumbstickX);
    let ry = scale_axis(reading.RightThumbstickY);

    let mut changed = false;
    let dpad = [
        pressed(reading.Buttons, GamepadButtons::DPadUp),
        pressed(reading.Buttons, GamepadButtons::DPadDown),
        pressed(reading.Buttons, GamepadButtons::DPadLeft),
        pressed(reading.Buttons, GamepadButtons::DPadRight),
    ];
    let stick = [
        ly >= STICK_DIGITAL_THRESH,
        ly <= -STICK_DIGITAL_THRESH,
        lx <= -STICK_DIGITAL_THRESH,
        lx >= STICK_DIGITAL_THRESH,
    ];
    let want = [
        dpad[0] || stick[0],
        dpad[1] || stick[1],
        dpad[2] || stick[2],
        dpad[3] || stick[3],
    ];
    changed |= emit_dir_edges(emit_pad, id, &mut st.dir, timestamp, want);

    for (mask, code_u32) in BTN_MAP {
        let new_pressed = pressed(reading.Buttons, mask);
        let old_pressed = pressed(st.buttons_prev, mask);
        if new_pressed == old_pressed {
            continue;
        }
        changed = true;
        (emit_pad)(PadEvent::RawButton {
            id,
            timestamp,
            code: PadCode(code_u32),
            uuid,
            value: if new_pressed { 1.0 } else { 0.0 },
            pressed: new_pressed,
        });
    }
    st.buttons_prev = reading.Buttons;

    let axes = [
        (CODE_AXIS_LT, lt),
        (CODE_AXIS_RT, rt),
        (CODE_AXIS_LX, lx),
        (CODE_AXIS_LY, ly),
        (CODE_AXIS_RX, rx),
        (CODE_AXIS_RY, ry),
    ];
    for (i, (code_u32, v)) in axes.iter().enumerate() {
        if st.axes_prev[i] == *v {
            continue;
        }
        st.axes_prev[i] = *v;
        changed = true;
        (emit_pad)(PadEvent::RawAxis {
            id,
            timestamp,
            code: PadCode(*code_u32),
            uuid,
            value: f32::from(*v),
        });
    }

    // Treat triggers as digital buttons (lets the mappings UI capture them).
    let old_lt_pressed = old_lt >= TRIGGER_DIGITAL_THRESH;
    let old_rt_pressed = old_rt >= TRIGGER_DIGITAL_THRESH;
    let lt_pressed = lt >= TRIGGER_DIGITAL_THRESH;
    let rt_pressed = rt >= TRIGGER_DIGITAL_THRESH;

    if lt_pressed != old_lt_pressed {
        changed = true;
        (emit_pad)(PadEvent::RawButton {
            id,
            timestamp,
            code: PadCode(CODE_BTN_LT2),
            uuid,
            value: if lt_pressed { 1.0 } else { 0.0 },
            pressed: lt_pressed,
        });
    }
    if rt_pressed != old_rt_pressed {
        changed = true;
        (emit_pad)(PadEvent::RawButton {
            id,
            timestamp,
            code: PadCode(CODE_BTN_RT2),
            uuid,
            value: if rt_pressed { 1.0 } else { 0.0 },
            pressed: rt_pressed,
        });
    }

    st.last_time = reading.Timestamp;
    changed
}

fn pump_raw(
    emit_pad: &mut (dyn FnMut(PadEvent) + Send),
    id: PadId,
    uuid: [u8; 16],
    controller: &RawGameController,
    st: &mut RawState,
) -> bool {
    let Ok(time) =
        controller.GetCurrentReading(&mut st.buttons_now, &mut st.switches, &mut st.axes)
    else {
        return false;
    };
    let timestamp = Instant::now();

    let mut changed = false;
    let n = st.buttons_now.len().min(st.buttons_prev.len());
    for i in 0..n {
        if st.buttons_now[i] == st.buttons_prev[i] {
            continue;
        }
        let Some(code_u32) = RAW_BTN_BASE.checked_add(i as u32) else {
            continue;
        };
        changed = true;
        (emit_pad)(PadEvent::RawButton {
            id,
            timestamp,
            code: PadCode(code_u32),
            uuid,
            value: if st.buttons_now[i] { 1.0 } else { 0.0 },
            pressed: st.buttons_now[i],
        });
    }
    std::mem::swap(&mut st.buttons_prev, &mut st.buttons_now);

    let mut want = [false; 4];
    for s in &st.switches {
        let (x, y) = dir_xy_from_switch(*s);
        want[0] |= y > 0;
        want[1] |= y < 0;
        want[2] |= x < 0;
        want[3] |= x > 0;
    }
    changed |= emit_dir_edges(emit_pad, id, &mut st.dir, timestamp, want);

    let n = st.axes.len().min(st.axes_prev.len());
    for i in 0..n {
        let v = scale_axis(st.axes[i]);
        if st.axes_prev[i] == v {
            continue;
        }
        st.axes_prev[i] = v;
        let Some(code_u32) = RAW_AXIS_BASE.checked_add(i as u32) else {
            continue;
        };
        changed = true;
        (emit_pad)(PadEvent::RawAxis {
            id,
            timestamp,
            code: PadCode(code_u32),
            uuid,
            value: f32::from(v),
        });
    }
    st.last_time = time;
    changed
}

fn enumerate_existing(ctx: &mut Ctx) {
    let Ok(list) = RawGameController::RawGameControllers() else {
        return;
    };
    let Ok(count) = list.Size() else {
        return;
    };
    // Avoid RawGameControllers.into_iter(); it has a history of crashing under Steam.
    for i in 0..count {
        if let Ok(c) = list.GetAt(i) {
            add_controller(ctx, c);
        }
    }
}

pub fn run(
    emit_pad: impl FnMut(PadEvent) + Send + 'static,
    emit_sys: impl FnMut(GpSystemEvent) + Send + 'static,
) {
    let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };

    let (tx, rx) = mpsc::channel::<Msg>();

    let added_tx = tx.clone();
    let added_handler = EventHandler::<RawGameController>::new(move |_, c| {
        if let Some(c) = c.as_ref() {
            let _ = added_tx.send(Msg::Added(c.clone()));
        }
        Ok(())
    });
    let _added_token = RawGameController::RawGameControllerAdded(&added_handler).unwrap();

    let removed_tx = tx.clone();
    let removed_handler = EventHandler::<RawGameController>::new(move |_, c| {
        if let Some(c) = c.as_ref() {
            let _ = removed_tx.send(Msg::Removed(c.clone()));
        }
        Ok(())
    });
    let _removed_token = RawGameController::RawGameControllerRemoved(&removed_handler).unwrap();

    // WGI can surface already-connected controllers slightly after startup due to WinRT's
    // async device discovery. Treat very-early adds/removes as "initial" to avoid hotplug
    // overlays for devices that were plugged in before launch.
    const STARTUP_GRACE: Duration = Duration::from_millis(3000);
    let mut ctx = Ctx {
        emit_pad: Box::new(emit_pad),
        emit_sys: Box::new(emit_sys),
        devs: Vec::new(),
        idx_by_uuid: HashMap::new(),
        id_by_uuid: HashMap::new(),
        next_id: 0,
        startup_grace_until: Instant::now() + STARTUP_GRACE,
    };

    enumerate_existing(&mut ctx);
    (ctx.emit_sys)(GpSystemEvent::StartupComplete);

    loop {
        while let Ok(msg) = rx.try_recv() {
            match msg {
                Msg::Added(c) => add_controller(&mut ctx, c),
                Msg::Removed(c) => remove_controller(&mut ctx, c),
            }
        }

        if ctx.devs.is_empty() {
            let Ok(msg) = rx.recv() else {
                continue;
            };
            match msg {
                Msg::Added(c) => add_controller(&mut ctx, c),
                Msg::Removed(c) => remove_controller(&mut ctx, c),
            }
            continue;
        }

        let emit_pad = ctx.emit_pad.as_mut();
        let mut did_update = false;
        for dev in &mut ctx.devs {
            let id = dev.id;
            let uuid = dev.uuid;
            match &mut dev.kind {
                Kind::Gamepad(st) => did_update |= pump_gamepad(emit_pad, id, uuid, st),
                Kind::Raw(st) => {
                    did_update |= pump_raw(emit_pad, id, uuid, &dev.controller, st);
                }
            }
        }
        if !did_update {
            std::thread::yield_now();
        }
    }
}
