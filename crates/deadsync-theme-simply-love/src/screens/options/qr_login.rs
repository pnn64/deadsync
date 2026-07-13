//! QR device-login overlay — shared state machine and renderer.
//!
//! Mirrors Simply Love's `ScreenGrooveStatsLogin` design: one QR code
//! per joined player, shown side by side
//! (`BGAnimations/ScreenGrooveStatsLogin underlay/default.lua:117-165`).
//! The state machine, panel renderer, and slot bookkeeping consume plain
//! events prepared by the shell. Network workers, cancellation, and
//! credential persistence stay outside the concrete theme.

use std::sync::Arc;

use crate::act;
use crate::assets::i18n::{tr, tr_fmt};
use crate::screens::components::shared::qr_code;
use deadlib_present::actors::Actor;
use deadlib_present::color;
use deadlib_present::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use deadsync_profile as profile_data;

const ALL_SIDES: [profile_data::PlayerSide; 2] =
    [profile_data::PlayerSide::P1, profile_data::PlayerSide::P2];

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

/// Which online service this overlay is presenting.
type BackendKind = crate::SimplyLoveQrLoginService;

#[derive(Debug, Clone)]
pub(crate) enum SlotState {
    /// Side is not joined to the session; the slot is hidden entirely.
    NotJoined,
    /// Side is joined but has no Local profile loaded — login is refused.
    Guest,
    /// Shell request has started, awaiting the first display event.
    Starting,
    /// Worker has the short code + verification URL and is polling.
    Pending {
        short_code: String,
        verification_url: String,
    },
    /// Shell persisted the credential and reported completion.
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

pub(crate) struct QrLoginUiState {
    pub(crate) slots: [LoginSlot; 2],
}

/// Build theme-owned display state from the shell-prepared login request.
pub(crate) fn create_login_ui(request: &crate::SimplyLoveQrLoginRequest) -> QrLoginUiState {
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
    }
}

/// Apply credential-free progress events prepared by the shell.
pub(crate) fn apply_events(
    ui: &mut QrLoginUiState,
    events: impl IntoIterator<Item = crate::SimplyLoveQrLoginEvent>,
) {
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
                short_code,
                verification_url,
            },
            crate::SimplyLoveQrLoginEvent::Succeeded { display_name, .. } => {
                slot.display_name = display_name;
                SlotState::Success
            }
            crate::SimplyLoveQrLoginEvent::Failed { reason, .. } => SlotState::Failed { reason },
        };
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
            SlotState::Pending { ref short_code, .. } if short_code == "XYZ"
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
            SlotState::Failed { ref reason } if reason == "boom"
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
