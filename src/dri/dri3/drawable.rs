// MIT/Apache2 License

use super::Dri3Screen;
use crate::{
    config::GlConfig,
    context::GlInternalContext,
    cstr::{const_cstr, ConstCstr},
    dri::ffi,
};
use breadx::{
    display::{Connection, Display},
    Drawable, PropMode, PropertyFormat, PropertyType, Window,
};
use std::{
    ffi::c_void,
    os::raw::{c_int, c_uchar},
    ptr::{self, NonNull},
    sync::Arc,
};

#[cfg(feature = "async")]
use crate::util::GenericFuture;
#[cfg(feature = "async")]
use futures_lite::future;

#[derive(Debug)]
pub struct Dri3Drawable {
    drawable: NonNull<ffi::__DRIdrawable>,
    config: GlConfig,
    is_different_gpu: bool,
    screen: Dri3Screen,
    width: u16,
    height: u16,
    swap_interval: c_int,
}

unsafe impl Send for Dri3Drawable {}
unsafe impl Sync for Dri3Drawable {}

const VBLANK_NEVER: c_int = 0;
const VBLANK_DEF_INTERVAL0: c_int = 1;
const VBLANK_DEF_INTERVAL1: c_int = 2;
const VBLANK_DEF_ALWAYS_SYNC: c_int = 3;

const VBLANK_MODE: ConstCstr<'static> = const_cstr(&*b"vblank_mode\0");
const ADAPTIVE_SYNC: ConstCstr<'static> = const_cstr(&*b"adaptive_sync\0");

impl Dri3Drawable {
    #[inline]
    pub fn new<Conn: Connection>(
        dpy: &mut Display<Conn>,
        drawable: Drawable,
        screen: Dri3Screen,
        config: GlConfig,
    ) -> breadx::Result<Arc<Self>> {
        let (adaptive_sync, vblank_mode) = get_adaptive_sync_and_vblank_mode(&screen);
        let swap_interval = match vblank_mode {
            0 | 1 => 0,
            _ => 1,
        };

        if adaptive_sync == 0 {
            set_adaptive_sync(dpy, drawable, false)?;
        }

        // get the width and height of the drawable
        let geometry = breadx::drawable::get_geometry_immediate(dpy, drawable)?;

        let mut this = Arc::new(Self {
            drawable: NonNull::dangling(),
            config,
            is_different_gpu: screen.inner.is_different_gpu,
            screen,
            width: geometry.width,
            height: geometry.height,
            swap_interval,
        });

        // create the drawable pointer
        let dri_drawable =
            create_the_drawable(&this.screen, &this.config, Arc::as_ptr(&this) as _)?;
        Arc::get_mut(&mut this)
            .expect("Infallible Arc::get_mut()")
            .drawable = dri_drawable.0;

        Ok(this)
    }

    #[cfg(feature = "async")]
    #[inline]
    pub async fn new_async<Conn: Connection>(
        dpy: &mut Display<Conn>,
        drawable: Drawable,
        screen: Dri3Screen,
        config: GlConfig,
    ) -> breadx::Result<Arc<Self>> {
        let (adaptive_sync, vblank_mode, screen) = blocking::unblock(move || {
            let (adaptive_sync, vblank_mode) = get_adaptive_sync_and_vblank_mode(&screen);
            (adaptive_sync, vblank_mode, screen)
        })
        .await;
        let swap_interval = match vblank_mode {
            0 | 1 => 0,
            _ => 1,
        };

        let geometry = breadx::drawable::get_geometry_immediate_async(dpy, drawable).await?;

        // TODO: figure out if this is more expensive than it's worth
        let as_future = if adaptive_sync == 0 {
            Box::pin(set_adaptive_sync_async(dpy, drawable, false))
                as GenericFuture<'_, breadx::Result>
        } else {
            Box::pin(async { Ok(()) }) as GenericFuture<'_, breadx::Result>
        };

        let mut this = Arc::new(Self {
            drawable: NonNull::dangling(),
            config,
            is_different_gpu: screen.inner.is_different_gpu,
            screen,
            width: geometry.width,
            height: geometry.height,
            swap_interval,
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
    pub fn dri_drawable(&self) -> NonNull<ffi::__DRIdrawable> {
        self.drawable
    }

    #[inline]
    pub fn invalidate(&self) {
        // call the equivalent function on the flush driver
        if self.screen.inner.flush.is_null() {
            log::warn!("Cannot invalidate DRI3 drawable; flush driver is not present");
        } else {
            unsafe {
                ((*self.screen.inner.flush)
                    .invalidate
                    .expect("invalidate not present"))(self.dri_drawable().as_ptr())
            };
        }
    }

    #[cfg(feature = "async")]
    #[inline]
    pub async fn invalidate_async(this: Arc<Self>) {
        blocking::unblock(move || this.invalidate()).await
    }
}

#[inline]
fn get_adaptive_sync_and_vblank_mode(screen: &Dri3Screen) -> (c_uchar, c_int) {
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
fn create_the_drawable(
    screen: &Dri3Screen,
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

#[repr(transparent)]
struct DriDrawablePtr(NonNull<ffi::__DRIdrawable>);

unsafe impl Send for DriDrawablePtr {}
unsafe impl Sync for DriDrawablePtr {}

const VARIABLE_REFRESH: &str = "_VARAIBLE_REFRESH";

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

impl Drop for Dri3Drawable {
    #[inline]
    fn drop(&mut self) {}
}
