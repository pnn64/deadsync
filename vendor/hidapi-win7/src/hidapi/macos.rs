//! The extra beahviour for macOS

use super::HidDevice;
use crate::{ffi, HidDeviceBackendBase, HidDeviceBackendMacos, HidResult};

impl HidDeviceBackendMacos for HidDevice {
    fn get_location_id(&self) -> HidResult<u32> {
        let mut location_id: u32 = 0;

        let res = unsafe {
            ffi::macos::hid_darwin_get_location_id(self._hid_device, &mut location_id as *mut u32)
        };

        if res == -1 {
            match self.check_error() {
                Ok(err) => Err(err),
                Err(err) => Err(err),
            }
        } else {
            Ok(location_id)
        }
    }

    fn is_open_exclusive(&self) -> HidResult<bool> {
        let res = unsafe { ffi::macos::hid_darwin_is_device_open_exclusive(self._hid_device) };

        if res == -1 {
            match self.check_error() {
                Ok(err) => Err(err),
                Err(err) => Err(err),
            }
        } else {
            Ok(res == 1)
        }
    }
}
