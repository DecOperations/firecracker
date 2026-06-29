// Copyright 2026 DecOperations. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Real PCIe passthrough of a physical PCI function (e.g. an SR-IOV virtual function) into the
//! guest, using the host kernel VFIO API.
//!
//! Milestone 1 scope: open a VFIO group/container/device for a single-function VF, expose its
//! config space and BARs to the guest PCI bus, DMA-map guest memory through the host IOMMU, route
//! its MSI-X vectors via KVM irqfd, and FLR-reset it on attach. See
//! `docs/vfio-passthrough-plan.md`.

/// The passthrough PCI device implementation (`PciDevice` + `BusDevice`).
pub mod device;

pub use device::{VfioPciDevice, VfioPciError};
