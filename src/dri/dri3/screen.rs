// MIT/Apache2 License

use super::super::ExtensionContainer;
use crate::{
    cstr::{const_cstr, ConstCstr},
    dll::Dll,
    dri::{ffi, load},
};
use breadx::{Connection, Display};
use std::{
    cell::UnsafeCell,
    ffi::{c_void, CStr},
    os::raw::c_int,
    ptr::{self, NonNull},
    sync::Arc,
};

#[repr(transparent)]
#[derive(Debug, Clone)]
pub struct Dri3Screen {
    inner: Arc<Dri3ScreenInner>,
}

#[derive(Debug)]
struct Dri3ScreenInner {
    // the library loaded for the DRI layer
    driver: Dll,
    // the fd the screen is running on
    fd: c_int,
    // if this is running on a different GPU
    is_different_gpu: bool,

    // the internal pointer to the actual DRI screen
    dri_screen: Option<NonNull<ffi::__DRIscreen>>,

    // pointers to the extensions
    image: *const ffi::__DRIimageExtension,
    image_driver: *const ffi::__DRIimageDriverExtension,
    core: *const ffi::__DRIcoreExtension,
    flush: *const ffi::__DRI2flushExtension,
    config: *const ffi::__DRI2configQueryExtension,
    tex_buffer: *const ffi::__DRItexBufferExtension,
    renderer_query: *const ffi::__DRI2rendererQueryExtension,
    interop: *const ffi::__DRI2interopExtension,

    driver_configs: *mut *const ffi::__DRIconfig,
}

unsafe impl Send for Dri3ScreenInner {}
unsafe impl Sync for Dri3ScreenInner {}

impl Dri3ScreenInner {
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
        extensions: &mut [ExtensionContainer],
    ) -> breadx::Result<()> {
        let mut driver_configs = ptr::null_mut();
        let dri_screen = unsafe {
            ((*self.image_driver).createNewScreen2.unwrap())(
                scr as _,
                fd,
                super::LOADER_EXTENSIONS.as_ptr() as *mut _,
                extensions.as_mut_ptr() as *mut *const ffi::__DRIextension,
                &mut driver_configs,
                // while this could cause undefined behavior, the places where this is used (loader extensions)
                // casts this value to an immutable reference. therefore, for all intents and purposes, this is
                // an immutable reference. in order to hold the variant that this memory is shared, we use an
                // Arc instead of a Box later on
                self as *mut Dri3ScreenInner as *mut c_void,
            )
        };
        let dri_screen = NonNull::new(dri_screen)
            .ok_or(breadx::BreadError::StaticMsg("Failed to create DRI screen"))?;
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

impl Dri3Screen {
    #[inline]
    pub(crate) fn new<Conn: Connection>(
        dpy: &mut Display<Conn>,
        scr: usize,
    ) -> breadx::Result<Self> {
        cfg_if::cfg_if! {
            if #[cfg(feature = "async")] {
                async_io::block_on(Self::new_async(dpy, scr))
            } else {
                Self::new_blocking(dpy, scr)
            }
        }
    }

    #[inline]
    fn dri_screen(&self) -> NonNull<ffi::__DRIscreen> {
        self.inner.dri_screen.expect("Failed to load DRI screen")
    }

    #[inline]
    fn new_blocking<Conn: Connection>(dpy: &mut Display<Conn>, scr: usize) -> breadx::Result<Self> {
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
        });

        // use the image driver to actually create the screen
        Arc::get_mut(&mut this)
            .expect("Infallible Arc::get_mut()")
            .create_dri_screen(scr, fd, unsafe {
                &mut *(extensions.as_mut_slice() as *mut [ExtensionContainer])
            })?;

        // now we can load up the extension
        Arc::get_mut(&mut this)
            .expect("Infallible Arc::get_mut()")
            .get_extensions()?;

        Ok(Dri3Screen { inner: this.into() })
    }

    #[cfg(feature = "async")]
    #[inline]
    pub async fn new_async<Conn: Connection>(
        dpy: &mut Display<Conn>,
        scr: usize,
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
        });

        // use the image driver to actually create the screen
        let this = blocking::unblock(move || -> breadx::Result<Arc<Dri3ScreenInner>> {
            let exts = unsafe { &mut *(extensions.as_mut_slice() as *mut [ExtensionContainer]) };

            Arc::get_mut(&mut this)
                .expect("Infallible Arc::get_mut()")
                .create_dri_screen(scr, fd, exts)?;

            // now we can load up the extension
            Arc::get_mut(&mut this)
                .expect("Infallible Arc::get_mut()")
                .get_extensions()?;
            Ok(this)
        })
        .await?;

        Ok(Dri3Screen { inner: this })
    }
}

impl Drop for Dri3ScreenInner {
    #[inline]
    fn drop(&mut self) {
        // destroy the screen
        if let Some(destroy_screen) = unsafe { *self.core }.destroyScreen.take() {
            unsafe { (destroy_screen)(self.dri_screen.unwrap().as_ptr()) };
        }

        // drop the configurations
        unsafe { iter_configs(self.driver_configs, |ext| libc::free(ext as *mut _)) };
        unsafe { libc::free(self.driver_configs as *mut _) };

        unsafe { libc::close(self.fd) };
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
