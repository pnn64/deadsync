use deadsync_config::prelude::{ArrowCloudQrLoginWhen, GrooveStatsQrLoginWhen};
use deadsync_online::{arrowcloud, groovestats};
use deadsync_profile::{PlayerSide, compat as profile};
use deadsync_theme_simply_love::{
    SimplyLoveQrLoginEvent, SimplyLoveQrLoginRequest, SimplyLoveQrLoginService,
    SimplyLoveQrLoginSlot, SimplyLoveQrLoginSlotAvailability,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};

const PENDING_EVENTS: usize = 64;
const SIDES: [PlayerSide; 2] = [PlayerSide::P1, PlayerSide::P2];

enum WorkerEvent {
    Started {
        side: PlayerSide,
        short_code: String,
        verification_url: String,
    },
    Consumed {
        side: PlayerSide,
        api_key: String,
        username: Option<String>,
    },
    Failed {
        side: PlayerSide,
        reason: String,
    },
}

struct ActiveLogin {
    id: u64,
    service: SimplyLoveQrLoginService,
    slots: [SimplyLoveQrLoginSlot; 2],
    pending_sides: u8,
    cancel: Arc<AtomicBool>,
}

/// Shell-owned QR-login workers, secret-bearing transport, and persistence.
pub(crate) struct Service {
    tx: mpsc::SyncSender<(u64, WorkerEvent)>,
    rx: mpsc::Receiver<(u64, WorkerEvent)>,
    active: Option<ActiveLogin>,
    next_id: u64,
}

impl Default for Service {
    fn default() -> Self {
        let (tx, rx) = mpsc::sync_channel(PENDING_EVENTS);
        Self {
            tx,
            rx,
            active: None,
            next_id: 0,
        }
    }
}

impl Service {
    pub(crate) fn start(&mut self, request: SimplyLoveQrLoginRequest) {
        self.cancel_any();
        self.next_id = self.next_id.wrapping_add(1);
        let id = self.next_id;
        let cancel = Arc::new(AtomicBool::new(false));
        let pending_sides = request
            .slots
            .iter()
            .filter(|slot| matches!(slot.availability, SimplyLoveQrLoginSlotAvailability::Ready))
            .fold(0, |mask, slot| mask | side_bit(slot.side));
        if pending_sides == 0 {
            self.active = None;
            return;
        }
        self.active = Some(ActiveLogin {
            id,
            service: request.service,
            slots: request.slots.clone(),
            pending_sides,
            cancel: Arc::clone(&cancel),
        });
        match request.service {
            SimplyLoveQrLoginService::ArrowCloud => {
                self.start_arrowcloud(id, &request.slots, cancel)
            }
            SimplyLoveQrLoginService::GrooveStats => {
                self.start_groovestats(id, &request.slots, cancel)
            }
        }
    }

    pub(crate) fn cancel(&self, service: SimplyLoveQrLoginService) {
        if let Some(active) = &self.active
            && active.service == service
        {
            active.cancel.store(true, Ordering::Relaxed);
        }
    }

    pub(crate) fn cancel_any(&self) {
        if let Some(active) = &self.active {
            active.cancel.store(true, Ordering::Relaxed);
        }
    }

    pub(crate) fn poll(&mut self) -> Vec<SimplyLoveQrLoginEvent> {
        let worker_events = self.rx.try_iter().collect::<Vec<_>>();
        let mut events = Vec::with_capacity(worker_events.len());
        for (id, event) in worker_events {
            let Some(active) = self.active.as_mut() else {
                continue;
            };
            if active.id != id {
                continue;
            }
            let service = active.service;
            match event {
                WorkerEvent::Started {
                    side,
                    short_code,
                    verification_url,
                } => events.push(SimplyLoveQrLoginEvent::Started {
                    service,
                    side,
                    short_code,
                    verification_url,
                }),
                WorkerEvent::Consumed {
                    side,
                    api_key,
                    username,
                } => {
                    let slot = &active.slots[deadsync_profile::player_side_index(side)];
                    persist_credentials(service, slot, &api_key, username.as_deref());
                    active.pending_sides &= !side_bit(side);
                    events.push(SimplyLoveQrLoginEvent::Succeeded {
                        service,
                        side,
                        display_name: slot.display_name.clone(),
                    });
                }
                WorkerEvent::Failed { side, reason } => {
                    active.pending_sides &= !side_bit(side);
                    events.push(SimplyLoveQrLoginEvent::Failed {
                        service,
                        side,
                        reason,
                    });
                }
            }
        }
        if self
            .active
            .as_ref()
            .is_some_and(|active| active.pending_sides == 0)
        {
            self.active = None;
        }
        events
    }

    fn start_arrowcloud(
        &self,
        id: u64,
        slots: &[SimplyLoveQrLoginSlot; 2],
        cancel: Arc<AtomicBool>,
    ) {
        for slot in ready_slots(slots) {
            let side = slot.side;
            let tx = self.tx.clone();
            let thread_cancel = Arc::clone(&cancel);
            std::thread::spawn(move || {
                arrowcloud::run_device_login_session(thread_cancel, |event| {
                    let event = match event {
                        arrowcloud::DeviceLoginEvent::Started {
                            short_code,
                            verification_url,
                        } => WorkerEvent::Started {
                            side,
                            short_code,
                            verification_url,
                        },
                        arrowcloud::DeviceLoginEvent::StatusUpdate => return true,
                        arrowcloud::DeviceLoginEvent::Consumed { api_key } => {
                            WorkerEvent::Consumed {
                                side,
                                api_key,
                                username: None,
                            }
                        }
                        arrowcloud::DeviceLoginEvent::Failed { reason } => {
                            WorkerEvent::Failed { side, reason }
                        }
                    };
                    tx.send((id, event)).is_ok()
                });
            });
        }
    }

    fn start_groovestats(
        &self,
        id: u64,
        slots: &[SimplyLoveQrLoginSlot; 2],
        cancel: Arc<AtomicBool>,
    ) {
        let uuid = groovestats::generate_qr_login_uuid();
        let ready_mask = ready_slots(slots).fold(0, |mask, slot| mask | side_bit(slot.side));
        for slot in ready_slots(slots) {
            let _ = self.tx.send((
                id,
                WorkerEvent::Started {
                    side: slot.side,
                    short_code: String::new(),
                    verification_url: groovestats::qr_login_url(
                        &uuid,
                        deadsync_profile::player_side_number(slot.side),
                    ),
                },
            ));
        }
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            groovestats::run_qr_login_session(uuid, cancel, |event| match event {
                groovestats::GrooveStatsQrLoginEvent::Failed { reason } => {
                    for side in SIDES {
                        if ready_mask & side_bit(side) != 0 {
                            let _ = tx.send((
                                id,
                                WorkerEvent::Failed {
                                    side,
                                    reason: reason.clone(),
                                },
                            ));
                        }
                    }
                }
                groovestats::GrooveStatsQrLoginEvent::Consumed {
                    side,
                    api_key,
                    username,
                } => {
                    let side = if side == 2 {
                        PlayerSide::P2
                    } else {
                        PlayerSide::P1
                    };
                    if ready_mask & side_bit(side) != 0 {
                        let _ = tx.send((
                            id,
                            WorkerEvent::Consumed {
                                side,
                                api_key,
                                username: Some(username),
                            },
                        ));
                    }
                }
            });
        });
    }
}

pub(crate) fn request(
    service: SimplyLoveQrLoginService,
    target_profile: Option<(String, String)>,
) -> SimplyLoveQrLoginRequest {
    let slots = if let Some((profile_id, display_name)) = target_profile {
        [
            SimplyLoveQrLoginSlot {
                side: PlayerSide::P1,
                availability: SimplyLoveQrLoginSlotAvailability::Ready,
                had_existing_key: profile_has_key(service, &profile_id),
                display_name,
                target_profile_id: Some(profile_id),
            },
            unavailable_slot(PlayerSide::P2, SimplyLoveQrLoginSlotAvailability::NotJoined),
        ]
    } else {
        SIDES.map(|side| session_slot(service, side))
    };
    SimplyLoveQrLoginRequest { service, slots }
}

pub(crate) fn should_auto_show_arrowcloud(when: ArrowCloudQrLoginWhen) -> bool {
    match when {
        ArrowCloudQrLoginWhen::Always => true,
        ArrowCloudQrLoginWhen::Sometimes => {
            session_has_missing_key(SimplyLoveQrLoginService::ArrowCloud)
        }
        ArrowCloudQrLoginWhen::Disabled => false,
    }
}

pub(crate) fn should_auto_show_groovestats(when: GrooveStatsQrLoginWhen) -> bool {
    match when {
        GrooveStatsQrLoginWhen::Always => true,
        GrooveStatsQrLoginWhen::Sometimes => {
            session_has_missing_key(SimplyLoveQrLoginService::GrooveStats)
        }
        GrooveStatsQrLoginWhen::Disabled => false,
    }
}

fn session_has_missing_key(service: SimplyLoveQrLoginService) -> bool {
    ready_slots(&request(service, None).slots).any(|slot| !slot.had_existing_key)
}

fn session_slot(service: SimplyLoveQrLoginService, side: PlayerSide) -> SimplyLoveQrLoginSlot {
    if !profile::is_session_side_joined(side) {
        return unavailable_slot(side, SimplyLoveQrLoginSlotAvailability::NotJoined);
    }
    if profile::is_session_side_guest(side) {
        return unavailable_slot(side, SimplyLoveQrLoginSlotAvailability::Guest);
    }
    let current = profile::get_for_side(side);
    let had_existing_key = match service {
        SimplyLoveQrLoginService::ArrowCloud => !current.arrowcloud_api_key.trim().is_empty(),
        SimplyLoveQrLoginService::GrooveStats => !current.groovestats_api_key.trim().is_empty(),
    };
    SimplyLoveQrLoginSlot {
        side,
        availability: SimplyLoveQrLoginSlotAvailability::Ready,
        display_name: current.display_name,
        had_existing_key,
        target_profile_id: None,
    }
}

fn unavailable_slot(
    side: PlayerSide,
    availability: SimplyLoveQrLoginSlotAvailability,
) -> SimplyLoveQrLoginSlot {
    SimplyLoveQrLoginSlot {
        side,
        availability,
        display_name: String::new(),
        had_existing_key: false,
        target_profile_id: None,
    }
}

fn ready_slots(slots: &[SimplyLoveQrLoginSlot; 2]) -> impl Iterator<Item = &SimplyLoveQrLoginSlot> {
    slots
        .iter()
        .filter(|slot| matches!(slot.availability, SimplyLoveQrLoginSlotAvailability::Ready))
}

fn profile_has_key(service: SimplyLoveQrLoginService, profile_id: &str) -> bool {
    match service {
        SimplyLoveQrLoginService::ArrowCloud => !profile::get_arrowcloud_api_key_for_id(profile_id)
            .trim()
            .is_empty(),
        SimplyLoveQrLoginService::GrooveStats => {
            profile::get_groovestats_api_key_for_id(profile_id).is_some()
        }
    }
}

fn persist_credentials(
    service: SimplyLoveQrLoginService,
    slot: &SimplyLoveQrLoginSlot,
    api_key: &str,
    username: Option<&str>,
) {
    match service {
        SimplyLoveQrLoginService::ArrowCloud => {
            if let Some(profile_id) = slot.target_profile_id.as_deref() {
                profile::set_arrowcloud_api_key_for_id(profile_id, api_key);
            } else {
                profile::set_arrowcloud_api_key_for_side(slot.side, api_key);
            }
            deadsync_online::runtime::refresh_arrowcloud_status();
        }
        SimplyLoveQrLoginService::GrooveStats => {
            let username = username.unwrap_or_default();
            if let Some(profile_id) = slot.target_profile_id.as_deref() {
                profile::set_groovestats_credentials_for_id(profile_id, api_key, username);
            } else {
                profile::set_groovestats_credentials_for_side(slot.side, api_key, username);
            }
        }
    }
}

const fn side_bit(side: PlayerSide) -> u8 {
    1 << deadsync_profile::player_side_index(side)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_request_prepares_one_ready_slot() {
        let request = request(
            SimplyLoveQrLoginService::ArrowCloud,
            Some(("missing-profile".to_owned(), "Player".to_owned())),
        );
        assert_eq!(
            request.slots[0].availability,
            SimplyLoveQrLoginSlotAvailability::Ready
        );
        assert_eq!(request.slots[0].display_name, "Player");
        assert_eq!(
            request.slots[0].target_profile_id.as_deref(),
            Some("missing-profile")
        );
        assert_eq!(
            request.slots[1].availability,
            SimplyLoveQrLoginSlotAvailability::NotJoined
        );
    }

    #[test]
    fn unavailable_slot_contains_no_profile_target() {
        let slot = unavailable_slot(PlayerSide::P2, SimplyLoveQrLoginSlotAvailability::Guest);
        assert_eq!(slot.availability, SimplyLoveQrLoginSlotAvailability::Guest);
        assert!(slot.display_name.is_empty());
        assert!(slot.target_profile_id.is_none());
    }
}
