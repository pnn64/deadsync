//! BLE discovery and characteristic notifications on a background worker.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

pub use uuid::Uuid;

// Keep this selection in sync with the target dependencies in Cargo.toml.
#[cfg_attr(
    not(any(
        all(target_os = "windows", not(target_vendor = "win7")),
        target_os = "linux",
        target_os = "macos"
    )),
    path = "unsupported.rs"
)]
mod platform;

pub use platform::SUPPORTED;

#[derive(Clone, Copy, Debug)]
pub struct Subscription {
    pub service: Uuid,
    pub characteristic: Uuid,
    pub unnamed_device_label: &'static str,
}

/// Desired device IDs in connection priority order. Duplicate IDs share a connection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection<const N: usize> {
    pub enabled: bool,
    pub discover: bool,
    pub device_ids: [Option<Arc<str>>; N],
}

impl<const N: usize> Default for Selection<N> {
    fn default() -> Self {
        Self {
            enabled: false,
            discover: false,
            device_ids: std::array::from_fn(|_| None),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoverySnapshot {
    pub supported: bool,
    pub scanning: bool,
    pub devices: Vec<Device>,
    pub error: Option<String>,
}

impl Default for DiscoverySnapshot {
    fn default() -> Self {
        Self {
            supported: SUPPORTED,
            scanning: false,
            devices: Vec::new(),
            error: None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Event<'a> {
    Connecting(&'a str),
    Connected(&'a str),
    Notification { device_id: &'a str, value: &'a [u8] },
    Disconnected(&'a str),
    Disabled,
    Unavailable,
}

type EventHandler<const N: usize> = fn(&Selection<N>, Event<'_>);

#[derive(Default)]
struct Shared {
    discovery: RwLock<DiscoverySnapshot>,
    generation: AtomicU64,
    stopped: AtomicBool,
}

pub struct Monitor {
    shared: Arc<Shared>,
}

impl Monitor {
    /// Starts a worker that reads the caller's selection and reports events with
    /// the selection snapshot used for that polling pass.
    pub fn start<const N: usize>(
        subscription: Subscription,
        desired: Arc<Mutex<Selection<N>>>,
        on_event: fn(&Selection<N>, Event<'_>),
    ) -> Self {
        let shared = Arc::new(Shared::default());
        platform::start_worker(subscription, desired, Arc::clone(&shared), on_event);
        Self { shared }
    }

    pub fn discovery_snapshot(&self) -> DiscoverySnapshot {
        self.shared
            .discovery
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    pub fn discovery_generation(&self) -> u64 {
        self.shared.generation.load(Ordering::Acquire)
    }
}

impl Drop for Monitor {
    fn drop(&mut self) {
        self.shared.stopped.store(true, Ordering::Release);
    }
}
