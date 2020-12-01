# plugin_tls

Thread-local storage support across dynamic-library boundaries.

Normally, a `std::thread::LocalKey` refers to a specific memory location in a
single binary. This library adds support for thread-local storage across
binaries by storing thread-local values in a shared map between a host and one
or more plugins.

## Use

Use the `host` feature to enable the host capabilities (in exactly one binary to
be loaded in memory), and use the `plugin` feature to enable plugin capabilities
(in zero or more binaries to be loaded).

In each plugin binary, call `plugin_tls::initialize` with the host context prior
to any thread-local storage being accessed (typically as part of the plugin
loading routine).

Besides that, the `thread_local!` macro works exactly like `std::thread_local!`
for both the host and plugins. Note that thead-local values are indexed by the
given name, type, and module path, so these _must not_ conflict. Also note that
stored values _must_ be abi-stable. Abi-stability is not enforced in `LocalKey`
with the `abi_stable::StableAbi` trait for library flexibility, but use of
`abi_stable` is highly encouraged.
