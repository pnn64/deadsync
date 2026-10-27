//! App-lifetime read-only chat connection, independent of screen transitions.
use crate::latest_worker_value::LatestWorkerValue;
use deadlib_assets::AssetManager;
use deadlib_present::{actors::Actor, space};
use deadsync_config::streaming::{TwitchChannel, TwitchOptions};
use deadsync_online::twitch::{self, Entry, Snapshot};
use deadsync_theme_simply_love::screens::{
    SimplyLoveScreen,
    components::shared::twitch_chat::{self, GameplayLayout, Overlay},
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
};

struct Worker {
    id: u64,
    channel: TwitchChannel,
    cancel: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

#[derive(Default)]
pub(crate) struct Runtime {
    worker: Option<Worker>,
    latest: Arc<LatestWorkerValue<Snapshot>>,
    desired: Option<TwitchChannel>,
    next_id: u64,
    entries: Vec<Arc<Entry>>,
    revision: u64,
    overlay: Overlay,
    screen: Option<SimplyLoveScreen>,
    sides: [bool; 2],
    options: TwitchOptions,
}

impl Runtime {
    pub(crate) fn sync(&mut self, options: TwitchOptions) {
        let desired = (options.enabled && !options.channel.is_empty()).then_some(options.channel);
        if self.desired != desired || self.options.enabled != options.enabled {
            self.desired = desired;
            self.entries.clear();
            self.overlay.clear();
            self.revision = self.revision.wrapping_add(1);
            if let Some(worker) = &self.worker {
                worker.cancel.store(true, Ordering::Release);
            }
        }
        self.options = options;
        if self
            .worker
            .as_ref()
            .is_some_and(|worker| worker.thread.is_finished())
        {
            let worker = self.worker.take().unwrap();
            let _ = worker.thread.join();
            self.latest.finish(worker.id);
        }
        // Wait for a cancelled worker to exit before replacing it. DNS and TLS
        // may take time, but rapid channel changes can never accumulate threads.
        if self.worker.is_none()
            && let Some(channel) = self.desired
        {
            self.next_id = self.next_id.wrapping_add(1).max(1);
            let id = self.next_id;
            self.latest.start(id);
            let latest = Arc::clone(&self.latest);
            let cancel = Arc::new(AtomicBool::new(false));
            let worker_cancel = Arc::clone(&cancel);
            match std::thread::Builder::new()
                .name("twitch-chat".into())
                .spawn(move || {
                    twitch::run(channel, &worker_cancel, |snapshot| {
                        latest.publish(id, snapshot)
                    });
                }) {
                Ok(thread) => {
                    self.worker = Some(Worker {
                        id,
                        channel,
                        cancel,
                        thread,
                    })
                }
                Err(error) => {
                    self.entries = vec![Arc::new(Entry::Error(format!(
                        "Unable to start Twitch chat: {error}"
                    )))];
                    self.revision = self.revision.wrapping_add(1);
                }
            }
        }
        if let Some(worker) = &self.worker
            && Some(worker.channel) == self.desired
            && !worker.cancel.load(Ordering::Acquire)
            && let Some(snapshot) = self.latest.take(worker.id)
        {
            self.entries = snapshot.entries;
            self.revision = self.revision.wrapping_add(1);
        }
        if options.enabled && options.channel.is_empty() && self.entries.is_empty() {
            self.entries.push(Arc::new(Entry::Status {
                text: "Set Twitch Channel in Streaming Services.".into(),
                notice: None,
            }));
            self.revision = self.revision.wrapping_add(1);
        }
    }

    pub(crate) fn push(
        &mut self,
        actors: &mut Vec<Actor>,
        screen: SimplyLoveScreen,
        gameplay: Option<GameplayLayout>,
        tournament: bool,
        assets: &AssetManager,
    ) {
        if !self.options.enabled {
            return;
        }
        if self.screen != Some(screen) {
            self.screen = Some(screen);
            let session = deadsync_profile::compat::get_session_snapshot();
            self.sides = [
                session.side_joined(deadsync_profile::PlayerSide::P1),
                session.side_joined(deadsync_profile::PlayerSide::P2),
            ];
        }
        if screen == SimplyLoveScreen::Gameplay && !self.options.show_gameplay {
            return;
        }
        if let Some(rect) = twitch_chat::placement(
            screen,
            space::screen_width(),
            space::screen_height(),
            self.sides,
            gameplay,
            tournament,
        ) {
            self.overlay.push(
                actors,
                &self.entries,
                self.revision,
                rect,
                self.options,
                assets,
            );
        }
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            worker.cancel.store(true, Ordering::Release);
            // Never wait for DNS/network shutdown on the window thread.
            if worker.thread.is_finished() {
                let _ = worker.thread.join();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twitch_runtime_cancels_channel_changes_and_rejects_stale_snapshots() {
        let a = TwitchChannel::parse("first").unwrap();
        let b = TwitchChannel::parse("second").unwrap();
        let options = TwitchOptions {
            enabled: true,
            channel: a,
            ..Default::default()
        };
        let mut runtime = Runtime::default();
        runtime.desired = Some(a);
        runtime.options = options;
        let (release, wait) = std::sync::mpsc::channel::<()>();
        let cancel = Arc::new(AtomicBool::new(false));
        runtime.latest.start(1);
        runtime.worker = Some(Worker {
            id: 1,
            channel: a,
            cancel: Arc::clone(&cancel),
            thread: std::thread::spawn(move || {
                let _ = wait.recv();
            }),
        });
        runtime.sync(TwitchOptions {
            show_badges: false,
            ..options
        });
        assert!(
            !cancel.load(Ordering::Acquire),
            "display changes keep the connection"
        );
        runtime.sync(TwitchOptions {
            channel: b,
            ..options
        });
        assert!(cancel.load(Ordering::Acquire));
        runtime.latest.publish(
            1,
            Snapshot {
                connection: twitch::Connection::Connected,
                entries: vec![Arc::new(Entry::Error("stale".into()))],
            },
        );
        for _ in 0..100 {
            runtime.sync(TwitchOptions {
                channel: b,
                ..options
            });
        }
        assert!(runtime.entries.is_empty());
        assert_eq!(
            runtime.worker.as_ref().unwrap().id,
            1,
            "no second worker while the first is stopping"
        );
        runtime.sync(TwitchOptions::default());
        release.send(()).unwrap();
        let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while runtime.worker.is_some() && std::time::Instant::now() < until {
            runtime.sync(TwitchOptions::default());
            std::thread::yield_now();
        }
        assert!(runtime.worker.is_none());
        assert!(runtime.entries.is_empty());
    }

    #[test]
    fn twitch_runtime_disabled_or_empty_channel_never_connects() {
        let mut runtime = Runtime::default();
        runtime.sync(TwitchOptions::default());
        assert!(runtime.worker.is_none());
        assert!(runtime.entries.is_empty());
        runtime.sync(TwitchOptions {
            enabled: true,
            ..Default::default()
        });
        assert!(runtime.worker.is_none());
        assert_eq!(runtime.entries.len(), 1);
    }
}
