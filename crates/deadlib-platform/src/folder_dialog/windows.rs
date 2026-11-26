use super::DialogError;
use crate::windows_shell_path::{ComTaskAllocator, ShellPath};
use std::path::PathBuf;
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
    core::{HRESULT, PCWSTR},
};

struct Apartment;

impl Drop for Apartment {
    fn drop(&mut self) {
        // SAFETY: created only after successful initialization on this thread,
        // and dropped after all of this dialog's COM interfaces.
        unsafe { CoUninitialize() };
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
        let allocator = ComTaskAllocator::new().map_err(DialogError::new)?;
        // GetDisplayName transfers a NUL-terminated COM-allocated path.
        let path = ShellPath::from_raw(
            item.GetDisplayName(SIGDN_FILESYSPATH)
                .map_err(DialogError::new)?,
            &allocator,
        );
        path.into_path_buf()
            .map(Some)
            .ok_or_else(|| DialogError::new("Folder dialog returned no filesystem path"))
    }
}
