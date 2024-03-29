# plugin_tls

Thread-local and static-duration storage support across dynamic-library
boundaries.

Normally, a `std::thread::LocalKey` refers to a specific memory location in a
single binary. This library adds support for thread-local storage across
binaries by storing thread-local values in a shared map between a host and one
or more plugins.

The library also adds support for static-duration (but necessarily
lazily-initialized) storage across binary boundaries, since such an
implementation is very similar to that of thread-local storage.

## Use

Use the `host` feature to enable the host capabilities (in exactly one binary to
be loaded in memory), and use the `plugin` feature to enable plugin capabilities
(in zero or more binaries to be loaded).

In each plugin binary, call `Context::initialize` with the host context prior to
any thread-local or static storage being accessed (typically as part of the
binary startup routine). The host binary will default to the correct state,
however this may be overwritten with `initialize` as well.

Besides that, the `thread_local!` macro works exactly like `std::thread_local!`
for both the host and plugins, and `lazy_static!` works exactly like the
`lazy_static` crate. Note that thread-local and static values are indexed by the
given name, type, and module path, so these _must not_ conflict. Also note that
stored values _must_ be abi-stable. Abi-stability is not enforced in `LocalKey`
(e.g. with the `abi_stable::StableAbi` trait) for library flexibility, but use
of the `abi_stable` crate is highly encouraged.

Note that both `host` and `plugin` may be enabled together, specifically for use
in workspaces that may contain both the host and plugin(s).

## Safety
If unloading plugins, one must ensure that any memory set by plugins in
thread-local or static storage is removed (including any initial values!). In
the future there could be additions to track which initial values are created by
a particular plugin, but for now one solution is for the host to call
`Context::reset` to clear all thread-local and static values.
