//! Thread-local variables that may be accessed across dynamic library boundaries.

pub use abi_stable as macro_support;
use abi_stable::std_types::{RBox, RStr};

/// A key into thread-local storage.
pub struct LocalKey<T: 'static> {
    #[doc(hidden)]
    pub id: RStr<'static>,
    #[doc(hidden)]
    pub init: extern "C" fn() -> RBox<()>,
    #[doc(hidden)]
    pub __phantom: std::marker::PhantomData<extern "C" fn() -> T>,
}

impl<T: 'static> LocalKey<T> {
    #[doc(hidden)]
    pub fn new(id: RStr<'static>, init: extern "C" fn() -> RBox<()>) -> Self {
        LocalKey {
            id,
            init,
            __phantom: std::marker::PhantomData,
        }
    }
}

/// Create one or more thread-local values.
///
/// This macro has identical syntax to `std::thread_local!`.
#[macro_export]
macro_rules! thread_local {
    () => {};
    ($(#[$attr:meta])* $vis:vis static $name:ident: $t:ty = $init:expr; $($rest:tt)*) => {
        $crate::__thread_local_inner!($(#[$attr])* $vis $name: $t = $init);
        $crate::thread_local!($($rest)*);
    };
    ($(#[$attr:meta])* $vis:vis static $name:ident: $t:ty = $init:expr) => {
        $crate::__thread_local_inner!($(#[$attr])* $vis $name: $t = $init);
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __thread_local_inner {
    ($(#[$attr:meta])* $vis:vis $name:ident: $t:ty = $init:expr) => {
        $(#[$attr])*
        $vis static $name: $crate::LocalKey<$t> = {
            use $crate::macro_support::{pointer_trait::TransmuteElement, std_types::{RBox, RStr}};

            extern "C" fn __init() -> RBox<()> {
                let __val: $t = $init;
                unsafe { RBox::new(__val).transmute_element() }
            }

            $crate::LocalKey {
                // Order key id by the likely most specific to least specific identifiers for
                // faster map comparison.
                id: RStr::from_str(std::concat!(std::stringify!($name), std::stringify!($t), std::module_path!())),
                init: __init,
                __phantom: std::marker::PhantomData
            }
        };
    }
}

#[cfg(feature = "host")]
mod host {
    use abi_stable::std_types::{RBox, RStr};
    use parking_lot::RwLock;
    use std::collections::BTreeMap as Map;

    std::thread_local! {
        static PLUGIN_TLS: RwLock<Map<RStr<'static>, RBox<()>>> = parking_lot::const_rwlock(Default::default());
    }

    pub unsafe extern "C" fn tls(
        id: &RStr<'static>,
        init: extern "C" fn() -> RBox<()>,
    ) -> *const () {
        PLUGIN_TLS.with(|m| {
            let guard = m.upgradable_read();
            let guard = if !guard.contains_key(id) {
                let mut guard = parking_lot::RwLockUpgradableReadGuard::upgrade(guard);
                // Check again in case it was added while we waited for an upgrade
                if !guard.contains_key(id) {
                    guard.insert(id.clone(), init());
                }
                parking_lot::RwLockWriteGuard::downgrade(guard)
            } else {
                parking_lot::RwLockUpgradableReadGuard::downgrade(guard)
            };
            // We leak the reference from PLUGIN_TLS as well as the reference out of the RwLock
            // guard, however this will be safe because:
            // 1. the reference will be used shortly within the thread's runtime (not sending to
            //    another thread) due to the `with` implementation, and
            // 2. the RwLock guard is protecting access/changes to the map, however we _only_ ever
            //    add to the map if a key does not exist (so this box won't disappear on us).
            guard.get(id).unwrap().as_ref() as *const ()
        })
    }
}

type TlsFunction = unsafe extern "C" fn(&RStr<'static>, extern "C" fn() -> RBox<()>) -> *const ();

/// The context to be installed in plugins.
#[repr(transparent)]
pub struct Context(TlsFunction);

impl Context {
    /// Get the context.
    ///
    /// Separate instances of `Context` will always be identical.
    #[cfg(feature = "host")]
    pub fn get() -> Self {
        Context(host::tls)
    }

    /// Initialize the thread local storage.
    ///
    /// # Safety
    /// This must be called only once in each binary, and prior to any thread-local values managed by
    /// this library being accessed within the binary. Otherwise UB may occur.
    pub unsafe fn initialize_tls(self) {
        HOST_TLS = Some(self.0);
    }
}

static mut HOST_TLS: Option<TlsFunction> = None;

impl<T: 'static> LocalKey<T> {
    /// Acquires a reference to the value in this TLS key.
    ///
    /// If neither `host` nor `plugin` features are enabled, this will panic.
    #[cfg(any(feature = "host", feature = "plugin"))]
    pub fn with<F, R>(&'static self, f: F) -> R
    where
        F: FnOnce(&T) -> R,
    {
        let host_tls =
            unsafe { HOST_TLS.as_ref() }.expect("host thread local storage improperly initialized");
        f(unsafe {
            (host_tls(&self.id, self.init) as *const T)
                .as_ref()
                .unwrap()
        })
    }

    /// Acquires a reference to the value in this TLS key.
    ///
    /// If neither `host` nor `plugin` features are enabled, this will panic.
    #[cfg(not(any(feature = "host", feature = "plugin")))]
    pub fn with<F, R>(&'static self, _f: F) -> R
    where
        F: FnOnce(&T) -> R,
    {
        panic!("plugin_tls built without 'host' or 'plugin' enabled")
    }
}
