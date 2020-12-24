// MIT/Apache2 License

//! Attempt to load the DRI library, given that the DRM library is present.

mod pci_table;

use crate::{
    cstr::{const_cstr, ConstCstr},
    dll::Dll,
    dri::{ffi, ExtensionContainer},
    mesa,
};
use std::{
    borrow::Cow,
    ffi::{CStr, OsString},
    mem,
    os::{
        raw::{c_char, c_int},
        unix::ffi::OsStringExt,
    },
    ptr::{self, NonNull},
    slice,
};

const DRM_PLATFORM_DEVICE_NAME_LEN: usize = 512;
const DRM_HOST1X_DEVICE_NAME_LEN: usize = 512;

// FFI Naughtiness

const DRM_BUS_PCI: c_int = 0;

#[repr(C)]
struct DrmVersion {
    version_major: c_int,
    version_minor: c_int,
    version_patchlevel: c_int,
    name_len: c_int,
    name: *mut c_char,
    date_len: c_int,
    date: *mut c_char,
    desc_len: c_int,
    desc: *mut c_char,
}

#[repr(C)]
struct DrmPciBusInfo {
    domain: u16,
    bus: u8,
    dev: u8,
    func: u8,
}

#[repr(C)]
struct DrmPciDeviceInfo {
    vendor_id: u16,
    device_id: u16,
    subvendor_id: u16,
    subdevice_id: u16,
    revision_id: u16,
}

#[repr(C)]
struct DrmUsbBusInfo {
    bus: u8,
    dev: u8,
}

#[repr(C)]
struct DrmUsbDeviceInfo {
    vendor: u16,
    product: u16,
}

#[repr(C)]
struct DrmPlatformBusInfo {
    fullname: [c_char; DRM_PLATFORM_DEVICE_NAME_LEN],
}

#[repr(C)]
struct DrmPlatformDeviceInfo {
    compatible: *mut *mut c_char,
}

#[repr(C)]
struct DrmHost1xBusInfo {
    fullname: [c_char; DRM_HOST1X_DEVICE_NAME_LEN],
}

#[repr(C)]
struct DrmHost1xDeviceInfo {
    compatible: *mut *mut c_char,
}

#[repr(C)]
union DrmBusInfo {
    pci: *mut DrmPciBusInfo,
    usb: *mut DrmUsbBusInfo,
    platform: *mut DrmPlatformBusInfo,
    host1x: *mut DrmHost1xBusInfo,
}

#[repr(C)]
union DrmDeviceInfo {
    pci: *mut DrmPciDeviceInfo,
    usb: *mut DrmUsbDeviceInfo,
    platform: *mut DrmPlatformDeviceInfo,
    host1x: *mut DrmHost1xDeviceInfo,
}

#[repr(C)]
struct DrmDevice {
    nodes: *mut *mut c_char,
    available_nodes: c_int,
    bustype: c_int,
    businfo: DrmBusInfo,
    deviceinfo: DrmDeviceInfo,
}

type DrmGetVersion = unsafe extern "C" fn(c_int) -> *mut DrmVersion;
const DRM_GET_VERSION: ConstCstr<'static> = const_cstr(&*b"drmGetVersion");

type DrmFreeVersion = unsafe extern "C" fn(*mut DrmVersion);
const DRM_FREE_VERSION: ConstCstr<'static> = const_cstr(&*b"drmFreeVersion");

type DrmGetDevice2 = unsafe extern "C" fn(c_int, u32, *mut *mut DrmDevice) -> c_int;
const DRM_GET_DEVICE2: ConstCstr<'static> = const_cstr(&*b"drmGetDevice2");

type DrmFreeDevice = unsafe extern "C" fn(*mut DrmDevice);
const DRM_FREE_DEVICE: ConstCstr<'static> = const_cstr(&*b"drmFreeDevice");

#[inline]
pub(crate) fn driver_name_from_kernel_name(drm: &Dll, fd: c_int) -> breadx::Result<String> {
    let drmGetVersion: DrmGetVersion =
        unsafe { drm.function(&DRM_GET_VERSION) }.expect("drmGetVersion not present");
    let drmFreeVersion: DrmFreeVersion =
        unsafe { drm.function(&DRM_FREE_VERSION) }.expect("drmFreeVersion not present");

    let version: *mut DrmVersion = unsafe { (drmGetVersion)(fd) };

    if version.is_null() {
        return Err(breadx::BreadError::StaticMsg(
            "Failed to get driver name for FD",
        ));
    }

    // convert the name to a slice of c_chars
    let name = unsafe { slice::from_raw_parts((*version).name, (*version).name_len as usize + 1) };

    // convert the name to its OsString equivalent
    let name = OsString::from_vec(name.iter().map(|c| *c as u8).collect());

    // free the version data
    unsafe { (drmFreeVersion)(version) };

    Ok(name
        .to_str()
        .ok_or(breadx::BreadError::StaticMsg("Failed string conversion"))?
        .to_string())
}

#[inline]
fn ids_from_pci_id(drm: &Dll, fd: c_int) -> Option<(c_int, c_int)> {
    let drmGetDevice2: DrmGetDevice2 =
        unsafe { drm.function(&DRM_GET_DEVICE2) }.expect("drmGetDevice2 not present");
    let drmFreeDevice: DrmFreeDevice =
        unsafe { drm.function(&DRM_FREE_DEVICE) }.expect("drmFreeDevice not present");

    let mut device: *mut DrmDevice = ptr::null_mut();

    // get the device
    if unsafe { (drmGetDevice2)(fd, 0, &mut device) != 0 } {
        return None;
    }

    let mut device = NonNull::new(device).unwrap();
    let device = unsafe { device.as_mut() };

    if device.bustype != DRM_BUS_PCI {
        unsafe { (drmFreeDevice)(device) };
        return None;
    }

    let vendor_id = unsafe { (*device.deviceinfo.pci).vendor_id };
    let chip_id = unsafe { (*device.deviceinfo.pci).device_id };
    unsafe { (drmFreeDevice)(device) };

    Some((vendor_id as _, chip_id as _))
}

#[inline]
fn driver_name_from_pci(drm: &Dll, fd: c_int) -> breadx::Result<&'static str> {
    let (vendor_id, chip_id) =
        ids_from_pci_id(drm, fd).ok_or(breadx::BreadError::StaticMsg("Failed to load PCI IDs"))?;

    pci_table::PCI_TABLE
        .iter()
        .find_map(|entry| {
            if vendor_id != entry.id {
                return None;
            }

            if let Some(predicate) = entry.pred {
                if !(predicate)(drm, fd) {
                    return None;
                }
            }

            match entry.pci_ids.len() {
                0 => Some(entry.driver),
                _ => {
                    if entry.pci_ids.contains(&chip_id) {
                        Some(entry.driver)
                    } else {
                        None
                    }
                }
            }
        })
        .ok_or_else(|| breadx::BreadError::StaticMsg("Unable to match ID to driver"))
}

const POSSIBLE_DRI_LEN: usize = 2;

#[inline]
fn dri_lib_name(name: &str) -> [String; POSSIBLE_DRI_LEN] {
    [format!("dri/{}-dri.so", name), format!("{}-dri.so", name)]
}

#[inline]
pub(crate) fn load_dri_driver(
    fd: c_int,
    extensions: &mut Vec<*const ffi::__DRIextension>,
) -> breadx::Result<Dll> {
    cfg_if::cfg_if! {
        if #[cfg(feature = "async")] {
            async_io::block_on(load_dri_driver_async(fd, extensions))
        } else {
            let drm = mesa::drm()?;
            let driver_name: Cow<'static, str> = match driver_name_from_pci(drm, fd) {
                Ok(driver) => driver.into(),
                Err(_) => driver_name_from_kernel_name(drm, fd)?.into(),
            };

            let dlls = dri_lib_name(&driver_name);
            let dll = Dll::load("DRI", &dlls)?;

            extensions.extend_from_slice(super::extensions::load_extensions(&dll, &driver_name)?);
            Ok(dll)
        }
    }
}

#[cfg(feature = "async")]
#[inline]
pub(crate) async fn load_dri_driver_async(
    fd: c_int,
    extensions: &mut Vec<*const ffi::__DRIextension>,
) -> breadx::Result<Dll> {
    let drm = mesa::drm_async().await?;
    let driver_name: Cow<'static, str> = {
        match blocking::unblock(move || driver_name_from_pci(drm, fd)).await {
            Ok(driver) => driver.into(),
            Err(_) => blocking::unblock(move || driver_name_from_kernel_name(drm, fd))
                .await?
                .into(),
        }
    };

    let dlls = dri_lib_name(&driver_name);
    let dll = blocking::unblock(move || Dll::load("DRI", &dlls)).await?;

    let mut ext_container = vec![];
    let (dll, e) =
        blocking::unblock(
            move || match super::extensions::load_extensions(&dll, &driver_name) {
                Err(e) => Err(e),
                Ok(a) => Ok((
                    dll,
                    a.iter()
                        .cloned()
                        .map(|e| ExtensionContainer(e))
                        .collect::<Box<[ExtensionContainer]>>(),
                )),
            },
        )
        .await?;
    ext_container.extend(e.into_iter());
    extensions.extend(ext_container.into_iter().map(|e| e.0));
    Ok(dll)
}
