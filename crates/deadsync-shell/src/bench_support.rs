use crate::{qr_login, score_import, sync_analysis};
use deadsync_theme_simply_love::{
    SimplyLoveQrLoginEvent, SimplyLoveQrLoginService, SimplyLoveSyncEvent, SimplyLoveSyncOwner,
};
use std::hint::black_box;

const POLLS_PER_FRAME: usize = 256;

/// Old and current idle worker-maintenance paths used by the release benchmark.
pub struct GameplayIdleWorkersBenchmark {
    qr_login: qr_login::Service,
    score_import: score_import::Service,
    sync_analysis: sync_analysis::Service,
}

impl Default for GameplayIdleWorkersBenchmark {
    fn default() -> Self {
        Self {
            qr_login: qr_login::Service::default(),
            score_import: score_import::Service::default(),
            sync_analysis: sync_analysis::Service::default(),
        }
    }
}

impl GameplayIdleWorkersBenchmark {
    pub fn legacy_qr_frame(&mut self) -> usize {
        (0..POLLS_PER_FRAME)
            .map(|_| route_qr(black_box(&mut self.qr_login).poll_idle_legacy()))
            .sum()
    }

    pub fn gated_qr_frame(&mut self) -> usize {
        (0..POLLS_PER_FRAME)
            .map(|_| black_box(&mut self.qr_login).poll().map_or(0, route_qr))
            .sum()
    }

    pub fn legacy_score_frame(&mut self) -> usize {
        (0..POLLS_PER_FRAME)
            .map(|_| black_box(&mut self.score_import).poll_idle_legacy().len())
            .sum()
    }

    pub fn gated_score_frame(&mut self) -> usize {
        (0..POLLS_PER_FRAME)
            .map(|_| {
                black_box(&mut self.score_import)
                    .poll()
                    .map_or(0, |events| events.len())
            })
            .sum()
    }

    pub fn legacy_sync_frame(&mut self) -> usize {
        (0..POLLS_PER_FRAME)
            .map(|_| route_sync(black_box(&mut self.sync_analysis).poll_idle_legacy()))
            .sum()
    }

    pub fn gated_sync_frame(&mut self) -> usize {
        (0..POLLS_PER_FRAME)
            .map(|_| {
                black_box(&mut self.sync_analysis)
                    .poll()
                    .map_or(0, route_sync)
            })
            .sum()
    }
}

fn route_qr(events: Vec<SimplyLoveQrLoginEvent>) -> usize {
    let mut arrowcloud = Vec::new();
    let mut groovestats = Vec::new();
    for event in events {
        match event.service() {
            SimplyLoveQrLoginService::ArrowCloud => arrowcloud.push(event),
            SimplyLoveQrLoginService::GrooveStats => groovestats.push(event),
        }
    }
    arrowcloud.len() + groovestats.len()
}

fn route_sync(events: Vec<(SimplyLoveSyncOwner, SimplyLoveSyncEvent)>) -> usize {
    let mut song = Vec::new();
    let mut select_pack = Vec::new();
    let mut options_pack = Vec::new();
    for (owner, event) in events {
        match owner {
            SimplyLoveSyncOwner::SelectMusicSong => song.push(event),
            SimplyLoveSyncOwner::SelectMusicPack => select_pack.push(event),
            SimplyLoveSyncOwner::OptionsPack => options_pack.push(event),
        }
    }
    song.len() + select_pack.len() + options_pack.len()
}
