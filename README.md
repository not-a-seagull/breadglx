# breadglx

`breadglx` is an extension to [`breadx`](https://crates.io/crates/breadx) designed to tie together breadx's GLX API with the hardware support offered by the Mesa3D libraries found on most Unix systems.

This is nowhere near ready yet.

## Why is this not just a part of breadx?

* It's a large library on its own that incurs a handful of new dependencies.
* It unavoidably requires a handful of unsafe code.
* It is not cross-platform as `breadx` is: it is only supported on Unix operating systems.

## License

MIT/Apache2 License, just like Rust and breadx is.
