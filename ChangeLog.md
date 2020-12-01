# plugin_tls ChangeLog

## 0.2.0  -- 2020-12-01
* Change `Context::new()` to `Context::get()`.

## 0.1.1  -- 2020-12-01
* Allow type inference in initializer of `thread_local!`.
* Add `with` implementation that panics if neither `host` nor `plugin` features
  are enabled.
* Allow both `host` and `plugin` to be enabled together.

## 0.1.0  -- 2020-12-01
* Initial release.
