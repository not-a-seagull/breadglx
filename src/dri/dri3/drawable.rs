// MIT/Apache2 License

// TODO: this file is a literal trainwreck. seperate it into several files if possible, comment the
//       whole thing, and maybe clean it all up

use super::{Dri3Context, Dri3Screen, WeakDri3ScreenRef};
use crate::{
    config::GlConfig,
    context::{promote_anyarc_ref, ContextDispatch, GlContext, GlInternalContext},
    cstr::{const_cstr, ConstCstr},
    display::{DisplayLike, DisplayLock, GlDisplay},
    dri::ffi,
    mesa::xshmfence,
    util::{CallOnDrop, ThreadSafe},
};
use breadx::{
    auto::{
        dri3::{BufferFromPixmapReply, BuffersFromPixmapReply},
        present::EventMask as PresentEventMask,
        sync::Fence,
    },
    display::{Connection, Display, Modifiers},
    BreadError::StaticMsg,
    Drawable, Event, GcParameters, Gcontext, Pixmap, PropMode, PropertyFormat, PropertyType,
    Window,
};
use std::{
    cell::Cell,
    cmp,
    ffi::c_void,
    future::Future,
    hint, iter,
    mem::{self, MaybeUninit},
    num::NonZeroU64,
    ops::{Deref, DerefMut},
    os::raw::{c_int, c_uchar, c_uint},
    pin::Pin,
    ptr::{self, NonNull},
    sync::{
        self,
        atomic::{AtomicBool, AtomicI32, AtomicU16, AtomicU32, AtomicU64, AtomicU8, Ordering},
        Arc,
    },
    task::{Context, Poll, Waker},
};
use tinyvec::ArrayVec;

#[cfg(feature = "async")]
use crate::{offload::offload, util::GenericFuture};
#[cfg(feature = "async")]
use async_lock::MutexGuard;
#[cfg(feature = "async")]
use breadx::display::AsyncConnection;
#[cfg(feature = "async")]
use futures_lite::future;

#[cfg(not(feature = "async"))]
use once_cell::sync::Lazy;
#[cfg(not(feature = "async"))]
use std::sync::MutexGuard;

#[derive(Debug)]
pub struct Dri3Drawable<Dpy> {
    drawable: NonNull<ffi::__DRIdrawable>,
    x_drawable: Drawable,
    config: GlConfig,
    is_different_gpu: bool,
    multiplanes_available: bool,
    // TODO: this should be a weak screen reference, otherwise we have a self
    //       sustaining loop
    screen: WeakDri3ScreenRef<Dpy>,
    context: Dri3Context<Dpy>,

    width: AtomicU16,
    height: AtomicU16,
    depth: AtomicU8,

    present_capabilities: AtomicU32,
    eid: AtomicU32,
    is_initialized: AtomicBool,
    window: AtomicU32,
    gc: AtomicU32,
    has_fake_front: AtomicBool,
    has_back: AtomicBool,

    swap_interval: AtomicI32,
    swap_method: c_int,
    is_pixmap: AtomicBool,

    // waiter for the drawable
    has_event_waiter: AtomicBool,
    #[cfg(not(feature = "async"))]
    event_waiter: sync::Condvar,
    #[cfg(feature = "async")]
    event_waiter: event_listener::Event,

    #[cfg(not(feature = "async"))]
    state: sync::Mutex<DrawableState>,
    #[cfg(feature = "async")]
    state: async_lock::Mutex<DrawableState>,

    display: GlDisplay<Dpy>, // cloned reference to display
}

#[derive(Debug, Default)]
pub struct DrawableState {
    send_sbc: u64,
    recv_sbc: u64,
    notify_msc: i64,
    notify_ust: i64,
    msc: i64,
    ust: i64,
    sbc: i64,
    last_present_mode: u8,
    cur_back: usize,
    cur_num_back: usize,
    max_num_back: usize,
    cur_blit_source: i32,
    have_fake_front: bool,
    buffers: [Option<Arc<Dri3Buffer>>; NUM_BUFFERS],
}

#[derive(Debug)]
pub struct Dri3Buffer {
    pub image: NonNull<ffi::__DRIimage>,
    linear_buffer: Option<NonNull<ffi::__DRIimage>>,

    sync_fence: Fence,
    shm_fence: NonNull<c_void>,

    cpp: u32,
    modifier: u64,
    width: u16,
    height: u16,

    // we need to reallocate
    reallocate: bool,

    busy: i32,
    pixmap: Pixmap,
    own_pixmap: bool,
    last_swap: AtomicU64,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum BufferType {
    Front,
    Back,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
struct SwapBufferCount {
    ust: i64,
    msc: i64,
    sbc: i64,
}

/// Simple wrapper around a MutexGuard<DrawableState> that logs a message on drop.
#[derive(Debug)]
#[repr(transparent)]
pub struct StateGuard<'a> {
    #[cfg(not(feature = "async"))]
    inner: Option<sync::MutexGuard<'a, DrawableState>>,
    #[cfg(Feature = "async")]
    inner: Option<async_lock::MutexGuard<'a, DrawableState>>,
}

impl<'a> StateGuard<'a> {
    #[cfg(not(feature = "async"))]
    #[inline]
    fn into_inner(mut self) -> sync::MutexGuard<'a, DrawableState> {
        log::trace!("Dropping lock for condvar");
        let inner = self.inner.take().unwrap();
        mem::forget(self);
        inner
    }
}

impl<'a> Deref for StateGuard<'a> {
    type Target = DrawableState;

    #[inline]
    fn deref(&self) -> &DrawableState {
        &*self.inner.as_ref().unwrap()
    }
}

impl<'a> DerefMut for StateGuard<'a> {
    #[inline]
    fn deref_mut(&mut self) -> &mut DrawableState {
        &mut *self.inner.as_mut().unwrap()
    }
}

impl<'a> Drop for StateGuard<'a> {
    #[inline]
    fn drop(&mut self) {
        log::trace!("Dropped state guard");
    }
}

unsafe impl<Dpy: Send> Send for Dri3Drawable<Dpy> {}
unsafe impl<Dpy: Sync> Sync for Dri3Drawable<Dpy> {}
unsafe impl Send for DrawableState {}
unsafe impl Sync for DrawableState {}
unsafe impl Send for Dri3Buffer {}
unsafe impl Sync for Dri3Buffer {}

const VBLANK_NEVER: c_int = 0;
const VBLANK_DEF_INTERVAL0: c_int = 1;
const VBLANK_DEF_INTERVAL1: c_int = 2;
const VBLANK_DEF_ALWAYS_SYNC: c_int = 3;

const PRESENT_MODE_COPY: u8 = 0;
const PRESENT_MODE_FLIP: u8 = 1;
const PRESENT_MODE_SKIP: u8 = 2;
const PRESENT_MODE_SUBOPTIMAL_COPY: u8 = 3;

const VBLANK_MODE: ConstCstr<'static> = const_cstr(&*b"vblank_mode\0");
const ADAPTIVE_SYNC: ConstCstr<'static> = const_cstr(&*b"adaptive_sync\0");

const DRM_FORMAT_RESERVED: u64 = (1u64 << 56) - 1;
const DRM_CORRUPTED_MODIFIER: u64 = DRM_FORMAT_RESERVED & 0x00ffffffffffffffu64;

const STATE_LOCK_FAILED: &str = "Unable to acquire state lock";

pub const MAX_BACK: usize = 4;
pub const FRONT_ID: usize = MAX_BACK;
pub const NUM_BUFFERS: usize = MAX_BACK + 1;
#[inline]
pub const fn back_id(i: usize) -> usize {
    i
}

#[derive(Copy, Clone)]
#[repr(transparent)]
struct CtxPtr(Option<NonNull<ffi::__DRIcontext>>);
unsafe impl Send for CtxPtr {}
unsafe impl Sync for CtxPtr {}

#[derive(Copy, Clone)]
#[repr(transparent)]
struct ImgPtr(NonNull<ffi::__DRIimage>);
unsafe impl Send for ImgPtr {}
unsafe impl Sync for ImgPtr {}

impl From<NonNull<ffi::__DRIimage>> for ImgPtr {
    #[inline]
    fn from(n: NonNull<ffi::__DRIimage>) -> Self {
        Self(n)
    }
}

// Context used to blit images if no other context is available.
#[cfg(not(feature = "async"))]
static BLIT_CONTEXT: Lazy<sync::Mutex<Option<BlitContext>>> = Lazy::new(|| sync::Mutex::new(None));
#[cfg(feature = "async")]
static BLIT_CONTEXT: async_lock::Mutex<Option<BlitContext>> = async_lock::Mutex::new(None);

/// Type for the blit context.
struct BlitContext {
    screen: NonNull<ffi::__DRIscreen>,
    core: *const ffi::__DRIcoreExtension,
    context: NonNull<ffi::__DRIcontext>,
}

unsafe impl Send for BlitContext {}
unsafe impl Sync for BlitContext {}

impl BlitContext {
    #[inline]
    fn free(self) {
        unsafe { ((&*self.core).destroyContext.unwrap())(self.context.as_ptr()) };
        mem::forget(self);
    }

    #[cfg(feature = "async")]
    #[inline]
    fn free_async(self) -> impl Future<Output = ()> {
        blocking::unblock(move || self.free())
    }
}

#[inline]
fn get_blit_context_internal<Dpy>(
    draw: &Dri3Drawable<Dpy>,
    lock: &mut Option<BlitContext>,
) -> CtxPtr {
    if let Some(bc) = lock {
        if bc.screen != draw.screen().dri_screen() {
            let bc = lock.take();
            bc.unwrap().free();
        }
    }

    let ctx = if let Some(bc) = lock {
        bc.context
    } else {
        let scr = draw.screen();
        let core = scr.inner.core;
        let ctx = unsafe {
            ((&*core).createNewContext.unwrap())(
                scr.dri_screen().as_ptr(),
                ptr::null(),
                ptr::null_mut(),
                ptr::null_mut(),
            )
        };

        let ctx = match NonNull::new(ctx) {
            Some(ctx) => ctx,
            None => return CtxPtr(None),
        };

        *lock = Some(BlitContext {
            screen: scr.dri_screen(),
            core,
            context: ctx,
        });
        ctx
    };

    CtxPtr(Some(ctx))
}

#[inline]
fn get_blit_context<Dpy>(
    draw: &Dri3Drawable<Dpy>,
) -> (CtxPtr, MutexGuard<'static, Option<BlitContext>>) {
    #[cfg(not(feature = "async"))]
    let mut blit_context = BLIT_CONTEXT
        .lock()
        .expect("Unable to acquire lock on blit context");
    #[cfg(feature = "async")]
    let mut blit_context = future::block_on(BLIT_CONTEXT.lock());

    (
        get_blit_context_internal(draw, &mut *blit_context),
        blit_context,
    )
}

#[cfg(feature = "async")]
#[inline]
async fn get_blit_context_async<Dpy>(
    draw: Arc<Dri3Drawable<Dpy>>,
) -> (CtxPtr, MutexGuard<'static, Option<BlitContext>>) {
    let mut blit_context = BLIT_CONTEXT.lock().await;
    let draw = draw.clone();

    blocking::unblock(move || {
        (
            get_blit_context_internal(&draw, &mut *blit_context),
            blit_context,
        )
    })
    .await
}

impl DrawableState {
    #[inline]
    fn update_max_back(&mut self, draw_interval: i32) {
        if self.last_present_mode == PRESENT_MODE_FLIP {
            let new_max = if draw_interval == 0 { 4 } else { 3 };

            if new_max < self.max_num_back {
                self.cur_num_back = 2;
            }

            self.max_num_back = new_max;
        } else if self.last_present_mode == PRESENT_MODE_SKIP {
            ()
        } else {
            if self.max_num_back != 2 {
                self.cur_num_back = 1;
            }

            self.max_num_back = 2;
        }
    }
}

impl<Dpy> Dri3Drawable<Dpy> {
    #[inline]
    pub fn is_pixmap(&self) -> bool {
        self.is_pixmap.load(Ordering::SeqCst)
    }

    #[inline]
    pub fn is_different_gpu(&self) -> bool {
        self.is_different_gpu
    }

    #[inline]
    pub fn swap_method(&self) -> c_int {
        self.swap_method
    }

    #[inline]
    pub fn set_have_fake_front(&self, val: bool) {
        self.has_fake_front.store(val, Ordering::SeqCst)
    }

    #[inline]
    pub fn set_have_back(&self, val: bool) {
        self.has_back.store(val, Ordering::SeqCst)
    }

    #[inline]
    pub fn dri_drawable(&self) -> NonNull<ffi::__DRIdrawable> {
        self.drawable
    }

    #[inline]
    fn screen(&self) -> Dri3Screen<Dpy> {
        self.screen.promote()
    }

    #[inline]
    fn invalidate_internal(&self) {
        // call the equivalent function on the flush driver
        if self.screen().inner.flush.is_null() {
            log::warn!("Cannot invalidate DRI3 drawable; flush driver is not present");
        } else {
            unsafe {
                ((*self.screen().inner.flush)
                    .invalidate
                    .expect("invalidate not present"))(self.dri_drawable().as_ptr())
            };
        }
    }

    /// Do we have blit functionality?
    #[inline]
    fn has_blit_image(&self) -> bool {
        match unsafe { self.screen().inner.image.as_ref() } {
            Some(img) => img.base.version >= 9 && img.blitImage.is_some(),
            None => false,
        }
    }

    /// Process a single present event.
    #[inline]
    fn process_present_event(
        &self,
        state: &mut DrawableState,
        event: Event,
    ) -> breadx::Result<bool> {
        const NEB_ERROR: breadx::BreadError =
            breadx::BreadError::StaticMsg("Invalid event: not enough bytes");

        macro_rules! geti {
            ($arr: expr, $index: expr) => {{
                *($arr)
                    .get($index)
                    .ok_or(breadx::BreadError::StaticErr(&NEB_ERROR))?
            }};
        }

        // convert the event into its bytes
        // it shouldn't be differentiated yet, return an error if it is
        // TODO: use branch prediction to set the error branch to "unlikely"
        let bytes = match event {
            Event::NoneOfTheAbove { bytes, .. } => bytes,
            _ => {
                return Err(breadx::BreadError::StaticMsg(
                    "Event was already differentiated",
                ))
            }
        };

        // for present, the event id is at bytes 8 thru 9
        let event_id = u16::from_ne_bytes([geti!(bytes, 8), geti!(bytes, 9)]);

        match event_id {
            // XCB_PRESENT_CONFIGURE_NOTIFY
            0 => {
                // width is at bytes 24 and 25, height is at bytes 26 and 27
                let width = u16::from_ne_bytes([geti!(bytes, 24), geti!(bytes, 25)]);
                let height = u16::from_ne_bytes([geti!(bytes, 26), geti!(bytes, 27)]);

                self.width.store(width, Ordering::Release);
                self.height.store(height, Ordering::Release);

                return Ok(true);
            }
            // XCB_PRESENT_COMPLETE_NOTIFY
            1 => {
                // serial is at bytes 20 to 24
                let serial = u32::from_ne_bytes([
                    geti!(bytes, 20),
                    geti!(bytes, 21),
                    geti!(bytes, 22),
                    geti!(bytes, 23),
                ]);
                // ust is at bytes 25 thru 32
                let mut ust = [0; 8];
                ust.copy_from_slice(&bytes[25..32]);
                let ust = i64::from_ne_bytes(ust);
                // mst is at bytes 36 thru 44
                let mut msc = [0; 8];
                msc.copy_from_slice(&bytes[36..44]);
                let msc = i64::from_ne_bytes(msc);
                // kind is at byte 10
                match bytes[10] {
                    0 => {
                        let recv_sbc = (state.send_sbc & 0xFFFFFFFF00000000u64) | (serial as u64);

                        if recv_sbc <= state.send_sbc {
                            state.recv_sbc = recv_sbc;
                        } else if recv_sbc == state.recv_sbc.wrapping_add(0x100000001u64) {
                            state.recv_sbc = recv_sbc.wrapping_sub(0x100000000u64);
                        }

                        let mode = bytes[11];
                        if (mode == PRESENT_MODE_COPY
                            && state.last_present_mode == PRESENT_MODE_FLIP)
                            || (mode == PRESENT_MODE_SUBOPTIMAL_COPY
                                && state.last_present_mode != PRESENT_MODE_SUBOPTIMAL_COPY)
                        {
                            state.buffers.iter_mut().for_each(|buffer| {
                                if let Some(buffer) = buffer.as_mut() {
                                    Arc::get_mut(buffer).unwrap().reallocate = true
                                }
                            });
                        }

                        state.last_present_mode = mode;
                        state.ust = ust;
                        state.msc = msc;
                    }
                    _ => {
                        if self.eid.load(Ordering::Acquire) == serial {
                            state.notify_ust = ust;
                            state.notify_msc = msc;
                        }
                    }
                }
            }
            // XCB_PRESENT_IDLE_NOTIFY
            2 => {
                // pixmap is at bytes 24 through 28
                let pixmap = u32::from_ne_bytes([
                    geti!(bytes, 24),
                    geti!(bytes, 25),
                    geti!(bytes, 26),
                    geti!(bytes, 27),
                ]);
                let pixmap = Pixmap::const_from_xid(pixmap);

                state.buffers.iter_mut().for_each(|buffer| {
                    if let Some(buffer) = buffer.as_mut() {
                        if buffer.pixmap == pixmap {
                            Arc::get_mut(buffer).unwrap().busy = 0;
                        }
                    }
                });
            }
            _ => (),
        }

        Ok(false)
    }

    #[inline]
    fn state(&self) -> StateGuard<'_> {
        log::trace!("Creating state lock for drawable");

        cfg_if::cfg_if! {
            if #[cfg(feature = "async")] {
                StateGuard { inner: Some(future::block_on(self.state.lock())) }
            } else {
                StateGuard { inner: Some(self.state.lock().expect(STATE_LOCK_FAILED)) }
            }
        }
    }

    #[cfg(feature = "async")]
    #[inline]
    async fn state_async(&self) -> StateGuard<'_> {
        StateGuard {
            inner: Some(self.state.lock().await),
        }
    }

    #[inline]
    fn event_wait<'a>(&'a self, guard: StateGuard<'a>) -> StateGuard<'a> {
        cfg_if::cfg_if! {
            if #[cfg(feature = "async")] {
                mem::drop(guard);
                self.event_waiter.listen().wait();
                future::block_on(self.state.lock())
            } else {
                let guard = guard.into_inner();
                StateGuard { inner: Some(self.event_waiter
                    .wait(guard)
                    .expect("Failed to wait for present events")) }
            }
        }
    }

    #[cfg(feature = "async")]
    #[inline]
    async fn event_wait_async(&self, guard: StateGuard<'_>) -> StateGuard<'_> {
        mem::drop(guard);
        self.event_waiter.listen().await;
        self.state.lock().await
    }

    #[inline]
    fn event_broadcast(&self) {
        #[cfg(not(feature = "async"))]
        {
            self.event_waiter.notify_all()
        }
        #[cfg(feature = "async")]
        {
            self.event_waiter.notify_additional(usize::MAX)
        }
    }
}

impl<Dpy: DisplayLike> Dri3Drawable<Dpy> {
    /// Process present events.
    #[inline]
    fn process_present_events<'a>(
        &self,
        conn: &mut Display<Dpy::Connection>,
        state_lock: &mut DrawableState,
    ) -> breadx::Result<bool> {
        // use an iterator to handle the events
        let needs_invalidate = conn
            .get_special_events(self.eid.load(Ordering::Relaxed))
            .map(|event| self.process_present_event(state_lock, event))
            .collect::<breadx::Result<Vec<bool>>>()?;
        Ok(needs_invalidate.iter().any(|b| *b))
    }
}

impl<Dpy: DisplayLike> Dri3Drawable<Dpy>
where
    Dpy::Connection: Connection,
{
    #[inline]
    pub fn new(
        dpy: &GlDisplay<Dpy>,
        drawable: Drawable,
        screen: Dri3Screen<Dpy>,
        context: Dri3Context<Dpy>,
        config: GlConfig,
        has_multiplane: bool,
    ) -> breadx::Result<Arc<Self>> {
        let (adaptive_sync, vblank_mode) = get_adaptive_sync_and_vblank_mode(&screen);
        let swap_interval = match vblank_mode {
            0 | 1 => 0,
            _ => 1,
        };

        if adaptive_sync == 0 {
            set_adaptive_sync(&mut *dpy.display(), drawable, false)?;
        }

        // get the width and height of the drawable
        let geometry = dpy.display().get_drawable_geometry_immediate(drawable)?;

        let mut swap_method = ffi::__DRI_ATTRIB_SWAP_UNDEFINED;
        if unsafe { (&*screen.inner.core) }.base.version >= 2 {
            unsafe {
                ((&*screen.inner.core).getConfigAttrib.unwrap())(
                    screen.driconfig_from_fbconfig(&config).unwrap().as_ptr(),
                    ffi::__DRI_ATTRIB_SWAP_METHOD,
                    &mut swap_method,
                )
            };
        }

        let mut this = Arc::new(Self {
            drawable: NonNull::dangling(),
            x_drawable: drawable,
            config,
            is_different_gpu: screen.inner.is_different_gpu,
            multiplanes_available: has_multiplane,
            screen: screen.weak_ref(),
            context,
            width: AtomicU16::new(geometry.width),
            height: AtomicU16::new(geometry.height),
            depth: AtomicU8::new(geometry.depth),
            eid: AtomicU32::new(0),
            is_initialized: AtomicBool::new(false),
            present_capabilities: AtomicU32::new(0),
            window: AtomicU32::new(0),
            gc: AtomicU32::new(0),
            swap_interval: AtomicI32::new(swap_interval as _),
            is_pixmap: AtomicBool::new(false),
            display: dpy.clone(),
            swap_method: swap_method as _,
            has_fake_front: AtomicBool::new(false),
            has_back: AtomicBool::new(true),
            #[cfg(feature = "async")]
            state: async_lock::Mutex::new(Default::default()),
            #[cfg(not(feature = "async"))]
            state: sync::Mutex::new(Default::default()),
            has_event_waiter: AtomicBool::new(false),
            #[cfg(feature = "async")]
            event_waiter: event_listener::Event::new(),
            #[cfg(not(feature = "async"))]
            event_waiter: sync::Condvar::new(),
        });

        // create the drawable pointer
        let dri_drawable = create_the_drawable(&screen, &this.config, Arc::as_ptr(&this) as _)?;
        Arc::get_mut(&mut this)
            .expect("Infallible Arc::get_mut()")
            .drawable = dri_drawable.0;

        Ok(this)
    }

    #[inline]
    fn drawable_gc(&self, conn: &mut Display<Dpy::Connection>) -> breadx::Result<Gcontext> {
        let mut gc = Gcontext::const_from_xid(self.gc.load(Ordering::Acquire));
        if gc.xid == 0 {
            gc = conn.create_gc(
                self.x_drawable,
                GcParameters {
                    graphics_exposures: Some(0),
                    ..Default::default()
                },
            )?;
            self.gc.store(gc.xid, Ordering::Release);
        }
        Ok(gc)
    }

    /// Wait for present events to occur.
    #[inline]
    fn wait_for_event<'a, 'b>(
        &'a self,
        state_lock: &'b mut Option<StateGuard<'a>>,
    ) -> breadx::Result<()>
    where
        'a: 'b,
    {
        log::trace!("Beginning wait for present event...");

        let res = if self.has_event_waiter.load(Ordering::SeqCst) {
            // another thread is polling for events for this drawable, wait a minute
            let sl = state_lock.take().expect("Non-exclusive lock?!?!");
            *state_lock = Some(self.event_wait(sl));
            Ok(())
        } else {
            self.has_event_waiter.store(true, Ordering::SeqCst);
            // drop the lock, then poll the display, then re-acquire the lock
            mem::drop(state_lock.take());
            let mut conn = self.display.display();
            let res = conn.wait_for_special_event(self.eid.load(Ordering::Relaxed));
            mem::drop(conn);
            *state_lock = Some(self.state());
            self.has_event_waiter.store(false, Ordering::SeqCst);
            self.event_broadcast();
            let event = res?;

            if self.process_present_event(state_lock.as_mut().unwrap(), event)? {
                self.invalidate();
            }

            Ok(())
        };

        log::trace!("Ending wait for present event...");
        res
    }

    #[inline]
    fn wait_for_sbc(&self, target_sbc: Option<NonZeroU64>) -> breadx::Result<SwapBufferCount> {
        let mut state = self.state();
        let target_sbc = match target_sbc {
            Some(tsbc) => tsbc.get(),
            None => state.send_sbc,
        };

        let mut state = Some(state);

        while {
            let r = state.as_ref().unwrap().recv_sbc;
            r < target_sbc
        } {
            self.wait_for_event(&mut state)?;
        }

        // we're good panicking here, we abort on panic anyways
        let state = state.expect("Shouldn't ever happen (unless we've somehow panicked!)!");
        Ok(SwapBufferCount {
            ust: state.ust,
            msc: state.msc,
            sbc: state.sbc,
        })
    }

    #[inline]
    fn swapbuffer_barrier(&self) -> breadx::Result<()> {
        self.wait_for_sbc(None)?;
        Ok(())
    }

    /// Find the ID associated with the back buffer.
    #[inline]
    fn find_back<'a, 'b>(
        &'a self,
        mut conn: DisplayLock<'_, Dpy>,
        state: &'b mut Option<StateGuard<'a>>,
    ) -> breadx::Result<usize>
    where
        'a: 'b,
    {
        log::trace!("Entering scope for find_back");

        if self.process_present_events(&mut *conn, state.as_mut().unwrap())? {
            self.invalidate();
        }
        mem::drop(conn); // we need to poll it later

        let (mut num_to_consider, max_num) = if self.has_blit_image() {
            (
                state.as_mut().unwrap().cur_num_back,
                state.as_mut().unwrap().max_num_back,
            )
        } else {
            state.as_mut().unwrap().cur_blit_source = -1;
            (1, 1)
        };

        loop {
            for i in 0..num_to_consider {
                let id = back_id(
                    (i + state.as_mut().unwrap().cur_back) * state.as_mut().unwrap().cur_num_back,
                );
                if state.as_mut().unwrap().buffers[id]
                    .as_ref()
                    .map(|b| b.busy == 0)
                    .unwrap_or(true)
                {
                    state.as_mut().unwrap().cur_back = id;
                    return Ok(id);
                }
            }

            if num_to_consider < max_num {
                state.as_mut().unwrap().cur_num_back += 1;
                num_to_consider = state.as_mut().unwrap().cur_num_back;
            } else {
                // wait for an event
                self.wait_for_event(state)?;
            }
        }
    }

    /// Blit two images associated with this drawable.
    #[inline]
    fn blit_images(
        &self,
        dst: ImgPtr,
        src: ImgPtr,
        dstx: c_int,
        dsty: c_int,
        width: c_int,
        height: c_int,
        srcx: c_int,
        srcy: c_int,
        mut flush_flag: c_int,
    ) -> breadx::Result<()> {
        if !self.has_blit_image() {
            return Err(StaticMsg("Unable to blit images"));
        }

        // get the context we need to do the blitting
        let (dri_context, _guard) = if self.context.is_current() {
            (self.context.dri_context(), None)
        } else {
            flush_flag |= ffi::__BLIT_FLAG_FLUSH as c_int;
            let (dri_context, guard) = get_blit_context(self);
            let dri_context = match dri_context.0 {
                Some(dc) => dc,
                None => return Err(StaticMsg("Unable to creat blitting context")),
            };
            (dri_context, Some(guard))
        };

        unsafe {
            ((&*self.screen().inner.image)
                .blitImage
                .expect("BlitImage not present"))(
                dri_context.as_ptr(),
                dst.0.as_ptr(),
                src.0.as_ptr(),
                dstx,
                dsty,
                width,
                height,
                srcx,
                srcy,
                width,
                height,
                flush_flag,
            )
        };
        Ok(())
    }

    /// Free all buffers of an associated type.
    #[inline]
    pub fn free_buffers(&self, buffer_type: BufferType) -> breadx::Result<()> {
        log::trace!("Entering scope for free_buffers");
        let mut state = self.state();
        let (first_id, n_ids) = match buffer_type {
            BufferType::Back => {
                state.cur_blit_source = -1;
                (back_id(0), MAX_BACK)
            }
            BufferType::Front => (
                FRONT_ID,
                if state.cur_blit_source == FRONT_ID as _ {
                    0
                } else {
                    1
                },
            ),
        };

        (first_id..first_id + n_ids).try_for_each::<_, breadx::Result<()>>(|i| {
            if let Some(buffer) = state.buffers[i].take() {
                free_buffer_arc(buffer, self)?;
            }
            Ok(())
        })?;

        Ok(())
    }

    /// Free unneeded back buffers.
    #[inline]
    pub fn free_back_buffers(&self) -> breadx::Result {
        log::trace!("Entering scope for free_back_buffers");
        let mut state = self.state();
        for id in state.cur_num_back..MAX_BACK {
            if id as i32 != state.cur_blit_source && state.buffers[id].is_some() {
                free_buffer_arc(state.buffers[id].take().unwrap(), self)?;
            }
        }
        Ok(())
    }

    #[inline]
    fn buffer_id(&self, ty: BufferType) -> breadx::Result<usize> {
        Ok(match ty {
            BufferType::Back => {
                let mut state = Some(self.state());
                self.find_back(self.display.display(), &mut state)?
            }
            BufferType::Front => FRONT_ID,
        })
    }

    /// Get the buffer associated with the given format and buffer type.
    #[inline]
    pub fn get_buffer<'a>(
        &'a self,
        buffer_type: BufferType,
        format: c_uint,
    ) -> breadx::Result<Arc<Dri3Buffer>> {
        log::trace!("Entering scope for get_buffer");

        let buf_id = self.buffer_id(buffer_type)?;

        // see if there is a buffer; if there isn't a buffer (or if there is, but it's wrong),
        // rellocate it
        let width = self.width.load(Ordering::SeqCst);
        let height = self.height.load(Ordering::SeqCst);
        let mut state: Option<StateGuard<'a>> = None;
        let mut fence_await = false;

        let mut create_new_buffer =
            move |state: &mut Option<StateGuard<'a>>| -> breadx::Result<Arc<Dri3Buffer>> {
                // create a new buffer
                let mut new_buffer = Dri3Buffer::new(
                    self,
                    format,
                    width,
                    height,
                    self.depth.load(Ordering::SeqCst),
                )?;

                let mut conn = self.display.display();
                if state.is_none() {
                    *state = Some(self.state());
                }

                let buffer = state.as_mut().unwrap().buffers[buf_id].take();
                if buffer.is_some()
                    && (!matches!(buffer_type, BufferType::Front)
                        || self.has_fake_front.load(Ordering::Acquire))
                {
                    let buffer = buffer.unwrap();
                    if self
                        .blit_images(
                            ImgPtr(new_buffer.image),
                            ImgPtr(buffer.image),
                            0,
                            0,
                            cmp::min(buffer.width.into(), new_buffer.width.into()),
                            cmp::min(buffer.height.into(), new_buffer.height.into()),
                            0,
                            0,
                            0,
                        )
                        .is_err()
                        && buffer.linear_buffer.is_none()
                    {
                        reset_fence(new_buffer.shm_fence);
                        let gc = self.drawable_gc(&mut *conn)?;
                        conn.copy_area(
                            buffer.pixmap,
                            new_buffer.pixmap,
                            gc,
                            0,
                            0,
                            width,
                            height,
                            0,
                            0,
                        )?;
                        trigger_fence::<Dpy>(&mut conn, new_buffer.sync_fence)?;
                        fence_await = true;
                    }

                    mem::drop(conn);
                    free_buffer_arc(buffer, self)?;
                } else if matches!(buffer_type, BufferType::Front) {
                    // fill the new fake front with data from the real front
                    mem::drop((conn, state.take()));
                    self.swapbuffer_barrier()?;
                    let mut conn = self.display.display();
                    reset_fence(new_buffer.shm_fence);
                    let gc = self.drawable_gc(&mut *conn)?;
                    conn.copy_area(
                        self.x_drawable,
                        new_buffer.pixmap,
                        gc,
                        0,
                        0,
                        width,
                        height,
                        0,
                        0,
                    )?;
                    trigger_fence::<Dpy>(&mut conn, new_buffer.sync_fence)?;

                    if let Some(linear_buffer) = new_buffer.linear_buffer {
                        block_on_fence(&mut *conn, Some(self), &new_buffer)?;
                        self.blit_images(
                            ImgPtr(new_buffer.image),
                            ImgPtr(linear_buffer),
                            0,
                            0,
                            width.into(),
                            height.into(),
                            0,
                            0,
                            0,
                        )?;
                    } else {
                        fence_await = true;
                    }
                }

                if state.is_none() {
                    *state = Some(self.state());
                }
                state.as_mut().unwrap().buffers[buf_id] = Some(new_buffer.clone());
                Ok(new_buffer)
            };

        let mut temp_state = self.state();
        let buffer = match temp_state.buffers[buf_id] {
            None => { mem::drop(temp_state); create_new_buffer(&mut state)? },
            Some(ref buffer)
                if buffer.reallocate || buffer.width != width || buffer.height != height =>
            {
                mem::drop(temp_state);
                create_new_buffer(&mut state)?
            }
            Some(ref buffer) => {
                let res = buffer.clone();
                mem::drop(temp_state);
                res
            }
        };

        log::trace!("Finished buffer initialization");

        if fence_await {
            mem::drop(state.take());
            block_on_fence(&mut *self.display.display(), Some(self), &buffer)?;
        }

        // if we need to preserve the content of previous buffers...
        let mut state = match state {
            Some(lock) => lock,
            None => self.state(),
        };
        if matches!(buffer_type, BufferType::Back)
            && state.cur_blit_source != -1
            && state.buffers[state.cur_blit_source as usize].is_some()
            && !Arc::ptr_eq(
                &buffer,
                &state.buffers[state.cur_blit_source as usize]
                    .as_ref()
                    .unwrap(),
            )
        {
            self.blit_images(
                ImgPtr(buffer.image),
                ImgPtr(
                    state.buffers[state.cur_blit_source as usize]
                        .as_ref()
                        .unwrap()
                        .image,
                ),
                0,
                0,
                width.into(),
                height.into(),
                0,
                0,
                0,
            )?;

            buffer.last_swap.store(
                state.buffers[state.cur_blit_source as usize]
                    .as_ref()
                    .unwrap()
                    .last_swap
                    .load(Ordering::Acquire),
                Ordering::Release,
            );
            state.cur_blit_source = -1;
        }

        log::trace!("Leaving scope for get_buffer");
        Ok(buffer)
    }

    #[inline]
    pub fn get_pixmap_buffer(
        &self,
        buffer_type: BufferType,
        format: c_uint,
    ) -> breadx::Result<Arc<Dri3Buffer>> {
        log::trace!("Entering scope for get_pixmap_buffer");

        let buf_id = self.buffer_id(buffer_type)?;
        if let Some(buffer) = self.state().buffers[buf_id].as_ref().cloned() {
            return Ok(buffer);
        }

        // TODO: a lot of stuff is reused here from Dri3Buffer::new(). consolidate it into its
        // own function
        let mut buffer: Arc<MaybeUninit<Dri3Buffer>> = Arc::new_uninit();
        let this_screen = self.screen();

        let xshmfence = xshmfence()?;
        let alloc_shm: XshmfenceAllocShm = unsafe { xshmfence.function(&XSHMFENCE_ALLOC_SHM) }
            .expect("xshmfence_alloc_shm not present");
        let map_shm: XshmfenceMapShm = unsafe { xshmfence.function(&XSHMFENCE_MAP_SHM) }
            .expect("xshmfence_map_shm not present");
        let unmap_shm: XshmfenceUnmapShm =
            unsafe { xshmfence.function(&XSHMFENCE_UNMAP_SHM) }.expect("xshmfence_unmap_shm");

        // set up fencing and all
        let fence_fd = unsafe { (alloc_shm)() };
        if fence_fd < 0 {
            return Err(StaticMsg("Failed to allocate SHM fence"));
        }

        let shm_fence = unsafe { (map_shm)(fence_fd) };
        let shm_fence = match NonNull::new(shm_fence) {
            Some(shm_fence) => shm_fence,
            None => {
                unsafe { libc::close(fence_fd) };
                return Err(StaticMsg("Failed to map SHM fence"));
            }
        };

        let mut conn = self.display.display();
        let sync_fence = match conn.fence_from_fd(self.x_drawable, false, fence_fd) {
            Ok(sf) => sf,
            Err(e) => {
                unsafe { (unmap_shm)(shm_fence.as_ptr()) };
                unsafe { libc::close(fence_fd) };
                return Err(e);
            }
        };

        let fence_guard = CallOnDrop::new(|| {
            unsafe { (unmap_shm)(shm_fence.as_ptr()) };
            unsafe { libc::close(fence_fd) };
        });

        // get the screen belogning to the current context, or our screen if nothing else
        let screen = if let Some(ref ctx) = GlContext::<Dpy>::get()
            .as_ref()
            .and_then(|m| promote_anyarc_ref::<Dpy>(m))
        {
            if let ContextDispatch::Dri3(d3) = ctx.dispatch() {
                d3.screen().dri_screen()
            } else {
                this_screen.dri_screen()
            }
        } else {
            this_screen.dri_screen()
        };
        let pixmap = Pixmap::const_from_xid(self.x_drawable.xid);

        let (image, width, height) = if self.multiplanes_available
            && unsafe { (&*this_screen.inner.image) }.base.version >= 15
            && unsafe { (&*this_screen.inner.image) }
                .createImageFromDmaBufs2
                .is_some()
        {
            let bfp = conn.buffers_from_pixmap_immediate(pixmap)?;
            let width = bfp.width;
            let height = bfp.height;
            let image = image_from_buffers(
                &self.screen(),
                format,
                bfp,
                unsafe { ThreadSafe::new(screen.as_ptr()) },
                unsafe { ThreadSafe::new(buffer.as_ptr() as *const ()) },
            )?;
            (image.into_inner(), width, height)
        } else {
            let bfp = conn.buffer_from_pixmap_immediate(pixmap)?;
            let width = bfp.width;
            let height = bfp.height;
            let image = image_from_buffer(
                &self.screen(),
                format,
                bfp,
                unsafe { ThreadSafe::new(screen.as_ptr()) },
                unsafe { ThreadSafe::new(buffer.as_ptr() as *const ()) },
            )?;
            (image.into_inner(), width, height)
        };

        mem::forget(fence_guard);

        unsafe {
            ptr::write(
                Arc::get_mut(&mut buffer)
                    .expect("Infallible Arc::get_mut()")
                    .as_mut_ptr(),
                Dri3Buffer {
                    image,
                    linear_buffer: None,
                    sync_fence,
                    shm_fence,
                    pixmap,
                    own_pixmap: false,
                    busy: 0,
                    reallocate: false,
                    cpp: 0,
                    modifier: 0,
                    width,
                    height,
                    last_swap: AtomicU64::new(0),
                },
            )
        };

        Ok(unsafe { buffer.assume_init() })
    }

    #[inline]
    pub fn invalidate(&self) {
        self.invalidate_internal()
    }

    /// Update this drawable.
    #[inline]
    pub fn update(&self) -> breadx::Result {
        log::trace!("Entering scope for update");

        let mut guard = self.state();

        // acquire a lock on the display
        let mut conn = self.display.display();

        if !self.is_initialized.load(Ordering::Acquire) {
            log::trace!("Initializing drawable");
            self.is_initialized.store(true, Ordering::Release);

            // activate checked mode if we haven't already
            let old_checked = conn.checked();
            conn.set_checked(true);

            // now that we have a lock, create an EID that represents the
            // special event selection
            let eid = conn.generate_xid()?;
            self.eid.store(eid, Ordering::Relaxed);

            // query a few requests
            conn.register_special_event(eid);
            let capabilities_tok = conn.present_capabilities(self.x_drawable)?;
            let geometry_tok = conn.get_drawable_geometry(self.x_drawable)?;
            let select_input = conn.present_select_input(
                eid,
                Window::const_from_xid(self.x_drawable.xid),
                PresentEventMask::CONFIGURE_NOTIFY
                    | PresentEventMask::COMPLETE_NOTIFY
                    | PresentEventMask::IDLE_NOTIFY,
            );

            // if we got a BadWindow error, we can just circumvent that
            let is_pixmap = match select_input {
                Ok(()) => false,
                Err(breadx::BreadError::XProtocol {
                    error_code: breadx::ErrorCode(3),
                    ..
                }) => {
                    conn.unregister_special_event(eid);
                    self.is_pixmap.store(true, Ordering::Relaxed);
                    true
                }
                Err(err) => return Err(err),
            };

            // match the error on the geometry and select input results, if they are
            // BadWindow, this is a pixmap
            log::trace!("Resolving geometry");
            let geometry = conn.resolve_request(geometry_tok)?;

            self.present_capabilities.store(
                match conn.resolve_request(capabilities_tok) {
                    Ok(cap) => cap.capabilities,
                    Err(_) => 0,
                },
                Ordering::Relaxed,
            );

            self.window.store(
                if is_pixmap {
                    geometry.root.xid
                } else {
                    self.x_drawable.xid
                },
                Ordering::Relaxed,
            );

            self.width.store(geometry.width, Ordering::Relaxed);
            self.height.store(geometry.height, Ordering::Relaxed);

            conn.set_checked(old_checked);
        }

        if self.process_present_events(&mut conn, &mut guard)? {
            self.invalidate();
        }

        Ok(())
    }

    #[inline]
    pub fn update_max_back(&self) {
        log::trace!("Entering scope for update_max_back");
        let mut state = self.state();

        let swap_interval = self.swap_interval.load(Ordering::Acquire);
        state.update_max_back(swap_interval);
    }

    #[inline]
    pub fn has_supported_modifier(&self, format: c_uint, modifiers: &[u64]) -> bool {
        let query_dma_bufs = match unsafe { &*self.screen().inner.image }.queryDmaBufModifiers {
            Some(qdb) => qdb,
            None => return false,
        };

        // first, get the actual number of supported modifiers
        let mut mod_count = MaybeUninit::<c_int>::uninit();
        if unsafe {
            query_dma_bufs(
                self.screen().dri_screen().as_ptr(),
                format as _,
                0,
                ptr::null_mut(),
                ptr::null_mut(),
                mod_count.as_mut_ptr(),
            )
        } == 0
        {
            return false;
        }

        // SAFETY: If queryDmaBufModifiers succeeded, mod_count is contractly supposed
        //         to have been set.
        let mut mod_count = unsafe { MaybeUninit::assume_init(mod_count) };
        if mod_count == 0 {
            return false;
        }

        // then query for modifiers, now that we know we have enough memory to store it
        let mut modifiers = Box::<[u64]>::new_uninit_slice(mod_count as usize);
        unsafe {
            query_dma_bufs(
                self.screen().dri_screen().as_ptr(),
                format as _,
                mod_count,
                ptr::null_mut(),
                ptr::null_mut(),
                &mut mod_count,
            )
        };
        // SAFETY: Same rules as above.
        let modifiers = unsafe { modifiers.assume_init() };

        modifiers
            .into_iter()
            .flat_map(|i| modifiers.iter().map(move |j| (i, j)))
            .find(|(i, j)| i == j)
            .is_some()
    }
}

#[cfg(feature = "async")]
impl<Dpy: DisplayLike> Dri3Drawable<Dpy>
where
    Dpy::Connection: AsyncConnection + Send,
{
    #[inline]
    pub async fn new_async(
        dpy: &GlDisplay<Dpy>,
        drawable: Drawable,
        screen: Dri3Screen<Dpy>,
        config: GlConfig,
    ) -> breadx::Result<Arc<Self>> {
        // we can double up here to hopefully save some time
        let ((adaptive_sync, vblank_mode, screen), geometry) = future::zip(
            blocking::unblock(move || {
                let (adaptive_sync, vblank_mode) = get_adaptive_sync_and_vblank_mode(&screen);
                (adaptive_sync, vblank_mode, screen)
            }),
            async {
                breadx::drawable::get_geometry_immediate_async(
                    &mut *dpy.display_async().await,
                    drawable,
                )
                .await
            },
        )
        .await;
        let swap_interval = match vblank_mode {
            0 | 1 => 0,
            _ => 1,
        };

        // TODO: figure out if this is more expensive than it's worth
        let as_future = if adaptive_sync == 0 {
            Box::pin(async {
                set_adaptive_sync_async(&mut *dpy.display_async().await, drawable, false).await
            }) as GenericFuture<'_, breadx::Result>
        } else {
            Box::pin(future::ready(Ok(()))) as GenericFuture<'_, breadx::Result>
        };

        let mut this = Arc::new(Self {
            drawable: NonNull::dangling(),
            x_drawable: drawable,
            config,
            is_different_gpu: screen.inner.is_different_gpu,
            screen: screen.weak_ref(),
            width: AtomicU16::new(geometry.width),
            height: AtomicU16::new(geometry.height),
            is_initialized: AtomicBool::new(false),
            present_capabilities: AtomicU32::new(0),
            window: AtomicU32::new(0),
            swap_interval: AtomicI32::new(swap_interval as _),
            is_pixmap: false,
            display: dpy.clone(),
        });

        let this1 = this.clone();
        let (res, dri_drawable) = future::zip(as_future, async move {
            blocking::unblock(move || {
                let dri_drawable = create_the_drawable(
                    &this1.screen,
                    &this1.config,
                    Arc::as_ptr(&this1) as *const _,
                );
                dri_drawable
            })
            .await
        })
        .await;

        let dri_drawable = res.and(dri_drawable)?;
        Arc::get_mut(&mut this)
            .expect("Infallible Arc::get_mut()")
            .drawable = dri_drawable.0;

        Ok(this)
    }

    #[inline]
    pub async fn invalidate_async(this: Arc<Self>) {
        blocking::unblock(move || this.invalidate_internal()).await
    }
}

const XSHMFENCE_ALLOC_SHM: ConstCstr<'static> = const_cstr(&*b"xshmfence_alloc_shm\0");
type XshmfenceAllocShm = unsafe extern "C" fn() -> c_int;
const XSHMFENCE_MAP_SHM: ConstCstr<'static> = const_cstr(&*b"xshmfence_map_shm\0");
type XshmfenceMapShm = unsafe extern "C" fn(c_int) -> *mut c_void;
const XSHMFENCE_UNMAP_SHM: ConstCstr<'static> = const_cstr(&*b"xshmfence_unmap_shm\0");
type XshmfenceUnmapShm = unsafe extern "C" fn(*mut c_void);
const XSHMFENCE_TRIGGER: ConstCstr<'static> = const_cstr(&*b"xshmfence_trigger\0");
type XshmfenceTrigger = XshmfenceUnmapShm;
const XSHMFENCE_RESET: ConstCstr<'static> = const_cstr(&*b"xshmfence_reset\0");
type XshmfenceReset = XshmfenceTrigger;
const XSHMFENCE_AWAIT: ConstCstr<'static> = const_cstr(&*b"xshmfence_await\0");
type XshmfenceAwait = XshmfenceReset;

impl Dri3Buffer {
    #[inline]
    fn new<Dpy: DisplayLike>(
        drawable: &Dri3Drawable<Dpy>,
        format: c_uint,
        width: u16,
        height: u16,
        depth: u8,
    ) -> breadx::Result<Arc<Self>>
    where
        Dpy::Connection: Connection,
    {
        // TODO: this function is absolutely massive, it's not even funny. break it up into smaller
        //       functions if we have a chance

        let xshmfence = xshmfence()?;
        let alloc_shm: XshmfenceAllocShm = unsafe { xshmfence.function(&XSHMFENCE_ALLOC_SHM) }
            .expect("xshmfence_alloc_shm not present");
        let map_shm: XshmfenceMapShm = unsafe { xshmfence.function(&XSHMFENCE_MAP_SHM) }
            .expect("xshmfence_map_shm not present");
        let unmap_shm: XshmfenceUnmapShm =
            unsafe { xshmfence.function(&XSHMFENCE_UNMAP_SHM) }.expect("xshmfence_unmap_shm");

        // create an xshm object
        let fence_fd = unsafe { (alloc_shm)() };
        if fence_fd < 0 {
            return Err(StaticMsg("Failed to allocate XSHM Fence"));
        }

        // we set up a variety of CallOnDrop objects that destroy the file descriptors if
        // the function errors out
        let fd_guard = CallOnDrop::new(|| unsafe {
            libc::close(fence_fd);
        });

        let shm_fence = unsafe { (map_shm)(fence_fd) };
        let shm_fence = match NonNull::new(shm_fence) {
            Some(shm_fence) => shm_fence,
            None => return Err(StaticMsg("Failed to map XSHM Fence")),
        };

        let shm_guard = CallOnDrop::new(move || unsafe { (unmap_shm)(shm_fence.as_ptr()) });

        let cpp = cpp_for_format(format).ok_or(StaticMsg("failed to find cpp for format"))?;

        // allocate the memory necessary for the buffer ahead of time
        // TODO: as far as I know, the memory isn't actually used for any loaders and the loader
        //       parameter is used mostly just in case we need to do it in the future. so that's what
        //       we do
        let mut buffer = Arc::<Dri3Buffer>::new_uninit();
        let mut conn = drawable.display.display();
        let screen = drawable.screen();

        // we use the image extension pretty heavily up above
        let image_ext = match unsafe { drawable.screen().inner.image.as_ref() } {
            Some(r) => r,
            None => return Err(StaticMsg("No image extension!")),
        };

        let (image, linear_buffer, pixmap_buffer) = if !drawable.is_different_gpu {
            let mut image = ptr::null_mut();

            // check to see if we can use modifiers
            if drawable.multiplanes_available
                && image_ext.base.version >= 15
                && image_ext.queryDmaBufModifiers.is_some()
                && image_ext.createImageWithModifiers.is_some()
            {
                let mut x_modifiers = conn.get_supported_modifiers_immediate(
                    drawable.x_drawable.xid,
                    depth,
                    cpp as u8 * 8,
                )?;
                let mut modifiers: Option<Vec<u64>> = None;

                if !x_modifiers.window.is_empty() {
                    if drawable.has_supported_modifier(
                        image_format_to_fourcc(format) as _,
                        &x_modifiers.window,
                    ) {
                        modifiers = Some(mem::take(&mut x_modifiers.window));
                    }
                }

                if !x_modifiers.screen.is_empty() && modifiers.is_none() {
                    modifiers = Some(x_modifiers.screen);
                }

                // if we were able to get the modifiers, use them to create the image
                if let Some(modifiers) = modifiers {
                    image = unsafe {
                        (image_ext.createImageWithModifiers.unwrap())(
                            drawable.screen().dri_screen().as_ptr(),
                            width as _,
                            height as _,
                            format as _,
                            modifiers.as_ptr(),
                            modifiers.len() as _,
                            buffer.as_ptr() as *const _ as *mut _,
                        )
                    };
                }
            }

            // if the above block of code didn't create an image, create an image w/o modifiers
            if image.is_null() {
                image = unsafe {
                    (image_ext.createImage.expect("createImage not present"))(
                        drawable.screen().dri_screen().as_ptr(),
                        width as _,
                        height as _,
                        format as _,
                        ffi::__DRI_IMAGE_USE_SHARE
                            | ffi::__DRI_IMAGE_USE_SCANOUT
                            | ffi::__DRI_IMAGE_USE_BACKBUFFER,
                        buffer.as_ptr() as *const _ as *mut _,
                    )
                };
            }

            let image = NonNull::new(image).ok_or(StaticMsg("createImage returned null"))?;

            (image, None, image)
        } else {
            // create an image without making GPU assumptions
            let image = unsafe {
                (image_ext.createImage.expect("createImage not present"))(
                    drawable.screen().dri_screen().as_ptr(),
                    width as _,
                    height as _,
                    format as _,
                    0,
                    buffer.as_ptr() as *const _ as *mut _,
                )
            };

            let image = match NonNull::new(image) {
                Some(image) => image,
                None => return Err(StaticMsg("createImage returned null")),
            };

            let linear_buffer = unsafe {
                (image_ext.createImage.expect("createImage not present"))(
                    drawable.screen().dri_screen().as_ptr(),
                    width as _,
                    height as _,
                    linear_format(&mut *conn, format as _) as _,
                    ffi::__DRI_IMAGE_USE_SHARE
                        | ffi::__DRI_IMAGE_USE_LINEAR
                        | ffi::__DRI_IMAGE_USE_BACKBUFFER,
                    buffer.as_ptr() as *const _ as *mut _,
                )
            };

            let linear_buffer = match NonNull::new(linear_buffer) {
                Some(linear_buffer) => linear_buffer,
                None => {
                    unsafe { (image_ext.destroyImage.unwrap())(image.as_ptr()) };
                    return Err(StaticMsg("createImage returned null"));
                }
            };

            (image, Some(linear_buffer), linear_buffer)
        };

        // destroy the images if we exit early
        let image_guard = CallOnDrop::new(|| {
            let image_driver = unsafe { &*drawable.screen().inner.image };
            let destroy_image = image_driver.destroyImage.unwrap();

            unsafe { destroy_image(image.as_ptr()) };
            if let Some(linear_buffer) = linear_buffer {
                unsafe { destroy_image(linear_buffer.as_ptr()) };
            }
        });

        // we need the number of planes for what comes next
        let mut plane_num = MaybeUninit::<c_int>::uninit();
        let plane_num = match unsafe {
            (image_ext.queryImage.unwrap())(
                pixmap_buffer.as_ptr(),
                ffi::__DRI_IMAGE_ATTRIB_NUM_PLANES as _,
                plane_num.as_mut_ptr(),
            )
        } {
            0 => 1,
            _ => unsafe { plane_num.assume_init() },
        };

        let mut buffer_fds: Vec<Cell<c_int>> = iter::repeat(Cell::new(-1)).take(4).collect();
        let mut strides: [c_int; 4] = [0; 4];
        let mut offsets: [c_int; 4] = [0; 4];
        let buffer_guard = CallOnDrop::new(|| {
            buffer_fds.iter().cloned().for_each(|fd| {
                let fd = fd.get();
                if fd != -1 {
                    unsafe { libc::close(fd) };
                }
            });
        });

        for i in 0..(plane_num as usize) {
            let cur_image = unsafe {
                (image_ext.fromPlanar.expect("fromPlanar not present"))(
                    pixmap_buffer.as_ptr(),
                    i as _,
                    ptr::null_mut(),
                )
            };
            let cur_image = match NonNull::new(cur_image) {
                Some(cur_image) => cur_image,
                None => {
                    assert_eq!(i, 0);
                    pixmap_buffer
                }
            };

            let mut buffer_fd: c_int = -1;
            let mut ret = unsafe {
                (image_ext.queryImage.unwrap())(
                    cur_image.as_ptr(),
                    ffi::__DRI_IMAGE_ATTRIB_FD as _,
                    &mut buffer_fd,
                )
            };
            buffer_fds[i].set(buffer_fd);

            ret &= unsafe {
                (image_ext.queryImage.unwrap())(
                    cur_image.as_ptr(),
                    ffi::__DRI_IMAGE_ATTRIB_STRIDE as _,
                    &mut strides[i],
                )
            };
            ret &= unsafe {
                (image_ext.queryImage.unwrap())(
                    cur_image.as_ptr(),
                    ffi::__DRI_IMAGE_ATTRIB_OFFSET as _,
                    &mut offsets[i],
                )
            };

            if cur_image != pixmap_buffer {
                unsafe { (image_ext.destroyImage.unwrap())(cur_image.as_ptr()) };
            }

            if ret == 0 {
                return Err(StaticMsg("Failed to query buffer attributes"));
            }
        }

        let mut modifier_upper = MaybeUninit::<c_int>::uninit();
        let mut ret = unsafe {
            (image_ext.queryImage.unwrap())(
                pixmap_buffer.as_ptr(),
                ffi::__DRI_IMAGE_ATTRIB_MODIFIER_UPPER as _,
                modifier_upper.as_mut_ptr(),
            )
        };
        let mut modifier_lower = MaybeUninit::<c_int>::uninit();
        ret &= unsafe {
            (image_ext.queryImage.unwrap())(
                pixmap_buffer.as_ptr(),
                ffi::__DRI_IMAGE_ATTRIB_MODIFIER_LOWER as _,
                modifier_lower.as_mut_ptr(),
            )
        };

        let modifier = if ret == 0 {
            DRM_CORRUPTED_MODIFIER
        } else {
            // SAFETY: if queryImage succeeded on both tries, both modifiers should contractually
            //         be fully init
            let upper = unsafe { modifier_upper.assume_init() } as u64;
            let lower = unsafe { modifier_lower.assume_init() } as u64;
            (upper << 32u64) | (lower & 0xffffffffu64)
        };

        // convert the strides and offsets array from c_int to u32
        macro_rules! cvt_array {
            ($arr: expr) => {
                [
                    $arr[0] as u32,
                    $arr[1] as u32,
                    $arr[2] as u32,
                    $arr[3] as u32,
                ]
            };
        }
        let strides = cvt_array!(strides);
        let offsets = cvt_array!(offsets);

        let pixmap = if drawable.multiplanes_available && modifier != DRM_CORRUPTED_MODIFIER {
            conn.pixmap_from_buffers(
                Window::const_from_xid(drawable.window.load(Ordering::Acquire)),
                width,
                height,
                depth,
                strides,
                offsets,
                (cpp as u8) * 8,
                modifier,
                buffer_fds.iter().map(|t| t.get()).collect(),
            )?
        } else {
            conn.pixmap_from_buffer(
                drawable.x_drawable,
                0,
                width,
                height,
                strides[0] as _,
                depth,
                (cpp as u8) * 8,
                buffer_fds[0].get(),
            )?
        };

        let sync_fence = conn.fence_from_fd(pixmap.into(), false, fence_fd)?;
        set_fence(shm_fence);

        mem::forget((fd_guard, shm_guard, image_guard, buffer_guard));

        // create the object proper
        unsafe {
            ptr::write(
                MaybeUninit::as_mut_ptr(
                    Arc::get_mut(&mut buffer).expect("Infallible Arc::get_mut()"),
                ),
                Dri3Buffer {
                    image,
                    linear_buffer,
                    own_pixmap: true,
                    pixmap,
                    shm_fence,
                    sync_fence,
                    cpp,
                    modifier,
                    reallocate: false,
                    busy: 0,
                    width,
                    height,
                    last_swap: AtomicU64::new(0),
                },
            )
        };
        Ok(unsafe { buffer.assume_init() })
    }

    /// Free the renderbuffer's data. This isn't done in a drop handle because we need the reference to
    /// the Dri3Drawable (and also because we block).
    #[inline]
    fn free<Dpy: DisplayLike>(mut self, drawable: &Dri3Drawable<Dpy>) -> breadx::Result
    where
        Dpy::Connection: Connection,
    {
        let mut conn = drawable.display.display();
        if self.own_pixmap {
            self.pixmap.free(&mut conn)?;
        }

        // free the sync fence
        conn.free_sync_fence(self.sync_fence)?;
        // free the shm fence
        let xshmfence = xshmfence()?;
        let unmap_shm: XshmfenceUnmapShm = unsafe { xshmfence.function(&XSHMFENCE_UNMAP_SHM) }
            .ok_or(StaticMsg("Failed to load xshmfence_unmap_shm"))?;
        unsafe { unmap_shm(self.shm_fence.as_ptr() as *mut _) };

        // destroy the buffers
        let image_ext = unsafe { &*drawable.screen().inner.image };
        unsafe { (image_ext.destroyImage.expect("destroyImage not present"))(self.image.as_ptr()) };
        if let Some(linear_buffer) = self.linear_buffer.take() {
            unsafe { (image_ext.destroyImage.unwrap())(linear_buffer.as_ptr()) };
        }

        // the destructor is just a signififer that we've error'd out
        mem::forget(self);
        Ok(())
    }
}

#[inline]
fn get_adaptive_sync_and_vblank_mode<Dpy: DisplayLike>(screen: &Dri3Screen<Dpy>) -> (c_uchar, c_int)
where
    Dpy::Connection: Connection,
{
    let mut adaptive_sync: c_uchar = 0;
    let mut vblank_mode: c_int = VBLANK_DEF_INTERVAL1;
    if !screen.inner.config.is_null() {
        unsafe {
            ((*screen.inner.config)
                .configQueryi
                .expect("configQueryi not present"))(
                screen.dri_screen().as_ptr(),
                &*VBLANK_MODE.as_ptr(),
                &mut vblank_mode,
            );
            ((*screen.inner.config)
                .configQueryb
                .expect("configQueryb not present"))(
                screen.dri_screen().as_ptr(),
                &*ADAPTIVE_SYNC.as_ptr(),
                &mut adaptive_sync,
            );
        }
    }
    (adaptive_sync, vblank_mode)
}

#[inline]
fn create_the_drawable<Dpy: DisplayLike>(
    screen: &Dri3Screen<Dpy>,
    config: &GlConfig,
    drawable: *const c_void,
) -> breadx::Result<DriDrawablePtr>
where
    Dpy::Connection: Connection,
{
    let config = match screen.driconfig_from_fbconfig(&config) {
        Some(config) => config.as_ptr(),
        None => {
            return Err(breadx::BreadError::StaticMsg(
                "Config doesn't match any in DRIconfig set",
            ))
        }
    };
    let dri_drawable = unsafe {
        ((*screen.inner.image_driver)
            .createNewDrawable
            .expect("createNewDrawable not present"))(
            screen.dri_screen().as_ptr(),
            config,
            drawable as *mut _,
        )
    };
    let dri_drawable = NonNull::new(dri_drawable).ok_or(breadx::BreadError::StaticMsg(
        "Failed createNewDrawable call",
    ))?;
    Ok(DriDrawablePtr(dri_drawable))
}

#[inline]
fn image_from_buffer<Dpy>(
    screen: &Dri3Screen<Dpy>,
    format: c_uint,
    mut bfp: BufferFromPixmapReply,
    dri_screen: ThreadSafe<*mut ffi::__DRIscreen>,
    loader: ThreadSafe<*const ()>,
) -> breadx::Result<ThreadSafe<NonNull<ffi::__DRIimage>>> {
    let mut offset = 0;
    let mut stride = bfp.stride as c_int;

    // createImageFromFds
    let image_planar = unsafe {
        ((&*screen.inner.image).createImageFromFds.unwrap())(
            dri_screen.into_inner(),
            bfp.width as _,
            bfp.height as _,
            image_format_to_fourcc(format),
            bfp.pixmap_fd.as_mut_ptr() as *mut _,
            1,
            &mut stride,
            &mut offset,
            loader.into_inner() as *mut _,
        )
    };
    unsafe { libc::close(bfp.pixmap_fd[0]) };

    if image_planar.is_null() {
        return Err(StaticMsg("Failed to create image from fd"));
    }

    let ret = unsafe {
        ((&*screen.inner.image).fromPlanar.unwrap())(image_planar, 0, loader.into_inner() as *mut _)
    };
    match NonNull::new(ret) {
        // SAFETY: __DRIimage is thread safe if I recall correctly
        Some(ret) => Ok(unsafe { ThreadSafe::new(ret) }),
        None => unsafe {
            ((&*screen.inner.image).destroyImage.unwrap())(image_planar);
            Ok(unsafe { ThreadSafe::new(NonNull::new_unchecked(image_planar)) })
        },
    }
}

#[inline]
fn image_from_buffers<Dpy>(
    screen: &Dri3Screen<Dpy>,
    format: c_uint,
    mut bfp: BuffersFromPixmapReply,
    dri_screen: ThreadSafe<*mut ffi::__DRIscreen>,
    loader: ThreadSafe<*const ()>,
) -> breadx::Result<ThreadSafe<NonNull<ffi::__DRIimage>>> {
    let mut strides: [c_int; 4] = bfp
        .strides
        .iter()
        .map(|i| *i as c_int)
        .take(4)
        .collect::<ArrayVec<[c_int; 4]>>()
        .into_inner();
    let mut offsets: [c_int; 4] = bfp
        .offsets
        .iter()
        .map(|i| *i as c_int)
        .take(4)
        .collect::<ArrayVec<[c_int; 4]>>()
        .into_inner();
    let mut error: MaybeUninit<c_uint> = MaybeUninit::uninit();

    let ret = unsafe {
        ((&*screen.inner.image).createImageFromDmaBufs2.unwrap())(
            dri_screen.into_inner(),
            bfp.width as _,
            bfp.height as _,
            image_format_to_fourcc(format),
            bfp.modifier,
            bfp.buffers.as_mut_ptr(), // SAFETY: shouldn't actually modify the buffers
            bfp.nfd as _,
            strides.as_mut_ptr(),
            offsets.as_mut_ptr(),
            0,
            0,
            0,
            0,
            error.as_mut_ptr(),
            loader.into_inner() as *mut _,
        )
    };

    // close all of our fds
    bfp.buffers.iter().for_each(|fd| unsafe {
        libc::close(*fd);
    });

    match NonNull::new(ret) {
        Some(ret) => Ok(unsafe { ThreadSafe::new(ret) }),
        None => Err(StaticMsg("Failed to create image from buffers")),
    }
}

#[inline]
fn set_fence(fence: NonNull<c_void>) {
    let xshmfence = xshmfence().expect("Failed to load xshmfence"); // should be infallible
    let trigger: XshmfenceTrigger =
        unsafe { xshmfence.function(&*XSHMFENCE_TRIGGER) }.expect("xshmfence_trigger not found");
    unsafe { (trigger)(fence.as_ptr()) };
}

#[inline]
fn reset_fence(fence: NonNull<c_void>) {
    let xshmfence = xshmfence().expect("Infallible!");
    let reset: XshmfenceReset =
        unsafe { xshmfence.function(&*XSHMFENCE_RESET) }.expect("xshmfence_reset found found");
    unsafe { (reset)(fence.as_ptr()) };
}

#[inline]
fn trigger_fence<Dpy: DisplayLike>(
    conn: &mut Display<Dpy::Connection>,
    fence: Fence,
) -> breadx::Result<()>
where
    Dpy::Connection: Connection,
{
    conn.trigger_fence(fence)
}

#[inline]
fn block_on_fence<Dpy: DisplayLike>(
    conn: &mut Display<Dpy::Connection>,
    drawable: Option<&Dri3Drawable<Dpy>>,
    buffer: &Dri3Buffer,
) -> breadx::Result<()>
where
    Dpy::Connection: Connection,
{
    let xshmfence = xshmfence().expect("Infallible!");
    let xawait: XshmfenceAwait =
        unsafe { xshmfence.function(&*XSHMFENCE_AWAIT) }.expect("xshmfence_await not found");
    unsafe { (xawait)(buffer.shm_fence.as_ptr()) };

    if let Some(drawable) = drawable {
        log::trace!("Borrowing guard for drawable");
        let mut guard = drawable.state();
        drawable.process_present_events(conn, &mut *guard)?;
    }

    Ok(())
}

#[repr(transparent)]
struct DriDrawablePtr(NonNull<ffi::__DRIdrawable>);

unsafe impl Send for DriDrawablePtr {}
unsafe impl Sync for DriDrawablePtr {}

const VARIABLE_REFRESH: &str = "_VARAIBLE_REFRESH";

#[inline]
pub fn free_buffer_arc<Dpy: DisplayLike>(
    buffer: Arc<Dri3Buffer>,
    draw: &Dri3Drawable<Dpy>,
) -> breadx::Result<()>
where
    Dpy::Connection: Connection,
{
    let mut bufarc = Some(buffer);
    'tryunwraploop: loop {
        match Arc::try_unwrap(bufarc.take().unwrap()) {
            Ok(buffer) => {
                buffer.free(draw)?;
                break 'tryunwraploop Ok(());
            }
            Err(buffer) => {
                log::error!("Hopefully infallible Arc::try_unwrap() failed.");
                log::error!("This likely means the internal state is corrupted.");
                log::error!("Trying again.");
                bufarc = Some(buffer);
                hint::spin_loop();
            }
        }
    }
}

#[inline]
fn set_adaptive_sync<Conn: Connection>(
    dpy: &mut Display<Conn>,
    drawable: Drawable,
    val: bool,
) -> breadx::Result<()> {
    let window: Window = Window::const_from_xid(drawable.xid);
    let variable_refresh = dpy.intern_atom_immediate(VARIABLE_REFRESH.to_string(), false)?;

    if val {
        window.change_property::<_, u32>(
            dpy,
            variable_refresh,
            PropertyType::Atom,
            PropertyFormat::ThirtyTwo,
            PropMode::Replace,
            &[1],
        )
    } else {
        window.delete_property(dpy, variable_refresh)
    }
}

#[inline]
fn linear_format<Conn>(dpy: &mut Display<Conn>, format: u32) -> u32 {
    match format {
        ffi::__DRI_IMAGE_FORMAT_XRGB2101010 | ffi::__DRI_IMAGE_FORMAT_XBGR2101010 => {
            if red_mask_for_depth(dpy, 30) == 0x3ff {
                ffi::__DRI_IMAGE_FORMAT_XBGR2101010
            } else {
                ffi::__DRI_IMAGE_FORMAT_XRGB2101010
            }
        }
        ffi::__DRI_IMAGE_FORMAT_ARGB2101010 | ffi::__DRI_IMAGE_FORMAT_ABGR2101010 => {
            if red_mask_for_depth(dpy, 30) == 0x3ff {
                ffi::__DRI_IMAGE_FORMAT_ABGR2101010
            } else {
                ffi::__DRI_IMAGE_FORMAT_ARGB2101010
            }
        }
        format => format,
    }
}

#[inline]
fn red_mask_for_depth<Conn>(dpy: &mut Display<Conn>, depth: u8) -> c_uint {
    dpy.setup()
        .roots
        .iter()
        .flat_map(|s| s.allowed_depths.iter())
        .find_map(|d| {
            if d.depth == depth {
                Some(d.visuals.first()?.red_mask as _)
            } else {
                None
            }
        })
        .unwrap_or(0)
}

#[cfg(feature = "async")]
#[inline]
async fn set_adaptive_sync_async<Conn: Connection>(
    dpy: &mut Display<Conn>,
    drawable: Drawable,
    val: bool,
) -> breadx::Result<()> {
    let window: Window = Window::const_from_xid(drawable.xid);
    let variable_refresh = dpy
        .intern_atom_immediate_async(VARIABLE_REFRESH.to_string(), false)
        .await?;

    if val {
        window
            .change_property_async::<_, u32>(
                dpy,
                variable_refresh,
                PropertyType::Atom,
                PropertyFormat::ThirtyTwo,
                PropMode::Replace,
                &[1u32],
            )
            .await
    } else {
        window.delete_property_async(dpy, variable_refresh).await
    }
}

#[inline]
const fn cpp_for_format(format: c_uint) -> Option<u32> {
    Some(match format {
        // __DRI_IMAGE_FORMAT_R8
        4102 => 1,
        // __DRI_IMAGE_FORMAT_RGB565, _GR88
        4097 | 4103 => 2,
        // XRGB8888, ARGB8888, ABGR8888, XBGR8888, XRGB2101010, ARGB2101010, XBGR2101010,
        // ABGR2101010, SARGB8, SABGR8, SXRGB8,
        4098 | 4099 | 4100 | 4101 | 4106 | 4106 | 4112 | 4113 | 4107 | 4114 | 4118 => 4,
        // XBGR16161616F, ABGR16161616F
        4116 | 4117 => 8,
        _ => return None,
    })
}

#[inline]
const fn image_format_to_fourcc(format: c_uint) -> c_int {
    #[inline]
    const fn fourcc_code(a: char, b: char, c: char, d: char) -> c_int {
        // should be translated into a compile error
        //assert!(a.is_ascii() && b.is_ascii() && c.is_ascii() && d.is_ascii());
        ((a as u32) | ((b as u32) << 8) | ((c as u32) << 16) | ((d as u32) << 24)) as c_int
    }

    const DRM_FORMAT_RGB565: c_int = fourcc_code('R', 'G', '1', '6');
    const DRM_FORMAT_XRGB8888: c_int = fourcc_code('X', 'R', '2', '4');
    const DRM_FORMAT_ARGB8888: c_int = fourcc_code('A', 'R', '2', '4');
    const DRM_FORMAT_ABGR8888: c_int = fourcc_code('A', 'B', '2', '4');
    const DRM_FORMAT_XBGR8888: c_int = fourcc_code('X', 'B', '2', '4');
    const DRM_FORMAT_XRGB2101010: c_int = fourcc_code('X', 'R', '3', '0');
    const DRM_FORMAT_ARGB2101010: c_int = fourcc_code('A', 'R', '3', '0');
    const DRM_FORMAT_ABGR2101010: c_int = fourcc_code('A', 'B', '3', '0');
    const DRM_FORMAT_XBGR2101010: c_int = fourcc_code('X', 'B', '3', '0');
    const DRM_FORMAT_XBGR16161616F: c_int = fourcc_code('X', 'B', '4', 'H');
    const DRM_FORMAT_ABGR16161616F: c_int = fourcc_code('A', 'B', '4', 'H');

    match format {
        // __DRI_IMAGE_FORMAT_SARGB8
        4107 => ffi::__DRI_IMAGE_FOURCC_SARGB8888 as _,
        // __DRI_IMAGE_FORMAT_SABGR8
        4114 => ffi::__DRI_IMAGE_FOURCC_SABGR8888 as _,
        // __DRI_IMAGE_FORMAT_SXRGB8
        4118 => ffi::__DRI_IMAGE_FOURCC_SXRGB8888 as _,
        // __DRI_IMAGE_FORMAT_RGB565,
        4097 => DRM_FORMAT_RGB565,
        // __DRI_IMAGE_FORMAT_XRGB8888
        4098 => DRM_FORMAT_XRGB8888,
        // __DRI_IMAGE_FORMAT_ARGB8888
        4099 => DRM_FORMAT_ARGB8888,
        // __DRI_IMAGE_FORMAT_ABGR8888
        4100 => DRM_FORMAT_ABGR8888,
        // __DRI_IMAGE_FORMAT_XBGR8888
        4101 => DRM_FORMAT_XBGR8888,
        // __DRI_IMAGE_FORMAT_XRGB2101010
        4105 => DRM_FORMAT_XRGB2101010,
        // __DRI_IMAGE_FORMAT_ARGB2101010
        4106 => DRM_FORMAT_ARGB2101010,
        // __DRI_IMAGE_FORMAT_XBGR2101010
        4112 => DRM_FORMAT_XBGR2101010,
        // __DRI_IMAGE_FORMAT_ABGR2101010
        4113 => DRM_FORMAT_ABGR2101010,
        // __DRI_IMAGE_FORMAT_XBGR16161616F
        4116 => DRM_FORMAT_XBGR16161616F,
        // __DRI_IMAGE_FORMAT_ABGR16161616F
        4117 => DRM_FORMAT_ABGR16161616F,
        _ => 0,
    }
}

impl<Dpy> Drop for Dri3Drawable<Dpy> {
    #[inline]
    fn drop(&mut self) {
        // TODO
    }
}

impl Drop for Dri3Buffer {
    #[inline]
    fn drop(&mut self) {
        log::error!("Dropping a Dri3Buffer! This should never happen! Should use free()!");
    }
}
