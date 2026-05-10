use std::path::Path;

#[cfg(any(
    windows,
    target_os = "linux",
    target_os = "freebsd",
    target_os = "macos"
))]
mod fsrio;
#[cfg(any(
    windows,
    target_os = "linux",
    target_os = "freebsd",
    target_os = "macos"
))]
mod smx;

pub const VIEW_SENSOR_COUNT: usize = 4;

#[derive(Clone, Copy, Debug)]
pub struct BarView {
    pub label: &'static str,
    pub raw_value: u16,
    pub value_norm: f32,
    pub raw_threshold: u16,
    pub threshold_norm: f32,
    pub min_raw_threshold: u16,
    pub max_raw_threshold: u16,
    pub active: bool,
}

#[derive(Clone, Debug)]
pub struct View {
    pub device_name: Option<String>,
    pub bars: [BarView; VIEW_SENSOR_COUNT],
}

#[cfg(any(
    windows,
    target_os = "linux",
    target_os = "freebsd",
    target_os = "macos"
))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActiveMonitor {
    None,
    Fsrio,
    Smx,
}

#[cfg(any(
    windows,
    target_os = "linux",
    target_os = "freebsd",
    target_os = "macos"
))]
pub struct Monitor {
    fsrio: fsrio::Monitor,
    smx: smx::Monitor,
    active: ActiveMonitor,
}

#[cfg(any(
    windows,
    target_os = "linux",
    target_os = "freebsd",
    target_os = "macos"
))]
impl Monitor {
    pub fn new() -> Self {
        Self {
            fsrio: fsrio::Monitor::new(),
            smx: smx::Monitor::new(),
            active: ActiveMonitor::None,
        }
    }

    pub fn poll_view(&mut self) -> Option<View> {
        if let Some(view) = self.fsrio.poll_view() {
            self.active = ActiveMonitor::Fsrio;
            return Some(view);
        }
        if let Some(view) = self.smx.poll_view() {
            self.active = ActiveMonitor::Smx;
            return Some(view);
        }
        self.active = ActiveMonitor::None;
        None
    }

    pub fn update_threshold(&mut self, sensor_index: usize, threshold: u16) -> bool {
        match self.active {
            ActiveMonitor::Fsrio => self.fsrio.update_threshold(sensor_index, threshold),
            ActiveMonitor::Smx => self.smx.update_threshold(sensor_index, threshold),
            ActiveMonitor::None => {
                self.fsrio.update_threshold(sensor_index, threshold)
                    || self.smx.update_threshold(sensor_index, threshold)
            }
        }
    }

    pub fn write_debug_dump(&mut self, path: &Path) -> Result<(), String> {
        let mut out = String::new();
        out.push_str(&self.fsrio.debug_dump());
        out.push_str("\n\n");
        out.push_str(&self.smx.debug_dump());
        write_dump_file(path, out)
    }
}

#[cfg(not(any(
    windows,
    target_os = "linux",
    target_os = "freebsd",
    target_os = "macos"
)))]
mod unsupported {
    use super::View;
    use std::fmt::Write as _;
    use std::path::Path;
    use std::time::SystemTime;

    #[derive(Default)]
    pub struct Monitor;

    impl Monitor {
        pub const fn new() -> Self {
            Self
        }

        pub fn poll_view(&mut self) -> Option<View> {
            None
        }

        pub fn update_threshold(&mut self, _sensor_index: usize, _threshold: u16) -> bool {
            false
        }

        pub fn write_debug_dump(&mut self, path: &Path) -> Result<(), String> {
            let mut out = String::new();
            let _ = writeln!(out, "DeadSync FSR debug dump");
            let _ = writeln!(out, "generated: {:?}", SystemTime::now());
            let _ = writeln!(out);
            let _ = writeln!(
                out,
                "FSR HID diagnostics are not available on this platform."
            );
            super::write_dump_file(path, out)
        }
    }
}

#[cfg(not(any(
    windows,
    target_os = "linux",
    target_os = "freebsd",
    target_os = "macos"
)))]
pub use unsupported::Monitor;

fn write_dump_file(path: &Path, content: String) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create '{}': {e}", parent.display()))?;
    }
    std::fs::write(path, content).map_err(|e| format!("failed to write '{}': {e}", path.display()))
}
