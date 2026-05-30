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
use crate::engine::present::actors::Actor;
use crate::engine::present::color;
use crate::engine::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use crate::game::online::arrowcloud as ac_online;
use crate::game::profile;
use crate::screens::components::shared::qr_code;

const POLL_INTERVAL_MIN_S: f32 = 1.0;
const POLL_INTERVAL_MAX_S: f32 = 10.0;
const POLL_INTERVAL_DEFAULT_S: f32 = 3.0;

const ALL_SIDES: [profile::PlayerSide; 2] = [profile::PlayerSide::P1, profile::PlayerSide::P2];

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
fn side_ix(side: profile::PlayerSide) -> usize {
    match side {
        profile::PlayerSide::P1 => 0,
        profile::PlayerSide::P2 => 1,
    }
}

#[inline]
fn side_label(kind: BackendKind, side: profile::PlayerSide) -> Arc<str> {
    let section = i18n_section(kind);
    match side {
        profile::PlayerSide::P1 => tr(section, "Player1"),
        profile::PlayerSide::P2 => tr(section, "Player2"),
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
    pub(crate) side: profile::PlayerSide,
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
    let slots = build_initial_slots(&cancel, BackendKind::ArrowCloud, |side, tx| {
        let cancel_for_thread = Arc::clone(&cancel);
        std::thread::spawn(move || {
            run_login_session(
                side,
                tx,
                cancel_for_thread,
                ac_online::device_login_start,
                ac_online::device_login_poll,
            );
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
        run_login_session(
            profile::PlayerSide::P1, // unused when target_profile_id is Some
            tx,
            cancel_for_thread,
            ac_online::device_login_start,
            ac_online::device_login_poll,
        );
    });
    let p1_slot = LoginSlot {
        side: profile::PlayerSide::P1,
        state: SlotState::Starting,
        kind: BackendKind::ArrowCloud,
        display_name,
        had_existing_key,
        target_profile_id: Some(profile_id),
        rx: Some(rx),
    };
    let p2_slot = LoginSlot {
        side: profile::PlayerSide::P2,
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
    F: FnMut(profile::PlayerSide, std::sync::mpsc::Sender<LoginMsg>),
{
    let p1 = build_one_slot(profile::PlayerSide::P1, kind, &mut spawn);
    let p2 = build_one_slot(profile::PlayerSide::P2, kind, &mut spawn);
    [p1, p2]
}

fn build_one_slot<F>(side: profile::PlayerSide, kind: BackendKind, spawn: &mut F) -> LoginSlot
where
    F: FnMut(profile::PlayerSide, std::sync::mpsc::Sender<LoginMsg>),
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

fn run_login_session<S, P>(
    _side: profile::PlayerSide,
    tx: std::sync::mpsc::Sender<LoginMsg>,
    cancel: Arc<AtomicBool>,
    start_fn: S,
    poll_fn: P,
) where
    S: Fn(
        &ac_online::DeviceLoginStartReq,
    ) -> Result<ac_online::DeviceLoginStartResp, deadsync_net::NetworkError>,
    P: Fn(
        &ac_online::DeviceLoginPollReq,
    ) -> Result<ac_online::DeviceLoginPollResp, deadsync_net::NetworkError>,
{
    if cancel.load(Ordering::Relaxed) {
        return;
    }

    let req = ac_online::DeviceLoginStartReq {
        machine_label: None,
        client_version: Some(format!("deadsync {}", env!("CARGO_PKG_VERSION"))),
        theme_version: None,
    };
    let start = match start_fn(&req) {
        Ok(resp) => resp,
        Err(err) => {
            let _ = tx.send(LoginMsg::Failed {
                reason: format!("{err}"),
            });
            return;
        }
    };

    let mut interval_s = clamp_poll_interval(start.poll_interval_seconds);
    let poll_req = ac_online::DeviceLoginPollReq {
        session_id: start.session_id.clone(),
        poll_token: start.poll_token.clone(),
    };

    if tx
        .send(LoginMsg::Started {
            short_code: start.short_code.clone(),
            verification_url: start.verification_url.clone(),
        })
        .is_err()
    {
        return;
    }

    loop {
        if !sleep_with_cancel(interval_s, &cancel) {
            return;
        }
        match poll_fn(&poll_req) {
            Ok(resp) => {
                interval_s = clamp_poll_interval(resp.poll_interval_seconds);
                match resp.status {
                    ac_online::DeviceLoginStatus::Consumed => {
                        let api_key = resp.api_key.unwrap_or_default();
                        if api_key.trim().is_empty() {
                            let _ = tx.send(LoginMsg::Failed {
                                reason: "server returned empty api key".to_string(),
                            });
                        } else {
                            let _ = tx.send(LoginMsg::Consumed {
                                api_key,
                                username: None,
                            });
                        }
                        return;
                    }
                    ac_online::DeviceLoginStatus::Cancelled
                    | ac_online::DeviceLoginStatus::Expired => {
                        let _ = tx.send(LoginMsg::Failed {
                            reason: format!("{:?}", resp.status).to_lowercase(),
                        });
                        return;
                    }
                    ac_online::DeviceLoginStatus::Pending
                    | ac_online::DeviceLoginStatus::Approved => {
                        if tx.send(LoginMsg::StatusUpdate).is_err() {
                            return;
                        }
                    }
                }
            }
            Err(err) => {
                let _ = tx.send(LoginMsg::Failed {
                    reason: format!("{err}"),
                });
                return;
            }
        }
    }
}

fn clamp_poll_interval(seconds: Option<u64>) -> f32 {
    let raw = seconds.map(|s| s as f32).unwrap_or(POLL_INTERVAL_DEFAULT_S);
    raw.clamp(POLL_INTERVAL_MIN_S, POLL_INTERVAL_MAX_S)
}

fn sleep_with_cancel(seconds: f32, cancel: &Arc<AtomicBool>) -> bool {
    let total = std::time::Duration::from_millis((seconds * 1000.0).max(50.0) as u64);
    let mut elapsed = std::time::Duration::ZERO;
    let tick = std::time::Duration::from_millis(100);
    while elapsed < total {
        if cancel.load(Ordering::Relaxed) {
            return false;
        }
        let chunk = tick.min(total - elapsed);
        std::thread::sleep(chunk);
        elapsed += chunk;
    }
    !cancel.load(Ordering::Relaxed)
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
    let visible_sides: Vec<profile::PlayerSide> = ALL_SIDES
        .iter()
        .copied()
        .filter(|s| ui.slots[side_ix(*s)].state.is_visible())
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
        let slot = &ui.slots[side_ix(*side)];
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

const GROOVESTATS_QR_LOGIN_WS_URL: &str = "ws://qrlogin.groovestats.com:3000";
const GROOVESTATS_QR_BASE_URL: &str = "https://www.groovestats.com/qrlogin.php";
const GROOVESTATS_WS_READ_TIMEOUT_MS: u64 = 100;

/// 32-character uppercase hex string mirroring SL's
/// `CRYPTMAN:GenerateRandomUUID():gsub("-",""):upper()`.  Stable across
/// the lifetime of one overlay so the server can correlate an `apiKey`
/// push back to the right machine.
#[allow(dead_code)]
pub(crate) fn generate_qr_uuid() -> String {
    use rand::Rng;
    let mut bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    let mut out = String::with_capacity(32);
    for b in bytes {
        out.push_str(&format!("{:02X}", b));
    }
    out
}

#[allow(dead_code)]
pub(crate) fn groovestats_qr_url(uuid: &str, side: u8) -> String {
    format!("{GROOVESTATS_QR_BASE_URL}?UUID={uuid}&SIDE={side}")
}

/// Parsed envelope from the GrooveStats QR-login server.  Mirrors
/// SL's `data.uuid / data.apiKey / data.username / data.side` payload.
#[derive(serde::Deserialize, Debug)]
struct GrooveStatsWsEnvelope {
    event: String,
    #[serde(default)]
    data: Option<GrooveStatsApiKeyPayload>,
}

#[derive(serde::Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct GrooveStatsApiKeyPayload {
    #[serde(default)]
    uuid: Option<String>,
    #[serde(default)]
    api_key: Option<String>,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    side: Option<u8>,
}

/// Decide what to dispatch when a raw text frame arrives off the ws.
/// Pure function so it can be unit-tested without a live socket.
#[derive(Debug, PartialEq, Eq)]
enum GrooveStatsWsEffect {
    Ignore,
    DeliverApiKey {
        side: u8,
        api_key: String,
        username: String,
    },
}

#[allow(dead_code)]
fn classify_ws_message(text: &str, expected_uuid: &str) -> GrooveStatsWsEffect {
    let Ok(env) = serde_json::from_str::<GrooveStatsWsEnvelope>(text) else {
        return GrooveStatsWsEffect::Ignore;
    };
    if env.event != "apiKey" {
        return GrooveStatsWsEffect::Ignore;
    }
    let Some(data) = env.data else {
        return GrooveStatsWsEffect::Ignore;
    };
    if data.uuid.as_deref() != Some(expected_uuid) {
        return GrooveStatsWsEffect::Ignore;
    }
    let api_key = data.api_key.unwrap_or_default();
    if api_key.trim().is_empty() {
        return GrooveStatsWsEffect::Ignore;
    }
    let side = match data.side {
        Some(1) | Some(2) => data.side.unwrap(),
        _ => return GrooveStatsWsEffect::Ignore,
    };
    GrooveStatsWsEffect::DeliverApiKey {
        side,
        api_key,
        username: data.username.unwrap_or_default(),
    }
}

/// WebSocket worker: opens one connection for the whole overlay,
/// announces the UUID, then routes each incoming `apiKey` push to the
/// per-side sender.  Exits on cancel, server close, or send error.
#[allow(dead_code)]
fn run_groovestats_session(
    uuid: String,
    p1_tx: Option<std::sync::mpsc::Sender<LoginMsg>>,
    p2_tx: Option<std::sync::mpsc::Sender<LoginMsg>>,
    cancel: Arc<AtomicBool>,
) {
    use tungstenite::Message;
    use tungstenite::stream::MaybeTlsStream;

    if cancel.load(Ordering::Relaxed) {
        return;
    }

    let mut socket = match tungstenite::connect(GROOVESTATS_QR_LOGIN_WS_URL) {
        Ok((sock, _resp)) => sock,
        Err(err) => {
            let reason = format!("{err}");
            if let Some(tx) = &p1_tx {
                let _ = tx.send(LoginMsg::Failed {
                    reason: reason.clone(),
                });
            }
            if let Some(tx) = &p2_tx {
                let _ = tx.send(LoginMsg::Failed { reason });
            }
            return;
        }
    };

    // Plaintext ws://; the maybe-tls stream is the plain branch.  Set a
    // short read timeout so the loop can poll the cancel flag.
    if let MaybeTlsStream::Plain(tcp) = socket.get_mut() {
        let _ = tcp.set_read_timeout(Some(std::time::Duration::from_millis(
            GROOVESTATS_WS_READ_TIMEOUT_MS,
        )));
    }

    // Announce the UUID once Open.
    let hello = serde_json::json!({ "event": "uuid", "data": { "uuid": &uuid } });
    if socket
        .send(Message::Text(hello.to_string().into()))
        .is_err()
    {
        return;
    }

    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = socket.close(None);
            return;
        }
        match socket.read() {
            Ok(Message::Text(text)) => {
                let effect = classify_ws_message(&text, &uuid);
                if let GrooveStatsWsEffect::DeliverApiKey {
                    side,
                    api_key,
                    username,
                } = effect
                {
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
            }
            Ok(Message::Close(_)) => {
                let _ = socket.close(None);
                return;
            }
            Ok(_) => {} // ping/pong/binary frames ignored
            Err(tungstenite::Error::Io(io))
                if matches!(
                    io.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut,
                ) =>
            {
                // Read-timeout — loop back so we can re-check the cancel flag.
            }
            Err(_) => {
                let _ = socket.close(None);
                return;
            }
        }
    }
}

/// Spawn GrooveStats QR-login workers for every joined Local side
/// (single shared WebSocket) and return a fresh UI ready to be rendered.
#[allow(dead_code)]
pub fn create_groovestats_login_ui() -> QrLoginUiState {
    let cancel = Arc::new(AtomicBool::new(false));
    let uuid = generate_qr_uuid();
    let mut p1_tx: Option<std::sync::mpsc::Sender<LoginMsg>> = None;
    let mut p2_tx: Option<std::sync::mpsc::Sender<LoginMsg>> = None;
    let slots = build_initial_slots(&cancel, BackendKind::GrooveStats, |side, tx| {
        // SL's GrooveStats flow doesn't roundtrip a `start` request — the
        // QR URL is fully known up front — so push the slot straight to
        // Pending with the per-side URL embedded.
        let _ = tx.send(LoginMsg::Started {
            short_code: String::new(),
            verification_url: groovestats_qr_url(&uuid, gs_side_byte(side)),
        });
        match side {
            profile::PlayerSide::P1 => p1_tx = Some(tx),
            profile::PlayerSide::P2 => p2_tx = Some(tx),
        }
    });
    if p1_tx.is_some() || p2_tx.is_some() {
        let cancel_for_thread = Arc::clone(&cancel);
        std::thread::spawn(move || {
            run_groovestats_session(uuid, p1_tx, p2_tx, cancel_for_thread);
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
    let uuid = generate_qr_uuid();
    let had_existing_key = profile::get_groovestats_api_key_for_id(&profile_id).is_some();
    let (tx, rx) = std::sync::mpsc::channel::<LoginMsg>();
    // Push the QR URL straight away — no server roundtrip required.
    let _ = tx.send(LoginMsg::Started {
        short_code: String::new(),
        verification_url: groovestats_qr_url(&uuid, 1),
    });
    let cancel_for_thread = Arc::clone(&cancel);
    let tx_for_thread = tx.clone();
    std::thread::spawn(move || {
        run_groovestats_session(uuid, Some(tx_for_thread), None, cancel_for_thread);
    });
    drop(tx);
    let p1_slot = LoginSlot {
        side: profile::PlayerSide::P1,
        state: SlotState::Starting,
        kind: BackendKind::GrooveStats,
        display_name,
        had_existing_key,
        target_profile_id: Some(profile_id),
        rx: Some(rx),
    };
    let p2_slot = LoginSlot {
        side: profile::PlayerSide::P2,
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

#[inline]
fn gs_side_byte(side: profile::PlayerSide) -> u8 {
    match side {
        profile::PlayerSide::P1 => 1,
        profile::PlayerSide::P2 => 2,
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
    use deadsync_net::NetworkError;
    use std::sync::mpsc;
    use std::sync::{Arc, Mutex};

    fn drain_msgs(rx: &mpsc::Receiver<LoginMsg>) -> Vec<LoginMsg> {
        let mut out = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            out.push(msg);
        }
        out
    }

    fn make_start_ok() -> ac_online::DeviceLoginStartResp {
        ac_online::DeviceLoginStartResp {
            session_id: "sess-1".into(),
            short_code: "ABCD2345".into(),
            poll_token: "tok-1".into(),
            poll_interval_seconds: Some(0),
            verification_url: "https://arrowcloud.dance/device-login/sess-1".into(),
        }
    }

    #[test]
    fn clamp_poll_interval_uses_default_when_missing() {
        assert!((clamp_poll_interval(None) - POLL_INTERVAL_DEFAULT_S).abs() < f32::EPSILON);
    }

    #[test]
    fn clamp_poll_interval_clamps_to_min() {
        assert!((clamp_poll_interval(Some(0)) - POLL_INTERVAL_MIN_S).abs() < f32::EPSILON);
    }

    #[test]
    fn clamp_poll_interval_clamps_to_max() {
        assert!((clamp_poll_interval(Some(9999)) - POLL_INTERVAL_MAX_S).abs() < f32::EPSILON);
    }

    #[test]
    fn worker_emits_started_then_consumed() {
        let (tx, rx) = mpsc::channel::<LoginMsg>();
        let cancel = Arc::new(AtomicBool::new(false));
        let polls = Arc::new(Mutex::new(0u32));
        let polls_clone = Arc::clone(&polls);

        let start = make_start_ok();
        let start_fn = move |_req: &ac_online::DeviceLoginStartReq| -> Result<_, NetworkError> {
            Ok(start.clone())
        };
        let poll_fn = move |_req: &ac_online::DeviceLoginPollReq| -> Result<_, NetworkError> {
            let mut n = polls_clone.lock().unwrap();
            *n += 1;
            if *n == 1 {
                Ok(ac_online::DeviceLoginPollResp {
                    status: ac_online::DeviceLoginStatus::Pending,
                    poll_interval_seconds: Some(0),
                    api_key: None,
                })
            } else {
                Ok(ac_online::DeviceLoginPollResp {
                    status: ac_online::DeviceLoginStatus::Consumed,
                    poll_interval_seconds: None,
                    api_key: Some("AC-KEY-7".into()),
                })
            }
        };

        run_login_session(profile::PlayerSide::P1, tx, cancel, start_fn, poll_fn);

        let msgs = drain_msgs(&rx);
        assert!(matches!(msgs.first(), Some(LoginMsg::Started { .. })));
        assert!(
            msgs.iter()
                .any(|m| matches!(m, LoginMsg::StatusUpdate { .. }))
        );
        assert!(matches!(
            msgs.last(),
            Some(LoginMsg::Consumed { api_key, username: None }) if api_key == "AC-KEY-7"
        ));
        assert_eq!(*polls.lock().unwrap(), 2);
    }

    #[test]
    fn worker_reports_failure_on_expired() {
        let (tx, rx) = mpsc::channel::<LoginMsg>();
        let cancel = Arc::new(AtomicBool::new(false));
        let start = make_start_ok();
        let start_fn = move |_req: &ac_online::DeviceLoginStartReq| -> Result<_, NetworkError> {
            Ok(start.clone())
        };
        let poll_fn = move |_req: &ac_online::DeviceLoginPollReq| -> Result<_, NetworkError> {
            Ok(ac_online::DeviceLoginPollResp {
                status: ac_online::DeviceLoginStatus::Expired,
                poll_interval_seconds: None,
                api_key: None,
            })
        };

        run_login_session(profile::PlayerSide::P1, tx, cancel, start_fn, poll_fn);
        let msgs = drain_msgs(&rx);
        assert!(matches!(msgs.last(), Some(LoginMsg::Failed { reason }) if reason == "expired"));
    }

    #[test]
    fn worker_reports_failure_when_start_errors() {
        let (tx, rx) = mpsc::channel::<LoginMsg>();
        let cancel = Arc::new(AtomicBool::new(false));
        let start_fn = |_req: &ac_online::DeviceLoginStartReq| -> Result<_, NetworkError> {
            Err(NetworkError::Request("boom".into()))
        };
        let poll_fn = |_req: &ac_online::DeviceLoginPollReq| -> Result<_, NetworkError> {
            unreachable!("poll should not be called when start fails")
        };

        run_login_session(profile::PlayerSide::P1, tx, cancel, start_fn, poll_fn);
        let msgs = drain_msgs(&rx);
        assert!(matches!(msgs.first(), Some(LoginMsg::Failed { .. })));
        assert_eq!(msgs.len(), 1);
    }

    #[test]
    fn worker_consumed_with_empty_key_is_failure() {
        let (tx, rx) = mpsc::channel::<LoginMsg>();
        let cancel = Arc::new(AtomicBool::new(false));
        let start = make_start_ok();
        let start_fn = move |_req: &ac_online::DeviceLoginStartReq| -> Result<_, NetworkError> {
            Ok(start.clone())
        };
        let poll_fn = move |_req: &ac_online::DeviceLoginPollReq| -> Result<_, NetworkError> {
            Ok(ac_online::DeviceLoginPollResp {
                status: ac_online::DeviceLoginStatus::Consumed,
                poll_interval_seconds: None,
                api_key: Some("   ".into()),
            })
        };

        run_login_session(profile::PlayerSide::P1, tx, cancel, start_fn, poll_fn);
        let msgs = drain_msgs(&rx);
        assert!(matches!(msgs.last(), Some(LoginMsg::Failed { .. })));
    }

    #[test]
    fn sleep_with_cancel_returns_false_when_cancelled_mid_wait() {
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_for_thread = Arc::clone(&cancel);
        let handle = std::thread::spawn(move || sleep_with_cancel(5.0, &cancel_for_thread));
        std::thread::sleep(std::time::Duration::from_millis(150));
        cancel.store(true, Ordering::Relaxed);
        assert!(!handle.join().unwrap());
    }

    fn slot(side: profile::PlayerSide, state: SlotState) -> LoginSlot {
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
                slot(profile::PlayerSide::P1, SlotState::Success),
                slot(profile::PlayerSide::P2, SlotState::NotJoined),
            ],
        };
        assert!(login_overlay_is_terminal(&ui));
    }

    #[test]
    fn login_overlay_is_terminal_false_when_any_slot_pending() {
        let ui = QrLoginUiState {
            cancel: Arc::new(AtomicBool::new(false)),
            slots: [
                slot(profile::PlayerSide::P1, SlotState::Success),
                slot(
                    profile::PlayerSide::P2,
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
        let mut s = slot(profile::PlayerSide::P1, SlotState::Starting);
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
            side: profile::PlayerSide::P2,
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
                    slot(profile::PlayerSide::P1, SlotState::Starting),
                    slot(profile::PlayerSide::P2, SlotState::NotJoined),
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
    fn generate_qr_uuid_is_32_uppercase_hex() {
        let id = generate_qr_uuid();
        assert_eq!(id.len(), 32);
        assert!(
            id.chars()
                .all(|c| c.is_ascii_digit() || ('A'..='F').contains(&c)),
            "uuid contained a non-hex-uppercase char: {id}"
        );
    }

    #[test]
    fn generate_qr_uuid_returns_distinct_values() {
        let a = generate_qr_uuid();
        let b = generate_qr_uuid();
        assert_ne!(a, b);
    }

    #[test]
    fn groovestats_qr_url_format_matches_simply_love() {
        assert_eq!(
            groovestats_qr_url("ABCDEF", 1),
            "https://www.groovestats.com/qrlogin.php?UUID=ABCDEF&SIDE=1"
        );
        assert_eq!(
            groovestats_qr_url("DEADBEEF", 2),
            "https://www.groovestats.com/qrlogin.php?UUID=DEADBEEF&SIDE=2"
        );
    }

    #[test]
    fn classify_ws_message_routes_matching_uuid() {
        let payload = r#"{"event":"apiKey","data":{"uuid":"ABC","apiKey":"GS-1","username":"alice","side":1}}"#;
        assert_eq!(
            classify_ws_message(payload, "ABC"),
            GrooveStatsWsEffect::DeliverApiKey {
                side: 1,
                api_key: "GS-1".into(),
                username: "alice".into(),
            }
        );
    }

    #[test]
    fn classify_ws_message_ignores_mismatched_uuid() {
        let payload = r#"{"event":"apiKey","data":{"uuid":"OTHER","apiKey":"GS-1","side":1}}"#;
        assert_eq!(
            classify_ws_message(payload, "ABC"),
            GrooveStatsWsEffect::Ignore,
        );
    }

    #[test]
    fn classify_ws_message_ignores_non_apikey_events() {
        let payload = r#"{"event":"hello","data":{"uuid":"ABC"}}"#;
        assert_eq!(
            classify_ws_message(payload, "ABC"),
            GrooveStatsWsEffect::Ignore,
        );
    }

    #[test]
    fn classify_ws_message_ignores_empty_api_key() {
        let payload = r#"{"event":"apiKey","data":{"uuid":"ABC","apiKey":"   ","side":1}}"#;
        assert_eq!(
            classify_ws_message(payload, "ABC"),
            GrooveStatsWsEffect::Ignore,
        );
    }

    #[test]
    fn classify_ws_message_ignores_unknown_side() {
        let payload = r#"{"event":"apiKey","data":{"uuid":"ABC","apiKey":"k","side":7}}"#;
        assert_eq!(
            classify_ws_message(payload, "ABC"),
            GrooveStatsWsEffect::Ignore,
        );
    }

    #[test]
    fn classify_ws_message_defaults_missing_username_to_empty() {
        let payload = r#"{"event":"apiKey","data":{"uuid":"ABC","apiKey":"k","side":2}}"#;
        assert_eq!(
            classify_ws_message(payload, "ABC"),
            GrooveStatsWsEffect::DeliverApiKey {
                side: 2,
                api_key: "k".into(),
                username: String::new(),
            }
        );
    }

    #[test]
    fn classify_ws_message_ignores_malformed_json() {
        assert_eq!(
            classify_ws_message("not json", "ABC"),
            GrooveStatsWsEffect::Ignore,
        );
    }

    #[test]
    fn apply_consumed_with_username_passes_through_for_groovestats_slot() {
        // We can't actually persist (no profile storage in tests), but
        // apply_login_msg should at least transition state and clear rx
        // without panicking, regardless of slot.kind.
        let (_tx, rx) = mpsc::channel::<LoginMsg>();
        let mut s = LoginSlot {
            side: profile::PlayerSide::P1,
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
