// MIT/Apache2 License

use crate::config::{
    COMPAT_PROFILE_BIT_ARB, CORE_PROFILE_BIT_ARB, ES_PROFILE_BIT_ARB, FLAGS_ARB,
    FLUSH_RELEASE_BEHAVIOR_ARB, LOSE_CONTEXT_RESET_NOTIFICATION, MAJOR_VERSION_ARB,
    MINOR_VERSION_ARB, NONE_RELEASE_BEHAVIOR_ARB, NO_ERROR_ARB, NO_RESET_NOTIFICATION,
    PROFILE_MASK_ARB, RELEASE_BEHAVIOR_ARB, RESET_NOTIFICATION_STRATEGY,
};
use std::{convert::TryInto, os::raw::c_int};
use tinyvec::{ArrayVec, TinyVec};

const DOESNT_FIT: &str = "value can't fit in c_int";

/// Rules for the context.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum GlContextRule {
    MajorVersion(i32),
    MinorVersion(i32),
    Flags(u32),
    NoError(bool),
    Profile(Profile),
    RenderType(u32),
    ResetNotificationStrategy(ResetNotificationStrategy),
    ReleaseBehavior(ReleaseBehavior),
}

/// Profile for the context.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Profile {
    Core,
    Compatibility,
    Es,
}

/// Reset notification strategy for the context.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum ResetNotificationStrategy {
    NoNotification,
    LoseContext,
}

/// Release behavior for the context.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum ReleaseBehavior {
    None,
    Flush,
}

impl GlContextRule {
    /// Convert a set of context rules into classical OpenGL
    #[inline]
    pub(crate) fn convert_ctx_attrib_to_classic(rules: &[GlContextRule]) -> TinyVec<[c_int; 4]> {
        rules
            .iter()
            .cloned()
            .flat_map(|rule| {
                let (name, value): (c_int, c_int) = match rule {
                    Self::MajorVersion(ci) => (MAJOR_VERSION_ARB, ci.try_into().expect(DOESNT_FIT)),
                    Self::MinorVersion(ci) => (MINOR_VERSION_ARB, ci.try_into().expect(DOESNT_FIT)),
                    Self::Flags(flags) => (FLAGS_ARB, flags.try_into().expect(DOESNT_FIT)),
                    Self::NoError(no_error) => (NO_ERROR_ARB, if no_error { 1 } else { 0 }),
                    Self::Profile(p) => (
                        PROFILE_MASK_ARB,
                        match p {
                            Profile::Core => CORE_PROFILE_BIT_ARB,
                            Profile::Compatibility => COMPAT_PROFILE_BIT_ARB,
                            Profile::Es => ES_PROFILE_BIT_ARB,
                        },
                    ),
                    Self::RenderType(rt) => (RENDER_TYPE_ARB, rt.try_into().expect(DOESNT_FIT)),
                    Self::ResetNotificationStrategy(rsn) => (
                        RESET_NOTIFICATION_STRATEGY,
                        match rsn {
                            ResetNotificationStrategy::NoNotification => NO_RESET_NOTIFICATION,
                            ResetNotificationStrategy::LoseContext => {
                                LOSE_CONTEXT_RESET_NOTIFICATION
                            }
                        },
                    ),
                    Self::ReleaseBehavior(rb) => (
                        RELEASE_BEHAVIOR_ARB,
                        match rb {
                            ReleaseBehavior::None => NONE_RELEASE_BEHAVIOR_ARB,
                            ReleaseBehavior::Flush => FLUSH_RELEASE_BEHAVIOR_ARB,
                        },
                    ),
                };

                ArrayVec::<[c_int; 2]>::from([name, value])
            })
            .collect()
    }
}
