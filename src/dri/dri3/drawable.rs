// MIT/Apache2 License

// TODO: this file is a literal trainwreck. seperate it into several files if possible, comment the
//       whole thing, and maybe clean it all up

use super::{Dri3Context, Dri3Screen, WeakDri3ScreenRef};
use crate::{
    config::GlConfig,
    context::{promote_anyarc_ref, GlContext, GlInternalContext},
    cstr::{const_cstr, ConstCstr},
    display::{DisplayLike, DisplayLock, GlDisplay},
    dri::ffi,
    mesa::xshmfence,
    util::CallOnDrop,
};
use breadx::{
    auto::{present::EventMask as PresentEventMask, sync::Fence},
    display::{Connection, Display, Modifiers},
    BreadError::StaticMsg,
    Drawable, Event, GcParameters, Pixmap, PropMode, PropertyFormat, PropertyType, Window,
};
use std::{
    cmp,
    ffi::c_void,
    future::Future,
    hint,
    mem::{self, MaybeUninit},
    num::NonZeroI64,
    os::raw::{c_int, c_uchar},
    pin::Pin,
    ptr::{self, NonNull},
    sync::{
        self,
        atomic::{AtomicBool, AtomicI32, AtomicU16, AtomicU32, Ordering},
        Arc,
    },
    task::{Context, Poll, Waker},
};

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
    present_capabilities: AtomicU32,
    eid: AtomicU32,
    is_initialized: AtomicBool,
    window: AtomicU32,
    gc: AtomicU32,
    has_fake_front: AtomicBool,

    swap_interval: AtomicI32,
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
    notify_mst: u64,
    notify_ust: u64,
    mst: u64,
    ust: u64,
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
    image: NonNull<ffi::__DRIimage>,
    linear_buffer: Option<NonNull<ffi::__DRIimage>>,

    sync_fence: Fence,
    shm_fence: NonNull<c_void>,

    cpp: u32,
    modifier: u64,

    // we need to reallocate
    reallocate: bool,

    busy: i32,
    pixmap: Pixmap,
    own_pixmap: bool,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum BufferType {
    Front,
    Back,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
struct SwapBufferCount {
    ust: i64,
    msc: i64,
    sbc: i64,
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
    async fn free_async(self) {
        blocking::unblock(move || self.free())
    }
}

#[inline]
fn get_blit_context_internal(draw: &Dri3Drawable, lock: &mut Option<BlitContext>) -> CtxPtr {
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
            ((&*core).createContext.unwrap())(
                scr.dri_screen(),
                ptr::null(),
                ptr::null(),
                ptr::null(),
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
fn get_blit_context(draw: &Dri3Drawable) -> (CtxPtr, MutexGuard<'static, BlitContext>) {
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
async fn get_blit_context_async(
    draw: Arc<Dri3Drawable>,
) -> (CtxPtr, MutexGuard<'static, BlitContext>) {
    let mut blit_context = BLIT_CONTEXT.lock().await;
    let draw = draw.clone();

    blocking::unblock(move || {
        (
            get_blit_context_internal(&draw, &mut *blit_context),
            blit_context,
        )
    })
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
                ($arr)
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
                let mut ust = [0u64; 8];
                ust.copy_from_slice(&bytes[25..32]);
                let ust = u64::from_ne_bytes(ust);
                // mst is at bytes 36 thru 44
                let mut mst = [0u64; 8];
                mst.copy_from_slice(&bytes[36..44]);
                let mst = u64::from_ne_bytes(mst);
                // kind is at byte 10
                match bytes[10] {
                    0 => {
                        let recv_sbc = (state.send_sbc & 0xFFFFFFFF00000000u64) | (serial as u64);

                        if recv_sbc <= state.send_sbc {
                            state.recv_sbc = recv_sbc;
                        } else if recv_sbc == state.recv_sbc.wrapping_add(0x100000001u64) {
                            state.recv_sc = recv_sbc.wrapping_sub(0x100000000u64);
                        }

                        let mode = bytes[11];
                        if (mode == PRESENT_MODE_COPY
                            && state.last_present_mode == PRESENT_MODE_FLIP)
                            || (mode == PRESENT_MODE_SUBOPTIMAL_COPY
                                && state.last_present_mode != PRESENT_MODE_SUBOPTIMAL_COPY)
                        {
                            state.buffers.iter_mut().for_each(|buffer| {
                                if let Some(buffer) = buffer.as_mut() {
                                    buffer.reallocate = true
                                }
                            });
                        }

                        state.last_present_mode = mode;
                        state.ust = ust;
                        state.mst = mst;
                    }
                    _ => {
                        if self.eid.load(Ordering::Acquire).xid == serial {
                            state.notify_ust = ust;
                            state.notify_mst = msg;
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
                            buffer.busy = 0;
                        }
                    }
                });
            }
            _ => (),
        }

        Ok(false)
    }

    #[inline]
    fn state(&self) -> MutexGuard<'_, DrawableState> {
        cfg_if! {
            if #[cfg(feature = "async")] {
                self.state.lock().expect(STATE_LOCK_FAILED)
            } else {
                future::block_on(self.state.lock())
            }
        }
    }

    #[cfg(feature = "async")]
    #[inline]
    async fn state_async(&self) -> MutexGuard<'_, DrawableState> {
        self.state.lock().await
    }

    #[inline]
    fn event_wait(&self, guard: MutexGuard<'_, DrawableState>) -> MutexGuard<'_, DrawableState> {
        #[cfg(not(feature = "async"))]
        {
            self.event_waiter
                .wait(guard)
                .expect("Failed to wait for present events")
        }
        #[cfg(feature = "async")]
        {
            mem::drop(guard);
            self.event_waiter.listen().wait();
            future::block_on(self.state.lock())
        }
    }

    #[cfg(feature = "async")]
    #[inline]
    async fn event_wait_async(
        &self,
        guard: MutexGuard<'_, DrawableState>,
    ) -> MutexGuard<'_, DrawableState> {
        mem::drop(guard);
        self.event_waiter.listen().await;
        self.state.lock().await
    }

    #[inline]
    fn event_broadcast(&self) {
        #[cfg(not(feature = "async"))]
        {
            self.event_waiter
                .notify_all()
                .expect("Failed to wake condvar")
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
        conn: &mut Display<Dpy::Conn>,
        state_lock: &mut DrawableState,
    ) -> breadx::Result<bool> {
        // use an iterator to handle the events
        let needs_invalidate = conn
            .get_special_events()
            .map(|event| self.process_present_event(state_lock, event))
            .collect::<breadx::Result<Vec<bool>>>()?;
        Ok(needs_invalidate.iter().any(|b| *b))
    }
}

impl<Dpy: DisplayLike> Dri3Drawable<Dpy>
where
    Dpy::Conn: Connection,
{
    #[inline]
    pub fn new(
        dpy: &GlDisplay<Dpy>,
        drawable: Drawable,
        screen: Dri3Screen<Dpy>,
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
        let geometry = breadx::drawable::get_geometry_immediate(&mut *dpy.display(), drawable)?;

        let mut this = Arc::new(Self {
            drawable: NonNull::dangling(),
            x_drawable: drawable,
            config,
            is_different_gpu: screen.inner.is_different_gpu,
            multiplanes_available: has_multiplanes,
            screen: screen.weak_ref(),
            width: AtomicU16::new(geometry.width),
            height: AtomicU16::new(geometry.height),
            eid: AtomicU32::new(0),
            is_initialized: AtomicBool::new(false),
            present_capabilities: AtomicU32::new(0),
            window: AtomicU32::new(0),
            gc: AtomicU32::new(0),
            swap_interval: AtomicI32::new(swap_interval as _),
            is_pixmap: false,
            display: dpy.clone(),
            #[cfg(feature = "async")]
            state: async_lock::Mutex::new(Default::default()),
            #[cfg(not(feature = "async"))]
            state: sync::Mutex::new(Default::default()),
            has_event_waiter: false,
            #[cfg(feature = "async")]
            event_waiter: event_listener::Event::new(),
            #[cfg(not(feature = "async"))]
            event_waiter: sync::Condvar::new(),
        });

        // create the drawable pointer
        let dri_drawable =
            create_the_drawable(&this.screen, &this.config, Arc::as_ptr(&this) as _)?;
        Arc::get_mut(&mut this)
            .expect("Infallible Arc::get_mut()")
            .drawable = dri_drawable.0;

        Ok(this)
    }

    #[inline]
    fn drawable_gc(&self, conn: &mut Display<Dpy::Conn>) {
        let mut gc = self.gc.load(Ordering::Acquire);
        if gc == 0 {
            gc = conn.create_gc(
                self.x_drawable,
                GcParameters {
                    graphics_exposures: Some(0),
                    ..Default::default()
                },
            )?;
            self.gc.store(gc, Ordering::Release);
        }
        gc
    }

    /// Wait for present events to occur.
    #[inline]
    fn wait_for_event(
        &self,
        state_lock: &mut Option<MutexGuard<'_, DrawableState>>,
    ) -> breadx::Result<()> {
        if self.has_event_waiter.load(Ordering::SeqCst) {
            // another thread is polling for events for this drawable, wait a minute
            let sl = state_lock.take().expect("Non-exclusive lock?!?!");
            *state_lock = Some(self.event_wait(sl));
            Ok(())
        } else {
            self.has_event_waiter.store(true, Ordering::SeqCst);
            // drop the lock, then poll the display, then re-acquire the lock
            mem::drop(state_lock.take());
            let res = self
                .display
                .display()
                .wait_for_special_event(self.eid.load(Ordering::Relaxed))?;
            *state_lock = Some(self.state());
            self.has_event_waiter.store(false, Ordering::SeqCst);
            self.event_broadcast();
            let event = res?;

            if self.process_event(event)? {
                self.invalidate();
            }

            Ok(())
        }
    }

    #[inline]
    fn wait_for_sbc(&self, target_sbc: Option<NonZeroI64>) -> breadx::Result<SwapBufferCount> {
        let mut state = self.state();
        let target_sbc = match target_sbc {
            Some(tsbc) => tsbc.get(),
            None => state.send_sbc,
        };

        let mut state = Some(state);

        while state.recv_sbc < target_sbc {
            self.wait_for_event(&mut Some(state))?;
        }

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
    fn find_back(
        &self,
        mut conn: DisplayLock<'_, Dpy>,
        state_lock: &mut Option<MutexGuard<'_, DrawableState>>,
    ) -> breadx::Result<i32> {
        if self.process_present_events(&mut *conn, state_lock)? {
            self.invalidate();
        }
        mem::drop(conn); // we need to poll it later

        let mut state = state_lock.as_deref_mut().expect("Infallible");
        let (mut num_to_consider, max_num) = if self.has_blit_image() {
            (state.cur_num_back, state.max_num_back)
        } else {
            state.cur_blit_source = -1;
            (1, 1)
        };

        loop {
            for i in 0..num_to_consider {
                let id = back_id((i + state.cur_back) * state.cur_num_back);
                match &mut state.buffers[id] {
                    None | Some(buffer { busy: 0, .. }) => {
                        state.cur_back = id;
                        return id;
                    }
                }
            }

            if num_to_consider < max_num {
                state.cur_num_back += 1;
                num_to_consider = state_lock.cur_num_back;
            } else {
                // wait for an event
                self.wait_for_event(state_lock)?;
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
            flush_flag |= ffi::__BLIT_FLAG_FLUSH;
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
        let mut state = self.state();
        let (first_id, n_ids) = match buffer_type {
            BufferType::Back => {
                state.cur_bit_source = -1;
                (back_id(0), MAX_BACK)
            }
            BufferType::Front => (
                FRONT_ID,
                if state.cur_blit_source == FRONT_ID {
                    0
                } else {
                    1
                },
            ),
        };

        (first_id..first_id + n_ids).try_for_each(|i| {
            if let Some(buffer) = state.buffers[i].take() {
                free_buffer_arc(buffer, self)?;
            }
            Ok(())
        })?;

        Ok(())
    }

    #[inline]
    fn buffer_id(&self, ty: BufferType) -> breadx::Result<usize> {
        match ty {
            BufferType::Back => self.find_back(self.display.display(), &mut Some(self.state()))?,
            BufferType::Front => FRONT_ID,
        }
    }

    /// Get the buffer associated with the given format and buffer type.
    #[inline]
    pub fn get_buffer(
        &self,
        buffer_type: BufferType,
        format: c_uint,
    ) -> breadx::Result<Arc<Dri3Buffer>> {
        let buf_id = self.buffer_id(buffer_type)?;

        // see if there is a buffer; if there isn't a buffer (or if there is, but it's wrong),
        // rellocate it
        let width = self.width.load(Ordering::SeqCst);
        let height = self.height.load(Ordering::SeqCst);
        let mut state = Some(self.state());
        let mut conn = self.display.display();
        let mut fence_await = false;

        let buffer = match state.as_ref().unwrap().buffers[buf_id] {
            None | Some(ref buffer)
                if buffer.reallocate || buffer.width != width || buffer.height != height =>
            {
                // create a new buffer
                let mut new_buffer = Dri3Buffer::new(
                    self,
                    format,
                    width,
                    height,
                    self.depth.load(Ordering::SeqCst),
                )?;

                let buffer = state.as_ref().unwrap().buffers[buf_id].take();
                if buffer.is_some()
                    && (!matches!(buffer_type, BufferType::Front)
                        || self.have_fake_front.load(Ordering::Acquire))
                {
                    let buffer = buffer.unwrap();
                    if self
                        .blit_images(
                            new_buffer.image,
                            buffer.image,
                            0,
                            0,
                            cmp::min(buffer.width, new_buffer.width),
                            cmp::min(buffer.height, new_buffer.height),
                            0,
                            0,
                            0,
                        )
                        .is_err()
                        && buffer.linear_buffer.is_none()
                    {
                        fence_reset(new_buffer.shm_fence);
                        gc = self.drawable_gc(&mut *conn)?;
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
                        fence_trigger(&mut conn, new_buffer.sync_fence)?;
                        fence_await = true;
                    }

                    if let Some(buffer) = buffer {
                        mem::drop(conn);
                        free_buffer_arc(buffer, self)?;
                    }
                } else if matches!(buffer_type, BufferType::Front) {
                    // fill the new fake front with data from the real front
                    mem::drop((conn, state.take()));
                    self.swapbuffer_barrier();
                    let mut conn = self.display.display();
                    fence_reset(new_buffer.shm_fence);
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
                    fence_trigger(&mut conn, new_buffer.sync_buffer);

                    if let Some(linear_buffer) = new_buffer.linear_buffer {
                        block_on_fence(&mut *conn, Some(self), &new_buffer)?;
                        self.blit_images(
                            new_buffer.image.into(),
                            new_buffer.linear_buffer.into(),
                            gc,
                            0,
                            0,
                            width,
                            height,
                            0,
                            0,
                        )?;
                    } else {
                        fence_await = true;
                    }
                }

                state = Some(self.state());
                let new_buffer = Arc::new(new_buffer);
                state.as_mut().unwrap().buffers[buf_id] = Some(new_buffer.clone());
                new_buffer
            }
            Some(ref buffer) => {
                mem::drop(conn);
                buffer.clone()
            }
        };

        if fence_await {
            block_on_fence(&mut *self.display.display(), Some(self), &buffer)?;
        }

        // if we need to preserve the content of previous buffers...
        let mut state = state.unwrap();
        if matches!(buffer_type, BufferType::Back)
            && state.cur_blit_source != -1
            && state.buffers[state.cur_blit_source]
            && !Arc::ptr_eq(&buffer, &state.buffers[state.cur_blit_source])
        {
            self.blit_images(
                buffer.image,
                state.buffers[state.cur_blit_source].image,
                0,
                0,
                width,
                height,
                0,
                0,
                0,
            )?;
            buffer.last_swap = state.buffers[state.cur_blit_source].last_swap;
            state.cur_blit_source = -1;
        }

        Ok(buffer)
    }

    #[inline]
    fn get_pixmap_buffer(
        &self,
        buffer_type: BufferType,
        format: c_uint,
    ) -> breadx::Result<Arc<Dri3Buffer>> {
        let buf_id = self.buffer_id(buffer_type)?;
        if let Some(buffer) = self.state().buffers[buf_id].as_ref().cloned() {
            return Ok(buffer);
        }

        // TODO: a lot of stuff is reused here from Dri3Buffer::new(). consolidate it into its
        // own function

        let xshmfence = xshmfence()?;
        let alloc_shm: XshmfenceAllocShm = unsafe { xshmfence.symbol(&XSHMFENCE_ALLOC_SHM) }
            .expect("xshmfence_alloc_shm not present");
        let map_shm: XshmfenceMapShm =
            unsafe { xshmfence.symbol(&XSHMFENCE_MAP_SHM) }.expect("xshmfence_map_shm not present");
        let unmap_shm: XshmfenceUnmapShm =
            unsafe { xshmfence.symbol(&XSHMFENCE_UNMAP_SHM) }.expect("xshmfence_unmap_shm");

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

        let fence_guard = CallOnDrop(|| {
            unsafe { (unmap_shm)(shm_fence.as_ptr()) };
            unsafe { libc::close(fence_fd) };
        });

        mem::forget(fence_guard);
    }

    #[inline]
    pub fn invalidate(&self) {
        self.invalidate_internal()
    }

    /// Update this drawable.
    #[inline]
    pub fn update(&self) -> breadx::Result {
        let guard = self.state();

        // acquire a lock on the display
        let mut conn = self.display.display();

        if !self.is_initialized.load(Ordering::Acquire) {
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
            let select_input_tok = conn.present_select_input(
                eid,
                self.drawable,
                PresentEventMask::CONFIGURE_NOTIFY
                    | PresentEventMask::COMPLETE_NOTIFY
                    | PresentEventMask::IDLE_NOTIFY,
            )?;
            let capabilities_tok = conn.present_capabilities(self.drawable)?;
            let geometry_tok = conn.get_drawable_geometry(self.drawable)?;

            // match the error on the geometry and select input results, if they are
            // BadWindow, this is a pixmap
            let select_input = conn.resolve_request(select_input_tok);
            let geometry = conn.resolve_request(geometry_tok)?;

            self.capabilities.store(
                match conn.resolve_request(capabilities_tok) {
                    Ok(cap) => cap.capabilities,
                    Err(_) => 0,
                },
                Ordering::Relaxed,
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
        let mut state = self.state();

        let swap_interval = self.swap_interval.load(Ordering::Acquire);
        state.update_max_back(swap_interval);
    }

    #[inline]
    pub fn has_supported_modifier(&self, format: c_uint, modifiers: &[u64]) -> bool {
        let query_dma_bufs = match unsafe { &*self.screen.inner.image }.queryDmaBufModifiers {
            Some(qdb) => qdb,
            None => return false,
        };

        // first, get the actual number of supported modifiers
        let mut mod_count = MaybeUninit::<i32>::uninit();
        if unsafe {
            query_dma_bufs(
                self.dri_drawable().as_ptr(),
                format,
                0,
                ptr::null_mut(),
                ptr::null_mut(),
                mod_count.as_mut_ptr(),
            )
        } == 0
        {
            return false;
        }
        let mut mod_count = unsafe { MaybeUninit::assume_init(mod_count) };
        if mod_count == 0 {
            return false;
        }

        // then query for modifiers, now that we know we have enough memory to store it
        let mut modifiers = Box::<[u64]>::new_uninit_slice(mod_count as usize);
        unsafe {
            query_dma_bufs(
                self.dri_drawable().as_ptr(),
                format,
                mod_count as i32,
                ptr::null_mut(),
                &mut mod_count,
            )
        };

        modifiers
            .into_iter()
            .flat_map(|i| modifiers.iter().map(|j| (i, j)))
            .find(|(i, j)| i == j)
            .is_some()
    }
}

#[cfg(feature = "async")]
impl<Dpy: DisplayLike> Dri3Drawable<Dpy>
where
    Dpy::Conn: AsyncConnection + Send,
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
        Dpy::Conn: Connection,
    {
        // TODO: this function is absolutely massive, it's not even funny. break it up into smaller
        //       functions if we have a chance

        let xshmfence = xshmfence()?;
        let alloc_shm: XshmfenceAllocShm = unsafe { xshmfence.symbol(&XSHMFENCE_ALLOC_SHM) }
            .expect("xshmfence_alloc_shm not present");
        let map_shm: XshmfenceMapShm =
            unsafe { xshmfence.symbol(&XSHMFENCE_MAP_SHM) }.expect("xshmfence_map_shm not present");
        let unmap_shm: XshmfenceUnmapShm =
            unsafe { xshmfence.symbol(&XSHMFENCE_UNMAP_SHM) }.expect("xshmfence_unmap_shm");

        // create an xshm object
        let fence_fd = unsafe { (alloc_shm)() };
        if fence_fd < 0 {
            return Err(StaticMsg("Failed to allocate XSHM Fence"));
        }

        // we set up a variety of CallOnDrop objects that destroy the file descriptors if
        // the function errors out
        let fd_guard = CallOnDrop::new(|| unsafe { libc::close(fence_fd) });

        let shm_fence = unsafe { (map_shm)(fence_fd) };
        let shm_fence = match NonNull::new(shm_fence) {
            Some(shm_fence) => shm_fence,
            None => return Err(StaticMsg("Failed to map XSHM Fence")),
        };

        let shm_guard = CallOnDrop::new(move || unsafe { (unmap_shm)(shm_fence) });

        let cpp = cpp_for_format(format).ok_or(StaticMsg("failed to find cpp for format"))?;

        // allocate the memory necessary for the buffer ahead of time
        // TODO: as far as I know, the memory isn't actually used for any loaders and the loader
        //       parameter is used mostly just in case we need to do it in the future. so that's what
        //       we do
        let mut buffer = Arc::<MaybeUninit<Dri3Buffer>>::new_uninit();
        let mut conn = drawable.display.display();
        let screen = draw.screen();

        // we use the image extension pretty heavily up above
        let image_ext = match unsafe { draw.screen().inner.image.as_ref() } {
            Some(r) => r,
            None => return Err(StaticMsg("No image extension!")),
        };

        let (image, linear_buffer, pixmap_buffer) = if !drawable.is_different_gpu {
            let mut image = ptr::null_mut();

            // check to see if we can use modifiers
            if draw.multiplanes_available
                && image_ext.base.version >= 15
                && image_ext.queryDmaBufModifiers.is_some()
                && image_ext.createImageWithModifiers.is_some()
            {
                let x_modifiers = conn.get_supported_modifiers_immediate()?;
                let mut modifiers: Option<Vec<u64>> = None;

                if !x_modifiers.window.is_empty() {
                    if drawable
                        .has_supported_modifier(image_format_to_fourcc(format), &x_modifiers.window)
                    {
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
                            drawable.screen().dri_screen(),
                            width as _,
                            height as _,
                            format,
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
                        drawable.screen.dri_screen(),
                        width as _,
                        height as _,
                        format,
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
                    drawable.screen.dri_screen(),
                    width as _,
                    height as _,
                    format,
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
                    drawable.screen.dri_screen(),
                    width as _,
                    height as _,
                    draw.linear_format(format),
                    ffi::__DRI_IMAGE_USE_SHARE
                        | ffi::__DRI_IMAGE_USE_LINEAR
                        | ffi::__DRI_IMAGE_USE_BACKBUFFER,
                    buffer.as_ptr() as *const _ as *mut _,
                )
            };

            let linear_buffer = match NonNull::new(linear_buffer) {
                Some(linear_buffer) => linear_buffer,
                None => {
                    unsafe { (image_ext.destroyImage.unwrap())(image) };
                    return Err(StaticMsg("createImage returned null"));
                }
            };

            (image, Some(linear_buffer), linear_buffer)
        };

        // destroy the images if we exit early
        let image_guard = CallOnDrop::new(|| {
            let image_driver = unsafe { &*drawable.screen.inner.image };
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
                ffi::__DRI_IMAGE_ATTRIB_NUM_PLANES,
                plane_num.as_mut_ptr(),
            )
        } {
            0 => 1,
            _ => unsafe { plane_num.assume_init() },
        };

        let mut buffer_fds: Vec<c_int> = iter::repeat(-1).take(4).collect();
        let mut strides: [c_int; 4] = [0; 4];
        let mut offsets: [c_int; 4] = [0; 4];
        let buffer_guard = CallOnDrop::new(|| {
            buffer_fds.iter().for_each(|fd| {
                if fd != -1 {
                    unsafe { libc::close(fd) };
                }
            });
        });

        for i in 0..plane_num {
            let cur_image = unsafe {
                (image.fromPlanar.expect("fromPlanar not present"))(
                    pixmap_buffer.as_ptr(),
                    i,
                    ptr::null(),
                )
            };
            let cur_image = match NonNull::new(cur_image) {
                Some(cur_image) => cur_image,
                None => {
                    assert_eq!(i, 0);
                    pixmap_buffer
                }
            };

            let mut ret = unsafe {
                (image.queryImage.unwrap())(
                    cur_image.as_ptr(),
                    ffi::__DRI_IMAGE_ATTRIB_FD,
                    &mut buffer_fds[i],
                )
            };
            ret &= unsafe {
                (image.queryImage.unwrap())(
                    cur_image.as_ptr(),
                    ffi::__DRI_IMAGE_ATTRIB_STRIDE,
                    &mut strides[i],
                )
            };
            ret &= unsafe {
                (image.queryImage.unwrap())(
                    cur_image.as_ptr(),
                    ffi::__DRI_IMAGE_ATTRIB_OFFSET,
                    &mut offsets[i],
                )
            };

            if cur_image != pixmap_buffer {
                unsafe { (image.destroyImage.unwrap())(cur_image.as_ptr()) };
            }

            if ret == 0 {
                return Err(StaticMsg("Failed to query buffer attributes"));
            }
        }

        let mut modifier_upper = MaybeUninit::<c_int>::uninit();
        let mut ret = unsafe {
            (image.queryImage.unwrap())(
                pixmap_buffer.as_ptr(),
                ffi::__DRI_IMAGE_ATTRIB_MODIFIER_UPPER,
                modifier_upper.as_mut_ptr(),
            )
        };
        let mut modifier_lower = MaybeUninit::<c_int>::uninit();
        ret &= unsafe {
            (image.queryImage.unwrap())(
                pixmap_buffer.as_ptr(),
                ffi::__DRI_IMAGE_ATTRIB_MODIFIER_LOWER,
                modifier_lower.as_mut_ptr(),
            )
        };

        let modifier = if ret == 0 {
            DRM_CORRUPTED_MODIFIER
        } else {
            // SAFETY: if queryImage succeeded on both tries, both modifiers should contractually
            //         be fully init
            let upper = unsafe { modifier_upper.assume_init() };
            let lower = unsafe { modifier_lower.assume_init() };
            (upper << 32) | (lower & 0xffffffff)
        };

        let pixmap = if drawable.has_multiplanes && modifer != DRM_CORRUPTED_MODIFIER {
            conn.pixmap_from_buffers(
                Window::const_from_xid(drawable.window.load(Ordering::Acquire)),
                depth,
                width,
                height,
                strides,
                offsets,
                cpp * 8,
                modifiers,
                buffer_fds,
            )?
        } else {
            conn.pixmap_from_buffer(
                drawable.x_drawable,
                0,
                width,
                height,
                strides[0] as _,
                depth,
                cpp * 8,
                buffer_fds[0],
            )?
        };

        let sync_fence = conn.fence_from_fd(pixmap.into(), false, fence_fd)?;
        set_fence(shm_fence);

        mem::forget((fd_guard, shm_guard, image_guard, buffer_guard));

        // create the object proper
        unsafe {
            ptr::write(
                Arc::get_mut(&mut buffer)
                    .expect("Infallible Arc::get_mut()")
                    .as_mut_ptr(),
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
                },
            )
        };
        Ok(unsafe { buffer.assume_init() })
    }

    /// Free the renderbuffer's data. This isn't done in a drop handle because we need the reference to
    /// the Dri3Drawable (and also because we block).
    #[inline]
    fn free<Dpy: DisplayLike>(self, drawable: &Dri3Drawable<Dpy>) -> breadx::Result {
        let mut conn = drawable.display.display();
        if self.own_pixmap {
            self.pixmap.free(&mut conn)?;
        }

        // free the sync fence
        conn.free_sync_fence(self.sync_fence)?;
        // free the shm fence
        let xshmfence = xshmfence()?;
        let unmap_shm: XshmfenceUnmapShm = unsafe { xshmfence.symbol(&XSHMFENCE_UNMAP_SHM) }
            .ok_or(StaticMsg("Failed to load xshmfence_unmap_shm"))?;
        unsafe { unmap_shm(self.sfm_fence as *mut _) };

        // destroy the buffers
        let image_ext = unsafe { &*self.screen.inner.image };
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
fn get_adaptive_sync_and_vblank_mode<Dpy: DisplayLike>(
    screen: &Dri3Screen<Dpy>,
) -> (c_uchar, c_int) {
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
) -> breadx::Result<DriDrawablePtr> {
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
fn buffer_from_pixmap<Dpy>(
    screen: &Dri3Screen<Dpy>,
    width: u16,
    height: u16,
    mut stride: u32,
    format: c_uint,
    fds: Vec<c_int>,
    dri_screen: ThreadSafe<*mut ffi::__DRIscreen>,
    loader: ThreadSafe<*const ()>,
) -> breadx::Result<ThreadSafe<NonNull<ffi::__DRIimage>>> {
    let mut offset = 0;

    // createImageFromFds
    let image_planar = unsafe {
        ((&*screen.inner.image).createImageFromFds.unwrap())(
            dri_screen.0,
            width,
            height,
            image_format_to_fourcc(format),
            fds.as_ptr() as *const _,
            1,
            &mut stride,
            &mut offset,
            loader.0 as *mut _,
        )
    };
    unsafe { libc::close(fds[0]) };

    if image_planar.is_null() {
        return Err(StaticMsg("Failed to create image from fd"));
    }

    let ret = unsafe {
        ((&*screen.inner.image).fromPlanar.unwrap())(image_planar, 0, loader.0 as *mut _)
    };
    match NonNull::new(ret) {
        Some(ret) => Ok(unsafe { ThreadSafe::new(ret) }),
        None => unsafe {
            ((&*screen.inner.image).destroyImage.unwrap())(image_planar);
            Ok(unsafe { ThreadSafe::new(NonNull::new_unchecked(image_planar)) })
        },
    }
}

#[inline]
fn buffers_from_pixmap<Dpy>(
    screen: &Dri3Screen<Dpy>,
    width: u16,
    height: u16,
    mut strides: Vec<u32>,
) -> breadx::Result<ThreadSafe<NonNull<ffi::__DRIimage>>> {
}

#[inline]
fn set_fence(fence: NonNull<c_void>) {
    let xshmfence = xshmfence().expect("Failed to load xshmfence"); // should be infallible
    let trigger: XshmfenceTrigger =
        unsafe { xshmfence.symbol(&*XSHMFENCE_TRIGGER) }.expect("xshmfence_trigger not found");
    unsafe { (trigger)(fence.as_ptr()) };
}

#[inline]
fn reset_fence(fence: NonNull<c_void>) {
    let xshmfence = xshmfence().expect("Infallible!");
    let reset: XshmfenceReset =
        unsafe { xshmfence.symbol(&*XSHMFENCE_RESET) }.expect("xshmfence_reset found found");
    unsafe { (reset)(fence.as_ptr()) };
}

#[inline]
fn trigger_fence<Dpy: DisplayLike>(
    conn: &mut Display<Dpy::Conn>,
    fence: Fence,
) -> breadx::Result<()>
where
    Dpy::Conn: Connection,
{
    conn.trigger_fence(fence)
}

#[inline]
fn block_on_fence<Dpy: DisplayLike>(
    conn: &mut Display<Dpy::Conn>,
    drawable: Option<&Dri3Drawable<Dpy>>,
    buffer: &Dri3Buffer,
) -> breadx::Result<()>
where
    Dpy::Conn: Connection,
{
    let xshmfence = xshmfence().expect("Infallible!");
    let xawait: XshmfenceAwait =
        unsafe { xshmfence.symbol(&*XSHMFENCE_AWAIT) }.expect("xshmfence_await not found");
    unsafe { (xawait)(buffer.shm_fence.as_ptr()) };

    if let Some(drawable) = drawable {
        let mut guard = drawable.state();
        let mut conn = drawable.display.display();
        self.process_present_events(&mut *conn, &mut *guard)?;
    }

    Ok(())
}

#[repr(transparent)]
struct DriDrawablePtr(NonNull<ffi::__DRIdrawable>);

unsafe impl Send for DriDrawablePtr {}
unsafe impl Sync for DriDrawablePtr {}

const VARIABLE_REFRESH: &str = "_VARAIBLE_REFRESH";

#[inline]
fn free_buffer_arc<Dpy: DisplayLike>(
    buffer: Arc<Dri3Buffer>,
    draw: &Dri3Drawable<Dpy>,
) -> breadx::Result<()>
where
    Dpy::Conn: Connection,
{
    let mut bufarc = Some(buffer);
    'tryunwraploop: loop {
        match Arc::try_unwrap(bufarc.take().unwrap()) {
            Ok(buffer) => {
                buffer.free(draw)?;
                break 'tryunwraploop;
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
const fn image_format_to_fourcc(format: c_int) -> c_int {
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
        4107 => ffi::__DRI_IMAGE_FOURCC_SARGB8888,
        // __DRI_IMAGE_FORMAT_SABGR8
        4114 => ffi::__DRI_IMAGE_FOURCC_SABGR8888,
        // __DRI_IMAGE_FORMAT_SXRGB8
        4118 => ffi::__DRI_IMAGE_FOURCC_SXRGB8888,
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

#[repr(transparent)]
struct ThreadSafe<T>(T);
unsafe impl<T> Send for ThreadSafe<T> {}
unsafe impl<T> Sync for ThreadSafe<T> {}

impl<T> ThreadSafe<T> {
    #[inline]
    unsafe fn new(val: T) -> Self {
        Self(val)
    }
}
