use super::*;

use crate::engine::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use crate::game::online::arrowcloud as ac_online;
use crate::screens::components::shared::qr_code;

const POLL_INTERVAL_MIN_S: f32 = 1.0;
const POLL_INTERVAL_MAX_S: f32 = 10.0;
const POLL_INTERVAL_DEFAULT_S: f32 = 3.0;

#[derive(Debug, Clone)]
pub(super) enum LoginMsg {
    /// `device-login/start` returned successfully.
    Started {
        short_code: String,
        verification_url: String,
    },
    /// A `device-login/poll` returned a non-terminal status (Pending /
    /// Approved). Currently rendered identically to Pending; we keep the
    /// variant so future UI can react to Approved-but-not-consumed.
    StatusUpdate,
    /// Login completed with an API key (status == `consumed`).
    Consumed { api_key: String },
    /// Terminal error or remote-cancel.
    Failed { reason: String },
}

#[derive(Debug, Clone)]
pub(super) enum LoginPhase {
    Starting,
    Pending {
        short_code: String,
        verification_url: String,
    },
    Success,
    Failed {
        reason: String,
    },
}

pub(super) struct ArrowCloudLoginUiState {
    pub(super) phase: LoginPhase,
    pub(super) cancel_requested: Arc<AtomicBool>,
    pub(super) target_side: profile::PlayerSide,
    pub(super) rx: std::sync::mpsc::Receiver<LoginMsg>,
}

/// Spawn a new ArrowCloud device-login session and install it as the active
/// overlay. No-op if a login overlay is already active.
pub(super) fn start_arrowcloud_login(state: &mut State) {
    if state.arrowcloud_login_ui.is_some() {
        return;
    }
    clear_navigation_holds(state);

    let target_side = profile::get_session_player_side();
    let cancel_requested = Arc::new(AtomicBool::new(false));
    let cancel_for_thread = Arc::clone(&cancel_requested);
    let (tx, rx) = std::sync::mpsc::channel::<LoginMsg>();

    state.arrowcloud_login_ui = Some(ArrowCloudLoginUiState {
        phase: LoginPhase::Starting,
        cancel_requested,
        target_side,
        rx,
    });

    std::thread::spawn(move || {
        run_login_session(
            tx,
            cancel_for_thread,
            ac_online::device_login_start,
            ac_online::device_login_poll,
        );
    });
}

/// Login worker. The start/poll callables are injected so tests can drive
/// the state machine without touching the network.
fn run_login_session<S, P>(
    tx: std::sync::mpsc::Sender<LoginMsg>,
    cancel: Arc<AtomicBool>,
    start_fn: S,
    poll_fn: P,
) where
    S: Fn(
        &ac_online::DeviceLoginStartReq,
    ) -> Result<ac_online::DeviceLoginStartResp, crate::engine::network::NetworkError>,
    P: Fn(
        &ac_online::DeviceLoginPollReq,
    ) -> Result<ac_online::DeviceLoginPollResp, crate::engine::network::NetworkError>,
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

    let poll_interval_s = clamp_poll_interval(start.poll_interval_seconds);
    let session_id = start.session_id.clone();
    let poll_token = start.poll_token.clone();
    if tx
        .send(LoginMsg::Started {
            short_code: start.short_code.clone(),
            verification_url: start.verification_url.clone(),
        })
        .is_err()
    {
        return;
    }

    let mut interval_s = poll_interval_s;
    let poll_req = ac_online::DeviceLoginPollReq {
        session_id,
        poll_token,
    };

    loop {
        if !sleep_with_cancel(interval_s, &cancel) {
            return;
        }
        match poll_fn(&poll_req) {
            Ok(resp) => {
                let next = clamp_poll_interval(resp.poll_interval_seconds);
                interval_s = next;
                match resp.status {
                    ac_online::DeviceLoginStatus::Consumed => {
                        let api_key = resp.api_key.unwrap_or_default();
                        if api_key.trim().is_empty() {
                            let _ = tx.send(LoginMsg::Failed {
                                reason: "server returned empty api key".to_string(),
                            });
                        } else {
                            let _ = tx.send(LoginMsg::Consumed { api_key });
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
    let raw = seconds
        .map(|s| s as f32)
        .unwrap_or(POLL_INTERVAL_DEFAULT_S);
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

/// Drain pending channel messages, updating the overlay state and (on
/// success) persisting the api key into the active profile.
pub(super) fn poll_arrowcloud_login_ui(ui: &mut ArrowCloudLoginUiState) {
    while let Ok(msg) = ui.rx.try_recv() {
        apply_login_msg(ui, msg);
    }
}

fn apply_login_msg(ui: &mut ArrowCloudLoginUiState, msg: LoginMsg) {
    match msg {
        LoginMsg::Started {
            short_code,
            verification_url,
            ..
        } => {
            ui.phase = LoginPhase::Pending {
                short_code,
                verification_url,
            };
        }
        LoginMsg::StatusUpdate => {
            // Approved-but-not-consumed is shown identically to Pending — we
            // only flip to Success once the api key has been delivered.
        }
        LoginMsg::Consumed { api_key } => {
            profile::set_arrowcloud_api_key_for_side(ui.target_side, &api_key);
            ac_online::refresh_status();
            ui.phase = LoginPhase::Success;
        }
        LoginMsg::Failed { reason } => {
            ui.phase = LoginPhase::Failed { reason };
        }
    }
}

/// `true` if Back input should fully dismiss the overlay (vs. just request
/// a cancel that the worker can ack).
pub(super) fn login_overlay_is_terminal(ui: &ArrowCloudLoginUiState) -> bool {
    matches!(ui.phase, LoginPhase::Success | LoginPhase::Failed { .. })
}

pub(super) fn build_arrowcloud_login_overlay_actors(
    ui: &ArrowCloudLoginUiState,
    active_color_index: i32,
) -> Vec<Actor> {
    let mut out: Vec<Actor> = Vec::with_capacity(10);

    out.push(act!(quad:
        align(0.0, 0.0):
        xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.78):
        z(300)
    ));

    let title = match &ui.phase {
        LoginPhase::Starting => "Contacting ArrowCloud...",
        LoginPhase::Pending { .. } => "Sign in to ArrowCloud",
        LoginPhase::Success => "Signed In",
        LoginPhase::Failed { .. } => "Login Failed",
    };
    let cx = screen_center_x();
    let cy = screen_center_y();

    out.push(act!(text:
        font("miso"):
        settext(title):
        align(0.5, 0.5):
        xy(cx, cy - 200.0):
        zoom(1.2):
        horizalign(center):
        z(301)
    ));

    match &ui.phase {
        LoginPhase::Starting => {
            out.push(act!(text:
                font("miso"):
                settext("Please wait..."):
                align(0.5, 0.5):
                xy(cx, cy):
                zoom(0.95):
                horizalign(center):
                z(301)
            ));
        }
        LoginPhase::Pending {
            short_code,
            verification_url,
        } => {
            let qr_size = 240.0_f32;
            let qr_actors = qr_code::build(qr_code::QrCodeParams {
                content: verification_url,
                center_x: cx,
                center_y: cy - 10.0,
                size: qr_size,
                border_modules: 2,
                z: 301,
            });
            if qr_actors.is_empty() {
                out.push(act!(text:
                    font("miso"):
                    settext("QR Unavailable"):
                    align(0.5, 0.5):
                    xy(cx, cy):
                    zoom(0.9):
                    horizalign(center):
                    z(301):
                    diffuse(1.0, 0.3, 0.3, 1.0)
                ));
            } else {
                out.extend(qr_actors);
            }

            out.push(act!(text:
                font("miso"):
                settext("Scan with your phone, or visit:"):
                align(0.5, 0.5):
                xy(cx, cy - 150.0):
                zoom(0.85):
                horizalign(center):
                z(301)
            ));

            out.push(act!(text:
                font("miso"):
                settext(verification_url.clone()):
                align(0.5, 0.5):
                xy(cx, cy + 130.0):
                zoom(0.7):
                maxwidth(screen_width() * 0.9):
                horizalign(center):
                z(301)
            ));

            let fill = color::decorative_rgba(active_color_index);
            out.push(act!(text:
                font("miso"):
                settext(format!("Code: {short_code}")):
                align(0.5, 0.5):
                xy(cx, cy + 160.0):
                zoom(1.1):
                horizalign(center):
                z(301):
                diffuse(fill[0], fill[1], fill[2], 1.0)
            ));

            out.push(act!(text:
                font("miso"):
                settext("Press Back to cancel."):
                align(0.5, 0.5):
                xy(cx, cy + 200.0):
                zoom(0.8):
                horizalign(center):
                z(301)
            ));
        }
        LoginPhase::Success => {
            let fill = color::decorative_rgba(active_color_index);
            out.push(act!(text:
                font("miso"):
                settext("ArrowCloud login complete."):
                align(0.5, 0.5):
                xy(cx, cy - 10.0):
                zoom(1.05):
                horizalign(center):
                z(301):
                diffuse(fill[0], fill[1], fill[2], 1.0)
            ));
            out.push(act!(text:
                font("miso"):
                settext("Press Start to dismiss."):
                align(0.5, 0.5):
                xy(cx, cy + 30.0):
                zoom(0.9):
                horizalign(center):
                z(301)
            ));
        }
        LoginPhase::Failed { reason } => {
            out.push(act!(text:
                font("miso"):
                settext(format!("Login failed: {reason}")):
                align(0.5, 0.5):
                xy(cx, cy - 10.0):
                zoom(0.95):
                maxwidth(screen_width() * 0.9):
                horizalign(center):
                z(301):
                diffuse(1.0, 0.4, 0.4, 1.0)
            ));
            out.push(act!(text:
                font("miso"):
                settext("Press Start to dismiss."):
                align(0.5, 0.5):
                xy(cx, cy + 30.0):
                zoom(0.9):
                horizalign(center):
                z(301)
            ));
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::network::NetworkError;
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
        let start_fn =
            move |_req: &ac_online::DeviceLoginStartReq| -> Result<_, NetworkError> {
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

        run_login_session(tx, cancel, start_fn, poll_fn);

        let msgs = drain_msgs(&rx);
        assert!(matches!(msgs.first(), Some(LoginMsg::Started { .. })));
        assert!(
            msgs.iter()
                .any(|m| matches!(m, LoginMsg::StatusUpdate { .. }))
        );
        assert!(matches!(
            msgs.last(),
            Some(LoginMsg::Consumed { api_key }) if api_key == "AC-KEY-7"
        ));
        assert_eq!(*polls.lock().unwrap(), 2);
    }

    #[test]
    fn worker_reports_failure_on_expired() {
        let (tx, rx) = mpsc::channel::<LoginMsg>();
        let cancel = Arc::new(AtomicBool::new(false));
        let start = make_start_ok();
        let start_fn =
            move |_req: &ac_online::DeviceLoginStartReq| -> Result<_, NetworkError> {
                Ok(start.clone())
            };
        let poll_fn = move |_req: &ac_online::DeviceLoginPollReq| -> Result<_, NetworkError> {
            Ok(ac_online::DeviceLoginPollResp {
                status: ac_online::DeviceLoginStatus::Expired,
                poll_interval_seconds: None,
                api_key: None,
            })
        };

        run_login_session(tx, cancel, start_fn, poll_fn);
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

        run_login_session(tx, cancel, start_fn, poll_fn);
        let msgs = drain_msgs(&rx);
        assert!(matches!(msgs.first(), Some(LoginMsg::Failed { .. })));
        assert_eq!(msgs.len(), 1);
    }

    #[test]
    fn worker_consumed_with_empty_key_is_failure() {
        let (tx, rx) = mpsc::channel::<LoginMsg>();
        let cancel = Arc::new(AtomicBool::new(false));
        let start = make_start_ok();
        let start_fn =
            move |_req: &ac_online::DeviceLoginStartReq| -> Result<_, NetworkError> {
                Ok(start.clone())
            };
        let poll_fn = move |_req: &ac_online::DeviceLoginPollReq| -> Result<_, NetworkError> {
            Ok(ac_online::DeviceLoginPollResp {
                status: ac_online::DeviceLoginStatus::Consumed,
                poll_interval_seconds: None,
                api_key: Some("   ".into()),
            })
        };

        run_login_session(tx, cancel, start_fn, poll_fn);
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
}
