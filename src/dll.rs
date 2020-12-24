// MIT/Apache2 License

use dashmap::DashMap;
use dlopen::raw::Library;
use std::{
    ffi::{c_void, CStr, OsStr},
    fmt, mem,
    ptr::NonNull,
};

/// A pointer to a dynamically loaded library.
#[derive(Debug)]
pub struct Dll {
    lib: Library,
    // TODO: probably not an efficient way of doing this. cache locality, et cetera
    funcs: DashMap<Box<CStr>, NonNull<c_void>>,
}

unsafe impl Send for Dll {}
unsafe impl Sync for Dll {}

impl Dll {
    /// Load a new Dll.
    #[inline]
    pub fn load<A: AsRef<OsStr>>(dbg_libname: &'static str, paths: &[A]) -> breadx::Result<Self> {
        let lib = match paths.iter().find_map(|path| Library::open(path).ok()) {
            Some(lib) => lib,
            None => return Err(breadx::BreadError::StaticMsg(dbg_libname)),
        };

        Ok(Self {
            lib,
            funcs: DashMap::new(),
        })
    }

    /// Load a function.
    pub unsafe fn function<T>(&self, name: &CStr) -> Option<T> {
        if mem::size_of::<T>() != mem::size_of::<NonNull<c_void>>() {
            panic!("Object is not the size of a pointer");
        }

        match self.funcs.get(name) {
            Some(func) => Some(mem::transmute_copy::<_, T>(&*func)),
            None => {
                // load symbol from library
                let sym: NonNull<c_void> = unsafe { self.lib.symbol_cstr(name) }.ok()?;
                self.funcs.insert(name.into(), sym.clone());
                Some(mem::transmute_copy::<_, T>(&sym))
            }
        }
    }
}
