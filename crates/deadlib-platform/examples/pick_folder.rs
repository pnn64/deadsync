//! Manual native-dialog smoke test, including the macOS main event loop.
//! Run with `cargo run -p deadlib-platform --example pick_folder`.

use deadlib_platform::folder_dialog::{DialogError, pick_folder};
use std::path::PathBuf;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
    window::WindowId,
};

type Selection = Result<Option<PathBuf>, DialogError>;

struct Picker {
    proxy: EventLoopProxy<Selection>,
    started: bool,
}

impl ApplicationHandler<Selection> for Picker {
    fn resumed(&mut self, _: &ActiveEventLoop) {
        if std::mem::replace(&mut self.started, true) {
            return;
        }
        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            let _ = proxy.send_event(pick_folder("Choose a folder — deadlib-platform"));
        });
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, selection: Selection) {
        match selection {
            Ok(Some(path)) => println!("Selected: {}", path.display()),
            Ok(None) => println!("Canceled"),
            Err(error) => eprintln!("Folder dialog failed: {error}"),
        }
        event_loop.exit();
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

fn main() -> Result<(), winit::error::EventLoopError> {
    let event_loop = EventLoop::with_user_event().build()?;
    let proxy = event_loop.create_proxy();
    event_loop.run_app(&mut Picker {
        proxy,
        started: false,
    })
}
