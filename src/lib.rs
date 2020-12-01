//! Thread-local variables that may be accessed across dynamic library boundaries.

pub use abi_stable as macro_support;
use abi_stable::std_types::{RBox, RStr};

#[cfg(any(feature = "host", feature = "plugin"))]
use abi_stable::pointer_trait::TransmuteElement;

#[cfg(all(feature = "host", feature = "plugin"))]
compile_error!("only one of the 'host' or 'plugin' features may be enabled");

pub struct LocalKey<T: 'static> {
    #[doc(hidden)]
    pub id: RStr<'static>,
    #[doc(hidden)]
    pub init: extern "C" fn() -> RBox<()>,
    __phantom: std::marker::PhantomData<extern "C" fn() -> T>,
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

            extern "C" fn __init() -> RBox<()> { unsafe { RBox::new($init).transmute_element() } }

            $crate::LocalKey::new(
                // Order key id by the likely most specific to least specific identifiers for
                // faster map comparison.
                RStr::from_str(std::concat!(std::stringify!($name), std::stringify!($t), std::module_path!())),
                __init
            )
        };
    }
}

#[cfg(feature = "host")]
mod host {
    use abi_stable::std_types::{RBox, RStr};
    use parking_lot::RwLock;
    use std::collections::BTreeMap as Map;

    std::thread_local! {
        #[cfg(feature = "host")]
        static PLUGIN_TLS: RwLock<Map<RStr<'static>, RBox<()>>> = parking_lot::const_rwlock(Default::default());
    }

    pub unsafe extern "C" fn tls(
        id: &RStr<'static>,
        init: extern "C" fn() -> RBox<()>,
        f: unsafe extern "C" fn(RBox<()>, *const ()) -> RBox<()>,
        data: RBox<()>,
    ) -> RBox<()> {
        PLUGIN_TLS.with(|m| {
            let guard = m.upgradable_read();
            let guard = if !guard.contains_key(id) {
                let mut guard = parking_lot::RwLockUpgradableReadGuard::upgrade(guard);
                guard.insert(id.clone(), init());
                parking_lot::RwLockWriteGuard::downgrade(guard)
            } else {
                parking_lot::RwLockUpgradableReadGuard::downgrade(guard)
            };
            f(data, guard.get(id).unwrap().as_ref() as *const ())
        })
    }
}

type TlsFunction = unsafe extern "C" fn(
    &RStr<'static>,
    extern "C" fn() -> RBox<()>,
    unsafe extern "C" fn(RBox<()>, *const ()) -> RBox<()>,
    RBox<()>,
) -> RBox<()>;

/// The context to be installed in plugins.
#[repr(transparent)]
pub struct Context(TlsFunction);

#[cfg(feature = "host")]
impl Context {
    /// Create a new Context.
    pub fn new() -> &'static Self {
        unsafe { std::mem::transmute(&host::tls) }
    }
}

#[cfg(feature = "host")]
static mut HOST_TLS: Option<TlsFunction> = Some(host::tls);

#[cfg(feature = "plugin")]
static mut HOST_TLS: Option<TlsFunction> = None;

/// Install the context into the plugin.
///
/// # Safety
/// This must be called only once, and prior to any thread-local values being accessed within the
/// plugin. Otherwise UB may occur.
#[cfg(feature = "plugin")]
pub unsafe fn initialize(ctx: &'static Context) {
    HOST_TLS = Some(ctx.0);
}

#[cfg(any(feature = "host", feature = "plugin"))]
unsafe extern "C" fn call<T, R, F: FnOnce(&T) -> R>(f: RBox<()>, data: *const ()) -> RBox<()> {
    RBox::new(RBox::into_inner(f.transmute_element::<F>())(
        (data as *const T).as_ref().unwrap(),
    ))
    .transmute_element()
}

impl<T: 'static> LocalKey<T> {
    #[cfg(any(feature = "host", feature = "plugin"))]
    pub fn with<F, R>(&'static self, f: F) -> R
    where
        F: FnOnce(&T) -> R,
    {
        let host_tls =
            unsafe { HOST_TLS.as_ref() }.expect("host thread local storage improperly initialized");
        RBox::into_inner(unsafe {
            host_tls(
                &self.id,
                self.init,
                call::<T, R, F>,
                RBox::new(f).transmute_element(),
            )
            .transmute_element()
        })
    }
}
