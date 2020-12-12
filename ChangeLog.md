# plugin_tls ChangeLog

## 0.2.3  -- 2020-12-11
* Use RefCell instead of RwLock (unnecessary).

## 0.2.2  -- 2020-12-03
* Automatically set the initial tls function for hosts.

## 0.2.1  -- 2020-12-01
* Add `Context::reset()` function to reset thread-local variables for the
  current thread.

## 0.2.0  -- 2020-12-01
* Change `Context::new()` to `Context::get()`.
* Fix soundness bug in `initialize`.
* Move `initialize` to be a method of `Context` and rename as `initialize_tls`
  to clarify intent.

## 0.1.1  -- 2020-12-01
* Allow type inference in initializer of `thread_local!`.
* Add `with` implementation that panics if neither `host` nor `plugin` features
  are enabled.
* Allow both `host` and `plugin` to be enabled together.

## 0.1.0  -- 2020-12-01
* Initial release.
