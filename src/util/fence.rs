// MIT/Apache2 License

use super::CallOnDrop;
use crate::{
    dll::Dll,
    cstr::{const_cstr, ConstCstr},
    mesa,
};
use breadx::{
    auto::sync::Fence as SyncFence,
    display::{AsyncConnection, Connection},
};
use std::{ffi::c_void, os::raw::c_int, ptr::NonNull};

#[cfg(feature = "async")]
use super::OffloadOnDrop;

const XSHMFENCE_ALLOC_SHM: ConstCstr<'static> = const_cstr(&*b"xshmfence_alloc_shm\0");
type XshmfenceAllocShm = unsafe extern "C" fn() -> c_int;
const XSHMFENCE_MAP_SHM: ConstCstr<'static> = const_cstr(&*b"xshmfence_map_shm\0");
type XshmfenceMapShm = unsafe extern "C" fn(c_int) -> *mut c_void;
const XSHMFENCE_UNMAP_SHM: ConstCstr<'static> = const_cstr(&*b"xshmfence_unmap_shm\0");
type XshmfenceUnmapShm = unsafe extern "C" fn(*mut c_void);

/// An SHM fence, which can be converted into an xsync fence.
#[derive(Debug)]
pub(crate) struct Fence {
    // the pointer to the xshm in-memory fence
    shm_fence: NonNull<c_void>,
    // the file descriptor associated with the
    // shm fence. if we've already created the sync
    // fence, this is -1
    fd: c_int,
    // the sync fence's XID
    sync_fence: SyncFence,
}

// Since it contains a raw pointer, we have to tell that we never do thread-unsafe
// things with it.
unsafe impl Send for Fence {}
unsafe impl Sync for Fence {}

impl Fence {
    #[inline]
    fn new_internal(xshmfence: &Dll) -> breadx::Result<Self> {
        // get the symbols that we need for the fence creation
        let alloc_shm: XshmfenceAllocShm = unsafe { xshmfence.symbol(&XSHMFENCE_ALLOC_SHM) }
            .expect("xshmfence_alloc_shm not present");
        let map_shm: XshmfenceMapShm =
            unsafe { xshmfence.symbol(&XSHMFENCE_MAP_SHM) }.expect("xshmfence_map_shm not present");

        // create the fence fd
        let fence_fd = unsafe { (alloc_shm)() };
        if fence_fd < 0 {
            return Err(breadx::BreadError::StaticMsg(
                "Unable to allocate an xshmfence",
            ));
        }

        let guard = CallOnDrop::new(|| unsafe { libc::close(fence_fd) });

        // create the shm fence
        let shm_fence = unsafe { (map_shm)(fence_fd) };
        let shm_fence = match NonNull::new(shm_fence) {
            Some(shm_fence) => shm_fence,
            None => return Err(breadx::BreadError::StaticMsg("Unable to map an xshmfence")),
        };

        mem::forget(guard);

        Ok(Self {
            shm_fence,
            fd: fence_fd,
            sync_fence: SyncFence::const_from_xid(0),
        })
    }
    
    /// Create a new memory fence.
    #[inline]
    pub fn new() -> breadx::Result<Self> {
        let xshmfence = mesa::xshmfence()?;
        Self::new_internal(xshmfence)
    }

    /// Create a new memory fence, async redox.
    #[cfg(feature = "async")]
    #[inline]
    pub async fn new_async() -> breadx::Result<Self> {
let xshmfence = mesa::xshmfence_async().await?;

// the rest of this calls FFI stuff, so we can safely put it in a blocking call
blocking::unblock(move || {
    Self::new_internal(xshmfence)
}).await
    }

    /// The fence should be freed without dropping it.
    #[inline]
    pub fn free(self) -> breadx::Result<Self> {

    }
}

impl Drop for Fence {
    #[inline]
    fn drop(&mut self) {
        log::error!("Dropped fence before free!");
    }
}
