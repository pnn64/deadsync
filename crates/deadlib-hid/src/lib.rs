//! Device discovery and report I/O for host HID devices.

#[cfg(windows)]
mod windows;

#[cfg(not(windows))]
pub use upstream_hidapi::{
    BusType, DeviceInfo, HidApi, HidDevice, HidError, HidResult, MAX_REPORT_DESCRIPTOR_SIZE,
};
#[cfg(windows)]
pub use windows::{
    BusType, DeviceInfo, HidApi, HidDevice, HidError, HidResult, MAX_REPORT_DESCRIPTOR_SIZE,
};
