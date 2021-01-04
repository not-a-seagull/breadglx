// MIT/Apache2 License

use std::os::raw::c_int;

pub const GLX_FBCONFIG_ID: c_int = 0x8013;
pub const DONT_CARE: c_int = 0xFFFFFFFF;
pub const CONFIG_NONE: c_int = 0x8000;
pub const WINDOW_BIT: c_int = 0x1;
pub const PIXMAP_BIT: c_int = 0x2;
pub const PBUFFER_BIT: c_int = 0x4;
pub const RGBA_BIT: c_int = 0x1;
pub const COLOR_INDEX_BIT: c_int = 0x2;
pub const TRANSPARENT_RGB: c_int = 0x8008;
pub const TRANSPARENT_INDEX: c_int = 0x8009;
pub const RGBA_TYPE: c_int = 0x8014;
pub const COLOR_INDEX_TYPE: c_int = 0x8015;
pub const NON_CONFORMANT_CONFIG: c_int = 0x800D;
pub const SLOW_CONFIG: c_int = 0x8001;
pub const RGBA_FLOAT_BIT_ARB: c_int = 0x4;
pub const RGBA_UNSIGNED_FLOAT_BIT_EXT: c_int = 0x8;
pub const TEXTURE_1D_BIT_EXT: c_int = 0x1;
pub const TEXTURE_2D_BIT_EXT: c_int = 0x2;
pub const TEXTURE_RECTANGLE_BIT_EXT: c_int = 0x4;

pub const BUFFER_SIZE: c_int = 2;
pub const LEVEL: c_int = 3;
pub const DOUBLEBUFFER_MODE: c_int = 5;
pub const STEREO_MODE: c_int = 6;
pub const AUX_BUFFERS: c_int = 7;
pub const RED_SIZE: c_int = 8;
pub const GREEN_SIZE: c_int = 9;
pub const BLUE_SIZE: c_int = 10;
pub const ALPHA_SIZE: c_int = 11;
pub const DEPTH_SIZE: c_int = 12;
pub const STENCIL_SIZE: c_int = 13;
pub const ACCUM_RED_SIZE: c_int = 14;
pub const ACCUM_GREEN_SIZE: c_int = 15;
pub const ACCUM_BLUE_SIZE: c_int = 16;
pub const ACCUM_ALPHA_SIZE: c_int = 17;
pub const VISUAL_CAVEAT_EXT: c_int = 0x20;
pub const VISUAL_TYPE: c_int = 0x22;
pub const TRANSPARENT_TYPE: c_int = 0x23;
pub const TRANSPARENT_INDEX: c_int = 0x24;
pub const TRANSPARENT_RED: c_int = 0x25;
pub const TRANSPARENT_GREEN: c_int = 0x26;
pub const TRANSPARENT_BLUE: c_int = 0x27;
pub const TRANSPARENT_ALPHA: c_int = 0x28;
pub const VISUAL_ID: c_int = 0x800B;
pub const DRAWABLE_TYPE: c_int = 0x8010;
pub const RENDER_TYPE: c_int = 0x8011;
pub const X_RENDERABLE: c_int = 0x8012;

pub const TRUE_COLOR: c_int = 0x8002;
pub const DIRECT_COLOR: c_int = 0x8003;
pub const PSEUDO_COLOR: c_int = 0x8004;
pub const STATIC_COLOR: c_int = 0x8005;
pub const GRAY_SCALE: c_int = 0x8006;
pub const STATIC_GRAY: c_int = 0x8007;

pub const MAJOR_VERSION_ARB: c_int = 0x2091;
pub const MINOR_VERSION_ARB: c_int = 0x2092;
pub const FLAGS_ARB: c_int = 0x2094;
pub const NO_ERROR_ARB: c_int = 0x31B3;

pub const PROFILE_MASK_ARB: c_int = 0x9126;
pub const CORE_PROFILE_BIT_ARB: c_int = 0x1;
pub const COMPAT_PROFILE_BIT_ARB: c_int = 0x2;
pub const ES_PROFILE_BIT_ARB: c_int = 0x4;

pub const RENDER_TYPE_ARB: c_int = 0x8011;

pub const RESET_NOTIFICATION_STRATEGY: c_int = 0x8256;
pub const NO_RESET_NOTIFICATION: c_int = 0x8261;
pub const LOSE_CONTEXT_RESET_NOTIFICATION: c_int = 0x8252;

pub const RELEASE_BEHAVIOR_ARB: c_int = 0x2097;
pub const NONE_RELEASE_BEHAVIOR_ARB: c_int = 0;
pub const FLUSH_RELEASE_BEHAVIOR_ARB: c_int = 0x2098;
