// MIT/Apache2 License

use super::{super::ExtensionContainer, Dri3Context, Dri3Drawable};
use crate::{
    config::{GlConfig, GLX_FBCONFIG_ID},
    context::{
        dispatch::ContextDispatch, promote_anyarc_ref, GlContext, GlContextRule, InnerGlContext,
    },
    cstr::{const_cstr, ConstCstr},
    display::{DisplayDispatch, DisplayLike, GlDisplay},
    dll::Dll,
    dri::{config, ffi, load},
    screen::GlInternalScreen,
    util::ThreadSafe,
};
use ahash::AHasher;
use breadx::{Connection, Display, Drawable};
use dashmap::DashMap;
use std::{
    cell::UnsafeCell,
    collections::HashMap,
    ffi::{c_void, CStr},
    fmt,
    hash::{Hash, Hasher},
    mem::ManuallyDrop,
    os::raw::c_int,
    ptr::{self, addr_of as raw_const, NonNull},
    sync::{Arc, Weak},
};

#[cfg(feature = "async")]
use crate::{offload, screen::AsyncGlInternalScreen, util::GenericFuture};
#[cfg(feature = "async")]
use breadx::display::AsyncConnection;

#[repr(transparent)]
#[derive(Debug)]
pub struct Dri3Screen<Dpy> {
    pub(crate) inner: Arc<Dri3ScreenInner<Dpy>>,
}

#[repr(transparent)]
#[derive(Debug)]
pub struct WeakDri3ScreenRef<Dpy> {
    pub(crate) inner: Weak<Dri3ScreenInner<Dpy>>,
}

impl<Dpy> Clone for Dri3Screen<Dpy> {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<Dpy> Clone for WeakDri3ScreenRef<Dpy> {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<Dpy> WeakDri3ScreenRef<Dpy> {
    #[inline]
    pub fn promote(&self) -> Dri3Screen<Dpy> {
        Dri3Screen {
            inner: self.inner.upgrade().expect("Failed to promote Dri3 screen"),
        }
    }
}

pub struct Dri3ScreenInner<Dpy> {
    // the library loaded for the DRI layer
    driver: Dll,
    // the fd the screen is running on
    fd: c_int,
    // if this is running on a different GPU
    pub is_different_gpu: bool,

    // the internal pointer to the actual DRI screen
    dri_screen: Option<NonNull<ffi::__DRIscreen>>,

    // a map matching the hash values of glconfigs to driconfig pointers
    dri_configmap: Option<HashMap<u64, NonNull<ffi::__DRIconfig>>>,

    // a map matching X11 drawables to DRI3 drawables
    drawable_map: ManuallyDrop<DashMap<Drawable, Arc<Dri3Drawable<Dpy>>>>,

    // store the fbconfigs and visualinfos in here as well
    fbconfigs: Arc<[GlConfig]>,
    visuals: Arc<[GlConfig]>,

    // pointers to the extensions
    pub(crate) image: *const ffi::__DRIimageExtension,
    pub(crate) image_driver: *const ffi::__DRIimageDriverExtension,
    pub(crate) core: *const ffi::__DRIcoreExtension,
    pub(crate) flush: *const ffi::__DRI2flushExtension,
    pub(crate) config: *const ffi::__DRI2configQueryExtension,
    tex_buffer: *const ffi::__DRItexBufferExtension,
    renderer_query: *const ffi::__DRI2rendererQueryExtension,
    interop: *const ffi::__DRI2interopExtension,

    driver_configs: *mut *const ffi::__DRIconfig,

    // specialized dropper mechanism
    dropper: fn(&mut Dri3ScreenInner<Dpy>),
}

impl<Dpy> fmt::Debug for Dri3ScreenInner<Dpy> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Dri3ScreenInner")
    }
}

unsafe impl<Dpy: Send> Send for Dri3ScreenInner<Dpy> {}
unsafe impl<Dpy: Sync> Sync for Dri3ScreenInner<Dpy> {}

impl<Dpy: DisplayLike> Dri3ScreenInner<Dpy> {
    #[inline]
    fn get_extensions_core(
        &mut self,
        mut extensions: *mut *const ffi::__DRIextension,
    ) -> breadx::Result<()> {
        const DRI_TEX_BUFFER: ConstCstr<'static> = const_cstr(ffi::__DRI_TEX_BUFFER);
        const DRI2_FLUSH: ConstCstr<'static> = const_cstr(ffi::__DRI2_FLUSH);
        const DRI_IMAGE: ConstCstr<'static> = const_cstr(ffi::__DRI_IMAGE);
        const DRI2_CONFIG_QUERY: ConstCstr<'static> = const_cstr(ffi::__DRI2_CONFIG_QUERY);
        const DRI2_ROBUSTNESS: ConstCstr<'static> = const_cstr(ffi::__DRI2_ROBUSTNESS);
        const DRI2_NO_ERROR: ConstCstr<'static> = const_cstr(ffi::__DRI2_NO_ERROR);
        const DRI2_RENDERER_QUERY: ConstCstr<'static> = const_cstr(ffi::__DRI2_RENDERER_QUERY);
        const DRI2_INTEROP: ConstCstr<'static> = const_cstr(ffi::__DRI2_INTEROP);
        const DRI2_FLUSH_CONTROL: ConstCstr<'static> = const_cstr(ffi::__DRI2_FLUSH_CONTROL);

        // run the getExtensions method of the core plugin
        while !(unsafe { *extensions }.is_null()) {
            let ext = unsafe { *extensions };
            let ext_name = unsafe { CStr::from_ptr((*ext).name) };

            if DRI_TEX_BUFFER == ext_name {
                self.tex_buffer = ext as *const _;
            } else if DRI2_FLUSH == ext_name {
                self.flush = ext as *const _;
            } else if DRI_IMAGE == ext_name {
                self.image = ext as *const _;
            } else if DRI2_CONFIG_QUERY == ext_name {
                self.config = ext as *const _;
            }

            extensions = unsafe { extensions.offset(1) };
        }

        Ok(())
    }

    #[inline]
    fn get_extensions(&mut self) -> breadx::Result<()> {
        let extensions = unsafe {
            ((*self.core)
                .getExtensions
                .expect("failed to load getextensions function"))(
                self.dri_screen.expect("NFI").as_ptr(),
            )
        };
        self.get_extensions_core(extensions)
    }

    #[inline]
    fn create_dri_screen(
        &mut self,
        scr: usize,
        fd: c_int,
        loader_extensions: &[ExtensionContainer],
        extensions: &mut [ExtensionContainer],
    ) -> breadx::Result<()> {
        let mut driver_configs = ptr::null_mut();
        let dri_screen = unsafe {
            ((*self.image_driver).createNewScreen2.unwrap())(
                scr as _,
                fd,
                loader_extensions.as_ptr() as *mut _,
                extensions.as_mut_ptr() as *mut *const ffi::__DRIextension,
                &mut driver_configs,
                // while this could cause undefined behavior, the places where this is used (loader extensions)
                // casts this value to an immutable reference. therefore, for all intents and purposes, this is
                // an immutable reference. in order to hold the variant that this memory is shared, we use an
                // Arc instead of a Box later on
                // (We also use the arc to simplify some async logic, but don't tell anyone that)
                self as *mut Dri3ScreenInner<Dpy> as *mut c_void,
            )
        };
        let dri_screen = NonNull::new(dri_screen)
            .ok_or(breadx::BreadError::StaticMsg("Failed to create DRI screen"))?;

        if driver_configs.is_null() {
            return Err(breadx::BreadError::StaticMsg(
                "createNewScreen2 did not return driver configurations",
            ));
        }

        if unsafe { *driver_configs }.is_null() {
            return Err(breadx::BreadError::StaticMsg(
                "createNewScreen2 returned 0 driver configurations",
            ));
        }

        self.dri_screen = Some(dri_screen);
        self.driver_configs = driver_configs;
        Ok(())
    }
}

#[inline]
fn get_bootstrap_extensions(
    extensions: &[ExtensionContainer],
) -> breadx::Result<(ExtensionContainer, ExtensionContainer)> {
    let mut image_driver = ExtensionContainer(ptr::null());
    let mut core = ExtensionContainer(ptr::null());

    extensions.iter().copied().for_each(|ext| {
        if !ext.0.is_null() {
            let ext_name = unsafe { (*ext.0).name };
            let ext_name = unsafe { CStr::from_ptr(ext_name) };

            if ext_name.to_bytes_with_nul() == ffi::__DRI_CORE {
                core = ext;
            } else if ext_name.to_bytes_with_nul() == ffi::__DRI_IMAGE_DRIVER {
                image_driver = ext;
            }
        }
    });

    match (core.0.is_null(), image_driver.0.is_null()) {
        (true, _) => Err(breadx::BreadError::StaticMsg(
            "Unable to load core driver for DRI3",
        )),
        (false, true) => Err(breadx::BreadError::StaticMsg(
            "Unable to load image driver for DRI3",
        )),
        (false, false) => Ok((core, image_driver)),
    }
}

impl<Dpy> Dri3Screen<Dpy> {
    #[inline]
    pub(crate) fn driconfig_from_fbconfig(
        &self,
        cfg: &GlConfig,
    ) -> Option<NonNull<ffi::__DRIconfig>> {
        let mut hasher = AHasher::default();
        cfg.hash(&mut hasher);
        self.inner
            .dri_configmap
            .as_ref()
            .unwrap()
            .get(&hasher.finish())
            .cloned()
    }

    #[inline]
    pub fn dri_screen(&self) -> NonNull<ffi::__DRIscreen> {
        self.inner.dri_screen.expect("Failed to load DRI screen")
    }

    #[inline]
    pub fn weak_ref(&self) -> WeakDri3ScreenRef<Dpy> {
        WeakDri3ScreenRef {
            inner: Arc::downgrade(&self.inner),
        }
    }
}

impl<Dpy: DisplayLike> Dri3Screen<Dpy>
where
    Dpy::Connection: Connection,
{
    #[inline]
    fn new_blocking(
        dpy: &mut Display<Dpy::Connection>,
        scr: usize,
        visuals: Arc<[GlConfig]>,
        fbconfigs: Arc<[GlConfig]>,
    ) -> breadx::Result<Self> {
        // first, figure out which file descriptor corresponds to our screen
        let root = dpy.screens()[scr].root;
        let fd = dpy.open_dri3_immediate(root, 0)?;

        // TODO: figure out if we need to use a different file descriptor for a different GPU

        // then, open the the driver associated with the fd
        let mut extensions = vec![];
        let driver = load::load_dri_driver(fd, &mut extensions)?;
        extensions.push(ExtensionContainer(ptr::null()));

        // assign the extensions that we need to bootstrap
        let (core, image_driver) = get_bootstrap_extensions(&extensions)?;

        // create the screen on the DRI end
        // this is done to pin the location of the screen object on the heap
        let mut this = Arc::new(Dri3ScreenInner {
            driver,
            fd,
            is_different_gpu: false,
            dri_screen: None,
            core: core.0 as *const _,
            image_driver: image_driver.0 as *const _,
            image: ptr::null(),
            flush: ptr::null(),
            config: ptr::null(),
            tex_buffer: ptr::null(),
            renderer_query: ptr::null(),
            interop: ptr::null(),
            driver_configs: ptr::null_mut(),
            dri_configmap: None,
            drawable_map: ManuallyDrop::new(DashMap::new()),
            fbconfigs: fbconfigs.clone(),
            visuals: visuals.clone(),
            dropper: Dropper::<Dpy>::sync_dropper,
        });

        // use the image driver to actually create the screen
        let thisref = Arc::get_mut(&mut this).expect("Infallible Arc::get_mut()");
        thisref.create_dri_screen(scr, fd, super::loader_extensions::<Dpy>(), unsafe {
            &mut *(extensions.as_mut_slice() as *mut [ExtensionContainer])
        })?;

        // now we can load up the extension and additional configs
        thisref.get_extensions()?;
        let mut extmap: HashMap<u64, NonNull<ffi::__DRIconfig>> = unsafe {
            config::convert_configs(
                ExtensionContainer(thisref.core as *const _),
                &visuals,
                thisref.driver_configs,
            )
        }
        .collect();
        extmap.extend(unsafe {
            config::convert_configs(
                ExtensionContainer(thisref.core as *const _),
                &fbconfigs,
                thisref.driver_configs,
            )
        });

        thisref.dri_configmap = Some(extmap);

        Ok(Dri3Screen { inner: this.into() })
    }

    #[inline]
    pub(crate) fn new(
        dpy: &mut Display<Dpy::Connection>,
        scr: usize,
        visuals: Arc<[GlConfig]>,
        fbconfigs: Arc<[GlConfig]>,
    ) -> breadx::Result<Self> {
        Self::new_blocking(dpy, scr, visuals, fbconfigs)
    }

    /// Get the DRI drawable associated with an X11 drawable.
    #[inline]
    pub(crate) fn fetch_dri_drawable(
        &self,
        dpy: &GlDisplay<Dpy>,
        context: &Dri3Context<Dpy>,
        drawable: Drawable,
    ) -> breadx::Result<Arc<Dri3Drawable<Dpy>>> {
        match self.inner.drawable_map.get(&drawable) {
            Some(d) => Ok(d.clone()),
            None => {
                let fbconfig = match context.fbconfig() {
                    Some(fbc) => fbc,
                    None => dpy
                        .load_drawable_property(drawable, GLX_FBCONFIG_ID as _)?
                        .and_then(|fbid| {
                            self.inner
                                .fbconfigs
                                .iter()
                                .find(|f| f.fbconfig_id == fbid as c_int)
                        })
                        .ok_or(breadx::BreadError::StaticMsg("Failed to find FbConfig ID"))?,
                };

                // SAFETY: the pointer is in a C struct which is guaranteed to be well-aligned and
                //         point to a valid object if it isn't null
                let has_multiplane = match (unsafe { self.inner.image.as_ref() }, dpy.dispatch()) {
                    (Some(image), DisplayDispatch::Dri3(d3)) => {
                        image.base.version >= 15
                            && (d3.dri3_version_major() > 1
                                || (d3.dri3_version_major() == 1 && d3.dri3_version_minor() >= 2))
                            && (d3.present_version_major() > 1
                                || (d3.present_version_major() == 1
                                    && d3.present_version_minor() >= 2))
                    }
                    _ => false,
                };

                let d = Dri3Drawable::new(
                    dpy,
                    drawable,
                    self.clone(),
                    context.clone(),
                    fbconfig.clone(),
                    has_multiplane,
                )?;
                self.inner.drawable_map.insert(drawable, d.clone());
                Ok(d)
            }
        }
    }
}

#[cfg(feature = "async")]
impl<Dpy: DisplayLike> Dri3Screen<Dpy>
where
    Dpy::Connection: AsyncConnection + Send,
{
    #[inline]
    pub async fn new_async(
        dpy: &mut Display<Dpy::Connection>,
        scr: usize,
        visuals: Arc<[GlConfig]>,
        fbconfigs: Arc<[GlConfig]>,
    ) -> breadx::Result<Self> {
        // first, figure out which file descriptor corresponds to our screen
        let root = dpy.screens()[scr].root;
        let fd = dpy.open_dri3_immediate_async(root, 0).await?;

        // TODO: figure out if we need to use a different file descriptor for a different GPU

        // then, open the the driver associated with the fd
        let mut extensions = vec![];
        let driver = load::load_dri_driver_async(fd, &mut extensions).await?;
        extensions.push(ExtensionContainer(ptr::null()));

        // assign the extensions that we need to bootstrap
        let (core, image_driver) = get_bootstrap_extensions(&extensions)?;

        // create the screen on the DRI end
        // this is done to pin the location of the screen object on the heap
        let mut this = Arc::new(Dri3ScreenInner {
            driver,
            fd,
            is_different_gpu: false,
            dri_screen: None,
            core: core.0 as *const _,
            image_driver: image_driver.0 as *const _,
            image: ptr::null(),
            flush: ptr::null(),
            config: ptr::null(),
            tex_buffer: ptr::null(),
            renderer_query: ptr::null(),
            interop: ptr::null(),
            driver_configs: ptr::null_mut(),
            dri_configmap: None,
            drawable_map: ManuallyDrop::new(DashMap::new()),
            fbconfigs: fbconfigs.clone(),
            visuals: visuals.clone(),
            dropper: Dropper::<Dpy>::async_dropper,
        });

        // use the image driver to actually create the screen
        let this = blocking::unblock(move || -> breadx::Result<Arc<Dri3ScreenInner<Dpy>>> {
            let exts = unsafe { &mut *(extensions.as_mut_slice() as *mut [ExtensionContainer]) };

            let thisref = Arc::get_mut(&mut this).expect("Infallible Arc::get_mut()");
            thisref.create_dri_screen(scr, fd, super::loader_extensions_async::<Dpy>(), exts)?;

            // now we can load up the extension
            thisref.get_extensions()?;

            let mut extmap: HashMap<u64, NonNull<ffi::__DRIconfig>> = unsafe {
                config::convert_configs(
                    ExtensionContainer(thisref.core as *const _),
                    &visuals,
                    thisref.driver_configs,
                )
            }
            .collect();
            extmap.extend(unsafe {
                config::convert_configs(
                    ExtensionContainer(thisref.core as *const _),
                    &fbconfigs,
                    thisref.driver_configs,
                )
            });

            thisref.dri_configmap = Some(extmap);

            Ok(this)
        })
        .await?;

        Ok(Dri3Screen { inner: this })
    }

    /// Get the DRI drawable associated with an X11 drawable, async redox.
    #[inline]
    pub(crate) async fn fetch_dri_drawable_async(
        &self,
        dpy: &GlDisplay<Dpy>,
        context: &Dri3Context<Dpy>,
        drawable: Drawable,
    ) -> breadx::Result<Arc<Dri3Drawable<Dpy>>> {
        match self.inner.drawable_map.get(&drawable) {
            Some(d) => Ok(d.clone()),
            None => {
                let fbconfig = match context.fbconfig() {
                    Some(fbconfig) => fbconfig,
                    None => dpy
                        .load_drawable_property_async(drawable, GLX_FBCONFIG_ID as _)
                        .await?
                        .and_then(|fbid| {
                            self.inner
                                .fbconfigs
                                .iter()
                                .find(|f| f.fbconfig_id == fbid as c_int)
                        })
                        .ok_or(breadx::BreadError::StaticMsg("Failed to find FbConfig ID"))?,
                };

                let has_multiplane = match (unsafe { self.inner.image.as_ref() }, dpy.dispatch()) {
                    (Some(image), DisplayDispatch::Dri3(d3)) => {
                        image.base.version >= 15
                            && (d3.dri3_version_major() > 1
                                || (d3.dri3_version_major() == 1 && d3.dri3_version_minor() >= 2))
                            && (d3.present_version_major() > 1
                                || (d3.present_version_major() == 1
                                    && d3.present_version_minor() >= 2))
                    }
                    _ => false,
                };

                let d = Dri3Drawable::new_async(
                    &dpy,
                    drawable,
                    self.clone(),
                    context.clone(),
                    fbconfig.clone(),
                    has_multiplane,
                )
                .await?;
                self.inner.drawable_map.insert(drawable, d.clone());
                Ok(d)
            }
        }
    }
}

impl<Dpy: DisplayLike> GlInternalScreen<Dpy> for Dri3Screen<Dpy>
where
    Dpy::Connection: Connection,
{
    #[inline]
    fn create_context(
        &self,
        base: &mut Arc<InnerGlContext<Dpy>>,
        fbconfig: &GlConfig,
        rules: &[GlContextRule],
        share: Option<&GlContext<Dpy>>,
    ) -> breadx::Result<ContextDispatch<Dpy>> {
        let cfg = super::Dri3Context::new(self, fbconfig, rules, share, base)?;
        Ok(cfg.into())
    }

    #[inline]
    fn swap_buffers(
        &self,
        dpy: &GlDisplay<Dpy>,
        drawable: Drawable,
        target_msc: i64,
        divisor: i64,
        remainder: i64,
        flush: bool,
    ) -> breadx::Result {
        if let Some(ref context) = GlContext::<Dpy>::get()
            .as_ref()
            .and_then(|m| promote_anyarc_ref(m))
        {
            if let ContextDispatch::Dri3(d3) = context.dispatch() {
                let drawable = self.fetch_dri_drawable(dpy, d3, drawable)?;
                let mut flush_flags = ffi::__DRI2_FLUSH_DRAWABLE;
                if flush {
                    flush_flags |= ffi::__DRI2_FLUSH_CONTEXT;
                }
                return drawable.swap_buffers_msc(
                    target_msc,
                    divisor,
                    remainder,
                    flush_flags,
                    &[],
                    false,
                );
            }
        }

        Err(breadx::BreadError::StaticMsg(
            "Unable to get context for swapping buffer",
        ))
    }
}

#[cfg(feature = "async")]
impl<Dpy: DisplayLike> AsyncGlInternalScreen<Dpy> for Dri3Screen<Dpy>
where
    Dpy::Connection: AsyncConnection + Send,
{
    #[inline]
    fn create_context_async<'future, 'a, 'b, 'c, 'd, 'e>(
        &'a self,
        base: &'b mut Arc<InnerGlContext<Dpy>>,
        fbconfig: &'c GlConfig,
        rules: &'d [GlContextRule],
        share: Option<&'e GlContext<Dpy>>,
    ) -> GenericFuture<'future, breadx::Result<ContextDispatch<Dpy>>>
    where
        'a: 'future,
        'b: 'future,
        'c: 'future,
        'd: 'future,
        'e: 'future,
    {
        Box::pin(async move {
            let cfg = super::Dri3Context::new_async(self, fbconfig, rules, share, base).await?;
            Ok(cfg.into())
        })
    }

    #[inline]
    fn swap_buffers_async<'future, 'a, 'b>(
        &'a self,
        dpy: &'b GlDisplay<Dpy>,
        drawable: Drawable,
        target_msc: i64,
        divisor: i64,
        remainder: i64,
        flush: bool,
    ) -> GenericFuture<'future, breadx::Result>
    where
        'a: 'future,
        'b: 'future,
    {
        Box::pin(async { unimplemented!() })
    }
}

struct Dropper<Dpy>(Dpy);

impl<Dpy: DisplayLike> Dropper<Dpy>
where
    Dpy::Connection: Connection,
{
    fn sync_dropper(screen: &mut Dri3ScreenInner<Dpy>) {
        // SAFETY: The drawables require access to the extensions contained within the screen
        //         so we make sure we destroy them first. None of the functions below use the drawables
        //         set.
        unsafe { ManuallyDrop::drop(&mut screen.drawable_map) };

        // destroy the screen
        if let Some(destroy_screen) = unsafe { *screen.core }.destroyScreen.take() {
            unsafe { (destroy_screen)(screen.dri_screen.unwrap().as_ptr()) };
        }

        // drop the configurations
        unsafe { iter_configs(screen.driver_configs, |ext| libc::free(ext as *mut _)) };
        unsafe { libc::free(screen.driver_configs as *mut _) };

        unsafe { libc::close(screen.fd) };
    }
}

#[cfg(feature = "async")]
impl<Dpy: DisplayLike> Dropper<Dpy>
where
    Dpy::Connection: AsyncConnection + Send,
{
    fn async_dropper(screen: &mut Dri3ScreenInner<Dpy>) {
        unsafe { ManuallyDrop::drop(&mut screen.drawable_map) };

        let screen_fd = screen.fd;
        let driver_configs = unsafe { ThreadSafe::new(screen.driver_configs) };
        let dri_screen = unsafe { ThreadSafe::new(screen.dri_screen.unwrap().as_ptr()) };
        let destroy_screen =
            unsafe { ThreadSafe::new((*screen.core).destroyScreen.as_ref().cloned()) };

        offload::offload(blocking::unblock(move || {
            if let Some(destroy_screen) = destroy_screen.into_inner() {
                unsafe { (destroy_screen)(dri_screen.into_inner()) };
            }

            unsafe { iter_configs(*driver_configs, |ext| libc::free(ext as *mut _)) };
            unsafe { libc::free(*driver_configs as *mut _) };
            unsafe { libc::close(screen_fd) };
        }));
    }
}

impl<Dpy> Drop for Dri3ScreenInner<Dpy> {
    #[inline]
    fn drop(&mut self) {
        (self.dropper)(self)
    }
}

#[inline]
unsafe fn iter_configs<F>(mut cfgs: *mut *const ffi::__DRIconfig, mut f: F)
where
    F: FnMut(*const ffi::__DRIconfig),
{
    while !(unsafe { *cfgs }.is_null()) {
        f(unsafe { *cfgs });

        cfgs = unsafe { cfgs.offset(1) };
    }
}
