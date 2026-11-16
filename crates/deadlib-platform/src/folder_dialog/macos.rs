use super::DialogError;
use objc2::rc::autoreleasepool;
use objc2_app_kit::{NSModalResponseCancel, NSModalResponseOK, NSOpenPanel};
use objc2_foundation::NSString;
use std::{
    ffi::{CStr, OsStr},
    os::unix::ffi::OsStrExt,
    path::PathBuf,
};

pub(super) fn pick_folder(title: &str) -> Result<Option<PathBuf>, DialogError> {
    dispatch2::run_on_main(|mtm| {
        autoreleasepool(|_| {
            // SAFETY: dispatch2 supplies the main-thread marker and every AppKit
            // object is created, used, and released inside this main-thread closure.
            unsafe {
                let panel = NSOpenPanel::openPanel(mtm);
                panel.setTitle(Some(&NSString::from_str(title)));
                panel.setCanChooseFiles(false);
                panel.setCanChooseDirectories(true);
                panel.setAllowsMultipleSelection(false);
                panel.setCanCreateDirectories(true);
                match panel.runModal() {
                    response if response == NSModalResponseOK => {
                        let url = panel
                            .URL()
                            .ok_or_else(|| DialogError::new("Folder dialog returned no URL"))?;
                        if !url.isFileURL() {
                            return Err(DialogError::new("Folder dialog returned a non-file URL"));
                        }
                        let path = CStr::from_ptr(url.fileSystemRepresentation().as_ptr());
                        Ok(Some(PathBuf::from(OsStr::from_bytes(path.to_bytes()))))
                    }
                    response if response == NSModalResponseCancel => Ok(None),
                    response => Err(DialogError::new(format!(
                        "Folder dialog failed (response {response})"
                    ))),
                }
            }
        })
    })
}
