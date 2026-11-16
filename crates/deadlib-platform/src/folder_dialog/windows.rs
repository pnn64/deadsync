use super::DialogError;
use std::{ffi::OsString, os::windows::ffi::OsStringExt, path::PathBuf};
use windows::{
    Win32::{
        Foundation::ERROR_CANCELLED,
        System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
            CoCreateInstance, CoInitializeEx, CoUninitialize,
        },
        UI::Shell::{
            FOS_FORCEFILESYSTEM, FOS_PATHMUSTEXIST, FOS_PICKFOLDERS, FileOpenDialog,
            IFileOpenDialog, SIGDN_FILESYSPATH,
        },
    },
    core::{HRESULT, PCWSTR, PWSTR},
};

// Bind this allocation's destructor explicitly to ole32.dll. The generated
// binding imports combase.dll, which prevents startup on Windows 7. A normal
// import-library link can also resolve to the wrong DLL when both are linked.
#[cfg_attr(
    target_arch = "x86",
    link(
        name = "ole32.dll",
        kind = "raw-dylib",
        modifiers = "+verbatim",
        import_name_type = "undecorated"
    )
)]
#[cfg_attr(
    not(target_arch = "x86"),
    link(name = "ole32.dll", kind = "raw-dylib", modifiers = "+verbatim")
)]
unsafe extern "system" {
    #[link_name = "CoTaskMemFree"]
    fn free_com_task_mem(ptr: *const std::ffi::c_void);
}

struct Apartment;

impl Drop for Apartment {
    fn drop(&mut self) {
        // SAFETY: created only after successful initialization on this thread,
        // and dropped after all of this dialog's COM interfaces.
        unsafe { CoUninitialize() };
    }
}

struct ShellPath(PWSTR);

impl Drop for ShellPath {
    fn drop(&mut self) {
        // SAFETY: GetDisplayName allocated this pointer with the COM allocator.
        unsafe { free_com_task_mem(self.0.0.cast()) };
    }
}

pub(super) fn pick_folder(title: &str) -> Result<Option<PathBuf>, DialogError> {
    // SAFETY: the caller owns this thread; a changed apartment mode is returned
    // as an error. Both S_OK and S_FALSE require a matching CoUninitialize.
    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) }
        .ok()
        .map_err(DialogError::new)?;
    let _apartment = Apartment;
    let title: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();

    // SAFETY: COM is initialized, the dialog is used only on this thread, and
    // the title buffer remains valid throughout SetTitle. All returned COM
    // interfaces are owned wrappers and are released before the apartment.
    unsafe {
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)
            .map_err(DialogError::new)?;
        let options = dialog.GetOptions().map_err(DialogError::new)?;
        dialog
            .SetOptions(options | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST)
            .map_err(DialogError::new)?;
        dialog
            .SetTitle(PCWSTR(title.as_ptr()))
            .map_err(DialogError::new)?;
        if let Err(error) = dialog.Show(None) {
            return if error.code() == HRESULT::from_win32(ERROR_CANCELLED.0) {
                Ok(None)
            } else {
                Err(DialogError::new(error))
            };
        }
        let item = dialog.GetResult().map_err(DialogError::new)?;
        let path = ShellPath(
            item.GetDisplayName(SIGDN_FILESYSPATH)
                .map_err(DialogError::new)?,
        );
        if path.0.is_null() {
            return Err(DialogError::new(
                "Folder dialog returned no filesystem path",
            ));
        }
        // GetDisplayName supplies a NUL-terminated UTF-16 buffer. OsString
        // preserves unpaired surrogates, unlike a lossy conversion to String.
        Ok(Some(PathBuf::from(OsString::from_wide(path.0.as_wide()))))
    }
}
