use super::{
    HidResult, descriptor,
    discovery::{DeviceInfo, PreparsedData, open_device_handle},
};
use std::{
    cell::{Cell, RefCell, UnsafeCell},
    ffi::CStr,
    fmt, io,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    ptr::null,
};
use windows_sys::{
    Win32::{
        Devices::HumanInterfaceDevice::{HidD_SetFeature, HidD_SetNumInputBuffers},
        Foundation::{ERROR_IO_INCOMPLETE, ERROR_IO_PENDING, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT},
        Storage::FileSystem::{ReadFile, WriteFile},
        System::{
            IO::{CancelIoEx, DeviceIoControl, GetOverlappedResult, OVERLAPPED},
            Threading::{CreateEventW, INFINITE, ResetEvent, WaitForSingleObject},
        },
    },
    core::BOOL,
};

const WRITE_TIMEOUT_MS: u32 = 1000;
// HID_CTL_CODE(100, METHOD_OUT_DIRECT), from hidclass.h.
const IOCTL_HID_GET_FEATURE: u32 = 0x000b_0192;

/// An independently opened HID interface. A timed-out read stays pending until
/// the next poll or until the handle is dropped.
pub struct HidDevice {
    handle: OwnedHandle,
    info: DeviceInfo,
    blocking: Cell<bool>,
    read: RefCell<ReportIo>,
    write: RefCell<ReportIo>,
    feature: RefCell<ReportIo>,
}

impl fmt::Debug for HidDevice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HidDevice")
            .field("path", &self.info.path())
            .finish()
    }
}

impl HidDevice {
    pub(super) fn open(path: &CStr) -> HidResult<Self> {
        let path_text = path
            .to_str()
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        let wide_path: Vec<u16> = path_text.encode_utf16().chain([0]).collect();
        let handle = open_device_handle(&wide_path)?;
        let raw = handle.as_raw_handle();
        // SAFETY: raw is a live HID handle.
        if !unsafe { HidD_SetNumInputBuffers(raw, 64) } {
            return Err(io::Error::last_os_error().into());
        }
        let caps = PreparsedData::load(raw)?.caps()?;
        let info = DeviceInfo::from_handle(path.to_owned(), &wide_path, raw, &caps);
        Ok(Self {
            handle,
            info,
            blocking: Cell::new(true),
            read: RefCell::new(ReportIo::new(caps.InputReportByteLength)?),
            write: RefCell::new(ReportIo::new(caps.OutputReportByteLength)?),
            feature: RefCell::new(ReportIo::new(caps.FeatureReportByteLength)?),
        })
    }

    pub fn set_blocking_mode(&self, blocking: bool) -> HidResult<()> {
        self.blocking.set(blocking);
        Ok(())
    }

    pub fn read(&self, output: &mut [u8]) -> HidResult<usize> {
        self.read_timeout(output, if self.blocking.get() { -1 } else { 0 })
    }

    pub fn read_timeout(&self, output: &mut [u8], timeout_ms: i32) -> HidResult<usize> {
        report_len(output)?;
        let mut state = self.read.borrow_mut();
        let handle = self.handle.as_raw_handle();
        let immediate = if state.operation.pending {
            None
        } else {
            // SAFETY: the device holds this state and drains it before closing.
            unsafe { state.start_read(handle) }?
        };
        let transferred = match immediate {
            Some(count) => Some(count),
            None => {
                let timeout = u32::try_from(timeout_ms).unwrap_or(INFINITE);
                state.operation.wait(handle, timeout)?
            }
        };
        let Some(count) = transferred else {
            return Ok(0);
        };
        Ok(copy_input_report(&state.buffer, count, output))
    }

    pub fn write(&self, data: &[u8]) -> HidResult<usize> {
        report_len(data)?;
        let mut state = self.write.borrow_mut();
        state.fill(data);
        let ReportIo { buffer, operation } = &mut *state;
        let handle = self.handle.as_raw_handle();
        let started = operation.start(|overlapped, bytes| {
            // SAFETY: the report is padded to the output length and remains live
            // through completion. This method drains a timed-out write before returning.
            unsafe {
                WriteFile(
                    handle,
                    buffer.as_ptr(),
                    buffer.len() as u32,
                    bytes,
                    overlapped,
                )
            }
        })?;
        if let Some(transferred) = started {
            return Ok(transferred);
        }
        match operation.wait(handle, WRITE_TIMEOUT_MS) {
            Ok(Some(transferred)) => Ok(transferred),
            result => {
                operation.cancel(handle);
                Err(result
                    .err()
                    .unwrap_or_else(|| io::ErrorKind::TimedOut.into())
                    .into())
            }
        }
    }

    pub fn send_feature_report(&self, data: &[u8]) -> HidResult<()> {
        report_len(data)?;
        let mut state = self.feature.borrow_mut();
        state.fill(data);
        // SAFETY: the handle is live and state owns the padded report for this synchronous call.
        let ok = unsafe {
            HidD_SetFeature(
                self.handle.as_raw_handle(),
                state.buffer.as_ptr().cast(),
                state.buffer.len() as u32,
            )
        };
        if !ok {
            return Err(io::Error::last_os_error().into());
        }
        Ok(())
    }

    /// Byte zero selects the report on input and retains the report ID on output.
    pub fn get_feature_report(&self, buffer: &mut [u8]) -> HidResult<usize> {
        let len = report_len(buffer)?;
        let mut state = self.feature.borrow_mut();
        let operation = &mut state.operation;
        let handle = self.handle.as_raw_handle();
        let pointer = buffer.as_mut_ptr().cast();
        let started = operation.start(|overlapped, bytes| {
            // SAFETY: this ioctl uses the caller's report buffer for input and
            // output. We wait for completion before returning the borrow.
            unsafe {
                DeviceIoControl(
                    handle,
                    IOCTL_HID_GET_FEATURE,
                    pointer,
                    len,
                    pointer,
                    len,
                    bytes,
                    overlapped,
                )
            }
        })?;
        let count = if let Some(transferred) = started {
            transferred
        } else {
            match operation.finish(handle, true) {
                Ok(transferred) => transferred,
                Err(error) => {
                    operation.cancel(handle);
                    return Err(error.into());
                }
            }
        };
        // Windows omits the leading zero report ID from its transferred count.
        Ok(count
            .saturating_add(usize::from(buffer[0] == 0))
            .min(buffer.len()))
    }

    pub fn get_report_descriptor(&self, output: &mut [u8]) -> HidResult<usize> {
        let data = PreparsedData::load(self.handle.as_raw_handle())?;
        let descriptor = descriptor::get_descriptor(&data)?;
        let len = output.len().min(descriptor.len());
        output[..len].copy_from_slice(&descriptor[..len]);
        Ok(len)
    }

    pub fn get_manufacturer_string(&self) -> HidResult<Option<String>> {
        Ok(self.info.manufacturer_string().map(str::to_owned))
    }

    pub fn get_product_string(&self) -> HidResult<Option<String>> {
        Ok(self.info.product_string().map(str::to_owned))
    }

    pub fn get_serial_number_string(&self) -> HidResult<Option<String>> {
        Ok(self.info.serial_number().map(str::to_owned))
    }
}

impl Drop for HidDevice {
    fn drop(&mut self) {
        let handle = self.handle.as_raw_handle();
        self.read.get_mut().operation.cancel(handle);
        self.write.get_mut().operation.cancel(handle);
        self.feature.get_mut().operation.cancel(handle);
    }
}

fn report_len(bytes: &[u8]) -> io::Result<u32> {
    if bytes.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "HID report is empty",
        ));
    }
    u32::try_from(bytes.len()).map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))
}

fn copy_input_report(report: &[u8], count: usize, output: &mut [u8]) -> usize {
    let report = &report[..count.min(report.len())];
    // For unnumbered input reports, Windows inserts a zero report ID.
    let report = report.strip_prefix(&[0]).unwrap_or(report);
    let len = output.len().min(report.len());
    output[..len].copy_from_slice(&report[..len]);
    len
}

struct ReportIo {
    buffer: Vec<u8>,
    operation: Operation,
}

impl ReportIo {
    fn new(size: u16) -> io::Result<Self> {
        Ok(Self {
            buffer: vec![0; usize::from(size)],
            operation: Operation::new()?,
        })
    }

    fn fill(&mut self, source: &[u8]) {
        let count = source.len().min(self.buffer.len());
        self.buffer[..count].copy_from_slice(&source[..count]);
        self.buffer[count..].fill(0);
    }

    // The caller must retain the handle and state until completion or cancellation.
    unsafe fn start_read(&mut self, handle: HANDLE) -> io::Result<Option<usize>> {
        assert!(!self.operation.pending);
        self.buffer.fill(0);
        let pointer = self.buffer.as_mut_ptr();
        let len = self.buffer.len() as u32;
        self.operation.start(|overlapped, bytes| {
            // SAFETY: the owning device holds the handle, buffer, and OVERLAPPED
            // until the read completes or is drained during drop.
            unsafe { ReadFile(handle, pointer, len, bytes, overlapped) }
        })
    }
}

struct Operation {
    event: OwnedHandle,
    overlapped: Box<UnsafeCell<OVERLAPPED>>,
    pending: bool,
}

// SAFETY: both handles may be used on another thread. OVERLAPPED stays at a
// stable address, and the only Rust access to it is while no operation is pending.
unsafe impl Send for Operation {}

impl Operation {
    fn new() -> io::Result<Self> {
        // SAFETY: create an unnamed manual-reset event with default attributes.
        let event = unsafe { CreateEventW(null(), 1, 0, null()) };
        if event.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: CreateEventW transferred ownership of this handle.
        let event = unsafe { OwnedHandle::from_raw_handle(event) };
        Ok(Self {
            event,
            overlapped: Box::new(UnsafeCell::new(OVERLAPPED::default())),
            pending: false,
        })
    }

    fn start(
        &mut self,
        issue: impl FnOnce(*mut OVERLAPPED, *mut u32) -> BOOL,
    ) -> io::Result<Option<usize>> {
        assert!(!self.pending);
        let event = self.event.as_raw_handle();
        let overlapped = self.overlapped.get();
        // SAFETY: the event is live and the OS holds no outstanding request.
        if unsafe { ResetEvent(event) } == 0 {
            return Err(io::Error::last_os_error());
        }
        unsafe {
            overlapped.write(OVERLAPPED {
                hEvent: event,
                ..Default::default()
            })
        };
        let mut transferred = 0;
        if issue(overlapped, &mut transferred) != 0 {
            return Ok(Some(transferred as usize));
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(ERROR_IO_PENDING as i32) {
            self.pending = true;
            Ok(None)
        } else {
            Err(error)
        }
    }

    fn wait(&mut self, handle: HANDLE, timeout: u32) -> io::Result<Option<usize>> {
        // SAFETY: this live event belongs to the pending request.
        match unsafe { WaitForSingleObject(self.event.as_raw_handle(), timeout) } {
            WAIT_TIMEOUT => Ok(None),
            WAIT_OBJECT_0 => match self.finish(handle, false) {
                Ok(count) => Ok(Some(count)),
                Err(_) if self.pending => Ok(None),
                Err(error) => Err(error),
            },
            _ => Err(io::Error::last_os_error()),
        }
    }

    fn finish(&mut self, handle: HANDLE, wait: bool) -> io::Result<usize> {
        let mut transferred = 0;
        // SAFETY: handle and OVERLAPPED are the pair used to start this request.
        let result = unsafe {
            GetOverlappedResult(handle, self.overlapped.get(), &mut transferred, wait.into())
        };
        if result == 0 {
            let error = io::Error::last_os_error();
            self.pending = error.raw_os_error() == Some(ERROR_IO_INCOMPLETE as i32);
            return Err(error);
        }
        self.pending = false;
        Ok(transferred as usize)
    }

    fn cancel(&mut self, handle: HANDLE) {
        if self.pending {
            // SAFETY: this is the live OVERLAPPED of the outstanding request.
            unsafe { CancelIoEx(handle, self.overlapped.get()) };
            // Cancellation is asynchronous. Drain even when completion raced
            // the cancel call so the buffers and event can safely be released.
            while self.pending {
                let _ = self.finish(handle, true);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs::OpenOptions, io::Write};
    use windows_sys::Win32::{
        Foundation::{ERROR_PIPE_CONNECTED, INVALID_HANDLE_VALUE},
        Storage::FileSystem::{FILE_FLAG_OVERLAPPED, PIPE_ACCESS_INBOUND},
        System::Pipes::{
            ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE, PIPE_WAIT,
        },
    };

    struct PipeReader {
        handle: OwnedHandle,
        read: ReportIo,
    }

    impl Drop for PipeReader {
        fn drop(&mut self) {
            self.read.operation.cancel(self.handle.as_raw_handle());
        }
    }

    #[test]
    fn timed_out_reads_keep_the_request_until_completion_or_cancel() -> io::Result<()> {
        let name = format!(r"\\.\pipe\deadlib-hid-read-{}", std::process::id());
        let wide: Vec<u16> = name.encode_utf16().chain([0]).collect();
        // SAFETY: wide is NUL-terminated and the server handle is transferred to us.
        let handle = unsafe {
            CreateNamedPipeW(
                wide.as_ptr(),
                PIPE_ACCESS_INBOUND | FILE_FLAG_OVERLAPPED,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                1,
                0,
                64,
                0,
                null(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: handle is a newly created and uniquely owned pipe handle.
        let server_handle = unsafe { OwnedHandle::from_raw_handle(handle) };
        let mut server = PipeReader {
            handle: server_handle,
            read: ReportIo::new(8)?,
        };
        let mut client = OpenOptions::new().write(true).open(name)?;
        let mut connection = OVERLAPPED::default();
        // SAFETY: the pipe is already connected by client. No request is started.
        assert_eq!(unsafe { ConnectNamedPipe(handle, &mut connection) }, 0);
        assert_eq!(
            io::Error::last_os_error().raw_os_error(),
            Some(ERROR_PIPE_CONNECTED as i32)
        );

        // SAFETY: PipeReader drains its state before closing the handle.
        assert_eq!(unsafe { server.read.start_read(handle) }?, None);
        assert_eq!(server.read.operation.wait(handle, 0)?, None);
        client.write_all(&[7, 8, 9])?;
        let transferred = server.read.operation.wait(handle, 5000)?;
        assert_eq!(transferred, Some(3));
        assert_eq!(&server.read.buffer[..3], &[7, 8, 9]);

        // SAFETY: PipeReader drains its state before closing the handle.
        assert_eq!(unsafe { server.read.start_read(handle) }?, None);
        server.read.operation.cancel(handle);
        assert!(!server.read.operation.pending);
        Ok(())
    }

    #[test]
    fn copy_input_reports_keeps_ids_and_omits_the_unnumbered_prefix() {
        let mut output = [0; 2];
        assert_eq!(copy_input_report(&[0, 7, 8], 3, &mut output), 2);
        assert_eq!(output, [7, 8]);
        assert_eq!(copy_input_report(&[1, 7, 8], 3, &mut output), 2);
        assert_eq!(output, [1, 7]);
    }
}
