# CLONED FROM https://github.com/dekellum/hyperx
The original crate is outdated. This clone just has updated dependencies so we can use it again.
This clone only really differs from the original in that various irrelevant files have been removed and the dependencies have been updated.

# hyper*x*

[![Rustdoc](https://docs.rs/hyperx/badge.svg)](https://docs.rs/hyperx)

[Hyper] is the low-level HTTP implementation for Rust. Hyper*x* is an
e*x*traction of the hyper 0.11 typed header module, with minimized
dependencies, for continued use with hyper 0.12 or later,
where it was dropped.

[Hyper]: https://github.com/hyperium/hyper

## Minimum supported rust version

MSRV := 1.46.0

The crate will fail fast on any lower rustc (via a build.rs version check) and
is also CI tested on this version. MSRV will only be increased in a new MINOR
(or MAJOR) release of this crate. However, some direct or transitive
dependencies unfortunately have or may increase MSRV in PATCH releases. Users
may need to selectively control updates by preserving/distributing a Cargo.lock
file in order to control MSRV.

## License

The MIT license ([LICENSE](LICENSE) or http://opensource.org/licenses/MIT)
