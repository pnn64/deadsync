mod descriptor;
mod device;
mod discovery;

pub use device::HidDevice;
pub use discovery::DeviceInfo;
use std::{ffi::CStr, fmt, io};

pub const MAX_REPORT_DESCRIPTOR_SIZE: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusType {
    Unknown,
    Usb,
    Bluetooth,
    I2c,
    Spi,
}

#[derive(Debug)]
pub enum HidError {
    HidApiError { message: String },
    IoError { error: io::Error },
}

impl fmt::Display for HidError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HidApiError { message } => write!(f, "hidapi error: {message}"),
            Self::IoError { error } => error.fmt(f),
        }
    }
}

impl std::error::Error for HidError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::IoError { error } => Some(error),
            Self::HidApiError { .. } => None,
        }
    }
}

impl From<io::Error> for HidError {
    fn from(error: io::Error) -> Self {
        Self::IoError { error }
    }
}

pub type HidResult<T> = Result<T, HidError>;

pub struct HidApi {
    devices: Vec<DeviceInfo>,
}

impl HidApi {
    /// Enumerates the present HID interfaces.
    pub fn new() -> HidResult<Self> {
        Ok(Self {
            devices: discovery::enumerate()?,
        })
    }

    /// Replaces the device list with the interfaces currently present.
    pub fn refresh_devices(&mut self) -> HidResult<()> {
        self.devices.clear();
        self.devices = discovery::enumerate()?;
        Ok(())
    }

    pub fn device_list(&self) -> impl Iterator<Item = &DeviceInfo> {
        self.devices.iter()
    }

    /// Opens an interface independently, allowing separate reader and writer handles.
    pub fn open_path(&self, path: &CStr) -> HidResult<HidDevice> {
        HidDevice::open(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_api_enumerates_and_refreshes_hid_interfaces() -> HidResult<()> {
        let mut api = HidApi::new()?;
        api.refresh_devices()?;
        for device in api.device_list() {
            assert!(!device.path().to_bytes().is_empty());
        }
        Ok(())
    }
}
