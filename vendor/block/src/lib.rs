//! Rust interface for Objective-C blocks.
//!
//! This is the `block` 0.1.6 API with explicit C ABIs and an inhabited opaque
//! FFI type for compatibility with current Rust compilers.

use std::marker::PhantomData;
use std::mem;
use std::ops::{Deref, DerefMut};
use std::os::raw::{c_int, c_ulong, c_void};
use std::ptr;

#[repr(C)]
struct Class {
    _private: [u8; 0],
}

#[cfg_attr(
    any(target_os = "macos", target_os = "ios"),
    link(name = "System", kind = "dylib")
)]
#[cfg_attr(
    not(any(target_os = "macos", target_os = "ios")),
    link(name = "BlocksRuntime", kind = "dylib")
)]
extern "C" {
    static _NSConcreteStackBlock: Class;

    fn _Block_copy(block: *const c_void) -> *mut c_void;
    fn _Block_release(block: *const c_void);
}

/// Types that may be used as the arguments to an Objective-C block.
pub trait BlockArguments: Sized {
    /// Invoke `block` with these arguments.
    ///
    /// # Safety
    ///
    /// `block` must point to a valid Objective-C block with a matching ABI.
    unsafe fn call_block<R>(self, block: *mut Block<Self, R>) -> R;
}

macro_rules! block_args_impl {
    ($($arg:ident : $ty:ident),*) => {
        impl<$($ty),*> BlockArguments for ($($ty,)*) {
            unsafe fn call_block<R>(self, block: *mut Block<Self, R>) -> R {
                let base = block as *mut BlockBase<Self, R>;
                // SAFETY: The caller guarantees `block` points to a block with
                // this argument tuple and return type.
                let invoke: unsafe extern "C" fn(*mut Block<Self, R> $(, $ty)*) -> R = unsafe {
                    mem::transmute((*base).invoke)
                };
                let ($($arg,)*) = self;
                // SAFETY: `invoke` was recovered from this block's descriptor
                // with the matching typed ABI above.
                unsafe { invoke(block $(, $arg)*) }
            }
        }
    };
}

block_args_impl!();
block_args_impl!(a: A);
block_args_impl!(a: A, b: B);
block_args_impl!(a: A, b: B, c: C);
block_args_impl!(a: A, b: B, c: C, d: D);
block_args_impl!(a: A, b: B, c: C, d: D, e: E);
block_args_impl!(a: A, b: B, c: C, d: D, e: E, f: F);
block_args_impl!(a: A, b: B, c: C, d: D, e: E, f: F, g: G);
block_args_impl!(a: A, b: B, c: C, d: D, e: E, f: F, g: G, h: H);
block_args_impl!(a: A, b: B, c: C, d: D, e: E, f: F, g: G, h: H, i: I);
block_args_impl!(a: A, b: B, c: C, d: D, e: E, f: F, g: G, h: H, i: I, j: J);
block_args_impl!(a: A, b: B, c: C, d: D, e: E, f: F, g: G, h: H, i: I, j: J, k: K);
block_args_impl!(a: A, b: B, c: C, d: D, e: E, f: F, g: G, h: H, i: I, j: J, k: K, l: L);

#[repr(C)]
struct BlockBase<A, R> {
    isa: *const Class,
    flags: c_int,
    _reserved: c_int,
    invoke: unsafe extern "C" fn(*mut Block<A, R>, ...) -> R,
}

/// An Objective-C block that takes arguments `A` and returns `R`.
#[repr(C)]
pub struct Block<A, R> {
    _base: PhantomData<BlockBase<A, R>>,
}

impl<A: BlockArguments, R> Block<A, R> {
    /// Invoke the block.
    ///
    /// # Safety
    ///
    /// The caller must uphold the foreign block's aliasing and thread-safety
    /// requirements while it executes.
    pub unsafe fn call(&self, args: A) -> R {
        // SAFETY: The caller upholds the requirements documented above, and
        // this pointer is derived from a live `Block` reference.
        unsafe { args.call_block(self as *const _ as *mut _) }
    }
}

/// A reference-counted Objective-C block.
pub struct RcBlock<A, R> {
    ptr: *mut Block<A, R>,
}

impl<A, R> RcBlock<A, R> {
    /// Take ownership of a block with a +1 retain count.
    ///
    /// # Safety
    ///
    /// `ptr` must be valid and carry a +1 retain count.
    pub unsafe fn new(ptr: *mut Block<A, R>) -> Self {
        Self { ptr }
    }

    /// Copy a valid block into reference-counted storage.
    ///
    /// # Safety
    ///
    /// `ptr` must point to a valid block.
    pub unsafe fn copy(ptr: *mut Block<A, R>) -> Self {
        // SAFETY: The caller guarantees `ptr` is a valid Objective-C block.
        let ptr = unsafe { _Block_copy(ptr as *const c_void) } as *mut Block<A, R>;
        Self { ptr }
    }
}

impl<A, R> Clone for RcBlock<A, R> {
    fn clone(&self) -> Self {
        // SAFETY: `self.ptr` remains valid while `self` owns a retain count.
        unsafe { Self::copy(self.ptr) }
    }
}

impl<A, R> Deref for RcBlock<A, R> {
    type Target = Block<A, R>;

    fn deref(&self) -> &Self::Target {
        // SAFETY: `self.ptr` is valid for the lifetime of this retained block.
        unsafe { &*self.ptr }
    }
}

impl<A, R> Drop for RcBlock<A, R> {
    fn drop(&mut self) {
        // SAFETY: This instance owns one retain count for `self.ptr`.
        unsafe { _Block_release(self.ptr as *const c_void) }
    }
}

/// A value that can be converted into a concrete Objective-C block.
pub trait IntoConcreteBlock<A>: Sized
where
    A: BlockArguments,
{
    /// Return type of the resulting block.
    type Ret;

    /// Convert this value into a concrete block.
    fn into_concrete_block(self) -> ConcreteBlock<A, Self::Ret, Self>;
}

macro_rules! concrete_block_impl {
    ($function:ident) => {
        concrete_block_impl!($function,);
    };
    ($function:ident, $($arg:ident : $ty:ident),*) => {
        impl<$($ty,)* R, X> IntoConcreteBlock<($($ty,)*)> for X
        where
            X: Fn($($ty,)*) -> R,
        {
            type Ret = R;

            fn into_concrete_block(self) -> ConcreteBlock<($($ty,)*), R, X> {
                unsafe extern "C" fn $function<$($ty,)* R, X>(
                    block_ptr: *mut ConcreteBlock<($($ty,)*), R, X>
                    $(, $arg: $ty)*
                ) -> R
                where
                    X: Fn($($ty,)*) -> R,
                {
                    // SAFETY: The Objective-C runtime passes the block pointer
                    // whose invoke field contains this function.
                    let block = unsafe { &*block_ptr };
                    (block.closure)($($arg),*)
                }

                let function: unsafe extern "C" fn(
                    *mut ConcreteBlock<($($ty,)*), R, X>
                    $(, $ty)*
                ) -> R = $function;
                // SAFETY: `function` has exactly the tuple-expanded ABI that
                // `ConcreteBlock` records for these arguments.
                unsafe { ConcreteBlock::with_invoke(mem::transmute(function), self) }
            }
        }
    };
}

concrete_block_impl!(concrete_block_invoke_args0);
concrete_block_impl!(concrete_block_invoke_args1, a: A);
concrete_block_impl!(concrete_block_invoke_args2, a: A, b: B);
concrete_block_impl!(concrete_block_invoke_args3, a: A, b: B, c: C);
concrete_block_impl!(concrete_block_invoke_args4, a: A, b: B, c: C, d: D);
concrete_block_impl!(concrete_block_invoke_args5, a: A, b: B, c: C, d: D, e: E);
concrete_block_impl!(concrete_block_invoke_args6, a: A, b: B, c: C, d: D, e: E, f: F);
concrete_block_impl!(concrete_block_invoke_args7, a: A, b: B, c: C, d: D, e: E, f: F, g: G);
concrete_block_impl!(concrete_block_invoke_args8, a: A, b: B, c: C, d: D, e: E, f: F, g: G, h: H);
concrete_block_impl!(concrete_block_invoke_args9, a: A, b: B, c: C, d: D, e: E, f: F, g: G, h: H, i: I);
concrete_block_impl!(concrete_block_invoke_args10, a: A, b: B, c: C, d: D, e: E, f: F, g: G, h: H, i: I, j: J);
concrete_block_impl!(concrete_block_invoke_args11, a: A, b: B, c: C, d: D, e: E, f: F, g: G, h: H, i: I, j: J, k: K);
concrete_block_impl!(concrete_block_invoke_args12, a: A, b: B, c: C, d: D, e: E, f: F, g: G, h: H, i: I, j: J, k: K, l: L);

/// An Objective-C block whose storage size is known at compile time.
#[repr(C)]
pub struct ConcreteBlock<A, R, F> {
    base: BlockBase<A, R>,
    descriptor: Box<BlockDescriptor<ConcreteBlock<A, R, F>>>,
    closure: F,
}

impl<A, R, F> ConcreteBlock<A, R, F>
where
    A: BlockArguments,
    F: IntoConcreteBlock<A, Ret = R>,
{
    /// Create a concrete block from a closure.
    pub fn new(closure: F) -> Self {
        closure.into_concrete_block()
    }
}

impl<A, R, F> ConcreteBlock<A, R, F> {
    unsafe fn with_invoke(invoke: unsafe extern "C" fn(*mut Self, ...) -> R, closure: F) -> Self {
        Self {
            base: BlockBase {
                // SAFETY: The linked Blocks runtime provides this process-wide
                // class object and the block stores only its address.
                isa: unsafe { &_NSConcreteStackBlock },
                flags: 1 << 25,
                _reserved: 0,
                // SAFETY: The caller guarantees the invoke ABI matches `Self`.
                invoke: unsafe { mem::transmute(invoke) },
            },
            descriptor: Box::new(BlockDescriptor::new()),
            closure,
        }
    }
}

impl<A, R, F: 'static> ConcreteBlock<A, R, F> {
    /// Copy this stack block into runtime-managed heap storage.
    pub fn copy(self) -> RcBlock<A, R> {
        let mut block = self;
        // SAFETY: `block` is a valid concrete block for the duration of this
        // call; `_Block_copy` invokes its copy helper before returning.
        let copied = unsafe { RcBlock::copy(&mut *block) };
        mem::forget(block);
        copied
    }
}

impl<A, R, F: Clone> Clone for ConcreteBlock<A, R, F> {
    fn clone(&self) -> Self {
        // SAFETY: The cloned block retains the same valid invoke ABI.
        unsafe { Self::with_invoke(mem::transmute(self.base.invoke), self.closure.clone()) }
    }
}

impl<A, R, F> Deref for ConcreteBlock<A, R, F> {
    type Target = Block<A, R>;

    fn deref(&self) -> &Self::Target {
        // SAFETY: `BlockBase` is the first field and both types are `repr(C)`.
        unsafe { &*(&self.base as *const _ as *const Block<A, R>) }
    }
}

impl<A, R, F> DerefMut for ConcreteBlock<A, R, F> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: `BlockBase` is the first field and both types are `repr(C)`.
        unsafe { &mut *(&mut self.base as *mut _ as *mut Block<A, R>) }
    }
}

unsafe extern "C" fn block_context_dispose<B>(block: &mut B) {
    // SAFETY: The Blocks runtime invokes this exactly once for initialized
    // block context storage that it is disposing.
    unsafe { ptr::read(block) };
}

unsafe extern "C" fn block_context_copy<B>(_dst: &mut B, _src: &B) {
    // The Blocks runtime already bitwise-copied the context into `dst`.
}

#[repr(C)]
struct BlockDescriptor<B> {
    _reserved: c_ulong,
    block_size: c_ulong,
    copy_helper: unsafe extern "C" fn(&mut B, &B),
    dispose_helper: unsafe extern "C" fn(&mut B),
}

impl<B> BlockDescriptor<B> {
    fn new() -> Self {
        Self {
            _reserved: 0,
            block_size: mem::size_of::<B>() as c_ulong,
            copy_helper: block_context_copy::<B>,
            dispose_helper: block_context_dispose::<B>,
        }
    }
}
