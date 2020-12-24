// MIT/Apache2 License

use super::driver_name_from_kernel_name;
use crate::{
    auto::pci_ids,
    cstr::{const_cstr, ConstCstr},
    dll::Dll,
};
use std::{
    env,
    ffi::c_void,
    mem,
    os::raw::{c_int, c_ulong},
};

// FFI Nonsense
const NOUVEAU_GETPARAM_CHIPSET_ID: u64 = 11;
const DRM_NOUVEAU_GETPARAM: c_ulong = 0x00;

#[repr(C)]
struct DrmNouveauGetParam {
    param: u64,
    value: u64,
}

type DrmCommandWriteRead = unsafe extern "C" fn(c_int, c_ulong, *mut c_void, c_ulong) -> c_int;
const DRM_COMMAND_WRITE_READ: ConstCstr<'static> = const_cstr(&*b"drmCommandWriteRead");

pub(crate) type Predicate = Option<&'static dyn Fn(&Dll, c_int) -> bool>;

// Entry in the PCI table
pub(crate) struct PciTableEntry {
    pub id: c_int,
    pub driver: &'static str,
    pub pci_ids: &'static [c_int],
    pub pred: Predicate,
}

impl PciTableEntry {
    #[inline]
    const fn new(id: c_int, name: &'static str, pci_ids: &'static [c_int]) -> PciTableEntry {
        Self {
            id,
            driver: name,
            pci_ids,
            pred: None,
        }
    }

    #[inline]
    const fn with_predicate(id: c_int, name: &'static str, pred: Predicate) -> PciTableEntry {
        Self {
            id,
            driver: name,
            pci_ids: &[],
            pred,
        }
    }
}

pub(crate) const PCI_TABLE: [PciTableEntry; 12] = [
    PciTableEntry::new(0x8086, "i915", &pci_ids::I915_PCI_IDS),
    PciTableEntry::new(0x8086, "i965", &pci_ids::I965_PCI_IDS),
    PciTableEntry::with_predicate(0x8086, "iris", Some(&is_kernel_i915)),
    PciTableEntry::new(0x1002, "radeon", &pci_ids::RADEON_PCI_IDS),
    PciTableEntry::new(0x1002, "r200", &pci_ids::R200_PCI_IDS),
    PciTableEntry::new(0x1002, "r300", &pci_ids::R300_PCI_IDS),
    PciTableEntry::new(0x1002, "r600", &pci_ids::R600_PCI_IDS),
    PciTableEntry::with_predicate(0x1002, "radeonsi", None),
    PciTableEntry::with_predicate(0x10de, "nouveau_vieux", Some(&is_nouveau_vieux)),
    PciTableEntry::with_predicate(0x10de, "nouveau", None),
    PciTableEntry::new(0x1af4, "virtio_gpu", &pci_ids::VIRTIO_GPU_PCI_IDS),
    PciTableEntry::new(0x15ad, "vmwgfx", &pci_ids::VMWGFX_PCI_IDS),
];

fn is_kernel_i915(drm: &Dll, fd: c_int) -> bool {
    match driver_name_from_kernel_name(drm, fd) {
        Ok(name) => name.as_str() == "i915",
        Err(_) => false,
    }
}

#[inline]
fn nouveau_chipset(drm: &Dll, fd: c_int) -> Option<c_int> {
    let drmCommandWriteRead: DrmCommandWriteRead =
        unsafe { drm.function(&DRM_COMMAND_WRITE_READ) }.expect("drmCommandWriteRead not found");

    let mut gp = DrmNouveauGetParam {
        param: NOUVEAU_GETPARAM_CHIPSET_ID,
        value: 0,
    };
    if unsafe {
        (drmCommandWriteRead)(
            fd,
            DRM_NOUVEAU_GETPARAM,
            &mut gp as *mut DrmNouveauGetParam as *mut c_void,
            mem::size_of::<DrmNouveauGetParam>() as _,
        )
    } != 0
    {
        None
    } else {
        Some(gp.value as _)
    }
}

fn is_nouveau_vieux(drm: &Dll, fd: c_int) -> bool {
    match nouveau_chipset(drm, fd) {
        None => false,
        Some(chipset) => {
            (chipset > 0 && chipset < 0x30)
                || (chipset < 0x40 && env::var_os("NOUVEAU_VIEUX").is_some())
        }
    }
}
