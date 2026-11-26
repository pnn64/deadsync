use super::{BusType, HidApi, HidDevice, HidError, HidResult};
use std::{
    ffi::{CStr, CString, c_void},
    io,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Devices::{
        DeviceAndDriverInstallation::{
            CM_GET_DEVICE_INTERFACE_LIST_PRESENT, CM_Get_DevNode_PropertyW,
            CM_Get_Device_Interface_List_SizeW, CM_Get_Device_Interface_ListW,
            CM_Get_Device_Interface_PropertyW, CM_Get_Parent, CM_LOCATE_DEVNODE_NORMAL,
            CM_Locate_DevNodeW, CR_BUFFER_SMALL, CR_SUCCESS,
        },
        HumanInterfaceDevice::{
            HIDD_ATTRIBUTES, HIDP_CAPS, HIDP_STATUS_SUCCESS, HidD_FreePreparsedData,
            HidD_GetAttributes, HidD_GetHidGuid, HidD_GetManufacturerString, HidD_GetPreparsedData,
            HidD_GetProductString, HidD_GetSerialNumberString, HidP_GetCaps, PHIDP_PREPARSED_DATA,
        },
        Properties::{
            DEVPKEY_Device_CompatibleIds, DEVPKEY_Device_HardwareIds, DEVPKEY_Device_InstanceId,
            DEVPKEY_Device_Manufacturer, DEVPKEY_NAME, DEVPROP_TYPE_STRING,
            DEVPROP_TYPE_STRING_LIST,
        },
    },
    Foundation::{DEVPROPKEY, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE},
    Storage::FileSystem::{
        CreateFileW, FILE_FLAG_OVERLAPPED, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    },
};
use windows_sys::core::GUID;

#[derive(Clone, Debug)]
pub struct DeviceInfo {
    path: CString,
    vendor: u16,
    product_id: u16,
    release: u16,
    manufacturer: Option<String>,
    product: Option<String>,
    serial: Option<String>,
    usage_page: u16,
    usage: u16,
    interface: i32,
    bus: BusType,
}

impl DeviceInfo {
    pub fn path(&self) -> &CStr {
        &self.path
    }

    pub fn vendor_id(&self) -> u16 {
        self.vendor
    }

    pub fn product_id(&self) -> u16 {
        self.product_id
    }

    pub fn release_number(&self) -> u16 {
        self.release
    }

    pub fn manufacturer_string(&self) -> Option<&str> {
        self.manufacturer.as_deref()
    }

    pub fn product_string(&self) -> Option<&str> {
        self.product.as_deref()
    }

    pub fn serial_number(&self) -> Option<&str> {
        self.serial.as_deref()
    }

    pub fn usage_page(&self) -> u16 {
        self.usage_page
    }

    pub fn usage(&self) -> u16 {
        self.usage
    }

    pub fn interface_number(&self) -> i32 {
        self.interface
    }

    pub fn bus_type(&self) -> BusType {
        self.bus
    }

    pub fn open_device(&self, api: &HidApi) -> HidResult<HidDevice> {
        api.open_path(self.path())
    }

    pub(super) fn from_handle(
        path: CString,
        path_wide: &[u16],
        handle: HANDLE,
        caps: &HIDP_CAPS,
    ) -> Self {
        let mut attributes = HIDD_ATTRIBUTES {
            Size: size_of::<HIDD_ATTRIBUTES>() as u32,
            ..Default::default()
        };
        // SAFETY: handle is live and attributes has the Win32 layout and size.
        unsafe { HidD_GetAttributes(handle, &mut attributes) };
        let mut info = Self {
            path,
            vendor: attributes.VendorID,
            product_id: attributes.ProductID,
            release: attributes.VersionNumber,
            manufacturer: read_string(handle, HidD_GetManufacturerString),
            product: read_string(handle, HidD_GetProductString),
            serial: read_string(handle, HidD_GetSerialNumberString),
            usage_page: caps.UsagePage,
            usage: caps.Usage,
            interface: -1,
            bus: BusType::Unknown,
        };
        info.read_device_properties(path_wide);
        info
    }

    fn read_device_properties(&mut self, path: &[u16]) {
        let Some(parent) = interface_parent(path) else {
            return;
        };
        let Some(ids) = devnode_property(
            parent,
            &DEVPKEY_Device_CompatibleIds,
            DEVPROP_TYPE_STRING_LIST,
        ) else {
            return;
        };
        for id in ids.split(|unit| *unit == 0) {
            let id = String::from_utf16_lossy(id).to_ascii_uppercase();
            self.bus = if id.starts_with("USB") {
                self.read_usb_properties(parent);
                BusType::Usb
            } else if id.starts_with("BTHLEDEVICE") {
                self.read_bluetooth_le_properties(parent);
                BusType::Bluetooth
            } else if id.starts_with("BTHENUM") {
                BusType::Bluetooth
            } else if id.starts_with("PNP0C50") {
                BusType::I2c
            } else if id.starts_with("PNP0C51") {
                BusType::Spi
            } else {
                continue;
            };
            break;
        }
    }

    fn read_usb_properties(&mut self, mut node: u32) {
        if node_string(node, &DEVPKEY_Device_InstanceId)
            .is_some_and(|id| hardware_id_value(&id, "IG_").is_some())
        {
            let Some(parent) = parent_node(node) else {
                return;
            };
            node = parent;
        }
        let Some(ids) =
            devnode_property(node, &DEVPKEY_Device_HardwareIds, DEVPROP_TYPE_STRING_LIST)
        else {
            return;
        };
        for id in ids.split(|unit| *unit == 0) {
            let id = String::from_utf16_lossy(id);
            if self.release == 0 {
                self.release = hardware_id_value(&id, "REV_").unwrap_or(0);
            }
            if self.interface < 0 {
                self.interface = hardware_id_value(&id, "MI_").map_or(-1, i32::from);
            }
        }
        if self.manufacturer.as_deref().is_none_or(str::is_empty) {
            self.manufacturer =
                node_string(node, &DEVPKEY_Device_Manufacturer).or(self.manufacturer.take());
        }
        if self.serial.as_deref().is_none_or(str::is_empty) {
            // Composite HID interfaces live below the USB device carrying the serial.
            let serial_node = if self.interface < 0 {
                Some(node)
            } else {
                parent_node(node)
            };
            let serial = serial_node
                .and_then(|node| node_string(node, &DEVPKEY_Device_InstanceId))
                .and_then(|id| instance_serial(&id).map(str::to_owned));
            if serial.is_some() {
                self.serial = serial;
            }
        }
        self.interface = self.interface.max(0);
    }

    fn read_bluetooth_le_properties(&mut self, node: u32) {
        // Bluetooth LE HID nodes expose strings through device properties.
        const BLUETOOTH_PROPERTIES: GUID = GUID::from_u128(0x2bd67d8b_8beb_48d5_87e0_6cda3428040a);
        let property = |pid| {
            node_string(
                node,
                &DEVPROPKEY {
                    fmtid: BLUETOOTH_PROPERTIES,
                    pid,
                },
            )
        };
        if self.serial.as_deref().is_none_or(str::is_empty) {
            self.serial = property(1).or(self.serial.take());
        }
        if self.manufacturer.as_deref().is_none_or(str::is_empty) {
            self.manufacturer = property(4).or(self.manufacturer.take());
        }
        if self.product.as_deref().is_none_or(str::is_empty) {
            self.product = property(5)
                .or_else(|| parent_node(node).and_then(|parent| node_string(parent, &DEVPKEY_NAME)))
                .or(self.product.take());
        }
    }
}

pub(super) fn enumerate() -> HidResult<Vec<DeviceInfo>> {
    let mut guid = GUID::default();
    // SAFETY: guid points to writable storage for the HID interface class GUID.
    unsafe { HidD_GetHidGuid(&mut guid) };
    let mut list = Vec::new();
    loop {
        let mut len = 0;
        // SAFETY: the class GUID and length pointer are valid for this call.
        let status = unsafe {
            CM_Get_Device_Interface_List_SizeW(
                &mut len,
                &guid,
                null(),
                CM_GET_DEVICE_INTERFACE_LIST_PRESENT,
            )
        };
        if status != CR_SUCCESS {
            return Err(config_error(status));
        }
        list.resize(len as usize, 0);
        // SAFETY: list has room for len UTF-16 units, including the list terminator.
        let status = unsafe {
            CM_Get_Device_Interface_ListW(
                &guid,
                null(),
                list.as_mut_ptr(),
                len,
                CM_GET_DEVICE_INTERFACE_LIST_PRESENT,
            )
        };
        match status {
            CR_SUCCESS => break,
            CR_BUFFER_SMALL => continue, // A device arrived between the queries.
            _ => return Err(config_error(status)),
        }
    }
    Ok(list
        .split_inclusive(|unit| *unit == 0)
        .take_while(|path| path.len() > 1 && path.last() == Some(&0))
        .filter_map(|wide_path| {
            let handle = open_handle(wide_path, 0).ok()?;
            let raw_handle = handle.as_raw_handle();
            let caps = PreparsedData::load(raw_handle)
                .and_then(|data| data.caps())
                .unwrap_or_default();
            let path =
                CString::new(String::from_utf16_lossy(&wide_path[..wide_path.len() - 1])).ok()?;
            Some(DeviceInfo::from_handle(path, wide_path, raw_handle, &caps))
        })
        .collect())
}

pub(super) fn open_device_handle(path: &[u16]) -> io::Result<OwnedHandle> {
    // Restricted system interfaces can still expose metadata and feature reports.
    open_handle(path, GENERIC_READ | GENERIC_WRITE).or_else(|_| open_handle(path, 0))
}

fn open_handle(path: &[u16], access: u32) -> io::Result<OwnedHandle> {
    debug_assert_eq!(path.last(), Some(&0));
    // SAFETY: path is NUL-terminated and the returned handle is owned by the caller.
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            null(),
            OPEN_EXISTING,
            FILE_FLAG_OVERLAPPED,
            null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: CreateFileW succeeded and ownership has not been transferred.
    Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
}

pub(super) struct PreparsedData(PHIDP_PREPARSED_DATA);

impl PreparsedData {
    pub(super) fn load(handle: HANDLE) -> io::Result<Self> {
        let mut data = 0;
        // SAFETY: handle is live and data is the out parameter for an owned allocation.
        if !unsafe { HidD_GetPreparsedData(handle, &mut data) } {
            return Err(io::Error::last_os_error());
        }
        if data == 0 {
            return Err(io::Error::other("HidD_GetPreparsedData returned no data"));
        }
        Ok(Self(data))
    }

    pub(super) fn caps(&self) -> io::Result<HIDP_CAPS> {
        let mut caps = HIDP_CAPS::default();
        // SAFETY: self owns valid preparsed data and caps is writable.
        let status = unsafe { HidP_GetCaps(self.0, &mut caps) };
        if status != HIDP_STATUS_SUCCESS {
            return Err(io::Error::other(format!(
                "HidP_GetCaps returned {status:#010x}"
            )));
        }
        Ok(caps)
    }

    pub(super) fn as_ptr(&self) -> *const c_void {
        self.0 as *const c_void
    }
}

impl Drop for PreparsedData {
    fn drop(&mut self) {
        // SAFETY: HidD_GetPreparsedData transferred this allocation to self.
        unsafe { HidD_FreePreparsedData(self.0) };
    }
}

fn read_string(
    handle: HANDLE,
    get: unsafe extern "system" fn(HANDLE, *mut c_void, u32) -> bool,
) -> Option<String> {
    let mut buffer = [0u16; 256];
    // SAFETY: the handle is live and the length is the size of the writable buffer in bytes.
    let ok = unsafe {
        get(
            handle,
            buffer.as_mut_ptr().cast(),
            size_of_val(&buffer) as u32,
        )
    };
    Some(if ok {
        wide_string(&buffer)
    } else {
        String::new()
    })
}

fn wide_string(buffer: &[u16]) -> String {
    String::from_utf16_lossy(buffer.split(|unit| *unit == 0).next().unwrap_or_default())
}

fn interface_parent(path: &[u16]) -> Option<u32> {
    let mut id = wide_property(DEVPROP_TYPE_STRING, |kind, buffer, len| {
        // SAFETY: path is NUL-terminated and wide_property supplies writable parameters.
        unsafe {
            CM_Get_Device_Interface_PropertyW(
                path.as_ptr(),
                &DEVPKEY_Device_InstanceId,
                kind,
                buffer,
                len,
                0,
            )
        }
    })?;
    let mut node = 0;
    id.push(0);
    // SAFETY: id is NUL-terminated and node is a valid out parameter.
    let status = unsafe { CM_Locate_DevNodeW(&mut node, id.as_ptr(), CM_LOCATE_DEVNODE_NORMAL) };
    if status == CR_SUCCESS {
        parent_node(node)
    } else {
        None
    }
}

fn parent_node(node: u32) -> Option<u32> {
    let mut parent = 0;
    // SAFETY: parent is a writable out parameter. PnP validates node.
    (unsafe { CM_Get_Parent(&mut parent, node, 0) } == CR_SUCCESS).then_some(parent)
}

fn node_string(node: u32, key: &DEVPROPKEY) -> Option<String> {
    devnode_property(node, key, DEVPROP_TYPE_STRING).map(|buffer| wide_string(&buffer))
}

fn devnode_property(node: u32, key: &DEVPROPKEY, expected: u32) -> Option<Vec<u16>> {
    wide_property(expected, |kind, buffer, len| {
        // SAFETY: key is valid and wide_property supplies writable parameters.
        unsafe { CM_Get_DevNode_PropertyW(node, key, kind, buffer, len, 0) }
    })
}

fn wide_property(
    expected: u32,
    mut get: impl FnMut(&mut u32, *mut u8, &mut u32) -> u32,
) -> Option<Vec<u16>> {
    let mut len = 0;
    let mut kind = 0;
    if get(&mut kind, null_mut(), &mut len) != CR_BUFFER_SMALL || kind != expected || len % 2 != 0 {
        return None;
    }
    let mut buffer = vec![0u16; len as usize / 2];
    let status = get(&mut kind, buffer.as_mut_ptr().cast(), &mut len);
    if status != CR_SUCCESS || kind != expected || len % 2 != 0 {
        return None;
    }
    buffer.truncate(len as usize / 2);
    Some(buffer)
}

fn hardware_id_value(id: &str, property: &str) -> Option<u16> {
    let normalized = id.to_ascii_uppercase();
    let (_, suffix) = normalized.split_once(property)?;
    let hex = suffix.split(|ch: char| !ch.is_ascii_hexdigit()).next()?;
    u16::from_str_radix(hex, 16).ok()
}

fn instance_serial(id: &str) -> Option<&str> {
    let (_, serial) = id.rsplit_once('\\')?;
    (!serial.is_empty() && !serial.contains('&')).then_some(serial)
}

fn config_error(status: u32) -> HidError {
    HidError::HidApiError {
        message: format!("could not enumerate HID interfaces ({status:#x})"),
    }
}
