use crate::{profile_import, qr_login, score_import, sync_analysis};
use deadsync_theme_simply_love::screens::components::shared::heart_rate::HeartRateViewSyncBenchmark;
use deadsync_theme_simply_love::screens::gameplay::{HeartRatePlayerView, HeartRateView};
use deadsync_theme_simply_love::{
    SimplyLoveQrLoginEvent, SimplyLoveQrLoginService, SimplyLoveSyncEvent, SimplyLoveSyncOwner,
};
use std::hint::black_box;

const POLLS_PER_FRAME: usize = 256;

/// Old and current idle worker-maintenance paths used by the release benchmark.
pub struct GameplayIdleWorkersBenchmark {
    qr_login: qr_login::Service,
    profile_import: profile_import::Service,
    score_import: score_import::Service,
    sync_analysis: sync_analysis::Service,
    heart_rate: HeartRateViewSyncBenchmark,
}

impl Default for GameplayIdleWorkersBenchmark {
    fn default() -> Self {
        let generation = deadsync_heart_rate::player_readings_generation();
        let heart_rate = HeartRateViewSyncBenchmark::new(generation, current_heart_rate_view());
        Self {
            qr_login: qr_login::Service::default(),
            profile_import: profile_import::Service::default(),
            score_import: score_import::Service::default(),
            sync_analysis: sync_analysis::Service::default(),
            heart_rate,
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

    pub fn legacy_profile_import_frame(&mut self) -> usize {
        (0..POLLS_PER_FRAME)
            .map(|_| black_box(&mut self.profile_import).poll_idle_legacy().len())
            .sum()
    }

    pub fn gated_profile_import_frame(&mut self) -> usize {
        (0..POLLS_PER_FRAME)
            .map(|_| {
                black_box(&mut self.profile_import)
                    .poll()
                    .map_or(0, |events| events.len())
            })
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

    pub fn legacy_heart_rate_frame(&mut self) -> usize {
        (0..POLLS_PER_FRAME).fold(0, |checksum, sample| {
            let view = current_heart_rate_view();
            checksum.rotate_left(5) ^ self.heart_rate.sync_legacy(view) ^ sample
        })
    }

    pub fn gated_heart_rate_frame(&mut self) -> usize {
        (0..POLLS_PER_FRAME).fold(0, |checksum, sample| {
            let generation = deadsync_heart_rate::player_readings_generation();
            let value = self
                .heart_rate
                .sync_generation(generation, current_heart_rate_view);
            checksum.rotate_left(5) ^ value ^ sample
        })
    }
}

fn current_heart_rate_view() -> HeartRateView {
    HeartRateView {
        players: deadsync_heart_rate::player_readings().map(|reading| HeartRatePlayerView {
            configured: reading.configured,
            connected: reading.connected,
            bpm: reading.bpm,
        }),
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
