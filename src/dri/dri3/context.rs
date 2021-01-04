// MIT/Apache2 License

use super::{Dri3Drawable, Dri3Screen};
use crate::{
    config::GlConfig,
    context::{ContextDispatch, GlContext, GlContextRule, GlInternalContext, InnerGlContext},
    display::GlDisplay,
    dri::{convert_dri_rules, ffi, DriRules, ExtensionContainer},
};
use breadx::{
    display::{Connection, Display},
    Drawable,
};
use std::{
    ffi::c_void,
    os::raw::c_uint,
    ptr::{self, NonNull},
    sync::{atomic::AtomicPtr, Arc},
};
use tinyvec::ArrayVec;

#[cfg(feature = "async")]
use crate::util::GenericFuture;

#[derive(Debug)]
struct Dri3ContextInner {
    dri_context: NonNull<ffi::__DRIcontext>,
    // Dri3Screen is wrapped in an Arc, we can keep a sneaky reference here
    screen: Dri3Screen,
    fbconfig: GlConfig,
}

#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct Dri3Context {
    inner: Arc<Dri3ContextInner>,
}

unsafe impl Send for Dri3Context {}
unsafe impl Sync for Dri3Context {}

impl Dri3Context {
    #[inline]
    fn new_internal(
        screen: Dri3Screen,
        fbconfig: GlConfig,
        rules: &[GlContextRule],
        share: Option<&GlContext>,
        base: &Arc<InnerGlContext>,
    ) -> breadx::Result<Dri3Context> {
        // convert the rules to the appropriate set of DRI rules
        let rules = convert_dri_rules(rules)?;
        let attrib = rules.as_dri3_attribs();

        let share: *mut ffi::__DRIcontext = match share.map(|s| s.dispatch()) {
            Some(ContextDispatch::Dri3(d3)) => d3.dri_context().as_ptr(),
            _ => ptr::null_mut(),
        };
        let mut error: c_uint = 0;

        let dri_context = unsafe {
            ((*(screen.inner.image_driver))
                .createContextAttribs
                .expect("Unable to find createContextAttribs"))(
                screen.dri_screen().as_ptr(),
                rules.api as _,
                match screen.driconfig_from_fbconfig(&fbconfig) {
                    Some(screen) => screen.as_ptr(),
                    None => ptr::null(),
                },
                share,
                attrib.len() as _,
                attrib.as_ptr(),
                &mut error,
                // This isn't *that* horribly unsafe if you think about it
                // See screen.rs for more info
                &**base as *const InnerGlContext as *mut InnerGlContext as *mut c_void,
            )
        };

        Ok(Self {
            inner: Arc::new(Dri3ContextInner {
                dri_context: NonNull::new(dri_context).ok_or(breadx::BreadError::StaticMsg(
                    "Failed to initialize DRI3 context",
                ))?,
                screen,
                fbconfig,
            }),
        })
    }

    #[inline]
    pub(crate) fn new(
        scr: &Dri3Screen,
        fbconfig: &GlConfig,
        rules: &[GlContextRule],
        share: Option<&GlContext>,
        base: &mut Arc<InnerGlContext>,
    ) -> breadx::Result<Dri3Context> {
        Self::new_internal(scr.clone(), fbconfig.clone(), rules, share, base)
    }

    #[cfg(feature = "async")]
    #[inline]
    pub(crate) async fn new_async(
        scr: &Dri3Screen,
        fbconfig: &GlConfig,
        rules: &[GlContextRule],
        share: Option<&GlContext>,
        base: &mut Arc<InnerGlContext>,
    ) -> breadx::Result<Dri3Context> {
        // we can just unblock on it
        let scr = scr.clone();
        let fbconfig = fbconfig.clone();
        let rules = rules.to_vec();
        let share = share.cloned();
        let base = base.clone();
        blocking::unblock(move || Self::new_internal(scr, fbconfig, &rules, share.as_ref(), &base))
            .await
    }

    #[inline]
    fn dri_context(&self) -> NonNull<ffi::__DRIcontext> {
        self.inner.dri_context
    }

    #[inline]
    fn screen(&self) -> &Dri3Screen {
        &self.inner.screen
    }

    #[inline]
    pub fn fbconfig(&self) -> Option<&GlConfig> {
        Some(&self.inner.fbconfig)
    }
}

impl GlInternalContext for Dri3Context {
    #[inline]
    fn bind<Conn: Connection, Dpy: AsRef<Display<Conn>> + AsMut<Display<Conn>>>(
        &self,
        dpy: &mut GlDisplay<Conn, Dpy>,
        read: Option<Drawable>,
        draw: Option<Drawable>,
    ) -> breadx::Result {
        // get the DRI drawable equivalent to read and draw
        let read = match read {
            Some(read) => Some(self.screen().fetch_dri_drawable(dpy, self, read)?),
            None => None,
        };
        let draw = match draw {
            Some(draw) => Some(self.screen().fetch_dri_drawable(dpy, self, draw)?),
            None => None,
        };

        // bind the context to the OpenGL driver
        if unsafe {
            ((*self.screen().inner.core)
                .bindContext
                .expect("bindContext not present"))(
                self.dri_context().as_ptr(),
                match draw {
                    Some(ref draw) => draw.dri_drawable().as_ptr(),
                    None => ptr::null_mut(),
                },
                match read {
                    Some(ref read) => read.dri_drawable().as_ptr(),
                    None => ptr::null_mut(),
                },
            )
        } == 0
        {
            Err(breadx::BreadError::StaticMsg("Failed to bind DRI3 context"))
        } else {
            // invalidate the two drawables
            if let Some(ref draw) = draw {
                draw.invalidate();
            }

            if let Some(ref read) = read {
                if let Some(ref draw) = draw {
                    if Arc::ptr_eq(&read, &draw) {
                        read.invalidate();
                    }
                } else {
                    read.invalidate();
                }
            }

            Ok(())
        }
    }

    #[cfg(feature = "async")]
    #[inline]
    fn bind_async<
        'future,
        'a,
        'b,
        Conn: Connection,
        Dpy: AsRef<Display<Conn>> + AsMut<Display<Conn>> + Send,
    >(
        &'a self,
        dpy: &'b mut GlDisplay<Conn, Dpy>,
        read: Option<Drawable>,
        draw: Option<Drawable>,
    ) -> GenericFuture<'future, breadx::Result>
    where
        'a: 'future,
        'b: 'future,
    {
        Box::pin(async move {
            // get the DRI drawable equivalent to read and draw
            let read = match read {
                Some(read) => Some(
                    self.screen()
                        .fetch_dri_drawable_async(dpy, self, read)
                        .await?,
                ),
                None => None,
            };
            let draw = match draw {
                Some(draw) => Some(
                    self.screen()
                        .fetch_dri_drawable_async(dpy, self, draw)
                        .await?,
                ),
                None => None,
            };

            // bind the context to the OpenGL driver
            let this = self.clone();
            let read2 = read.clone();
            let draw2 = draw.clone();
            let res = blocking::unblock(move || unsafe {
                ((*this.screen().inner.core)
                    .bindContext
                    .expect("bindContext not present"))(
                    this.dri_context().as_ptr(),
                    match draw2 {
                        Some(draw) => draw.dri_drawable().as_ptr(),
                        None => ptr::null_mut(),
                    },
                    match read2 {
                        Some(read) => read.dri_drawable().as_ptr(),
                        None => ptr::null_mut(),
                    },
                )
            })
            .await;

            if res == 0 {
                Err(breadx::BreadError::StaticMsg("Failed to bind DRI3 context"))
            } else {
                // invalidate the two drawables
                if let Some(ref draw) = draw {
                    Dri3Drawable::invalidate_async(draw.clone()).await;
                }

                if let Some(read) = read {
                    if let Some(draw) = draw {
                        if Arc::ptr_eq(&read, &draw) {
                            Dri3Drawable::invalidate_async(read).await;
                        }
                    } else {
                        Dri3Drawable::invalidate_async(read).await;
                    }
                }

                Ok(())
            }
        })
    }

    #[inline]
    fn unbind(&self) -> breadx::Result<()> {
        unsafe {
            ((*self.screen().inner.core)
                .unbindContext
                .expect("unbindContext not present"))(self.dri_context().as_ptr())
        };
        Ok(())
    }

    #[cfg(feature = "async")]
    #[inline]
    fn unbind_async<'future>(&'future self) -> GenericFuture<'future, breadx::Result> {
        let this = self.clone();
        Box::pin(blocking::unblock(move || this.unbind()))
    }
}

impl Drop for Dri3Context {
    #[inline]
    fn drop(&mut self) {
        // TODO
    }
}
