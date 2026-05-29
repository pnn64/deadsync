use libc::c_int;

use crate::ffi;
use crate::{HidApi, HidDevice, HidResult};

impl HidApi {
    /// Changes the behavior of all further calls that open a new [`HidDevice`]
    /// like [`HidApi::open`] or [`HidApi::open_path`]. By default on Darwin
    /// platform all devices opened by [`HidApi`] are opened in exclusive mode.
    ///
    /// When `exclusive` is set to:
    ///   * `false` - all further devices will be opened in non-exclusive mode.
    ///   * `true` all further devices will be opened in exclusive mode.
    pub fn set_open_exclusive(&self, exclusive: bool) {
        unsafe { ffi::macos::hid_darwin_set_open_exclusive(exclusive as c_int) }
    }

    /// Get the current opening behavior set by [`HidApi::set_open_exclusive`].
    pub fn get_open_exclusive(&self) -> bool {
        unsafe { ffi::macos::hid_darwin_get_open_exclusive() != 0 }
    }
}

impl HidDevice {
    /// Get the location ID for a [`HidDevice`] device.
    pub fn get_location_id(&self) -> HidResult<u32> {
        self.inner.get_location_id()
    }

    /// Check if the device was opened in exclusive mode.
    pub fn is_open_exclusive(&self) -> HidResult<bool> {
        self.inner.is_open_exclusive()
    }
}
