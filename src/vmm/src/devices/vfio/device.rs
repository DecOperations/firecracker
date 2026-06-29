// Copyright 2026 DecOperations. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! A physical PCI function passed through to the guest via VFIO.

use std::os::fd::AsRawFd;
use std::path::Path;
use std::sync::{Arc, Barrier, Mutex};

use vfio_bindings::bindings::vfio::{
    VFIO_PCI_BAR0_REGION_INDEX, VFIO_PCI_CONFIG_REGION_INDEX, VFIO_PCI_MSIX_IRQ_INDEX,
    VFIO_REGION_INFO_FLAG_MMAP,
};
use vfio_ioctls::{VfioContainer, VfioDevice};
use vm_allocator::AllocPolicy;
use vm_memory::{GuestMemory, GuestMemoryRegion};
use vmm_sys_util::eventfd::EventFd;

use crate::pci::PciDevice;
use crate::pci::configuration::{BAR0_REG_IDX, BarPrefetchable, Bars, NUM_BAR_REGS};
use crate::vstate::bus::BusDevice;
use crate::vstate::memory::GuestMemoryMmap;
use crate::vstate::resources::ResourceAllocator;

/// First config register index of the BAR window (== [`BAR0_REG_IDX`]).
const FIRST_BAR_REG: u16 = BAR0_REG_IDX;
/// Last config register index of the BAR window (BAR5).
const LAST_BAR_REG: u16 = BAR0_REG_IDX + (NUM_BAR_REGS as u16) - 1;

/// Errors that can occur while passing through a VFIO PCI device.
#[derive(Debug, thiserror::Error, displaydoc::Display)]
pub enum VfioPciError {
    /// Failed to open VFIO container/group/device for {path}: {source}
    OpenDevice {
        /// sysfs path of the device
        path: String,
        /// underlying vfio-ioctls error
        source: vfio_ioctls::VfioError,
    },
    /// VFIO container creation failed: {0}
    CreateContainer(vfio_ioctls::VfioError),
    /// Ran out of guest MMIO space allocating BAR {bar}: {source}
    BarAllocation {
        /// BAR index
        bar: u8,
        /// allocator error
        source: vm_allocator::Error,
    },
    /// Failed to DMA-map guest memory region at {gpa:#x} (len {len:#x}): {source}
    DmaMap {
        /// guest physical address
        gpa: u64,
        /// region length
        len: u64,
        /// vfio-ioctls error
        source: vfio_ioctls::VfioError,
    },
    /// Failed to enable MSI-X interrupts: {0}
    EnableMsix(vfio_ioctls::VfioError),
}

/// Per-BAR bookkeeping for a passed-through device.
#[derive(Debug, Clone, Copy)]
pub struct VfioBar {
    /// PCI BAR index (0..=5).
    pub index: u8,
    /// VFIO region index backing this BAR.
    pub region_index: u32,
    /// Guest physical address the BAR was placed at.
    pub gpa: u64,
    /// Size in bytes (power of two).
    pub size: u64,
    /// 64-bit BAR (occupies this slot and the next).
    pub is_64bit: bool,
    /// Prefetchable memory BAR.
    pub prefetchable: bool,
    /// Whether the VFIO region is mmap-able (fast-path eligible).
    pub mmappable: bool,
}

/// A physical PCI function (e.g. an SR-IOV VF) passed through to the guest.
pub struct VfioPciDevice {
    /// Firecracker device id.
    pub id: String,
    /// The VFIO container (owns the IOMMU/DMA mappings); kept alive for the device's lifetime.
    container: Arc<VfioContainer>,
    /// The VFIO device handle.
    device: Arc<VfioDevice>,
    /// Allocated BARs, in PCI index order.
    bars: Vec<VfioBar>,
    /// Emulated BAR registers presented to the guest (addresses are fixed at allocation time;
    /// the bus does not support BAR relocation — see `pci::configuration::Bars::write`).
    bar_regs: Bars,
}

impl std::fmt::Debug for VfioPciDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VfioPciDevice")
            .field("id", &self.id)
            .field("bars", &self.bars)
            .finish_non_exhaustive()
    }
}

impl VfioPciDevice {
    /// Open the VFIO device at `sysfs_path` (e.g. `/sys/bus/pci/devices/0000:01:00.1`), reset it,
    /// enumerate and place its BARs, and DMA-map all guest RAM through the host IOMMU.
    pub fn new(
        id: String,
        sysfs_path: &str,
        guest_memory: &GuestMemoryMmap,
        resource_allocator: &mut ResourceAllocator,
    ) -> Result<Self, VfioPciError> {
        // One container per device for now (each VF is its own IOMMU group in SR-IOV). A later
        // milestone can share a container across devices in the same group.
        let container =
            Arc::new(VfioContainer::new(None).map_err(VfioPciError::CreateContainer)?);
        let device = Arc::new(
            VfioDevice::new(Path::new(sysfs_path), container.clone()).map_err(|source| {
                VfioPciError::OpenDevice {
                    path: sysfs_path.to_string(),
                    source,
                }
            })?,
        );

        // Function-level reset so the guest gets the device in a clean state.
        device.reset();

        let mut dev = VfioPciDevice {
            id,
            container,
            device,
            bars: Vec::new(),
            bar_regs: Bars::default(),
        };

        dev.allocate_bars(resource_allocator)?;
        dev.map_dma(guest_memory)?;

        Ok(dev)
    }

    /// Raw config-space dword read straight from the VFIO config region.
    fn read_config_dword_raw(&self, reg_idx: u16) -> u32 {
        let offset = self.device.get_region_offset(VFIO_PCI_CONFIG_REGION_INDEX) + (reg_idx as u64) * 4;
        let mut buf = [0u8; 4];
        // SAFETY: `buf` is valid for 4 bytes; pread on the VFIO device fd is bounded to the config
        // region by the kernel.
        let n = unsafe {
            libc::pread(
                self.device.as_raw_fd(),
                buf.as_mut_ptr().cast(),
                buf.len(),
                offset as libc::off_t,
            )
        };
        if n != 4 {
            return 0xffff_ffff;
        }
        u32::from_le_bytes(buf)
    }

    /// Enumerate the device's hardware BARs (size/type via the BAR register bits + VFIO region
    /// size), place each in guest MMIO space, and record an emulated BAR register file.
    fn allocate_bars(&mut self, resources: &mut ResourceAllocator) -> Result<(), VfioPciError> {
        let mut idx: u8 = 0;
        while (idx as u16) < (NUM_BAR_REGS as u16) {
            let region_index = u32::from(VFIO_PCI_BAR0_REGION_INDEX) + u32::from(idx);
            let size = self.device.get_region_size(region_index);
            if size == 0 {
                idx += 1;
                continue;
            }
            let flags = self.device.get_region_flags(region_index);
            let mmappable = flags & VFIO_REGION_INFO_FLAG_MMAP != 0;

            // Read the real BAR register to learn memory-vs-IO, 32-vs-64-bit and prefetchable.
            let bar_reg = self.read_config_dword_raw(FIRST_BAR_REG + u16::from(idx));
            let is_io = bar_reg & 0x1 != 0;
            let is_64bit = (bar_reg & 0b110) == 0b100;
            let prefetchable = bar_reg & 0b1000 != 0;

            if is_io {
                // I/O-space BARs are not modeled by this PCI bus; skip (rare on modern VFs).
                idx += 1;
                continue;
            }

            let pref = if prefetchable {
                BarPrefetchable::Yes
            } else {
                BarPrefetchable::No
            };

            if is_64bit {
                // 64-bit BARs are placed in the high MMIO aperture (256 GiB window).
                let gpa = resources
                    .mmio64_memory
                    .allocate(size, size, AllocPolicy::FirstMatch)
                    .map_err(|source| VfioPciError::BarAllocation { bar: idx, source })?
                    .start();
                self.bar_regs.set_bar_64(idx, gpa, size, pref);
                self.bars.push(VfioBar {
                    index: idx,
                    region_index,
                    gpa,
                    size,
                    is_64bit: true,
                    prefetchable,
                    mmappable,
                });
                idx += 2; // consumes two register slots
            } else {
                // 32-bit BARs come from the sub-4G window.
                let gpa = resources
                    .mmio32_memory
                    .allocate(size, size, AllocPolicy::FirstMatch)
                    .map_err(|source| VfioPciError::BarAllocation { bar: idx, source })?
                    .start();
                self.bar_regs.set_bar_32(idx, gpa, size, pref);
                self.bars.push(VfioBar {
                    index: idx,
                    region_index,
                    gpa,
                    size,
                    is_64bit: false,
                    prefetchable,
                    mmappable,
                });
                idx += 1;
            }
        }
        Ok(())
    }

    /// Map every guest RAM region into the device's IOMMU domain so the device can DMA to guest
    /// memory. (Identity map: IOVA == guest physical address.)
    fn map_dma(&self, guest_memory: &GuestMemoryMmap) -> Result<(), VfioPciError> {
        for region in guest_memory.iter() {
            let gpa = region.start_addr().0;
            let len = region.len();
            let host_addr = region.as_ptr();
            // SAFETY: `host_addr` points at a mapped guest memory region of `len` bytes that
            // outlives the container (held in `self.container`); IOVA == GPA identity map.
            unsafe {
                self.container
                    .vfio_dma_map(gpa, len as usize, host_addr)
                    .map_err(|source| VfioPciError::DmaMap { gpa, len, source })?;
            }
        }
        Ok(())
    }

    /// Hand the kernel the EventFds that should be signalled for each MSI-X vector. These are the
    /// `notifier()` fds of an [`crate::vstate::interrupts::MsixVectorGroup`], already wired to KVM
    /// MSI routes via irqfd. The guest's MSI-X table writes drive enable/mask (TODO: full table
    /// emulation — Milestone 1 task 11).
    pub fn enable_msix(&self, event_fds: Vec<&EventFd>) -> Result<(), VfioPciError> {
        self.device
            .enable_irq(VFIO_PCI_MSIX_IRQ_INDEX, event_fds)
            .map_err(VfioPciError::EnableMsix)
    }

    /// Number of MSI-X vectors the hardware exposes.
    pub fn max_msix_vectors(&self) -> u32 {
        self.device.max_interrupts()
    }

    /// The placed BARs, for registration on the system MMIO bus.
    pub fn bars(&self) -> &[VfioBar] {
        &self.bars
    }

    /// Find the BAR whose window contains guest physical `addr`, returning the VFIO region offset.
    fn bar_for_addr(&self, addr: u64) -> Option<(&VfioBar, u64)> {
        self.bars.iter().find_map(|bar| {
            if addr >= bar.gpa && addr < bar.gpa + bar.size {
                Some((bar, addr - bar.gpa))
            } else {
                None
            }
        })
    }
}

impl PciDevice for VfioPciDevice {
    fn read_config_register(&mut self, reg_idx: u16) -> u32 {
        // BAR registers are virtualized: present the guest the addresses we assigned, not the
        // host's. Everything else is forwarded from the real device's config space.
        if (FIRST_BAR_REG..=LAST_BAR_REG).contains(&reg_idx) {
            let mut data = [0u8; 4];
            self.bar_regs.read(u8::try_from(reg_idx - FIRST_BAR_REG).unwrap(), 0, &mut data);
            return u32::from_le_bytes(data);
        }
        self.read_config_dword_raw(reg_idx)
    }

    fn write_config_register(
        &mut self,
        reg_idx: u16,
        offset: u8,
        data: &[u8],
    ) -> Option<Arc<Barrier>> {
        // BAR writes go to the emulated register file (size probe + relocation no-op). The bus
        // does not support BAR relocation, so the address stays where we placed it.
        if (FIRST_BAR_REG..=LAST_BAR_REG).contains(&reg_idx) {
            self.bar_regs
                .write(u8::try_from(reg_idx - FIRST_BAR_REG).unwrap(), offset, data);
            return None;
        }
        // Forward other config writes (command register, etc.) to the device.
        let region_off =
            self.device.get_region_offset(VFIO_PCI_CONFIG_REGION_INDEX) + (reg_idx as u64) * 4 + (offset as u64);
        // SAFETY: bounded write into the device's config region.
        unsafe {
            libc::pwrite(
                self.device.as_raw_fd(),
                data.as_ptr().cast(),
                data.len(),
                region_off as libc::off_t,
            );
        }
        None
    }

    fn read_bar(&mut self, base: u64, offset: u64, data: &mut [u8]) {
        if let Some((bar, bar_off)) = self.bar_for_addr(base + offset) {
            let region_off = self.device.get_region_offset(bar.region_index) + bar_off;
            // SAFETY: bounded read from the device's BAR region.
            unsafe {
                libc::pread(
                    self.device.as_raw_fd(),
                    data.as_mut_ptr().cast(),
                    data.len(),
                    region_off as libc::off_t,
                );
            }
        }
    }

    fn write_bar(&mut self, base: u64, offset: u64, data: &[u8]) -> Option<Arc<Barrier>> {
        if let Some((bar, bar_off)) = self.bar_for_addr(base + offset) {
            let region_off = self.device.get_region_offset(bar.region_index) + bar_off;
            // SAFETY: bounded write into the device's BAR region.
            unsafe {
                libc::pwrite(
                    self.device.as_raw_fd(),
                    data.as_ptr().cast(),
                    data.len(),
                    region_off as libc::off_t,
                );
            }
        }
        None
    }
}

impl BusDevice for VfioPciDevice {
    fn read(&mut self, base: u64, offset: u64, data: &mut [u8]) {
        self.read_bar(base, offset, data)
    }

    fn write(&mut self, base: u64, offset: u64, data: &[u8]) -> Option<Arc<Barrier>> {
        self.write_bar(base, offset, data)
    }
}

// VFIO passthrough devices are bound to host resources (device fd + IOMMU mappings) and are not
// snapshot/migration-safe. We deliberately do not implement `Persist` for them; the device manager
// rejects snapshots when a VFIO device is attached. See `docs/vfio-passthrough-plan.md` task 14.
#[allow(dead_code)]
type _AssertNotPersisted = Arc<Mutex<VfioPciDevice>>;
