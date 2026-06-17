//! QR device-login overlay — shared state machine and renderer.
//!
//! Mirrors Simply Love's `ScreenGrooveStatsLogin` design: one QR code
//! per joined player, shown side by side
//! (`BGAnimations/ScreenGrooveStatsLogin underlay/default.lua:117-165`).
//! The state machine, panel renderer, slot bookkeeping, and cancellation
//! plumbing are all backend-agnostic; the per-service worker that drives
//! the channel and the per-service persistence/QR-URL choice are selected
//! via [`BackendKind`].  ArrowCloud's `device-login` API is poll-based;
//! GrooveStats's flow is WebSocket-driven.  Both live in this module
//! and dispatch through the same `BackendKind` match arms.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::act;
use crate::assets::i18n::{tr, tr_fmt};
use crate::game::online::arrowcloud as ac_online;
use crate::game::profile;
use crate::screens::components::shared::qr_code;
use deadlib_present::actors::Actor;
use deadlib_present::color;
use deadlib_present::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use deadsync_online::arrowcloud as ac_api;
use deadsync_online::groovestats as gs_api;
use deadsync_profile as profile_data;

const ALL_SIDES: [profile_data::PlayerSide; 2] =
    [profile_data::PlayerSide::P1, profile_data::PlayerSide::P2];

/// Returns `true` when the ArrowCloud QR-login screen should be
/// auto-shown after Select Profile, given the current pref and live
/// session state.  Mirrors Simply Love's `Branch.AfterSelectProfile`
/// rule (`SL-Branches.lua:78-80`):
///
/// * `Always`    — always show, regardless of saved keys.
/// * `Sometimes` — show iff at least one joined Local side has an empty
///                 `arrowcloud_api_key`.  Guests and unjoined sides
///                 don't count toward the "needs key" check (matches
///                 SL's `for player in ivalues(GAMESTATE:GetHumanPlayers())`).
/// * `Disabled`  — never auto-show.
pub fn should_auto_show(when: crate::config::ArrowCloudQrLoginWhen) -> bool {
    should_auto_show_with(when, any_joined_local_side_missing_key)
}

fn should_auto_show_with<F: FnOnce() -> bool>(
    when: crate::config::ArrowCloudQrLoginWhen,
    missing_key_probe: F,
) -> bool {
    use crate::config::ArrowCloudQrLoginWhen;
    match when {
        ArrowCloudQrLoginWhen::Disabled => false,
        ArrowCloudQrLoginWhen::Always => true,
        ArrowCloudQrLoginWhen::Sometimes => missing_key_probe(),
    }
}

/// GrooveStats counterpart of [`should_auto_show`].  Same three-branch
/// rule, reading the GrooveStats per-side key for the `Sometimes` probe.
pub fn should_auto_show_groovestats(when: crate::config::GrooveStatsQrLoginWhen) -> bool {
    should_auto_show_groovestats_with(when, any_joined_local_side_missing_gs_key)
}

fn should_auto_show_groovestats_with<F: FnOnce() -> bool>(
    when: crate::config::GrooveStatsQrLoginWhen,
    missing_key_probe: F,
) -> bool {
    use crate::config::GrooveStatsQrLoginWhen;
    match when {
        GrooveStatsQrLoginWhen::Disabled => false,
        GrooveStatsQrLoginWhen::Always => true,
        GrooveStatsQrLoginWhen::Sometimes => missing_key_probe(),
    }
}

fn any_joined_local_side_missing_key() -> bool {
    ALL_SIDES.iter().any(|side| {
        if !profile::is_session_side_joined(*side) {
            return false;
        }
        if profile::is_session_side_guest(*side) {
            return false;
        }
        profile::get_for_side(*side)
            .arrowcloud_api_key
            .trim()
            .is_empty()
    })
}

#[inline]
fn side_label(kind: BackendKind, side: profile_data::PlayerSide) -> Arc<str> {
    let section = i18n_section(kind);
    match side {
        profile_data::PlayerSide::P1 => tr(section, "Player1"),
        profile_data::PlayerSide::P2 => tr(section, "Player2"),
    }
}

/// Translation section to read panel/title/footer strings out of.  Each
/// backend has its own `[<...>Login]` block in en.ini.
#[inline]
fn i18n_section(kind: BackendKind) -> &'static str {
    match kind {
        BackendKind::ArrowCloud => "ArrowCloudLogin",
        BackendKind::GrooveStats => "GrooveStatsLogin",
    }
}

/// Top-level chrome (Title / NoPlayerJoined / footer) is service-wide,
/// so it's keyed off the first slot's backend.  Slots within one UI are
/// always the same kind (set at construction time), so any slot would
/// give the same answer; this just avoids re-passing the kind around.
#[inline]
fn ui_section(ui: &QrLoginUiState) -> &'static str {
    i18n_section(ui.slots[0].kind)
}

#[derive(Debug, Clone)]
pub(crate) enum LoginMsg {
    Started {
        short_code: String,
        verification_url: String,
    },
    StatusUpdate,
    Consumed {
        api_key: String,
        /// Optional username delivered alongside the key.  ArrowCloud's
        /// device-login doesn't return one (always `None`); GrooveStats's
        /// QR-login WebSocket does and the GrooveStats backend forwards
        /// it for `[GrooveStats] Username=` persistence (SL parity).
        username: Option<String>,
    },
    Failed {
        reason: String,
    },
}

/// Which online service this overlay is talking to.  Used to dispatch
/// `Consumed` to the right `profile::set_*` helper, the per-service QR
/// URL builder, and the panel header label.  We keep this as a plain
/// enum rather than a backend trait — every per-service branch is
/// reached via `match` on this kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BackendKind {
    ArrowCloud,
    /// GrooveStats QR-login (WebSocket-driven).
    GrooveStats,
}

#[derive(Debug, Clone)]
pub(crate) enum SlotState {
    /// Side is not joined to the session; the slot is hidden entirely.
    NotJoined,
    /// Side is joined but has no Local profile loaded — login is refused.
    Guest,
    /// Worker has been spawned, awaiting the `device-login/start` response.
    Starting,
    /// Worker has the short code + verification URL and is polling.
    Pending {
        short_code: String,
        verification_url: String,
    },
    /// Server returned `consumed` and the API key was persisted.
    Success,
    /// Terminal failure for this side (network, expired, cancelled, etc.).
    Failed { reason: String },
}

impl SlotState {
    fn is_workless(&self) -> bool {
        matches!(
            self,
            SlotState::NotJoined | SlotState::Guest | SlotState::Success | SlotState::Failed { .. }
        )
    }

    fn is_visible(&self) -> bool {
        !matches!(self, SlotState::NotJoined)
    }
}

pub(crate) struct LoginSlot {
    pub(crate) side: profile_data::PlayerSide,
    pub(crate) state: SlotState,
    /// Which online service this slot is talking to.  Drives backend
    /// dispatch in [`apply_login_msg`].
    pub(crate) kind: BackendKind,
    /// Profile display name for this side (e.g. "Player 1", "Alice").
    /// Shown as the panel header so the user sees exactly which profile
    /// the key will land in.
    pub(crate) display_name: String,
    /// True iff this side already had a non-empty arrowcloud_api_key when
    /// the overlay was opened.  Used to render a "Currently signed in"
    /// badge so the user knows scanning will overwrite an existing key.
    pub(crate) had_existing_key: bool,
    /// Profile-scoped override.  When `Some`, `Consumed` writes the key
    /// via `profile::set_arrowcloud_api_key_for_id` instead of the
    /// session-side helper.  Used by the Manage Local Profiles "Link
    /// ArrowCloud" entry, where the target profile is identified by id
    /// and may not be loaded on any session side.
    pub(crate) target_profile_id: Option<String>,
    pub(crate) rx: Option<std::sync::mpsc::Receiver<LoginMsg>>,
}

pub(crate) struct QrLoginUiState {
    pub(crate) slots: [LoginSlot; 2],
    pub(crate) cancel: Arc<AtomicBool>,
}

impl Drop for QrLoginUiState {
    fn drop(&mut self) {
        // Defensive: if the overlay is torn down without going through
        // a cancel path, still signal workers to stop.
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// Spawn ArrowCloud device-login workers — one per joined Local side —
/// and return a fresh UI state ready to be rendered.
pub fn create_arrowcloud_login_ui() -> QrLoginUiState {
    let cancel = Arc::new(AtomicBool::new(false));
    let slots = build_initial_slots(&cancel, BackendKind::ArrowCloud, |_side, tx| {
        let cancel_for_thread = Arc::clone(&cancel);
        std::thread::spawn(move || {
            run_arrowcloud_login_worker(tx, cancel_for_thread);
        });
    });
    QrLoginUiState { slots, cancel }
}

/// Build a single-slot UI scoped to a specific profile (identified by
/// id + display name) rather than the active session sides.  Used by
/// the Manage Local Profiles "Link ArrowCloud" entry.
pub fn create_arrowcloud_login_ui_for_profile(
    profile_id: String,
    display_name: String,
) -> QrLoginUiState {
    let cancel = Arc::new(AtomicBool::new(false));
    let had_existing_key = !profile::get_arrowcloud_api_key_for_id(&profile_id)
        .trim()
        .is_empty();
    let (tx, rx) = std::sync::mpsc::channel::<LoginMsg>();
    let cancel_for_thread = Arc::clone(&cancel);
    std::thread::spawn(move || {
        run_arrowcloud_login_worker(tx, cancel_for_thread);
    });
    let p1_slot = LoginSlot {
        side: profile_data::PlayerSide::P1,
        state: SlotState::Starting,
        kind: BackendKind::ArrowCloud,
        display_name,
        had_existing_key,
        target_profile_id: Some(profile_id),
        rx: Some(rx),
    };
    let p2_slot = LoginSlot {
        side: profile_data::PlayerSide::P2,
        state: SlotState::NotJoined,
        kind: BackendKind::ArrowCloud,
        display_name: String::new(),
        had_existing_key: false,
        target_profile_id: None,
        rx: None,
    };
    QrLoginUiState {
        slots: [p1_slot, p2_slot],
        cancel,
    }
}

/// Decide which sides need a worker and build the slot array. `spawn`
/// callback is invoked once per side that should start polling; tests
/// inject a no-op or mock spawner.
fn build_initial_slots<F>(
    _cancel: &Arc<AtomicBool>,
    kind: BackendKind,
    mut spawn: F,
) -> [LoginSlot; 2]
where
    F: FnMut(profile_data::PlayerSide, std::sync::mpsc::Sender<LoginMsg>),
{
    let p1 = build_one_slot(profile_data::PlayerSide::P1, kind, &mut spawn);
    let p2 = build_one_slot(profile_data::PlayerSide::P2, kind, &mut spawn);
    [p1, p2]
}

fn build_one_slot<F>(side: profile_data::PlayerSide, kind: BackendKind, spawn: &mut F) -> LoginSlot
where
    F: FnMut(profile_data::PlayerSide, std::sync::mpsc::Sender<LoginMsg>),
{
    if !profile::is_session_side_joined(side) {
        return LoginSlot {
            side,
            state: SlotState::NotJoined,
            kind,
            display_name: String::new(),
            had_existing_key: false,
            target_profile_id: None,
            rx: None,
        };
    }
    if profile::is_session_side_guest(side) {
        return LoginSlot {
            side,
            state: SlotState::Guest,
            kind,
            display_name: String::new(),
            had_existing_key: false,
            target_profile_id: None,
            rx: None,
        };
    }
    let p = profile::get_for_side(side);
    let had_existing_key = match kind {
        BackendKind::ArrowCloud => !p.arrowcloud_api_key.trim().is_empty(),
        BackendKind::GrooveStats => !p.groovestats_api_key.trim().is_empty(),
    };

    let (tx, rx) = std::sync::mpsc::channel::<LoginMsg>();
    spawn(side, tx);
    LoginSlot {
        side,
        state: SlotState::Starting,
        kind,
        display_name: p.display_name,
        had_existing_key,
        target_profile_id: None,
        rx: Some(rx),
    }
}

fn run_arrowcloud_login_worker(tx: std::sync::mpsc::Sender<LoginMsg>, cancel: Arc<AtomicBool>) {
    ac_api::run_device_login_session(cancel, |event| {
        tx.send(login_msg_from_arrowcloud_event(event)).is_ok()
    });
}

fn login_msg_from_arrowcloud_event(event: ac_api::DeviceLoginEvent) -> LoginMsg {
    match event {
        ac_api::DeviceLoginEvent::Started {
            short_code,
            verification_url,
        } => LoginMsg::Started {
            short_code,
            verification_url,
        },
        ac_api::DeviceLoginEvent::StatusUpdate => LoginMsg::StatusUpdate,
        ac_api::DeviceLoginEvent::Consumed { api_key } => LoginMsg::Consumed {
            api_key,
            username: None,
        },
        ac_api::DeviceLoginEvent::Failed { reason } => LoginMsg::Failed { reason },
    }
}

/// Drain pending channel messages for every slot, updating slot state
/// and (on success) persisting api keys into per-side profiles.
pub(crate) fn poll_qr_login_ui(ui: &mut QrLoginUiState) {
    for slot in &mut ui.slots {
        let mut msgs: Vec<LoginMsg> = Vec::new();
        if let Some(rx) = slot.rx.as_ref() {
            while let Ok(msg) = rx.try_recv() {
                msgs.push(msg);
            }
        }
        for msg in msgs {
            apply_login_msg(slot, msg);
        }
    }
}

fn apply_login_msg(slot: &mut LoginSlot, msg: LoginMsg) {
    match msg {
        LoginMsg::Started {
            short_code,
            verification_url,
        } => {
            slot.state = SlotState::Pending {
                short_code,
                verification_url,
            };
        }
        LoginMsg::StatusUpdate => {
            // Approved-but-not-consumed renders identically to Pending —
            // we only flip to Success once the key has been delivered.
        }
        LoginMsg::Consumed { api_key, username } => {
            persist_consumed_key(slot, &api_key, username.as_deref());
            slot.state = SlotState::Success;
            slot.rx = None;
        }
        LoginMsg::Failed { reason } => {
            slot.state = SlotState::Failed { reason };
            slot.rx = None;
        }
    }
}

/// Dispatch the new key (and optional username) to the right per-service
/// `profile::set_*` helper, then refresh any service-status caches that
/// depend on the key.  This is the only place the `BackendKind` switch
/// is observed by the persistence layer.
fn persist_consumed_key(slot: &mut LoginSlot, api_key: &str, username: Option<&str>) {
    match slot.kind {
        BackendKind::ArrowCloud => {
            if let Some(profile_id) = slot.target_profile_id.as_ref() {
                profile::set_arrowcloud_api_key_for_id(profile_id, api_key);
            } else {
                profile::set_arrowcloud_api_key_for_side(slot.side, api_key);
                // Refresh display_name in case profile state changed.
                slot.display_name = profile::get_for_side(slot.side).display_name;
            }
            ac_online::refresh_status();
            let _ = username; // ArrowCloud's device-login never returns one.
        }
        BackendKind::GrooveStats => {
            let username = username.unwrap_or_default();
            if let Some(profile_id) = slot.target_profile_id.as_ref() {
                profile::set_groovestats_credentials_for_id(profile_id, api_key, username);
            } else {
                profile::set_groovestats_credentials_for_side(slot.side, api_key, username);
                // Refresh display_name in case profile state changed.
                slot.display_name = profile::get_for_side(slot.side).display_name;
            }
        }
    }
}

/// `true` when every slot is in a state that needs no further work —
/// i.e. it's safe to dismiss without silently dropping an in-flight
/// session.
pub(crate) fn login_overlay_is_terminal(ui: &QrLoginUiState) -> bool {
    ui.slots.iter().all(|s| s.state.is_workless())
}

pub(crate) fn build_qr_login_overlay_actors(
    ui: &QrLoginUiState,
    active_color_index: i32,
) -> Vec<Actor> {
    let mut out: Vec<Actor> = Vec::with_capacity(24);

    out.push(act!(quad:
        align(0.0, 0.0):
        xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.65):
        z(300)
    ));

    let cx = screen_center_x();
    let cy = screen_center_y();
    let visible_sides: Vec<profile_data::PlayerSide> = ALL_SIDES
        .iter()
        .copied()
        .filter(|s| {
            ui.slots[profile_data::player_side_index(*s)]
                .state
                .is_visible()
        })
        .collect();
    let section = ui_section(ui);

    out.push(act!(text:
        font("miso"):
        settext(tr(section, "Title").to_string()):
        align(0.5, 0.5):
        xy(cx, cy - 200.0):
        zoom(1.05):
        horizalign(center):
        z(301)
    ));

    if visible_sides.is_empty() {
        out.push(act!(text:
            font("miso"):
            settext(tr(section, "NoPlayerJoined").to_string()):
            align(0.5, 0.5):
            xy(cx, cy):
            zoom(0.95):
            horizalign(center):
            z(301)
        ));
        return out;
    }

    let two_up = visible_sides.len() > 1;
    let panel_offset: f32 = if two_up { 200.0 } else { 0.0 };
    let qr_size: f32 = if two_up { 150.0 } else { 200.0 };
    for (i, side) in visible_sides.iter().enumerate() {
        let slot = &ui.slots[profile_data::player_side_index(*side)];
        let dx = if two_up && i == 0 {
            -panel_offset
        } else if two_up {
            panel_offset
        } else {
            0.0
        };
        push_slot_panel(&mut out, slot, cx + dx, cy, qr_size, active_color_index);
    }

    let footer_key = if login_overlay_is_terminal(ui) {
        "ContinueHint"
    } else {
        "SkipHint"
    };
    out.push(act!(text:
        font("miso"):
        settext(tr(section, footer_key).to_string()):
        align(0.5, 0.5):
        xy(cx, cy + 200.0):
        zoom(0.9):
        horizalign(center):
        z(301)
    ));

    out
}

fn push_slot_panel(
    out: &mut Vec<Actor>,
    slot: &LoginSlot,
    panel_cx: f32,
    panel_cy: f32,
    qr_size: f32,
    active_color_index: i32,
) {
    let fill = color::decorative_rgba(active_color_index);
    let section = i18n_section(slot.kind);
    let side_label_str = side_label(slot.kind, slot.side);

    // Panel header — "Player 1 - <profile name>" so the user sees both
    // which side the panel is for and exactly which profile's
    // <service>.ini will receive the new key, on a single line.
    let header_text = if slot.display_name.is_empty() {
        side_label_str.to_string()
    } else {
        tr_fmt(
            section,
            "PanelHeader",
            &[
                ("side", side_label_str.as_ref()),
                ("name", &slot.display_name),
            ],
        )
        .to_string()
    };
    out.push(act!(text:
        font("miso"):
        settext(header_text):
        align(0.5, 0.5):
        xy(panel_cx, panel_cy - 145.0):
        zoom(0.95):
        maxwidth(320.0):
        horizalign(center):
        z(301):
        diffuse(fill[0], fill[1], fill[2], 1.0)
    ));

    match &slot.state {
        SlotState::NotJoined => {}
        SlotState::Guest => {
            out.push(act!(text:
                font("miso"):
                settext(tr_fmt(
                    section,
                    "GuestHint",
                    &[("side", side_label_str.as_ref())],
                ).to_string()):
                align(0.5, 0.5):
                xy(panel_cx, panel_cy):
                zoom(0.9):
                maxwidth(260.0):
                horizalign(center):
                z(301):
                diffuse(1.0, 0.85, 0.4, 1.0)
            ));
        }
        SlotState::Starting => {
            out.push(act!(text:
                font("miso"):
                settext(tr(section, "Contacting").to_string()):
                align(0.5, 0.5):
                xy(panel_cx, panel_cy):
                zoom(0.95):
                horizalign(center):
                z(301)
            ));
            push_status_badge(out, slot, panel_cx, panel_cy);
        }
        SlotState::Pending {
            short_code,
            verification_url,
        } => {
            let qr_actors = qr_code::build(qr_code::QrCodeParams {
                content: verification_url,
                center_x: panel_cx,
                center_y: panel_cy,
                size: qr_size,
                border_modules: 2,
                z: 301,
            });
            if qr_actors.is_empty() {
                out.push(act!(text:
                    font("miso"):
                    settext(tr(section, "QrUnavailable").to_string()):
                    align(0.5, 0.5):
                    xy(panel_cx, panel_cy):
                    zoom(0.95):
                    horizalign(center):
                    z(301):
                    diffuse(1.0, 0.3, 0.3, 1.0)
                ));
            } else {
                out.extend(qr_actors);
            }

            let below_qr = panel_cy + qr_size * 0.5;
            // GrooveStats's QR-login flow doesn't ship a short code —
            // the QR is the only verification factor.  Skip the "Code:"
            // line and slide the URL up into its slot so the panel
            // doesn't leave a "Code: " gap above the URL.
            let has_short_code = !short_code.is_empty();
            if has_short_code {
                out.push(act!(text:
                    font("miso"):
                    settext(tr_fmt(
                        section,
                        "Code",
                        &[("code", short_code.as_str())],
                    ).to_string()):
                    align(0.5, 0.5):
                    xy(panel_cx, below_qr + 20.0):
                    zoom(0.95):
                    horizalign(center):
                    z(301):
                    diffuse(fill[0], fill[1], fill[2], 1.0)
                ));
            }

            let url_y = if has_short_code {
                below_qr + 45.0
            } else {
                below_qr + 25.0
            };
            out.push(act!(text:
                font("miso"):
                settext(verification_url.clone()):
                align(0.5, 0.5):
                xy(panel_cx, url_y):
                zoom(0.7):
                maxwidth(if qr_size >= 180.0 { 360.0 } else { 260.0 }):
                horizalign(center):
                z(301):
                diffuse(0.85, 0.85, 0.85, 1.0)
            ));

            push_status_badge(out, slot, panel_cx, panel_cy - qr_size * 0.5);
        }
        SlotState::Success => {
            out.push(act!(text:
                font("miso"):
                settext(tr(section, "SignInComplete").to_string()):
                align(0.5, 0.5):
                xy(panel_cx, panel_cy):
                zoom(1.0):
                maxwidth(260.0):
                horizalign(center):
                z(301):
                diffuse(0.4, 1.0, 0.5, 1.0)
            ));
            out.push(act!(text:
                font("miso"):
                settext(tr(section, "KeySaved").to_string()):
                align(0.5, 0.5):
                xy(panel_cx, panel_cy + 26.0):
                zoom(0.8):
                maxwidth(260.0):
                horizalign(center):
                z(301):
                diffuse(0.85, 0.85, 0.85, 1.0)
            ));
        }
        SlotState::Failed { reason } => {
            out.push(act!(text:
                font("miso"):
                settext(tr_fmt(
                    section,
                    "SignInFailed",
                    &[("reason", reason.as_str())],
                ).to_string()):
                align(0.5, 0.5):
                xy(panel_cx, panel_cy):
                zoom(0.85):
                maxwidth(260.0):
                horizalign(center):
                z(301):
                diffuse(1.0, 0.4, 0.4, 1.0)
            ));
        }
    }
}

/// Small "Currently signed in" badge shown above an in-flight QR when
/// the side already has a saved API key.  Warns the user that scanning
/// will overwrite it.
fn push_status_badge(out: &mut Vec<Actor>, slot: &LoginSlot, panel_cx: f32, badge_y: f32) {
    if !slot.had_existing_key {
        return;
    }
    out.push(act!(text:
        font("miso"):
        settext(tr(i18n_section(slot.kind), "AlreadySignedInBadge").to_string()):
        align(0.5, 0.5):
        xy(panel_cx, badge_y - 18.0):
        zoom(0.65):
        maxwidth(280.0):
        horizalign(center):
        z(301):
        diffuse(1.0, 0.85, 0.4, 1.0)
    ));
}

/* -------------------- GrooveStats backend -------------------- */
//
// GrooveStats's QR-login flow is not poll-based like ArrowCloud — it
// hangs a single WebSocket on `ws://qrlogin.groovestats.com:3000`, sends
// the session UUID once, and listens for `apiKey` events keyed by side
// (1 = P1, 2 = P2).  One websocket handles both sides for the whole
// screen; the worker just routes each `apiKey` push to the matching
// per-slot sender.  Mirrors Simply Love's
// `ScreenGrooveStatsLogin underlay/default.lua`.

fn run_groovestats_login_worker(
    uuid: String,
    p1_tx: Option<std::sync::mpsc::Sender<LoginMsg>>,
    p2_tx: Option<std::sync::mpsc::Sender<LoginMsg>>,
    cancel: Arc<AtomicBool>,
) {
    gs_api::run_qr_login_session(uuid, cancel, |event| match event {
        gs_api::GrooveStatsQrLoginEvent::Failed { reason } => {
            if let Some(tx) = &p1_tx {
                let _ = tx.send(LoginMsg::Failed {
                    reason: reason.clone(),
                });
            }
            if let Some(tx) = &p2_tx {
                let _ = tx.send(LoginMsg::Failed { reason });
            }
        }
        gs_api::GrooveStatsQrLoginEvent::Consumed {
            side,
            api_key,
            username,
        } => {
            let tx = match side {
                1 => p1_tx.as_ref(),
                2 => p2_tx.as_ref(),
                _ => None,
            };
            if let Some(tx) = tx {
                let _ = tx.send(LoginMsg::Consumed {
                    api_key,
                    username: Some(username),
                });
            }
        }
    });
}

/// Spawn GrooveStats QR-login workers for every joined Local side
/// (single shared WebSocket) and return a fresh UI ready to be rendered.
#[allow(dead_code)]
pub fn create_groovestats_login_ui() -> QrLoginUiState {
    let cancel = Arc::new(AtomicBool::new(false));
    let uuid = gs_api::generate_qr_login_uuid();
    let mut p1_tx: Option<std::sync::mpsc::Sender<LoginMsg>> = None;
    let mut p2_tx: Option<std::sync::mpsc::Sender<LoginMsg>> = None;
    let slots = build_initial_slots(&cancel, BackendKind::GrooveStats, |side, tx| {
        // SL's GrooveStats flow doesn't roundtrip a `start` request — the
        // QR URL is fully known up front — so push the slot straight to
        // Pending with the per-side URL embedded.
        let _ = tx.send(LoginMsg::Started {
            short_code: String::new(),
            verification_url: gs_api::qr_login_url(&uuid, profile_data::player_side_number(side)),
        });
        match side {
            profile_data::PlayerSide::P1 => p1_tx = Some(tx),
            profile_data::PlayerSide::P2 => p2_tx = Some(tx),
        }
    });
    if p1_tx.is_some() || p2_tx.is_some() {
        let cancel_for_thread = Arc::clone(&cancel);
        std::thread::spawn(move || {
            run_groovestats_login_worker(uuid, p1_tx, p2_tx, cancel_for_thread);
        });
    }
    QrLoginUiState { slots, cancel }
}

/// Single-slot GrooveStats QR-login UI scoped to a specific profile
/// (Manage Local Profiles "Link GrooveStats" entry).
#[allow(dead_code)]
pub fn create_groovestats_login_ui_for_profile(
    profile_id: String,
    display_name: String,
) -> QrLoginUiState {
    let cancel = Arc::new(AtomicBool::new(false));
    let uuid = gs_api::generate_qr_login_uuid();
    let had_existing_key = profile::get_groovestats_api_key_for_id(&profile_id).is_some();
    let (tx, rx) = std::sync::mpsc::channel::<LoginMsg>();
    // Push the QR URL straight away — no server roundtrip required.
    let _ = tx.send(LoginMsg::Started {
        short_code: String::new(),
        verification_url: gs_api::qr_login_url(&uuid, 1),
    });
    let cancel_for_thread = Arc::clone(&cancel);
    let tx_for_thread = tx.clone();
    std::thread::spawn(move || {
        run_groovestats_login_worker(uuid, Some(tx_for_thread), None, cancel_for_thread);
    });
    drop(tx);
    let p1_slot = LoginSlot {
        side: profile_data::PlayerSide::P1,
        state: SlotState::Starting,
        kind: BackendKind::GrooveStats,
        display_name,
        had_existing_key,
        target_profile_id: Some(profile_id),
        rx: Some(rx),
    };
    let p2_slot = LoginSlot {
        side: profile_data::PlayerSide::P2,
        state: SlotState::NotJoined,
        kind: BackendKind::GrooveStats,
        display_name: String::new(),
        had_existing_key: false,
        target_profile_id: None,
        rx: None,
    };
    QrLoginUiState {
        slots: [p1_slot, p2_slot],
        cancel,
    }
}

/// Probe for `should_auto_show_groovestats(Sometimes)`.  Mirrors
/// `any_joined_local_side_missing_key` but reads the GrooveStats key
/// off each side's loaded profile.
pub(crate) fn any_joined_local_side_missing_gs_key() -> bool {
    ALL_SIDES.iter().any(|side| {
        if !profile::is_session_side_joined(*side) {
            return false;
        }
        if profile::is_session_side_guest(*side) {
            return false;
        }
        profile::get_for_side(*side)
            .groovestats_api_key
            .trim()
            .is_empty()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn slot(side: profile_data::PlayerSide, state: SlotState) -> LoginSlot {
        LoginSlot {
            side,
            state,
            kind: BackendKind::ArrowCloud,
            display_name: String::new(),
            had_existing_key: false,
            target_profile_id: None,
            rx: None,
        }
    }

    #[test]
    fn login_overlay_is_terminal_true_when_all_slots_workless() {
        let ui = QrLoginUiState {
            cancel: Arc::new(AtomicBool::new(false)),
            slots: [
                slot(profile_data::PlayerSide::P1, SlotState::Success),
                slot(profile_data::PlayerSide::P2, SlotState::NotJoined),
            ],
        };
        assert!(login_overlay_is_terminal(&ui));
    }

    #[test]
    fn login_overlay_is_terminal_false_when_any_slot_pending() {
        let ui = QrLoginUiState {
            cancel: Arc::new(AtomicBool::new(false)),
            slots: [
                slot(profile_data::PlayerSide::P1, SlotState::Success),
                slot(
                    profile_data::PlayerSide::P2,
                    SlotState::Pending {
                        short_code: "X".into(),
                        verification_url: "u".into(),
                    },
                ),
            ],
        };
        assert!(!login_overlay_is_terminal(&ui));
    }

    #[test]
    fn apply_started_message_moves_slot_to_pending() {
        let mut s = slot(profile_data::PlayerSide::P1, SlotState::Starting);
        apply_login_msg(
            &mut s,
            LoginMsg::Started {
                short_code: "XYZ".into(),
                verification_url: "https://example".into(),
            },
        );
        assert!(matches!(
            s.state,
            SlotState::Pending { ref short_code, .. } if short_code == "XYZ"
        ));
    }

    #[test]
    fn apply_failed_message_clears_rx_and_records_reason() {
        let (_tx, rx) = mpsc::channel::<LoginMsg>();
        let mut s = LoginSlot {
            side: profile_data::PlayerSide::P2,
            state: SlotState::Pending {
                short_code: "X".into(),
                verification_url: "u".into(),
            },
            kind: BackendKind::ArrowCloud,
            display_name: String::new(),
            had_existing_key: false,
            target_profile_id: None,
            rx: Some(rx),
        };
        apply_login_msg(
            &mut s,
            LoginMsg::Failed {
                reason: "boom".into(),
            },
        );
        assert!(matches!(s.state, SlotState::Failed { ref reason } if reason == "boom"));
        assert!(s.rx.is_none());
    }

    #[test]
    fn drop_signals_cancel() {
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let _ui = QrLoginUiState {
                cancel: Arc::clone(&cancel),
                slots: [
                    slot(profile_data::PlayerSide::P1, SlotState::Starting),
                    slot(profile_data::PlayerSide::P2, SlotState::NotJoined),
                ],
            };
            assert!(!cancel.load(Ordering::Relaxed));
        }
        assert!(cancel.load(Ordering::Relaxed));
    }

    #[test]
    fn slot_state_is_workless_classification() {
        assert!(SlotState::NotJoined.is_workless());
        assert!(SlotState::Guest.is_workless());
        assert!(SlotState::Success.is_workless());
        assert!(SlotState::Failed { reason: "x".into() }.is_workless());
        assert!(!SlotState::Starting.is_workless());
        assert!(
            !SlotState::Pending {
                short_code: "x".into(),
                verification_url: "y".into()
            }
            .is_workless()
        );
    }

    use crate::config::ArrowCloudQrLoginWhen;
    use crate::config::GrooveStatsQrLoginWhen;

    #[test]
    fn should_auto_show_disabled_is_always_false() {
        assert!(!should_auto_show_with(
            ArrowCloudQrLoginWhen::Disabled,
            || true
        ));
        assert!(!should_auto_show_with(
            ArrowCloudQrLoginWhen::Disabled,
            || false
        ));
    }

    #[test]
    fn should_auto_show_always_is_always_true() {
        assert!(should_auto_show_with(ArrowCloudQrLoginWhen::Always, || {
            true
        }));
        assert!(should_auto_show_with(ArrowCloudQrLoginWhen::Always, || {
            false
        }));
    }

    #[test]
    fn should_auto_show_sometimes_follows_missing_key_probe() {
        assert!(should_auto_show_with(
            ArrowCloudQrLoginWhen::Sometimes,
            || true
        ));
        assert!(!should_auto_show_with(
            ArrowCloudQrLoginWhen::Sometimes,
            || false
        ));
    }

    #[test]
    fn should_auto_show_groovestats_disabled_is_always_false() {
        assert!(!should_auto_show_groovestats_with(
            GrooveStatsQrLoginWhen::Disabled,
            || true
        ));
    }

    #[test]
    fn should_auto_show_groovestats_always_is_always_true() {
        assert!(should_auto_show_groovestats_with(
            GrooveStatsQrLoginWhen::Always,
            || false
        ));
    }

    #[test]
    fn should_auto_show_groovestats_sometimes_follows_probe() {
        assert!(should_auto_show_groovestats_with(
            GrooveStatsQrLoginWhen::Sometimes,
            || true
        ));
        assert!(!should_auto_show_groovestats_with(
            GrooveStatsQrLoginWhen::Sometimes,
            || false
        ));
    }

    /* -------------------- GrooveStats backend -------------------- */

    #[test]
    fn apply_consumed_with_username_passes_through_for_groovestats_slot() {
        // We can't actually persist (no profile storage in tests), but
        // apply_login_msg should at least transition state and clear rx
        // without panicking, regardless of slot.kind.
        let (_tx, rx) = mpsc::channel::<LoginMsg>();
        let mut s = LoginSlot {
            side: profile_data::PlayerSide::P1,
            state: SlotState::Pending {
                short_code: String::new(),
                verification_url: "u".into(),
            },
            kind: BackendKind::GrooveStats,
            display_name: "Alice".into(),
            had_existing_key: false,
            target_profile_id: Some("alice-id".into()),
            rx: Some(rx),
        };
        apply_login_msg(
            &mut s,
            LoginMsg::Consumed {
                api_key: "GS-KEY".into(),
                username: Some("alice".into()),
            },
        );
        assert!(matches!(s.state, SlotState::Success));
        assert!(s.rx.is_none());
    }
}
