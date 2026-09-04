//! QR device-login overlay — shared state machine and renderer.
//!
//! Mirrors Simply Love's `ScreenGrooveStatsLogin` design: one QR code
//! per joined player, shown side by side
//! (`BGAnimations/ScreenGrooveStatsLogin underlay/default.lua:117-165`).
//! The state machine, panel renderer, and slot bookkeeping consume plain
//! events prepared by the shell. Network workers, cancellation, and
//! credential persistence stay outside the concrete theme.

use std::cell::RefCell;
use std::sync::Arc;

use crate::act;
use crate::assets::i18n::{tr, tr_fmt};
use crate::screens::components::shared::qr_code;
use crate::screens::{Screen, ThemeEffect};
use deadlib_present::actors::{Actor, SizeSpec};
use deadlib_present::color;
use deadlib_present::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use deadsync_profile as profile_data;
use smallvec::SmallVec;

const ALL_SIDES: [profile_data::PlayerSide; 2] =
    [profile_data::PlayerSide::P1, profile_data::PlayerSide::P2];

pub fn append_dismiss_effects(
    effects: &mut Vec<ThemeEffect>,
    sound_path: &'static str,
    service: crate::SimplyLoveQrLoginService,
    next: Screen,
) {
    let start_len = effects.len();
    effects.extend([
        crate::effects::sfx(sound_path),
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Online(
            crate::SimplyLoveOnlineRequest::CancelQrLogin(service),
        )),
        ThemeEffect::Navigate(next),
    ]);
    debug_assert_eq!(effects.len() - start_len, 3);
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
const fn i18n_section(kind: BackendKind) -> &'static str {
    match kind {
        BackendKind::ArrowCloud => "ArrowCloudLogin",
        BackendKind::GrooveStats => "GrooveStatsLogin",
    }
}

/// Top-level chrome (Title / `NoPlayerJoined` / footer) is service-wide,
/// so it's keyed off the first slot's backend.  Slots within one UI are
/// always the same kind (set at construction time), so any slot would
/// give the same answer; this just avoids re-passing the kind around.
#[inline]
const fn ui_section(ui: &QrLoginUiState) -> &'static str {
    i18n_section(ui.slots[0].kind)
}

/// Which online service this overlay is presenting.
type BackendKind = crate::SimplyLoveQrLoginService;

#[derive(Debug, Clone)]
pub enum SlotState {
    /// Side is not joined to the session; the slot is hidden entirely.
    NotJoined,
    /// Side is joined but has no Local profile loaded — login is refused.
    Guest,
    /// Shell request has started, awaiting the first display event.
    Starting,
    /// Worker has the short code + verification URL and is polling.
    Pending {
        short_code: Arc<str>,
        verification_url: Arc<str>,
    },
    /// Shell persisted the credential and reported completion.
    Success,
    /// Terminal failure for this side (network, expired, cancelled, etc.).
    Failed { reason: Arc<str> },
}

impl SlotState {
    const fn is_workless(&self) -> bool {
        matches!(
            self,
            Self::NotJoined | Self::Guest | Self::Success | Self::Failed { .. }
        )
    }

    const fn is_visible(&self) -> bool {
        !matches!(self, Self::NotJoined)
    }
}

pub struct LoginSlot {
    pub(crate) side: profile_data::PlayerSide,
    pub(crate) state: SlotState,
    /// Which online service this slot is presenting.
    pub(crate) kind: BackendKind,
    /// Profile display name for this side (e.g. "Player 1", "Alice").
    /// Shown as the panel header so the user sees exactly which profile
    /// the key will land in.
    pub(crate) display_name: String,
    /// True iff this side already had a saved service credential when the
    /// overlay opened. Used to warn that scanning replaces that credential.
    pub(crate) had_existing_key: bool,
}

pub struct QrLoginUiState {
    pub(crate) slots: [LoginSlot; 2],
    presentation: RefCell<Option<QrLoginPresentation>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct QrLoginPresentationKey {
    active_color_index: i32,
    i18n_revision: u64,
    screen_width_bits: u32,
    screen_height_bits: u32,
}

struct QrLoginPresentation {
    key: QrLoginPresentationKey,
    children: Arc<[Actor]>,
}

/// Build theme-owned display state from the shell-prepared login request.
pub fn create_login_ui(request: &crate::SimplyLoveQrLoginRequest) -> QrLoginUiState {
    QrLoginUiState {
        slots: request.slots.clone().map(|slot| LoginSlot {
            side: slot.side,
            state: match slot.availability {
                crate::SimplyLoveQrLoginSlotAvailability::NotJoined => SlotState::NotJoined,
                crate::SimplyLoveQrLoginSlotAvailability::Guest => SlotState::Guest,
                crate::SimplyLoveQrLoginSlotAvailability::Ready => SlotState::Starting,
            },
            kind: request.service,
            display_name: slot.display_name,
            had_existing_key: slot.had_existing_key,
        }),
        presentation: RefCell::default(),
    }
}

/// Apply credential-free progress events prepared by the shell.
#[allow(clippy::redundant_pub_crate)]
pub(crate) fn apply_events(
    ui: &mut QrLoginUiState,
    events: impl IntoIterator<Item = crate::SimplyLoveQrLoginEvent>,
) {
    ui.presentation.get_mut().take();
    for event in events {
        let side = match &event {
            crate::SimplyLoveQrLoginEvent::Started { side, .. }
            | crate::SimplyLoveQrLoginEvent::Succeeded { side, .. }
            | crate::SimplyLoveQrLoginEvent::Failed { side, .. } => *side,
        };
        let slot = &mut ui.slots[profile_data::player_side_index(side)];
        slot.state = match event {
            crate::SimplyLoveQrLoginEvent::Started {
                short_code,
                verification_url,
                ..
            } => SlotState::Pending {
                short_code: short_code.into(),
                verification_url: verification_url.into(),
            },
            crate::SimplyLoveQrLoginEvent::Succeeded { display_name, .. } => {
                slot.display_name = display_name;
                SlotState::Success
            }
            crate::SimplyLoveQrLoginEvent::Failed { reason, .. } => SlotState::Failed {
                reason: reason.into(),
            },
        };
    }
}

/// `true` when every slot is in a state that needs no further work —
/// i.e. it's safe to dismiss without silently dropping an in-flight
/// session.
pub fn login_overlay_is_terminal(ui: &QrLoginUiState) -> bool {
    ui.slots.iter().all(|s| s.state.is_workless())
}

pub fn push_qr_login_overlay_actors(
    out: &mut Vec<Actor>,
    ui: &QrLoginUiState,
    active_color_index: i32,
    alpha_multiplier: f32,
) {
    let key = QrLoginPresentationKey {
        active_color_index,
        i18n_revision: crate::assets::i18n::revision(),
        screen_width_bits: screen_width().to_bits(),
        screen_height_bits: screen_height().to_bits(),
    };
    if let Some(cached) = ui.presentation.borrow().as_ref()
        && cached.key == key
    {
        push_shared_qr_login_overlay(out, Arc::clone(&cached.children), alpha_multiplier);
        return;
    }

    // One bounded immutable tree is retained by this overlay state and rebuilt only when
    // login data, color, locale, or screen geometry changes.
    let mut actors = Vec::with_capacity(24);
    push_qr_login_overlay(&mut actors, ui, active_color_index);
    let children: Arc<[Actor]> = Arc::from(actors);
    *ui.presentation.borrow_mut() = Some(QrLoginPresentation {
        key,
        children: Arc::clone(&children),
    });
    push_shared_qr_login_overlay(out, children, alpha_multiplier);
}

fn push_shared_qr_login_overlay(
    out: &mut Vec<Actor>,
    children: Arc<[Actor]>,
    alpha_multiplier: f32,
) {
    out.push(Actor::SharedFrame {
        align: [0.0, 0.0],
        offset: [0.0, 0.0],
        size: [SizeSpec::Fill, SizeSpec::Fill],
        children,
        background: None,
        z: 0,
        tint: [1.0, 1.0, 1.0, alpha_multiplier],
        blend: None,
    });
}

fn push_qr_login_overlay(out: &mut Vec<Actor>, ui: &QrLoginUiState, active_color_index: i32) {
    out.reserve(24);

    out.push(act!(quad:
        align(0.0, 0.0):
        xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.65):
        z(300)
    ));

    let cx = screen_center_x();
    let cy = screen_center_y();
    let visible_sides: SmallVec<[profile_data::PlayerSide; 2]> = ALL_SIDES
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
        settext(tr(section, "Title")):
        align(0.5, 0.5):
        xy(cx, cy - 200.0):
        zoom(1.05):
        horizalign(center):
        z(301)
    ));

    if visible_sides.is_empty() {
        out.push(act!(text:
            font("miso"):
            settext(tr(section, "NoPlayerJoined")):
            align(0.5, 0.5):
            xy(cx, cy):
            zoom(0.95):
            horizalign(center):
            z(301)
        ));
        return;
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
        push_slot_panel(out, slot, cx + dx, cy, qr_size, active_color_index);
    }

    let footer_key = if login_overlay_is_terminal(ui) {
        "ContinueHint"
    } else {
        "SkipHint"
    };
    out.push(act!(text:
        font("miso"):
        settext(tr(section, footer_key)):
        align(0.5, 0.5):
        xy(cx, cy + 200.0):
        zoom(0.9):
        horizalign(center):
        z(301)
    ));
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
        Arc::clone(&side_label_str)
    } else {
        tr_fmt(
            section,
            "PanelHeader",
            &[
                ("side", side_label_str.as_ref()),
                ("name", &slot.display_name),
            ],
        )
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
                )):
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
                settext(tr(section, "Contacting")):
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
            if !qr_code::push(
                out,
                qr_code::QrCodeParams {
                    content: verification_url,
                    center_x: panel_cx,
                    center_y: panel_cy,
                    size: qr_size,
                    border_modules: 2,
                    z: 301,
                },
            ) {
                out.push(act!(text:
                    font("miso"):
                    settext(tr(section, "QrUnavailable")):
                    align(0.5, 0.5):
                    xy(panel_cx, panel_cy):
                    zoom(0.95):
                    horizalign(center):
                    z(301):
                    diffuse(1.0, 0.3, 0.3, 1.0)
                ));
            }

            let below_qr = qr_size.mul_add(0.5, panel_cy);
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
                        &[("code", short_code.as_ref())],
                    )):
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
                settext(Arc::clone(verification_url)):
                align(0.5, 0.5):
                xy(panel_cx, url_y):
                zoom(0.7):
                maxwidth(if qr_size >= 180.0 { 360.0 } else { 260.0 }):
                horizalign(center):
                z(301):
                diffuse(0.85, 0.85, 0.85, 1.0)
            ));

            push_status_badge(out, slot, panel_cx, qr_size.mul_add(-0.5, panel_cy));
        }
        SlotState::Success => {
            out.push(act!(text:
                font("miso"):
                settext(tr(section, "SignInComplete")):
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
                settext(tr(section, "KeySaved")):
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
                    &[("reason", reason.as_ref())],
                )):
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
        settext(tr(i18n_section(slot.kind), "AlreadySignedInBadge")):
        align(0.5, 0.5):
        xy(panel_cx, badge_y - 18.0):
        zoom(0.65):
        maxwidth(280.0):
        horizalign(center):
        z(301):
        diffuse(1.0, 0.85, 0.4, 1.0)
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        SimplyLoveQrLoginEvent, SimplyLoveQrLoginRequest, SimplyLoveQrLoginService,
        SimplyLoveQrLoginSlot, SimplyLoveQrLoginSlotAvailability,
    };

    fn slot(side: profile_data::PlayerSide, state: SlotState) -> LoginSlot {
        LoginSlot {
            side,
            state,
            kind: BackendKind::ArrowCloud,
            display_name: String::new(),
            had_existing_key: false,
        }
    }

    fn request() -> SimplyLoveQrLoginRequest {
        SimplyLoveQrLoginRequest {
            service: SimplyLoveQrLoginService::ArrowCloud,
            slots: [
                SimplyLoveQrLoginSlot {
                    side: profile_data::PlayerSide::P1,
                    availability: SimplyLoveQrLoginSlotAvailability::Ready,
                    display_name: "Alice".into(),
                    had_existing_key: false,
                    target_profile_id: Some("alice".into()),
                },
                SimplyLoveQrLoginSlot {
                    side: profile_data::PlayerSide::P2,
                    availability: SimplyLoveQrLoginSlotAvailability::NotJoined,
                    display_name: String::new(),
                    had_existing_key: false,
                    target_profile_id: None,
                },
            ],
        }
    }

    #[test]
    fn prepared_request_creates_theme_only_slot_state() {
        let ui = create_login_ui(&request());
        assert!(matches!(ui.slots[0].state, SlotState::Starting));
        assert!(matches!(ui.slots[1].state, SlotState::NotJoined));
        assert_eq!(ui.slots[0].display_name, "Alice");
    }

    #[test]
    fn login_overlay_terminal_state_tracks_visible_work() {
        let mut ui = QrLoginUiState {
            slots: [
                slot(profile_data::PlayerSide::P1, SlotState::Success),
                slot(profile_data::PlayerSide::P2, SlotState::NotJoined),
            ],
            presentation: RefCell::default(),
        };
        assert!(login_overlay_is_terminal(&ui));
        ui.slots[1].state = SlotState::Starting;
        assert!(!login_overlay_is_terminal(&ui));
    }

    #[test]
    fn plain_events_update_slot_state_and_name() {
        let mut ui = create_login_ui(&request());
        apply_events(
            &mut ui,
            [SimplyLoveQrLoginEvent::Started {
                service: SimplyLoveQrLoginService::ArrowCloud,
                side: profile_data::PlayerSide::P1,
                short_code: "XYZ".into(),
                verification_url: "https://example".into(),
            }],
        );
        assert!(matches!(
            ui.slots[0].state,
            SlotState::Pending { ref short_code, .. } if short_code.as_ref() == "XYZ"
        ));

        apply_events(
            &mut ui,
            [SimplyLoveQrLoginEvent::Succeeded {
                service: SimplyLoveQrLoginService::ArrowCloud,
                side: profile_data::PlayerSide::P1,
                display_name: "Alice Updated".into(),
            }],
        );
        assert!(matches!(ui.slots[0].state, SlotState::Success));
        assert_eq!(ui.slots[0].display_name, "Alice Updated");
    }

    #[test]
    fn failed_event_records_reason() {
        let mut ui = create_login_ui(&request());
        apply_events(
            &mut ui,
            [SimplyLoveQrLoginEvent::Failed {
                service: SimplyLoveQrLoginService::ArrowCloud,
                side: profile_data::PlayerSide::P1,
                reason: "boom".into(),
            }],
        );
        assert!(matches!(
            ui.slots[0].state,
            SlotState::Failed { ref reason } if reason.as_ref() == "boom"
        ));
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
}
