// MIT/Apache2 License

use crate::{display::GlInternalDisplay, screen::GlScreen};
use breadx::display::{Connection, Display};
use std::{fmt, marker::PhantomData};

#[cfg(feature = "async")]
use crate::util::GenericFuture;

pub struct IndirectDisplay {
    _private: PhantomData<()>,
}

impl fmt::Debug for IndirectDisplay {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("IndirectDisplay")
    }
}

impl IndirectDisplay {
    #[inline]
    pub fn new<Conn: Connection>(_dpy: &mut Display<Conn>) -> breadx::Result<Self> {
        Ok(Self {
            _private: PhantomData,
        })
    }

    #[cfg(feature = "async")]
    #[inline]
    pub async fn new_async<Conn: Connection>(_dpy: &mut Display<Conn>) -> breadx::Result<Self> {
        Ok(Self {
            _private: PhantomData,
        })
    }
}

impl GlInternalDisplay for IndirectDisplay {
    #[inline]
    fn create_screen<Conn: Connection>(
        &mut self,
        dpy: &mut Display<Conn>,
        index: usize,
    ) -> breadx::Result<GlScreen> {
        unimplemented!()
    }

    #[cfg(feature = "async")]
    #[inline]
    fn create_screen_async<'future, 'a, 'b, Conn: Connection>(
        &'a mut self,
        dpy: &'b mut Display<Conn>,
        index: usize,
    ) -> GenericFuture<'future, breadx::Result<GlScreen>>
    where
        'a: 'future,
        'b: 'future,
    {
        unimplemented!()
    }
}
