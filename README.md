# breadglx

`breadglx` is an extension to [`breadx`](https://crates.io/crates/breadx) designed to tie together breadx's GLX API with the hardware support offered by the Mesa3D libraries found on most Unix systems.

We currently have an example that "works" in the most technical sense. However, it's held together by duct tape and silly string, and it only works on systems that support DRI3 (it panics otherwise).

## Why is this not just a part of breadx?

* It's a large library on its own that incurs a handful of new dependencies.
* It unavoidably requires a handful of unsafe code.
* Some of its design principles are diametrically opposed to breadx's. For instance, by the necessity
  of having to work across the FFI barrier, `breadglx` uses mutexes and shared memory.
* It is not cross-platform as `breadx` is: it is only supported on Unix operating systems.

## License

MIT/Apache2 License, just like Rust and breadx is.
