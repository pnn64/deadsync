use super::RawKeyboardEvent;
use std::ffi::c_void;
use std::mem::{MaybeUninit, size_of};
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::time::Instant;

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    MAPVK_VK_TO_VSC_EX, MapVirtualKeyW, VK_NUMLOCK, VK_SHIFT,
};
use windows::Win32::UI::Input::{
    GetRawInputData, HRAWINPUT, RAWINPUT, RAWINPUTDEVICE, RAWINPUTHEADER, RAWKEYBOARD, RID_INPUT,
    RIDEV_INPUTSINK, RIDEV_NOLEGACY, RegisterRawInputDevices,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CREATESTRUCTW, CreateWindowExW, DefWindowProcW, DispatchMessageW, GWLP_USERDATA, GetMessageW,
    GetWindowLongPtrW, HWND_MESSAGE, MSG, PostQuitMessage, RI_KEY_E0, RI_KEY_E1, RegisterClassExW,
    SetWindowLongPtrW, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CREATE, WM_DESTROY,
    WM_INPUT, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP, WNDCLASSEXW,
};
use windows::core::PCWSTR;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::platform::scancode::PhysicalKeyExtScancode;

const USAGE_PAGE_GENERIC: u16 = 0x01;
const USAGE_GENERIC_KEYBOARD: u16 = 0x06;
const RAW_KEY_HELD_SLOTS: usize = 3 * 256;
const WIN_SC_SHIFT: u16 = 0x0010;
const WIN_SC_NUMLOCK: u16 = 0x0045;
const WIN_SC_IGNORE_PAUSE_PREFIX: u16 = 0xe11d;
const WIN_SC_IGNORE_PRTSC_PREFIX: u16 = 0xe02a;

static WINDOW_FOCUSED: AtomicBool = AtomicBool::new(true);
static CAPTURE_ENABLED: AtomicBool = AtomicBool::new(false);
static RAW_KEYBOARD_HWND: AtomicIsize = AtomicIsize::new(0);

struct Ctx {
    emit_key: Box<dyn FnMut(RawKeyboardEvent) + Send>,
    held: [bool; RAW_KEY_HELD_SLOTS],
}

#[inline(always)]
pub fn set_window_focused(focused: bool) {
    WINDOW_FOCUSED.store(focused, Ordering::Relaxed);
}

#[inline(always)]
pub fn set_capture_enabled(enabled: bool) {
    CAPTURE_ENABLED.store(enabled, Ordering::Relaxed);
    let hwnd = HWND(RAW_KEYBOARD_HWND.load(Ordering::Acquire) as *mut c_void);
    if !hwnd.0.is_null() {
        unsafe { register_keyboard(hwnd, enabled) };
    }
}

#[inline(always)]
unsafe fn register_keyboard(hwnd: HWND, capture_enabled: bool) {
    let mut flags = RIDEV_INPUTSINK;
    if capture_enabled {
        flags |= RIDEV_NOLEGACY;
    }
    let devices = [RAWINPUTDEVICE {
        usUsagePage: USAGE_PAGE_GENERIC,
        usUsage: USAGE_GENERIC_KEYBOARD,
        dwFlags: flags,
        hwndTarget: hwnd,
    }];
    let _ = unsafe { RegisterRawInputDevices(&devices, size_of::<RAWINPUTDEVICE>() as u32) };
}

#[inline(always)]
const fn scancode_slot(scancode: u16) -> usize {
    let page = match scancode & 0xff00 {
        0xe000 => 1,
        0xe100 => 2,
        _ => 0,
    };
    page * 256 + (scancode as usize & 0x00ff)
}

#[inline(always)]
fn keyboard_scancode(keyboard: RAWKEYBOARD) -> u16 {
    let flags = u32::from(keyboard.Flags);
    let extension = if flags & RI_KEY_E0 != 0 {
        0xe000
    } else if flags & RI_KEY_E1 != 0 {
        0xe100
    } else {
        0x0000
    };
    if keyboard.MakeCode == 0 {
        unsafe { MapVirtualKeyW(u32::from(keyboard.VKey), MAPVK_VK_TO_VSC_EX) as u16 }
    } else {
        keyboard.MakeCode | extension
    }
}

#[inline(always)]
fn raw_keyboard_code(keyboard: RAWKEYBOARD) -> Option<(KeyCode, usize)> {
    let scancode = keyboard_scancode(keyboard);
    if matches!(
        scancode,
        WIN_SC_IGNORE_PAUSE_PREFIX | WIN_SC_IGNORE_PRTSC_PREFIX
    ) {
        return None;
    }

    let physical = if keyboard.VKey == VK_NUMLOCK.0 {
        PhysicalKey::Code(KeyCode::NumLock)
    } else {
        PhysicalKey::from_scancode(u32::from(scancode))
    };

    if keyboard.VKey == VK_SHIFT.0
        && matches!(
            physical,
            PhysicalKey::Code(
                KeyCode::NumpadDecimal
                    | KeyCode::Numpad0
                    | KeyCode::Numpad1
                    | KeyCode::Numpad2
                    | KeyCode::Numpad3
                    | KeyCode::Numpad4
                    | KeyCode::Numpad5
                    | KeyCode::Numpad6
                    | KeyCode::Numpad7
                    | KeyCode::Numpad8
                    | KeyCode::Numpad9
            )
        )
    {
        return None;
    }

    let PhysicalKey::Code(code) = physical else {
        return None;
    };
    let slot = scancode_slot(if keyboard.VKey == VK_NUMLOCK.0 {
        WIN_SC_NUMLOCK
    } else if keyboard.VKey == VK_SHIFT.0 && scancode == 0 {
        WIN_SC_SHIFT
    } else {
        scancode
    });
    Some((code, slot))
}

#[inline(always)]
fn handle_wm_input(ctx: &mut Ctx, hraw: HRAWINPUT) {
    if !WINDOW_FOCUSED.load(Ordering::Relaxed) {
        return;
    }

    unsafe {
        let mut data = MaybeUninit::<RAWINPUT>::zeroed();
        let mut size = size_of::<RAWINPUT>() as u32;
        let rc = GetRawInputData(
            hraw,
            RID_INPUT,
            Some(data.as_mut_ptr().cast::<c_void>()),
            &raw mut size,
            size_of::<RAWINPUTHEADER>() as u32,
        );
        if rc == u32::MAX || size < size_of::<RAWINPUTHEADER>() as u32 {
            return;
        }

        let data = data.assume_init();
        if data.header.dwType != windows::Win32::UI::Input::RIM_TYPEKEYBOARD.0 {
            return;
        }
        let keyboard = data.data.keyboard;
        let pressed = matches!(keyboard.Message, WM_KEYDOWN | WM_SYSKEYDOWN);
        let released = matches!(keyboard.Message, WM_KEYUP | WM_SYSKEYUP);
        if !pressed && !released {
            return;
        }

        let Some((code, slot)) = raw_keyboard_code(keyboard) else {
            return;
        };
        if ctx.held[slot] == pressed {
            return;
        }
        ctx.held[slot] = pressed;
        (ctx.emit_key)(RawKeyboardEvent {
            code,
            pressed,
            timestamp: Instant::now(),
        });
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let ctx_ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut Ctx;
    match msg {
        WM_CREATE => {
            let cs = lparam.0 as *const CREATESTRUCTW;
            if !cs.is_null() {
                let p = unsafe { (*cs).lpCreateParams }.cast::<Ctx>();
                unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, p as isize) };
            }
            LRESULT(0)
        }
        WM_INPUT => {
            if !ctx_ptr.is_null() {
                unsafe { handle_wm_input(&mut *ctx_ptr, HRAWINPUT(lparam.0 as *mut c_void)) };
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

pub fn run(emit_key: impl FnMut(RawKeyboardEvent) + Send + 'static) {
    unsafe {
        let class_name: Vec<u16> = "deadsync_raw_keyboard\0".encode_utf16().collect();
        let hinst: HINSTANCE = GetModuleHandleW(PCWSTR::null()).unwrap_or_default().into();

        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: hinst,
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        RegisterClassExW(&raw const wc);

        let mut ctx = Box::new(Ctx {
            emit_key: Box::new(emit_key),
            held: [false; RAW_KEY_HELD_SLOTS],
        });

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            wc.lpszClassName,
            PCWSTR::null(),
            WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(hinst),
            Some(ptr::addr_of_mut!(*ctx).cast::<c_void>().cast_const()),
        )
        .unwrap_or_default();
        if hwnd.0.is_null() {
            loop {
                std::thread::park();
            }
        }

        RAW_KEYBOARD_HWND.store(hwnd.0 as isize, Ordering::Release);
        register_keyboard(hwnd, CAPTURE_ENABLED.load(Ordering::Relaxed));

        let mut msg = MSG::default();
        loop {
            let ok = GetMessageW(&raw mut msg, None, 0, 0);
            if ok.0 <= 0 {
                break;
            }
            let _ = TranslateMessage(&raw const msg);
            DispatchMessageW(&raw const msg);
        }

        RAW_KEYBOARD_HWND.store(0, Ordering::Release);
        std::mem::forget(ctx);
    }
}
