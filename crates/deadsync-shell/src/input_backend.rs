use crate::UserEvent;
use deadsync_input_native::WindowsPadBackend;
use winit::event_loop::EventLoopProxy;

/// Startup settings for the platform and StepManiaX input backends.
pub struct InputBackendConfig {
    pub windows_pad_backend: WindowsPadBackend,
    pub smx_input: bool,
    pub smx_p1_serial: Option<String>,
    pub smx_p2_serial: Option<String>,
}

#[inline(always)]
fn native_input_host() -> deadsync_input_native::BackendHost {
    deadsync_input_native::backend_host(
        deadsync_config::pad_order::pad_index_for_uuid_saved,
        |vendor, product| {
            deadsync_smx::native_smx_owns_device(
                vendor,
                product,
                deadsync_config::runtime::get().smx_input,
            )
        },
    )
}

/// Launch input workers and connect their events to the application loop.
pub fn launch_input_backends(proxy: EventLoopProxy<UserEvent>, config: InputBackendConfig) {
    #[cfg(windows)]
    {
        let windows_pad_backend = config.windows_pad_backend;
        let proxy_pad = proxy.clone();
        let proxy_sys = proxy.clone();
        let proxy_key = proxy.clone();
        let input_host = native_input_host();
        std::thread::spawn(move || {
            deadsync_input_native::run_windows_backend(
                windows_pad_backend,
                move |event| {
                    let _ = proxy_pad.send_event(UserEvent::Pad(event));
                },
                move |event| {
                    let _ = proxy_sys.send_event(UserEvent::GamepadSystem(event));
                },
                move |event| {
                    let _ = proxy_key.send_event(UserEvent::Key(event));
                },
                input_host,
            );
        });
    }
    #[cfg(target_os = "linux")]
    {
        let proxy_pad = proxy.clone();
        let proxy_sys = proxy.clone();
        let proxy_key = proxy.clone();
        let input_host = native_input_host();
        std::thread::spawn(move || {
            deadsync_input_native::run_linux_backend(
                move |event| {
                    let _ = proxy_pad.send_event(UserEvent::Pad(event));
                },
                move |event| {
                    let _ = proxy_sys.send_event(UserEvent::GamepadSystem(event));
                },
                move |event| {
                    let _ = proxy_key.send_event(UserEvent::Key(event));
                },
                input_host,
            );
        });
    }
    #[cfg(target_os = "freebsd")]
    {
        let proxy_pad = proxy.clone();
        let proxy_sys = proxy.clone();
        let proxy_key = proxy.clone();
        let input_host = native_input_host();
        std::thread::spawn(move || {
            deadsync_input_native::run_freebsd_backend(
                move |event| {
                    let _ = proxy_pad.send_event(UserEvent::Pad(event));
                },
                move |event| {
                    let _ = proxy_sys.send_event(UserEvent::GamepadSystem(event));
                },
                move |event| {
                    let _ = proxy_key.send_event(UserEvent::Key(event));
                },
                input_host,
            );
        });
    }
    #[cfg(target_os = "macos")]
    {
        let proxy_pad = proxy.clone();
        let proxy_sys = proxy.clone();
        let proxy_key = proxy.clone();
        let input_host = native_input_host();
        std::thread::spawn(move || {
            deadsync_input_native::run_macos_backend(
                move |event| {
                    let _ = proxy_pad.send_event(UserEvent::Pad(event));
                },
                move |event| {
                    let _ = proxy_sys.send_event(UserEvent::GamepadSystem(event));
                },
                move |event| {
                    let _ = proxy_key.send_event(UserEvent::Key(event));
                },
                input_host,
            );
        });
    }

    if config.smx_input
        && deadsync_smx::init(deadsync_smx::InitConfig {
            p1_serial: config.smx_p1_serial,
            p2_serial: config.smx_p2_serial,
        })
    {
        let proxy_pad = proxy.clone();
        deadsync_smx::add_input_listener(Box::new(move |event| {
            let _ = proxy_pad.send_event(UserEvent::Pad(event));
        }));
        deadsync_smx::add_sys_listener(Box::new(move |event| {
            let _ = proxy.send_event(UserEvent::GamepadSystem(event));
        }));
    }
}
