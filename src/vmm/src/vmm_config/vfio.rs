// Copyright 2026 DecOperations. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Configuration for VFIO PCIe passthrough devices.
//!
//! A VFIO device passes a *physical* PCI function — typically an SR-IOV virtual function created on
//! the host (`echo N > /sys/bus/pci/devices/<PF>/sriov_numvfs`) and bound to `vfio-pci` — straight
//! through to the guest. Unlike the rest of the device builders, the device object itself can only
//! be created at boot (it needs guest memory + the MMIO allocator), so this builder just stores the
//! validated configs and the device is instantiated in `builder.rs`.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

/// Strongly-typed equivalent of the JSON body of a VFIO device request.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VfioDeviceConfig {
    /// Unique device id.
    pub vfio_dev_id: String,
    /// Host PCI address of the function to pass through, e.g. `0000:01:00.1`. This is the BDF of an
    /// SR-IOV VF (or any PCI function) already bound to the `vfio-pci` driver on the host.
    pub host_pci_address: String,
}

impl VfioDeviceConfig {
    /// The sysfs path VFIO opens for this device.
    pub fn sysfs_path(&self) -> String {
        format!("/sys/bus/pci/devices/{}", self.host_pci_address)
    }
}

/// Errors associated with VFIO device configuration.
#[derive(Debug, thiserror::Error, displaydoc::Display)]
pub enum VfioConfigError {
    /// A VFIO device with id {0} already exists.
    DeviceIdInUse(String),
    /// Host PCI function {0} is already passed through to this microVM.
    HostAddressInUse(String),
    /// Invalid host PCI address {0}: expected DDDD:BB:DD.F (e.g. 0000:01:00.1).
    InvalidHostAddress(String),
    /// VFIO passthrough requires PCI to be enabled for this microVM.
    PciDisabled,
}

fn validate_bdf(addr: &str) -> Result<(), VfioConfigError> {
    // Expect domain:bus:device.function — DDDD:BB:DD.F.
    let err = || VfioConfigError::InvalidHostAddress(addr.to_string());
    let (domain_bus_dev, func) = addr.split_once('.').ok_or_else(err)?;
    let parts: Vec<&str> = domain_bus_dev.split(':').collect();
    if parts.len() != 3 {
        return Err(err());
    }
    let ok_hex = |s: &str, len: usize| s.len() == len && s.bytes().all(|b| b.is_ascii_hexdigit());
    if !(ok_hex(parts[0], 4) && ok_hex(parts[1], 2) && ok_hex(parts[2], 2) && ok_hex(func, 1)) {
        return Err(err());
    }
    Ok(())
}

/// Builder that accumulates validated VFIO device configs.
#[derive(Debug, Default, Clone)]
pub struct VfioBuilder {
    configs: Vec<VfioDeviceConfig>,
}

impl VfioBuilder {
    /// Create an empty builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Immutable iterator over the configured VFIO devices.
    pub fn iter(&self) -> std::slice::Iter<'_, VfioDeviceConfig> {
        self.configs.iter()
    }

    /// Whether any VFIO devices are configured.
    pub fn is_empty(&self) -> bool {
        self.configs.is_empty()
    }

    /// Insert (or replace, by id) a VFIO device config after validation.
    pub fn insert(&mut self, cfg: VfioDeviceConfig) -> Result<(), VfioConfigError> {
        validate_bdf(&cfg.host_pci_address)?;

        // Reject duplicate host functions (a VF can only be mapped once).
        let mut seen = HashSet::new();
        for existing in &self.configs {
            if existing.vfio_dev_id != cfg.vfio_dev_id {
                seen.insert(existing.host_pci_address.clone());
            }
        }
        if seen.contains(&cfg.host_pci_address) {
            return Err(VfioConfigError::HostAddressInUse(cfg.host_pci_address));
        }

        // Replace on id collision (update semantics), else append.
        if let Some(slot) = self
            .configs
            .iter_mut()
            .find(|c| c.vfio_dev_id == cfg.vfio_dev_id)
        {
            *slot = cfg;
        } else {
            self.configs.push(cfg);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_bdf() {
        assert!(validate_bdf("0000:01:00.1").is_ok());
        assert!(validate_bdf("0000:65:00.0").is_ok());
        assert!(validate_bdf("01:00.1").is_err());
        assert!(validate_bdf("0000:01:00").is_err());
        assert!(validate_bdf("zzzz:01:00.1").is_err());
    }

    #[test]
    fn test_insert_and_dedup() {
        let mut b = VfioBuilder::new();
        b.insert(VfioDeviceConfig {
            vfio_dev_id: "vf0".into(),
            host_pci_address: "0000:01:00.1".into(),
        })
        .unwrap();
        // Same host addr, different id -> conflict.
        let err = b.insert(VfioDeviceConfig {
            vfio_dev_id: "vf1".into(),
            host_pci_address: "0000:01:00.1".into(),
        });
        assert!(matches!(err, Err(VfioConfigError::HostAddressInUse(_))));
        // Same id -> replace.
        b.insert(VfioDeviceConfig {
            vfio_dev_id: "vf0".into(),
            host_pci_address: "0000:02:00.0".into(),
        })
        .unwrap();
        assert_eq!(b.iter().count(), 1);
    }
}
