// MIT/Apache2 License

use super::ffi;
use crate::{
    config::RGBA_TYPE,
    context::{GlContextRule, Profile, ReleaseBehavior, ResetNotificationStrategy},
};
use std::os::raw::{c_int, c_uint};
use tinyvec::ArrayVec;

/// Rules for DRI.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) struct DriRules {
    pub(crate) major: c_uint,
    pub(crate) minor: c_uint,
    pub(crate) render_type: u32,
    pub(crate) reset: ResetNotificationStrategy,
    pub(crate) release: ReleaseBehavior,
    pub(crate) flags: u32,
    pub(crate) api: c_uint,
}

impl Default for DriRules {
    #[inline]
    fn default() -> Self {
        Self {
            major: 1,
            minor: 0,
            render_type: RGBA_TYPE as _,
            reset: ResetNotificationStrategy::NoNotification,
            release: ReleaseBehavior::Flush,
            flags: 0,
            api: ffi::__DRI_API_OPENGL,
        }
    }
}

impl DriRules {
    #[inline]
    pub(crate) fn as_dri3_attribs(self) -> ArrayVec<[u32; 12]> {
        let DriRules {
            major,
            minor,
            render_type,
            reset,
            release,
            flags,
            api,
        } = self;

        // TODO: what was I thinking when I was writing this? this is probably the least efficient way of
        //       implementing this. go back and rewrite once not sleep deprived.
        ArrayVec::<[u32; 4]>::from([
            ffi::__DRI_CTX_ATTRIB_MAJOR_VERSION,
            major as u32,
            ffi::__DRI_CTX_ATTRIB_MINOR_VERSION,
            minor as u32,
        ])
        .into_iter()
        .chain(if let ResetNotificationStrategy::LoseContext = reset {
            ArrayVec::<[u32; 2]>::from([
                ffi::__DRI_CTX_ATTRIB_RESET_STRATEGY,
                ffi::__DRI_CTX_RESET_LOSE_CONTEXT,
            ])
        } else {
            ArrayVec::<[u32; 2]>::new()
        })
        .chain(if let ReleaseBehavior::None = release {
            ArrayVec::<[u32; 2]>::from([
                ffi::__DRI_CTX_ATTRIB_RELEASE_BEHAVIOR,
                ffi::__DRI_CTX_RELEASE_BEHAVIOR_NONE,
            ])
        } else {
            ArrayVec::<[u32; 2]>::new()
        })
        .chain(if flags == 0 {
            ArrayVec::<[u32; 2]>::from([ffi::__DRI_CTX_ATTRIB_FLAGS, flags])
        } else {
            ArrayVec::<[u32; 2]>::new()
        })
        .collect()
    }
}

/// Get the important rules from the set of GlContext rules.
pub(crate) fn convert_dri_rules(glrules: &[GlContextRule]) -> breadx::Result<DriRules> {
    let mut rules: DriRules = Default::default();
    let mut profile = Profile::Core;

    if glrules.is_empty() {
        return Err(breadx::BreadError::StaticMsg("Rules list was empty"));
    }

    glrules.iter().for_each(|rule| match rule {
        GlContextRule::MajorVersion(mv) => {
            rules.major = *mv as _;
        }
        GlContextRule::MinorVersion(mv) => {
            rules.minor = *mv as _;
        }
        GlContextRule::Flags(f) => {
            rules.flags = *f;
        }
        GlContextRule::NoError(ne) => {
            if *ne {
                rules.flags |= ffi::__DRI_CTX_FLAG_NO_ERROR;
            }
        }
        GlContextRule::Profile(p) => {
            profile = *p;
        }
        GlContextRule::RenderType(rt) => {
            rules.render_type = *rt;
        }
        GlContextRule::ResetNotificationStrategy(rns) => {
            rules.reset = *rns;
        }
        GlContextRule::ReleaseBehavior(rb) => {
            rules.release = *rb;
        }
    });

    // if we have a profile, set the api value accordingly
    rules.api = match (profile, rules.major, rules.minor) {
        (Profile::Core, major, minor) if major > 3 || (major == 3 && minor >= 2) => {
            ffi::__DRI_API_OPENGL_CORE
        }
        (Profile::Core, _, _) | (Profile::Compatibility, _, _) => ffi::__DRI_API_OPENGL,
        (Profile::Es, major, minor) if major >= 3 => ffi::__DRI_API_GLES3,
        (Profile::Es, major, minor) if major == 2 && minor == 0 => ffi::__DRI_API_GLES2,
        (Profile::Es, major, minor) if major == 1 && minor < 2 => ffi::__DRI_API_GLES,
        _ => {
            return Err(breadx::BreadError::StaticMsg(
                "Failed to determine API version",
            ))
        }
    };

    Ok(rules)
}
