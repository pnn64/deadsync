// Adapted from unty 0.0.4 under MIT OR Apache-2.0. See PROVENANCE.md.

use core::{any::TypeId, marker::PhantomData, mem};

/// Returns whether two generic types are equal, ignoring lifetimes.
///
/// Bincode only compares `T` with lifetime-free numeric primitives, so the
/// lifetime limitation of `non_static_type_id` cannot produce a false positive.
pub(crate) fn type_equal<Src: ?Sized, Target: ?Sized>() -> bool {
    non_static_type_id::<Src>() == non_static_type_id::<Target>()
}

// Code by dtolnay in bincode issue 665. This erases only the lifetime on the
// trait object used to call `TypeId::of`; it does not change the type `T`.
fn non_static_type_id<T: ?Sized>() -> TypeId {
    trait NonStaticAny {
        fn get_type_id(&self) -> TypeId
        where
            Self: 'static;
    }

    impl<T: ?Sized> NonStaticAny for PhantomData<T> {
        fn get_type_id(&self) -> TypeId
        where
            Self: 'static,
        {
            TypeId::of::<T>()
        }
    }

    let phantom_data = PhantomData::<T>;
    // SAFETY: The temporary trait object is used only to dispatch
    // `get_type_id`. The method does not access borrowed data, and bincode only
    // compares a generic `T` with lifetime-free numeric primitives.
    NonStaticAny::get_type_id(unsafe {
        mem::transmute::<&dyn NonStaticAny, &(dyn NonStaticAny + 'static)>(&phantom_data)
    })
}

#[cfg(test)]
mod tests {
    use super::type_equal;

    #[test]
    fn compares_types() {
        assert!(type_equal::<u8, u8>());
        assert!(!type_equal::<u8, u16>());
    }
}
