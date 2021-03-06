//! Thread-local variables that may be accessed across dynamic library boundaries.

pub use abi_stable as macro_support;
use abi_stable::std_types::{RBox, RStr};

/// A key into thread-local storage.
pub struct LocalKey<T: 'static> {
    #[doc(hidden)]
    pub read: unsafe extern "C" fn() -> *const T,
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

            extern "C" fn __thread_local_init() -> RBox<()> {
                let __thread_local_val: $t = $init;
                unsafe { RBox::new(__thread_local_val).transmute_element() }
            }

            std::thread_local! {
                static VALUE: &'static $t = $crate::__get::<$t>(
                    &RStr::from_str(std::concat!(std::stringify!($name), std::stringify!($t), std::module_path!())),
                    __thread_local_init,
                );
            }

            unsafe extern "C" fn __thread_local_read() -> *const $t {
                VALUE.with(|v| *v as *const $t)
            }

            $crate::LocalKey { read: __thread_local_read }
        };
    }
}

#[cfg(feature = "host")]
mod host {
    use abi_stable::std_types::{RBox, RStr};
    use std::cell::RefCell;
    use std::collections::BTreeMap as Map;

    std::thread_local! {
        static PLUGIN_TLS: RefCell<Map<RStr<'static>, RBox<()>>> = RefCell::new(Default::default());
    }

    pub unsafe extern "C" fn tls(
        id: &RStr<'static>,
        init: extern "C" fn() -> RBox<()>,
    ) -> *const () {
        PLUGIN_TLS.with(|m| {
            let mut m = m.borrow_mut();
            if !m.contains_key(id) {
                m.insert(id.clone(), init());
            };
            // We leak the reference from PLUGIN_TLS as well as the reference out of the RefCell,
            // however this will be safe because:
            // 1. the reference will be used shortly within the thread's runtime (not sending to
            //    another thread) due to the `with` implementation, and
            // 2. the RefCell guard is protecting access/changes to the map, however we _only_ ever
            //    add to the map if a key does not exist (so this box won't disappear on us).
            m.get(id).unwrap().as_ref() as *const ()
        })
    }

    pub fn reset() {
        PLUGIN_TLS.with(|m| m.borrow_mut().clear())
    }
}

type TlsFunction = unsafe extern "C" fn(&RStr<'static>, extern "C" fn() -> RBox<()>) -> *const ();

/// The context to be installed in plugins.
#[repr(transparent)]
pub struct Context(TlsFunction);

impl Context {
    /// Initialize the thread local storage.
    ///
    /// # Safety
    /// This must be called only once in each plugin, and prior to any thread-local values managed by
    /// this library being accessed within the plugin. Otherwise UB may occur.
    pub unsafe fn initialize_tls(self) {
        HOST_TLS = Some(self.0);
    }
}

#[cfg(feature = "host")]
impl Context {
    /// Get the context.
    ///
    /// Separate instances of `Context` will always be identical.
    pub fn get() -> Self {
        Context(host::tls)
    }

    /// Reset the thread-local storage for the current thread.
    ///
    /// This destructs all values and returns the state to a point as if no values have yet been
    /// accessed on the current thread.
    pub fn reset() {
        host::reset();
    }
}

#[cfg(feature = "host")]
static mut HOST_TLS: Option<TlsFunction> = Some(host::tls);

#[cfg(not(feature = "host"))]
static mut HOST_TLS: Option<TlsFunction> = None;

#[doc(hidden)]
pub fn __get<T>(id: &RStr<'static>, init: extern "C" fn() -> RBox<()>) -> &'static T {
    let host_tls =
        unsafe { HOST_TLS.as_ref() }.expect("host thread local storage improperly initialized");
    unsafe { (host_tls(id, init) as *const T).as_ref().unwrap() }
}

impl<T: 'static> LocalKey<T> {
    /// Acquires a reference to the value in this TLS key.
    ///
    /// If neither `host` nor `plugin` features are enabled, this will panic.
    #[cfg(any(feature = "host", feature = "plugin"))]
    pub fn with<F, R>(&'static self, f: F) -> R
    where
        F: FnOnce(&T) -> R,
    {
        f(unsafe { (self.read)().as_ref().unwrap() })
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
