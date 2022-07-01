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
                static VALUE: &'static $t = $crate::__get_tls::<$t>(
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

/// Create a lazily-initialized static value.
///
/// This macro has identical syntax to `lazy_static::lazy_static!`.
#[macro_export]
macro_rules! lazy_static {
    () => {};
    ($(#[$attr:meta])* static ref $N:ident : $T:ty = $e:expr; $($t:tt)*) => {
        $crate::__lazy_static_inner!($(#[$attr])* () static ref $N : $T = $e; $($t)*);
    };
    ($(#[$attr:meta])* pub static ref $N:ident : $T:ty = $e:expr; $($t:tt)*) => {
        $crate::__lazy_static_inner!($(#[$attr])* (pub) static ref $N : $T = $e; $($t)*);
    };
    ($(#[$attr:meta])* pub ($($vis:tt)+) static ref $N:ident : $T:ty = $e:expr; $($t:tt)*) => {
        $crate::__lazy_static_inner!($(#[$attr])* (pub ($($vis)+)) static ref $N : $T = $e; $($t)*);
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __lazy_static_inner {
    ($(#[$attr:meta])* ($($vis:tt)*) static ref $N:ident : $T:ty = $e:expr; $($t:tt)*) => {
        #[allow(missing_copy_implementations)]
        #[allow(non_camel_case_types)]
        #[allow(dead_code)]
        $(#[$attr])*
        $($vis)* struct $N {__private_field: ()}
        #[doc(hidden)]
        $($vis)* static $N: $N = $N {__private_field: ()};
        impl std::ops::Deref for $N {
            type Target = $T;
            fn deref(&self) -> &$T {
                use $crate::macro_support::{pointer_trait::TransmuteElement, std_types::{RBox, RStr}};
                use std::sync::Once;
                use std::mem::MaybeUninit;

                extern "C" fn __initialize() -> RBox<()> {
                    let __val: $T = $e;
                    unsafe { RBox::new(__val).transmute_element() }
                }

                static VALUE_ONCE: Once = Once::new();
                static mut VALUE: MaybeUninit<&'static $T> = MaybeUninit::uninit();
                VALUE_ONCE.call_once(|| {
                    unsafe {
                        VALUE.write($crate::__get_static(
                            &RStr::from_str(std::concat!(std::stringify!($N), std::stringify!($T), std::module_path!())),
                            __initialize
                        ));
                    }
                });

                unsafe { VALUE.assume_init_read() }
            }
        }
        $crate::lazy_static!($($t)*);
    };
}

#[cfg(feature = "host")]
mod host {
    use abi_stable::std_types::{RBox, RStr};
    use std::cell::RefCell;
    use std::collections::BTreeMap as Map;
    use std::mem::MaybeUninit;
    use std::sync::{Mutex, Once};

    std::thread_local! {
        static PLUGIN_TLS: RefCell<Map<RStr<'static>, RBox<()>>> = RefCell::new(Default::default());
    }

    static PLUGIN_STATIC_ONCE: Once = Once::new();
    static mut PLUGIN_STATIC: MaybeUninit<Mutex<Map<RStr<'static>, RBox<()>>>> =
        MaybeUninit::uninit();

    fn static_map() -> std::sync::MutexGuard<'static, Map<RStr<'static>, RBox<()>>> {
        PLUGIN_STATIC_ONCE.call_once(|| unsafe {
            PLUGIN_STATIC.write(Default::default());
        });
        unsafe { PLUGIN_STATIC.assume_init_mut() }.lock().unwrap()
    }

    pub unsafe extern "C" fn tls(
        id: &RStr<'static>,
        init: extern "C" fn() -> RBox<()>,
    ) -> *const () {
        PLUGIN_TLS.with(|m| {
            let mut m = m.borrow_mut();
            if !m.contains_key(id) {
                m.insert(id.clone(), init());
            }
            // We leak the reference from PLUGIN_TLS as well as the reference out of the RefCell,
            // however this will be safe because:
            // 1. the reference will be used shortly within the thread's runtime (not sending to
            //    another thread) due to the `with` implementation, and
            // 2. the RefCell guard is protecting access/changes to the map, however we _only_ ever
            //    add to the map if a key does not exist (so this box won't disappear on us).
            m.get(id).unwrap().as_ref() as *const ()
        })
    }

    pub unsafe extern "C" fn statics(
        id: &RStr<'static>,
        init: extern "C" fn() -> RBox<()>,
    ) -> *const () {
        let mut m = static_map();
        if !m.contains_key(id) {
            m.insert(id.clone(), init());
        }
        // We leak the reference from PLUGIN_STATIC as well as the reference out of the Mutex,
        // however this will be safe because:
        // 1. the reference will have static lifetime once initially created, and
        // 2. the Mutex guard is protecting access/changes to the map, however we _only_ ever
        //    add to the map if a key does not exist (so this box won't disappear on us).
        m.get(id).unwrap().as_ref() as *const ()
    }

    pub fn reset() {
        PLUGIN_TLS.with(|m| m.borrow_mut().clear());
        static_map().clear();
    }
}

type AccessFunction =
    unsafe extern "C" fn(&RStr<'static>, extern "C" fn() -> RBox<()>) -> *const ();

/// The context to be installed in plugins.
#[repr(C)]
pub struct Context {
    tls: AccessFunction,
    statics: AccessFunction,
}

impl Context {
    /// Initialize the thread local storage and static storage.
    ///
    /// # Safety
    /// This must be called only once in each plugin, and prior to the plugin accessing any
    /// thread-local values or static values managed by this library. Otherwise UB may occur.
    pub unsafe fn initialize(self) {
        HOST_TLS = Some(self.tls);
        HOST_STATICS = Some(self.statics);
    }
}

#[cfg(feature = "host")]
impl Context {
    /// Get the context.
    ///
    /// Separate instances of `Context` will always be identical.
    pub fn get() -> Self {
        Context {
            tls: host::tls,
            statics: host::statics,
        }
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
static mut HOST_TLS: Option<AccessFunction> = Some(host::tls);

#[cfg(not(feature = "host"))]
static mut HOST_TLS: Option<AccessFunction> = None;

#[cfg(feature = "host")]
static mut HOST_STATICS: Option<AccessFunction> = Some(host::statics);

#[cfg(not(feature = "host"))]
static mut HOST_STATICS: Option<AccessFunction> = None;

#[doc(hidden)]
pub fn __get_tls<T>(id: &RStr<'static>, init: extern "C" fn() -> RBox<()>) -> &'static T {
    let host_tls =
        unsafe { HOST_TLS.as_ref() }.expect("host thread local storage improperly initialized");
    unsafe { (host_tls(id, init) as *const T).as_ref().unwrap() }
}

#[doc(hidden)]
pub fn __get_static<T>(id: &RStr<'static>, init: extern "C" fn() -> RBox<()>) -> &'static T {
    let host_statics =
        unsafe { HOST_STATICS.as_ref() }.expect("host static storage improperly initialized");
    unsafe { (host_statics(id, init) as *const T).as_ref().unwrap() }
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
