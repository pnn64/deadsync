use super::devd::{DevdEvent, DevdWatch};
use super::{GpSystemEvent, PadBackend, PadCode, PadDir, PadEvent, PadId, uuid_from_bytes};
use crate::core::host_time::now_nanos;
use hidparser::{Report, ReportField, VariableField, parse_report_descriptor};
use log::{debug, warn};
use std::ffi::c_void;
use std::fs;
use std::os::fd::AsRawFd;
use std::time::Instant;

const POLLIN: i16 = 0x0001;
const POLLERR: i16 = 0x0008;
const POLLHUP: i16 = 0x0010;
const POLLNVAL: i16 = 0x0020;
const HID_MAX_DESCRIPTOR_SIZE: usize = 4096;

const IOC_NRBITS: u32 = 8;
const IOC_TYPEBITS: u32 = 8;
const IOC_SIZEBITS: u32 = 14;
const IOC_NRSHIFT: u32 = 0;
const IOC_TYPESHIFT: u32 = IOC_NRSHIFT + IOC_NRBITS;
const IOC_SIZESHIFT: u32 = IOC_TYPESHIFT + IOC_TYPEBITS;
const IOC_DIRSHIFT: u32 = IOC_SIZESHIFT + IOC_SIZEBITS;
const IOC_READ: u32 = 2;

const USAGE_PAGE_GENERIC_DESKTOP: u16 = 0x01;
const USAGE_PAGE_BUTTON: u16 = 0x09;

const USAGE_JOYSTICK: u16 = 0x04;
const USAGE_GAMEPAD: u16 = 0x05;
const USAGE_MULTIAXIS: u16 = 0x08;

const USAGE_X: u16 = 0x30;
const USAGE_Y: u16 = 0x31;
const USAGE_Z: u16 = 0x32;
const USAGE_RX: u16 = 0x33;
const USAGE_RY: u16 = 0x34;
const USAGE_RZ: u16 = 0x35;
const USAGE_SLIDER: u16 = 0x36;
const USAGE_DIAL: u16 = 0x37;
const USAGE_WHEEL: u16 = 0x38;
const USAGE_HAT: u16 = 0x39;
const USAGE_DPAD_UP: u16 = 0x90;
const USAGE_DPAD_DOWN: u16 = 0x91;
const USAGE_DPAD_RIGHT: u16 = 0x92;
const USAGE_DPAD_LEFT: u16 = 0x93;

#[repr(C)]
#[derive(Clone, Copy)]
struct PollFd {
    fd: i32,
    events: i16,
    revents: i16,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct HidrawReportDescriptor {
    size: u32,
    value: [u8; HID_MAX_DESCRIPTOR_SIZE],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct HidrawDevInfo {
    bustype: u32,
    vendor: u16,
    product: u16,
}

unsafe extern "C" {
    fn poll(fds: *mut PollFd, nfds: usize, timeout: i32) -> i32;
    fn read(fd: i32, buf: *mut c_void, count: usize) -> isize;
}

const fn ioc(dir: u32, type_: u32, nr: u32, size: u32) -> libc::c_ulong {
    ((dir << IOC_DIRSHIFT)
        | (type_ << IOC_TYPESHIFT)
        | (nr << IOC_NRSHIFT)
        | (size << IOC_SIZESHIFT)) as libc::c_ulong
}

const fn ior(type_: u8, nr: u8, size: usize) -> libc::c_ulong {
    ioc(IOC_READ, type_ as u32, nr as u32, size as u32)
}

const HIDIOCGRDESCSIZE: libc::c_ulong = ior(b'H', 0x01, std::mem::size_of::<libc::c_int>());
const HIDIOCGRDESC: libc::c_ulong = ior(b'H', 0x02, std::mem::size_of::<HidrawReportDescriptor>());
const HIDIOCGRAWINFO: libc::c_ulong = ior(b'H', 0x03, std::mem::size_of::<HidrawDevInfo>());

#[inline(always)]
const fn hidiocgrawname(len: usize) -> libc::c_ulong {
    ior(b'H', 0x04, len)
}

struct Dev {
    id: PadId,
    uuid: [u8; 16],
    name: String,
    path: String,
    vendor_id: Option<u16>,
    product_id: Option<u16>,
    file: std::fs::File,
    max_report_len: usize,
    reports: Vec<ReportSpec>,
}

struct ReportSpec {
    report_id: Option<u8>,
    payload_bytes: usize,
    fields: Vec<FieldSpec>,
}

enum FieldSpec {
    Button(ButtonField),
    Axis(AxisField),
    Hat(HatField),
    Dpad(DpadField),
}

struct ButtonField {
    code: PadCode,
    field: VariableField,
    last_value: Option<i64>,
}

struct AxisField {
    code: PadCode,
    field: VariableField,
    last_value: Option<i64>,
}

struct HatField {
    code: PadCode,
    field: VariableField,
    last_value: Option<i64>,
    dir: [bool; 4],
}

struct DpadField {
    code: PadCode,
    dir: PadDir,
    field: VariableField,
    last_pressed: Option<bool>,
}

#[inline(always)]
fn is_controller_collection(field: &VariableField) -> bool {
    field.member_of.iter().any(|collection| {
        collection.usage.page() == USAGE_PAGE_GENERIC_DESKTOP
            && matches!(
                collection.usage.id(),
                USAGE_JOYSTICK | USAGE_GAMEPAD | USAGE_MULTIAXIS
            )
    })
}

#[inline(always)]
fn is_axis_usage(usage_id: u16) -> bool {
    matches!(
        usage_id,
        USAGE_X
            | USAGE_Y
            | USAGE_Z
            | USAGE_RX
            | USAGE_RY
            | USAGE_RZ
            | USAGE_SLIDER
            | USAGE_DIAL
            | USAGE_WHEEL
    )
}

#[inline(always)]
fn button_pressed(field: &VariableField, value: i64) -> bool {
    let min = i64::from(i32::from(field.logical_minimum));
    let max = i64::from(i32::from(field.logical_maximum));
    if min == 0 && max == 1 {
        value != 0
    } else {
        value > min
    }
}

#[inline(always)]
fn hat_dirs(field: &VariableField, raw_value: i64) -> [bool; 4] {
    let min = i32::from(field.logical_minimum);
    let max = i32::from(field.logical_maximum);
    let span = max.saturating_sub(min).saturating_add(1);
    let idx = (raw_value as i32).saturating_sub(min);
    if span == 4 {
        return [idx == 0, idx == 2, idx == 3, idx == 1];
    }
    if !(0..=7).contains(&idx) {
        return [false; 4];
    }
    [
        matches!(idx, 0 | 1 | 7),
        matches!(idx, 3..=5),
        matches!(idx, 5..=7),
        matches!(idx, 1..=3),
    ]
}

fn build_field_spec(field: &VariableField) -> Option<FieldSpec> {
    if field.attributes.constant {
        return None;
    }
    let code = PadCode(u32::from(field.usage));
    let usage_page = field.usage.page();
    let usage_id = field.usage.id();
    if usage_page == USAGE_PAGE_GENERIC_DESKTOP && usage_id == USAGE_HAT {
        return Some(FieldSpec::Hat(HatField {
            code,
            field: field.clone(),
            last_value: None,
            dir: [false; 4],
        }));
    }
    if usage_page == USAGE_PAGE_GENERIC_DESKTOP {
        let dir = match usage_id {
            USAGE_DPAD_UP => Some(PadDir::Up),
            USAGE_DPAD_DOWN => Some(PadDir::Down),
            USAGE_DPAD_LEFT => Some(PadDir::Left),
            USAGE_DPAD_RIGHT => Some(PadDir::Right),
            _ => None,
        };
        if let Some(dir) = dir {
            return Some(FieldSpec::Dpad(DpadField {
                code,
                dir,
                field: field.clone(),
                last_pressed: None,
            }));
        }
    }
    if usage_page == USAGE_PAGE_BUTTON || matches!(field.field_range(), Some(1)) {
        return Some(FieldSpec::Button(ButtonField {
            code,
            field: field.clone(),
            last_value: None,
        }));
    }
    if usage_page != USAGE_PAGE_GENERIC_DESKTOP || is_axis_usage(usage_id) {
        return Some(FieldSpec::Axis(AxisField {
            code,
            field: field.clone(),
            last_value: None,
        }));
    }
    Some(FieldSpec::Axis(AxisField {
        code,
        field: field.clone(),
        last_value: None,
    }))
}

fn build_report_spec(report: &Report) -> Option<ReportSpec> {
    let controller_report = report.fields.iter().any(|field| match field {
        ReportField::Variable(field) => is_controller_collection(field),
        _ => false,
    });
    if !controller_report {
        return None;
    }
    let mut fields = Vec::new();
    for field in &report.fields {
        let ReportField::Variable(field) = field else {
            continue;
        };
        if let Some(spec) = build_field_spec(field) {
            fields.push(spec);
        }
    }
    if fields.is_empty() {
        return None;
    }
    Some(ReportSpec {
        report_id: report.report_id.map(|id| u32::from(id) as u8),
        payload_bytes: report.size_in_bits.div_ceil(8),
        fields,
    })
}

fn raw_device_name(file: &std::fs::File) -> Option<String> {
    let mut buf = [0u8; 256];
    let rc = unsafe {
        libc::ioctl(
            file.as_raw_fd(),
            hidiocgrawname(buf.len()),
            buf.as_mut_ptr(),
        )
    };
    if rc < 0 {
        return None;
    }
    let len = buf.iter().position(|b| *b == 0).unwrap_or(buf.len());
    std::str::from_utf8(&buf[..len]).ok().map(str::to_owned)
}

fn raw_device_info(file: &std::fs::File) -> (Option<u16>, Option<u16>) {
    let mut info = HidrawDevInfo::default();
    let rc = unsafe { libc::ioctl(file.as_raw_fd(), HIDIOCGRAWINFO, &mut info) };
    if rc < 0 {
        return (None, None);
    }
    (Some(info.vendor), Some(info.product))
}

fn raw_report_descriptor(file: &std::fs::File) -> Result<Vec<u8>, String> {
    let mut size: libc::c_int = 0;
    let rc = unsafe { libc::ioctl(file.as_raw_fd(), HIDIOCGRDESCSIZE, &mut size) };
    if rc < 0 {
        return Err("HIDIOCGRDESCSIZE failed".to_owned());
    }
    if size <= 0 {
        return Err("hidraw descriptor size was zero".to_owned());
    }
    let size = (size as usize).min(HID_MAX_DESCRIPTOR_SIZE);
    let mut desc = HidrawReportDescriptor {
        size: size as u32,
        value: [0; HID_MAX_DESCRIPTOR_SIZE],
    };
    let rc = unsafe { libc::ioctl(file.as_raw_fd(), HIDIOCGRDESC, &mut desc) };
    if rc < 0 {
        return Err("HIDIOCGRDESC failed".to_owned());
    }
    Ok(desc.value[..(desc.size as usize).min(HID_MAX_DESCRIPTOR_SIZE)].to_vec())
}

#[inline(always)]
fn is_hidraw_path(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .is_some_and(|name| name.starts_with("hidraw"))
}

fn scan_hidraw_paths() -> Vec<String> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir("/dev") else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let path = path.to_string_lossy().to_string();
        if is_hidraw_path(&path) {
            out.push(path);
        }
    }
    out.sort_unstable();
    out
}

fn open_dev(
    path: String,
    id: PadId,
    initial: bool,
    emit_sys: &mut impl FnMut(GpSystemEvent),
) -> Option<Dev> {
    let file = match std::fs::File::open(&path) {
        Ok(file) => file,
        Err(err) => {
            warn!("freebsd hidraw could not open '{path}': {err}");
            return None;
        }
    };
    let desc = match raw_report_descriptor(&file) {
        Ok(desc) => desc,
        Err(err) => {
            debug!("freebsd hidraw '{path}' skipped: {err}");
            return None;
        }
    };
    let parsed = match parse_report_descriptor(&desc) {
        Ok(parsed) => parsed,
        Err(err) => {
            debug!("freebsd hidraw '{path}' descriptor parse failed: {err:?}");
            return None;
        }
    };
    let mut reports = Vec::new();
    for report in &parsed.input_reports {
        if let Some(spec) = build_report_spec(report) {
            reports.push(spec);
        }
    }
    if reports.is_empty() {
        debug!("freebsd hidraw '{path}' has no controller-class input reports");
        return None;
    }
    let max_report_len = reports
        .iter()
        .map(|report| report.payload_bytes + usize::from(report.report_id.is_some()))
        .max()
        .unwrap_or(0);
    if max_report_len == 0 {
        return None;
    }
    let (vendor_id, product_id) = raw_device_info(&file);
    let raw_name = raw_device_name(&file).unwrap_or_else(|| path.clone());
    let name = format!("hidraw:{raw_name}");
    emit_sys(GpSystemEvent::Connected {
        name: name.clone(),
        id,
        vendor_id,
        product_id,
        backend: PadBackend::FreeBsdHidraw,
        initial,
    });
    Some(Dev {
        id,
        uuid: uuid_from_bytes(path.as_bytes()),
        name,
        path,
        vendor_id,
        product_id,
        file,
        max_report_len,
        reports,
    })
}

fn add_dev_if_new(
    path: String,
    devs: &mut Vec<Dev>,
    next_id: &mut u32,
    initial: bool,
    emit_sys: &mut impl FnMut(GpSystemEvent),
) {
    if !is_hidraw_path(&path) || devs.iter().any(|dev| dev.path == path) {
        return;
    }
    let id = PadId(*next_id);
    if let Some(dev) = open_dev(path, id, initial, emit_sys) {
        *next_id = next_id.saturating_add(1);
        devs.push(dev);
    }
}

fn remove_dev_by_path(path: &str, devs: &mut Vec<Dev>, emit_sys: &mut impl FnMut(GpSystemEvent)) {
    if let Some(idx) = devs.iter().position(|dev| dev.path == path) {
        let dev = devs.swap_remove(idx);
        emit_sys(GpSystemEvent::Disconnected {
            name: dev.name,
            id: dev.id,
            backend: PadBackend::FreeBsdHidraw,
            initial: false,
        });
    }
}

#[inline(always)]
fn report_payload<'a>(report: &ReportSpec, buf: &'a [u8], len: usize) -> Option<&'a [u8]> {
    match report.report_id {
        Some(report_id) => {
            if len < report.payload_bytes + 1 || buf.first().copied()? != report_id {
                return None;
            }
            Some(&buf[1..1 + report.payload_bytes])
        }
        None => (len >= report.payload_bytes).then_some(&buf[..report.payload_bytes]),
    }
}

fn process_report(
    id: PadId,
    uuid: [u8; 16],
    payload: &[u8],
    timestamp: Instant,
    host_nanos: u64,
    emit_pad: &mut impl FnMut(PadEvent),
    fields: &mut [FieldSpec],
) {
    for field in fields {
        match field {
            FieldSpec::Button(field) => {
                let Some(value) = field.field.field_value(payload) else {
                    continue;
                };
                if field.last_value == Some(value) {
                    continue;
                }
                field.last_value = Some(value);
                let pressed = button_pressed(&field.field, value);
                emit_pad(PadEvent::RawButton {
                    id,
                    timestamp,
                    host_nanos,
                    code: field.code,
                    uuid,
                    value: value as f32,
                    pressed,
                });
            }
            FieldSpec::Axis(field) => {
                let Some(value) = field.field.field_value(payload) else {
                    continue;
                };
                if field.last_value == Some(value) {
                    continue;
                }
                field.last_value = Some(value);
                emit_pad(PadEvent::RawAxis {
                    id,
                    timestamp,
                    host_nanos,
                    code: field.code,
                    uuid,
                    value: value as f32,
                });
            }
            FieldSpec::Hat(field) => {
                let Some(value) = field.field.field_value(payload) else {
                    continue;
                };
                if field.last_value == Some(value) {
                    continue;
                }
                field.last_value = Some(value);
                emit_pad(PadEvent::RawAxis {
                    id,
                    timestamp,
                    host_nanos,
                    code: field.code,
                    uuid,
                    value: value as f32,
                });
                let want = hat_dirs(&field.field, value);
                for (idx, dir) in [PadDir::Up, PadDir::Down, PadDir::Left, PadDir::Right]
                    .into_iter()
                    .enumerate()
                {
                    if field.dir[idx] == want[idx] {
                        continue;
                    }
                    field.dir[idx] = want[idx];
                    emit_pad(PadEvent::Dir {
                        id,
                        timestamp,
                        host_nanos,
                        dir,
                        pressed: want[idx],
                    });
                }
            }
            FieldSpec::Dpad(field) => {
                let Some(value) = field.field.field_value(payload) else {
                    continue;
                };
                let pressed = button_pressed(&field.field, value);
                if field.last_pressed == Some(pressed) {
                    continue;
                }
                field.last_pressed = Some(pressed);
                emit_pad(PadEvent::RawButton {
                    id,
                    timestamp,
                    host_nanos,
                    code: field.code,
                    uuid,
                    value: if pressed { 1.0 } else { 0.0 },
                    pressed,
                });
                emit_pad(PadEvent::Dir {
                    id,
                    timestamp,
                    host_nanos,
                    dir: field.dir,
                    pressed,
                });
            }
        }
    }
}

pub fn run(
    emit_pad: &mut impl FnMut(PadEvent),
    emit_sys: &mut impl FnMut(GpSystemEvent),
) -> Result<(), String> {
    let watch = DevdWatch::new();
    let mut devs = Vec::new();
    let mut next_id = 0u32;
    let mut saw_hidraw = false;
    for path in scan_hidraw_paths() {
        saw_hidraw = true;
        add_dev_if_new(path, &mut devs, &mut next_id, true, emit_sys);
    }
    if devs.is_empty() && (!saw_hidraw || watch.is_none()) {
        return Err("no usable hidraw controller devices found".to_owned());
    }
    emit_sys(GpSystemEvent::StartupComplete);
    let mut pollfds = Vec::with_capacity(9);
    let mut buf = vec![0u8; 64];

    loop {
        pollfds.clear();
        let watch_offset = if let Some(watch) = &watch {
            pollfds.push(PollFd {
                fd: watch.fd(),
                events: POLLIN,
                revents: 0,
            });
            1usize
        } else {
            0usize
        };
        pollfds.extend(devs.iter().map(|dev| PollFd {
            fd: dev.file.as_raw_fd(),
            events: POLLIN,
            revents: 0,
        }));
        let poll_ptr = if pollfds.is_empty() {
            std::ptr::null_mut()
        } else {
            pollfds.as_mut_ptr()
        };
        let rc = unsafe { poll(poll_ptr, pollfds.len(), -1) };
        if rc < 0 {
            continue;
        }

        let mut hotplug = Vec::new();
        if watch_offset == 1 {
            let revents = pollfds[0].revents;
            if (revents & (POLLERR | POLLHUP | POLLNVAL)) != 0 {
                warn!("freebsd hidraw devd watcher reported poll error");
            }
            if (revents & POLLIN) != 0
                && let Some(watch) = &watch
            {
                hotplug = watch.collect_events();
            }
        }

        let mut remove = Vec::new();
        for idx in 0..devs.len() {
            let revents = pollfds[idx + watch_offset].revents;
            if (revents & (POLLERR | POLLHUP | POLLNVAL)) != 0 {
                remove.push(idx);
                continue;
            }
            if (revents & POLLIN) == 0 {
                continue;
            }
            let dev = &mut devs[idx];
            if dev.max_report_len > buf.len() {
                buf.resize(dev.max_report_len, 0);
            }
            let n = unsafe {
                read(
                    pollfds[idx + watch_offset].fd,
                    buf.as_mut_ptr().cast::<c_void>(),
                    dev.max_report_len,
                )
            };
            if n <= 0 {
                remove.push(idx);
                continue;
            }
            let timestamp = Instant::now();
            let host_nanos = now_nanos();
            let len = n as usize;
            let mut handled = false;
            let id = dev.id;
            let uuid = dev.uuid;
            for report in &mut dev.reports {
                let Some(payload) = report_payload(report, &buf, len) else {
                    continue;
                };
                handled = true;
                process_report(
                    id,
                    uuid,
                    payload,
                    timestamp,
                    host_nanos,
                    emit_pad,
                    &mut report.fields,
                );
                break;
            }
            if !handled {
                debug!(
                    "freebsd hidraw '{}' received unsupported report size/id",
                    dev.name
                );
            }
        }

        remove.sort_unstable();
        remove.dedup();
        for &idx in remove.iter().rev() {
            let dev = devs.swap_remove(idx);
            emit_sys(GpSystemEvent::Disconnected {
                name: dev.name,
                id: dev.id,
                backend: PadBackend::FreeBsdHidraw,
                initial: false,
            });
        }
        for event in hotplug {
            match event {
                DevdEvent::Create(path) => {
                    add_dev_if_new(path, &mut devs, &mut next_id, false, emit_sys)
                }
                DevdEvent::Destroy(path) => remove_dev_by_path(&path, &mut devs, emit_sys),
            }
        }
    }
}
