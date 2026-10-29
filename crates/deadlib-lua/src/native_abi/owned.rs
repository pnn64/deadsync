//! C-owned default states, with Rust borrows confined to each API operation.
use super::*;
use std::{cell::UnsafeCell, sync::atomic::AtomicBool};

struct OwnedVm {
    vm: UnsafeCell<crate::Vm>,
    bridge: Arc<Bridge>,
    main: usize,
    entered: AtomicBool,
}

pub(super) unsafe extern "C" fn new_state(anchor: *const c_void) -> *mut c_void {
    let result = std::panic::catch_unwind(|| -> Result<_, Box<[u8]>> {
        let image = Arc::new(Library::from_address(anchor)?);
        if !matching_version(image.try_symbol(b"boundary_abi_version")?) {
            return Err(b"Lua C API image version differs".as_slice().into());
        }
        // SAFETY: the anchor's live image is the same versioned C shim that
        // supplied this factory invocation. Retain it throughout VM ownership.
        let bridge = unsafe { Bridge::from_image(image) }?;
        let mut vm = crate::Vm::new_bare();
        vm.set_native_library_loader(Some(Arc::new(NativeCAbiLoader {
            bridge: std::sync::OnceLock::from(Ok(bridge.clone())),
        })));
        let mut owner = Box::new(OwnedVm {
            vm: UnsafeCell::new(vm),
            bridge,
            main: 0,
            entered: AtomicBool::new(false),
        });
        owner.main = owner
            .vm
            .get_mut()
            .with_native_stack(|stack| state_for(stack, &owner.bridge, stack.main_thread_id()))
            .map_err(|error| Box::<[u8]>::from(error.message_bytes()))?;
        let state = owner.main as *mut c_void;
        let pointer = (&mut *owner as *mut OwnedVm).cast();
        // SAFETY: publish the stable Box address only after initialization.
        // The C group borrows it until lua_close consumes that single owner.
        let attached = unsafe { (owner.bridge.own_state)(state, pointer, operate, close) };
        if attached != 1 {
            return Err(b"cannot attach owned native state".as_slice().into());
        }
        let _ = Box::into_raw(owner);
        Ok(state)
    });
    match result {
        Ok(Ok(state)) => state,
        Ok(Err(_)) => std::ptr::null_mut(),
        // Rust panic recovery cannot manufacture a valid C VM. No unwind may
        // cross the C factory boundary; ordinary Lua errors use operate below.
        Err(_) => std::process::abort(),
    }
}

unsafe extern "C" fn operate(
    pointer: *mut c_void,
    op: c_int,
    a: c_int,
    b: c_int,
    value: *mut BoundaryValue,
) -> c_int {
    // SAFETY: C retains this owned group and serializes its API calls. The
    // atomic guard rejects aliasing/reentrant root entries before borrowing Vm.
    let owner = unsafe { &*pointer.cast::<OwnedVm>() };
    if owner.entered.swap(true, Ordering::Acquire) {
        std::process::abort();
    }
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: this operation exclusively owns the VM until entered clears.
        // Nested native callbacks use their existing borrowed Context instead.
        unsafe { &mut *owner.vm.get() }.with_native_stack(|stack| {
            let mut context = Context {
                stack,
                error: None,
                yielded: None,
                bridge: owner.bridge.clone(),
                owner: None,
                state: owner.main,
            };
            // SAFETY: C lends the matching BoundaryValue for this operation;
            // dispatch and every Rust local return before C can jump or panic.
            let result = unsafe {
                dispatch(
                    (&mut context as *mut Context<'_, '_, '_>).cast(),
                    op,
                    a,
                    b,
                    value,
                )
            };
            if let Some(error) = context.error.take() {
                let error = if error.value().is_some() {
                    error
                } else {
                    let value = context.stack.intern(error.message_bytes());
                    error.with_value(value)
                };
                return Err(error);
            }
            Ok(result)
        })
    }));
    let status = match outcome {
        Ok(Ok(status)) => status,
        // The root entry has unwound and retained its Lua error value. C can
        // now invoke lua_atpanic or exit without skipping any Rust frames.
        Ok(Err(error)) => {
            // SAFETY: entered still excludes every other root borrow. The C
            // panic callback runs only after this status has been published.
            unsafe { &mut *owner.vm.get() }.native_root_panic(&error);
            2
        }
        Err(_) => std::process::abort(),
    };
    owner.entered.store(false, Ordering::Release);
    status
}

unsafe extern "C" fn close(pointer: *mut c_void) {
    // SAFETY: lua_close consumes the C group's sole Box owner, and C already
    // rejected an active native frame from this group. Other groups may run.
    let owner = unsafe { &*pointer.cast::<OwnedVm>() };
    if owner.entered.swap(true, Ordering::Acquire) {
        std::process::abort();
    }
    // SAFETY: the guard now excludes every VM borrow before reconstructing
    // unique ownership and resetting the main stack for close-time finalizers.
    let mut owner = unsafe { Box::from_raw(pointer.cast::<OwnedVm>()) };
    owner.vm.get_mut().prepare_native_close();
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(owner))).is_err() {
        std::process::abort();
    }
}
