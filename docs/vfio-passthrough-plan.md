# VFIO PCIe Passthrough (SR-IOV VFs) — Implementation Plan

**Repo:** `DecOperations/firecracker-next` · **Status:** design / comprehension pass (no source modified) ·
**Scope:** map a *physical* PCIe function (an SR-IOV VF) into the guest via the kernel VFIO API.
SR-IOV VFs are created on the host (`echo N > .../sriov_numvfs`); each VF is just a PCI function to
pass through, so the VMM work is **generic VFIO PCI passthrough** — SR-IOV adds no VMM-side logic.

> **Critical distinction (confirmed in tree):** this fork ships **virtio-PCI** (a virtio transport that
> *looks* like a PCI device) with MSI-X and PCI hotplug. That is **emulation**, not passthrough. There is
> **no VFIO/IOMMU code anywhere** in the tree (the only `vfio|iommu|passthrough` hits are a CPUID
> cache-topology comment and a KVM-cap comment). VFIO is a brand-new device class that reuses the
> existing PCI bus / MSI-X / allocator seams.

---

## 0. Build reality check (Phase 1.0)

- **PCI is compiled unconditionally.** `pub mod pci;` is ungated in `src/vmm/src/devices/mod.rs:16`
  and `src/vmm/src/pci/mod.rs`. The only cargo features in `src/vmm/Cargo.toml:11-15` are
  `tracing`, `gdb`, `fuzzing` — none gate PCI.
- **PCI is enabled at runtime per-microVM** via `VmResources.pci_enabled`
  (`src/vmm/src/resources.rs:136`), consumed in `src/vmm/src/builder.rs:216-220`
  (`device_manager.enable_pci(&kvm_vm)` else `cmdline pci=off`). VFIO will **require `pci_enabled = true`**.
- The PCI stack is a **Cloud-Hypervisor / crosvm-derived** in-tree implementation (its own `PciDevice`
  trait, `PciConfiguration`, `MsixConfig`), **not** the `rust-vmm/pci` crate. This matters for Phase 2:
  we implement a `VfioPciDevice` against the **fork's own** `PciDevice` + `BusDevice` traits and pull in
  `vfio-ioctls` for the container/device layer — we do **not** adopt rust-vmm `vm-device`/`pci`.

---

## (a) Phase 1 — Infrastructure map

### 1.1 Device model & transport split

| Concern | Location | Notes |
|---|---|---|
| Core PCI device trait | `src/vmm/src/pci/mod.rs:32` `trait PciDevice` | methods: `write_config_register(reg_idx,offset,data)`, `read_config_register(reg_idx)->u32`, `read_bar(base,offset,data)`, `write_bar(base,offset,data)`. **No** `bar_regions()`, capability list, `id()`, or IRQ hooks. |
| PCI bus + config-space handlers | `src/vmm/src/pci/bus.rs` | `PciBus` (`:79`) `devices: HashMap<u8, Arc<Mutex<dyn PciDevice>>>`, `add_device` (`:110`) is **fully generic**. `PciConfigIo` (`0xCF8/0xCFC`, `:154`) and `PciConfigMmio` (ECAM, `:306`). `PciRoot` host bridge (`:34`). |
| Config space (type-0) | `src/vmm/src/pci/configuration.rs:193` `PciConfiguration` | flat `[u32;1024]` + writable-bits mask; type-0 only (`new_type0` `:203`); `add_capability` (`:338`, MSI/MSI-X writable masks special-cased). |
| BAR model | `src/vmm/src/pci/configuration.rs:44-172` `Bar`/`Bars` | 6 slots; **only constructor is `set_bar_64` (`:93`)** → 64-bit hardwired (`is_64_bit=0b100`). |
| PCI segment (domain) | `src/vmm/src/devices/pci/pci_segment.rs:30` | one segment = one `PciBus` + ECAM window + mmio32/mmio64 apertures (`:48-52`, sourced from `ResourceAllocator`); ACPI `_CRS` emits the apertures (`:363-414`). |
| virtio-PCI transport | `src/vmm/src/devices/virtio/transport/pci/device.rs:260` `VirtioPciDevice` | wraps `dyn VirtioDevice`; owns its `PciConfiguration` + `Bars`; one 64-bit BAR (`VIRTIO_BAR_INDEX=0`, `CAPABILITY_BAR_SIZE=0x80000`, `:55/:230`); `impl PciDevice` (`:770`) + `impl BusDevice` (`:1019`); `read_bar/write_bar` demux sub-regions (`:836/:895`). |
| virtio-MMIO transport (contrast) | `src/vmm/src/devices/virtio/transport/mmio.rs:53` `MmioTransport` | `impl BusDevice` only; no config space, no BARs. |
| **Shared seam** | `src/vmm/src/vstate/bus.rs:20` `trait BusDevice` (`read`/`write`) + `BusDeviceSync` (`:31`) | **There is no virtio `Transport` trait.** The only common interface is `BusDevice` (for MMIO/BAR windows on `mmio_bus`) plus `PciDevice` (for config space). **These two traits are exactly the seam a `VfioPciDevice` implements.** |

**Hard constraint:** `PciConfigIo`/`PciConfigMmio` reject `bus != 0` and **`function > 0`**
(`bus.rs:180-187, 220-227, 319-327, 344-353` — verified). The model is **single-bus, single-function**.
A multi-function physical device cannot be represented as-is. An SR-IOV **VF is a single function**, so
Milestone 1 is unaffected; multi-function passthrough is a later constraint to lift.

### 1.2 Interrupt model (legacy IRQ vs MSI/MSI-X)

- **MSI-X table/PBA emulation:** `src/vmm/src/pci/msix.rs` — `MsixConfig` (`:70`), `MsixTableEntry`
  (`:30`), `MsixCap` (`:470`); up to 2048 vectors/device. Guest table writes → build `MsixVectorConfig`
  → `MsixVectorGroup::update(...)` (`msix.rs:279-342`).
- **GSI + irqfd + MSI route plumbing:** `src/vmm/src/vstate/interrupts.rs`
  - `MsixVector { gsi, event_fd, enabled }` (`:45`); `enable()` → `vmfd.register_irqfd(event_fd, gsi)` =
    **`KVM_IRQFD`** (`:67`); `notifier(index)` exposes the raw `EventFd` (`:134`).
  - `MsixVectorGroup::update(index, cfg, masked, set_gsi)` (`:139`) sequences:
    `register_msi` → `set_gsi_routes` → `enable` (irqfd registered **after** routing).
- **KVM routing:** `src/vmm/src/vstate/vm.rs` — `register_msi` (`KVM_IRQ_ROUTING_MSI`, sets
  `KVM_MSI_VALID_DEVID` when supported, `:667`), `set_gsi_routes` (`KVM_SET_GSI_ROUTING`, single commit
  point, `:715`), `create_msix_group(vm, count)` (`:697`), `register_irq` (legacy IOAPIC pin, `:634`).
- **GSI budget:** MSI GSIs `24..=4095` (`GSI_MSI_NUM=4072`), max `KVM_MAX_IRQ_ROUTES=4096`
  (`arch/x86_64/layout.rs:28-38`; aarch64 mirrors). Allocated via `IdAllocator`
  (`vstate/resources.rs:73`, `allocate_gsi_msi` `:108`).
- **Legacy path (contrast):** virtio-MMIO uses one shared `IrqTrigger` (`mmio.rs:393`) + IOAPIC pin via
  `register_irq` — no per-vector routing, no resample fd.

**Reusable for VFIO:** `KvmVm::create_msix_group` + `MsixVectorGroup::{update,notifier}` give exactly
"N irqfds, each wired to a KVM MSI route." A VFIO device reuses these directly and hands each
`notifier()` EventFd to `VFIO_DEVICE_SET_IRQS`. **Missing:** a resample-fd path for level/INTx
(`register_irqfd` has no resamplefd today), and any non-virtio consumer of `MsixVectorGroup`.

### 1.3 Guest-physical memory layout

**x86_64 (`src/vmm/src/arch/x86_64/layout.rs`):**

| Region | Value | Line |
|---|---|---|
| 32-bit device-MMIO window | `[0xC000_1000, 0xEEC0_0000)` ≈ **750 MiB** (`MEM_32BIT_DEVICES_*`) | `:118-120` |
| PCIe ECAM (MMCONFIG) | `0xEEC0_0000`, 256 MiB | `:105-107` |
| IOAPIC / APIC | `0xFEC0_0000` / `0xFEE0_0000` | `:58-61` |
| **64-bit MMIO aperture** | **`[256 GiB, 512 GiB)` = 256 GiB** (`MMIO64_MEM_START/SIZE`) | `:124-126` |
| Past-64-bit (hotplug: virtio-mem/pmem) | `[512 GiB, 1 TiB)` = 512 GiB | `:134-136` |

**aarch64 (`src/vmm/src/arch/aarch64/layout.rs`):** 32-bit window `[0x4000_3000, 0x7000_0000)` ≈ 768 MiB
(`:125-127`); identical 256 GiB 64-bit aperture at 256 GiB (`:131-133`).

**Takeaway:** A large >4 GiB MMIO aperture **already exists** (256 GiB at 256 GiB). GPU BARs fit there in
principle. The sub-4G window (~750 MiB) is small and today only hands out 4 KiB virtio-MMIO slots — not
enough for big 32-bit BARs, but VFs typically use 64-bit BARs.

### 1.4 Address / IRQ allocators

`src/vmm/src/vstate/resources.rs:67-92` `ResourceAllocator::new()` builds (vm-allocator `0.1.3`):

- `mmio32_memory` ← `[0xC000_1000, +~750 MiB)` (`:74`)
- `mmio64_memory` ← `[256 GiB, +256 GiB)` (`:79`) — **primary BAR allocator** for virtio-PCI
- `past_mmio64_memory` ← `[512 GiB, +512 GiB)` (`:84`) — virtio-mem/pmem hotplug
- `system_memory` ← ACPI/vmgenid (`:89`)
- `gsi_legacy_allocator` (5..23), `gsi_msi_allocator` (24..4095) `IdAllocator`s (`:71-73`)

BAR assignment today: `VirtioPciDevice::allocate_bars(&mut mmio64_memory)`
(`pci_mngr.rs:170` → `transport/pci/device.rs:343-361`) → single 512 KiB 64-bit BAR0. **No code path
enumerates a device's real BARs and requests their sizes/alignment** — that is new for VFIO.

### 1.5 API / config plumbing (template: network device)

1. **Config + builder:** `src/vmm/src/vmm_config/net.rs` — `NetworkInterfaceConfig` (`:20-33`),
   `NetBuilder` (`:82`), `build` validates + instantiates (`:107-141`). (`drive.rs` block builder is a good
   model for a config that carries *either* a host BDF *or* a `/dev/vfio/<group>` path, like its
   virtio/vhost-user split.)
2. **Central store:** `src/vmm/src/resources.rs:108` `VmResources` (`net_builder` `:120`, `block` `:114`,
   `pci_enabled` `:136`); `build_net_device` (`:370`).
3. **API:** swagger `src/firecracker/swagger/firecracker.yaml` (`/network-interfaces/{id}` `:707`, def
   `:1522`); parser `src/firecracker/src/api_server/request/net.rs:11-40` → `VmmAction::InsertNetworkDevice`;
   route table `parsed_request.rs:108-110`.
4. **Action handler:** `src/vmm/src/rpc_interface.rs` — `VmmAction::InsertNetworkDevice` (`:94`),
   dispatch (`:476`), `insert_net_device` (`:540-549`).
5. **Boot instantiation:** `src/vmm/src/builder.rs` — `attach_net_devices` (`:695-715`) calls
   `device_manager.attach_virtio_device(...)` (`:705`). The **PCI-vs-MMIO transport choice is in the
   device manager**, `device_manager/mod.rs:279-307 attach_virtio_device` →
   `if is_pci_enabled() { pci_devices.attach_pci_virtio_device } else { attach_mmio_virtio_device }`.
6. **PCI registration core (reusable):** `src/vmm/src/device_manager/pci_mngr.rs` —
   `attach_common` (`:109-139`: `pci_bus.add_device` `:121`, record in map, `register_bars_with_bus` `:130`,
   ioeventfds `:133`), `attach_pci_virtio_device` (`:141-175`: alloc SBDF `:150`, MSI-X group sized to
   queue count `:157-160`, build device `:163`, `allocate_bars` `:170`), `register_bars_with_bus`
   (`mmio_bus.insert` `:89-107`).
7. **Persistence:** `src/vmm/src/device_manager/persist.rs` + `pci_mngr.rs:225-257 PciDevicesState`;
   per-type vecs in `DeviceStates`; `restore_pci_device` (`pci_mngr.rs:177-204`).

### 1.6 Jailer + seccomp

- **Jailer device nodes:** `src/jailer/src/env.rs` — `mknod_and_own_dev(path, major, minor)` (`:422-452`,
  `mknod(S_IFCHR…, makedev)` + chown), `FOLDER_HIERARCHY` (`:65`, currently `["/","/dev","/dev/net","/run"]`),
  existing nodes `/dev/kvm` (10:232), `/dev/net/tun` (10:200), `/dev/userfaultfd` (minor discovered at
  runtime) created at `:681-707`.
- **Seccomp:** `resources/seccomp/x86_64-unknown-linux-musl.json` (+ aarch64). `ioctl` is **allow-listed
  per request number** via arg matching (`{"syscall":"ioctl","args":[{"index":1,"type":"dword","op":"eq",
  "val":<req>}]}`); see the KVM ioctl block `:574-643`. `mmap`/`pread64`/`pwrite64`/`open`/`read`/`write`
  already allowed.

---

## (b) Phase 2 — Gap analysis to VFIO passthrough

For each capability: **what exists**, **what's missing**, **where it hooks in**. rust-vmm crates noted.

### G1. Open VFIO group/container + device, map regions
- **Exists:** nothing (no VFIO code).
- **Missing:** container (`/dev/vfio/vfio`) + group (`/dev/vfio/<N>`) + device open; `VFIO_GET_API_VERSION`,
  `VFIO_CHECK_EXTENSION(VFIO_TYPE1v2_IOMMU)`, `VFIO_GROUP_SET_CONTAINER`, `VFIO_SET_IOMMU`,
  `VFIO_GROUP_GET_DEVICE_FD`, `VFIO_DEVICE_GET_INFO`, `VFIO_DEVICE_GET_REGION_INFO` (per BAR + config +
  ROM), `VFIO_DEVICE_GET_IRQ_INFO`.
- **Crate:** **`vfio-ioctls`** (`VfioContainer`, `VfioDevice`, `VfioRegionInfo`) + `vfio-bindings`. Use them
  directly; don't reimplement the ioctls.
- **Hooks:** new module `src/vmm/src/devices/vfio/` (e.g. `mod.rs`, `device.rs`). A `VfioDevice` wrapper is
  built in a new `attach_pci_vfio_device` next to `pci_mngr.rs:141`.

### G2. Map device BARs (incl. 64-bit/large) into guest PA; expose config space
- **Exists:** generic bus attach (`PciBus::add_device`, `pci_mngr.rs:121`), a 64-bit MMIO allocator
  (`mmio64_memory`), and the `read_bar/write_bar` + `mmio_bus.insert` mechanism.
- **Missing:**
  - **Real-BAR enumeration:** read each `VFIO_DEVICE_GET_REGION_INFO`, learn size/flags (64-bit,
    prefetchable, mmap-able), allocate from `mmio32_memory`/`mmio64_memory`. The current BAR API
    (`configuration.rs:93 set_bar_64`) is **64-bit-only** — add `set_bar_32` / a generic
    `add_pci_bar(idx, size, type, prefetchable)`.
  - **BAR forwarding:** for mmap-able regions, `mmap` the VFIO region fd and back the guest BAR window with
    a KVM memslot (`KVM_SET_USER_MEMORY_REGION`) for near-native MMIO; for non-mmap-able regions, trap via
    `BusDevice::read/write` → `pread/pwrite` on the device fd at the region offset. A `VfioPciDevice`
    implements both `PciDevice` (config) and `BusDevice` (BAR MMIO).
  - **Config-space virtualization:** expose the VF's config space through `read_config_register/
    write_config_register`, but **virtualize** BARs (present guest-assigned addresses, intercept the
    command register, hide/limit capabilities). See G5.
  - **BAR relocation:** **the bus does not support it** — `configuration.rs:125-140 Bars::write` is a
    deliberate no-op (verified: *"There is no BAR relocation support as of right now"*), and BARs are
    frozen to firmware via ACPI `_DSM` fn 5 (`pci_segment.rs:318-325`). VFIO must allocate the BAR GPA up
    front and present it as fixed. **Risk** if a guest insists on moving a BAR.
- **Crate:** `vfio-ioctls` region info/mmap; KVM memslot via existing `kvm-ioctls` in
  `vstate/vm.rs`/`memory`.

### G3. Route the device's MSI/MSI-X into the guest via irqfd
- **Exists (reusable):** `KvmVm::create_msix_group` (`vm.rs:697`), `MsixVectorGroup::update` + `notifier`
  (`interrupts.rs:139/:134`), `register_msi`/`set_gsi_routes` (`vm.rs:667/:715`), `KVM_MSI_VALID_DEVID`
  handling. MSI-X table emulation in `pci/msix.rs`.
- **Missing:** wire the group's `notifier()` EventFds into the **hardware** via
  `VFIO_DEVICE_SET_IRQS(VFIO_PCI_MSIX_IRQ_INDEX, eventfds[])`; drive enable/mask transitions from the
  guest's MSI-X table writes (reuse `MsixConfig`, but on unmask call `VFIO_DEVICE_SET_IRQS` instead of
  software trigger). INTx (level) passthrough needs a **resample fd**
  (`register_irqfd_with_resample` + `VFIO_DEVICE_SET_IRQS(VFIO_PCI_INTX_IRQ_INDEX)`), which **does not
  exist** — defer (MSI-X-only first; most NIC/NVMe/GPU VFs are MSI-X).
- **Hook:** `VfioPciDevice` owns an `MsixConfig` whose `MsixVectorGroup` notifiers are registered with VFIO.

### G4. Program host IOMMU / DMA (`VFIO_IOMMU_MAP_DMA` over guest memory)
- **Exists:** nothing.
- **Missing:** after guest memory is allocated, for **every** guest RAM region call
  `VFIO_IOMMU_MAP_DMA { iova = guest_phys_addr, size, vaddr = host_userspace_addr, RW }` on the container.
  Must iterate `GuestMemoryMmap` regions (the layout has holes at the 4G/256G splits — map RAM only, not
  MMIO). Must also map any later-added regions (balloon/virtio-mem → currently incompatible; document).
- **Crate:** `vfio-ioctls` `VfioContainer::vfio_dma_map(iova, size, vaddr)`.
- **Hook:** a new step in `builder.rs` after `create_guest_memory` and before/at device attach; iterate
  `vm.guest_memory().iter()`. This is a **new subsystem** and a top correctness risk (a wrong mapping = DMA
  to the wrong host memory).

### G5. Device reset (FLR) + config-space virtualization/quirks
- **Exists:** nothing.
- **Missing:** `VFIO_DEVICE_RESET` (FLR) on attach and on guest-initiated reset; config-space
  virtualization layer that: intercepts BAR registers (present guest GPAs), the Command register (gate
  MEM/IO/bus-master), the MSI-X capability (route to emulated `MsixConfig`), and **masks/quirks**
  capabilities that don't make sense passed through (e.g. PCIe, PM, VPD, ROM BAR handling). Mirror the
  rust-vmm/cloud-hypervisor `VfioPciDevice` config-space virtualization.
- **Hook:** `VfioPciDevice::{read,write}_config_register` (the fork's `PciDevice` trait, `pci/mod.rs:32`).

### G6. Jailer / seccomp
- **Jailer (`src/jailer/src/env.rs`):**
  - Add `/dev/vfio` to `FOLDER_HIERARCHY` (`:65`).
  - `mknod` `/dev/vfio/vfio` (the container; **misc major 10**, **dynamically-allocated minor** — read from
    `/sys/class/misc/vfio/dev` or `stat` the host node; the fixed-`major:minor` constant pattern at
    `:33-41` won't work).
  - `mknod` each `/dev/vfio/<group>` node — **per-group major/minor assigned at runtime by the vfio
    driver**, so `stat` the host node and replicate. **Most invasive jailer change** (nodes are dynamic,
    multiple, per-launch). Requires generalizing `mknod_and_own_dev` to accept discovered majors/minors and
    a new call site near `:688-707`.
  - Host pre-step (operator, out of jailer scope): bind the VF's IOMMU group to `vfio-pci`.
- **Seccomp (`resources/seccomp/*.json`, both arches):** add one `ioctl` `op:eq` rule per VFIO request
  number in the `"vmm"` filter near `:574`: `VFIO_GET_API_VERSION`, `VFIO_CHECK_EXTENSION`, `VFIO_SET_IOMMU`,
  `VFIO_GROUP_GET_STATUS`, `VFIO_GROUP_SET_CONTAINER`, `VFIO_GROUP_GET_DEVICE_FD`, `VFIO_DEVICE_GET_INFO`,
  `VFIO_DEVICE_GET_REGION_INFO`, `VFIO_DEVICE_GET_IRQ_INFO`, `VFIO_DEVICE_SET_IRQS`, `VFIO_DEVICE_RESET`,
  `VFIO_IOMMU_GET_INFO`, `VFIO_IOMMU_MAP_DMA`, `VFIO_IOMMU_UNMAP_DMA`. Verify `mmap` (BAR mmap) and
  `eventfd2` are permitted (they are, for existing devices). Mirror in aarch64.

### Gap summary

| # | Capability | Exists | Missing | Primary hook |
|---|---|---|---|---|
| G1 | Open container/group/device | — | all | new `devices/vfio/`, `pci_mngr.rs:141` peer |
| G2 | Map BARs + config space | bus attach, mmio64 alloc, BusDevice | real-BAR enum, 32-bit/generic BAR API, mmap+memslot, config virt | `configuration.rs:93`, `VfioPciDevice` |
| G3 | MSI/MSI-X via irqfd | full MSI-X+irqfd+routing | `VFIO_DEVICE_SET_IRQS` glue; INTx resamplefd (defer) | `interrupts.rs`, `pci/msix.rs` |
| G4 | IOMMU DMA map | — | map all guest RAM regions | new step in `builder.rs` |
| G5 | FLR + config virt/quirks | — | reset + virtualization | `VfioPciDevice` config path |
| G6 | Jailer + seccomp | generic mknod, per-req ioctl filter | dynamic vfio nodes, VFIO ioctl rules | `jailer/src/env.rs`, `resources/seccomp/*` |

---

## (c) Phase 3 — GPU-specific deltas

First target should be a **NIC or NVMe VF** (small 64-bit BARs, MSI-X, clean FLR). A **GPU VF** adds:

1. **Multi-GB BARs → memory-layout rework.** GPU BAR0/framebuffer/VRAM apertures are often 256 MiB–several
   GiB, and the *prefetchable* aperture can be **16–64 GiB** (resizable BAR). The 256 GiB `mmio64_memory`
   window (`layout.rs:124`) fits one large BAR, but: (a) the allocator is fine, but the **ACPI `_CRS`** and
   the guest's view must advertise enough 64-bit space (already 256 GiB — OK); (b) **prefetchable** BARs are
   **not modeled end-to-end** (`BarPrefetchable` exists but every caller passes `No` and the `_CRS` emits
   only `NotCacheable`, `pci_segment.rs:374-385`) — GPUs strongly prefer prefetchable mapping; add a
   prefetchable 64-bit sub-window. (c) **Resizable BAR (PCIe rebar) capability** may need virtualization.
   Sub-4G (~750 MiB) cannot hold a GPU 32-bit BAR — ensure 64-bit BAR placement.
2. **Reset quirks.** Many GPUs have **unreliable FLR**; need bus/secondary-bus reset or vendor-specific
   reset sequences, and a post-reset settle/poll. `VFIO_DEVICE_RESET` may be insufficient → may require
   `VFIO_DEVICE_GET_PCI_HOT_RESET_INFO` / `VFIO_DEVICE_PCI_HOT_RESET` (whole-group reset) — which **breaks
   the single-function assumption** and needs the multi-function bus work.
3. **MSI-X vector counts.** GPUs expose many vectors (dozens–hundreds). Within the 4072-GSI budget
   (`layout.rs`), but validate `create_msix_group` scaling and that `KVM_SET_GSI_ROUTING` with large tables
   is committed efficiently.
4. **Host-side prerequisites (outside the VMM).** SR-IOV enabled (`sriov_numvfs`), vendor vGPU/GRID/MIG
   manager configured, VF bound to `vfio-pci`, often a **host driver + licensing** (NVIDIA vGPU, AMD MxGPU).
   The guest needs the vendor driver. Document as operator prereqs; **no VMM code**.
5. **Large-BAR DMA & coherence.** Bigger mappings, possible need for `VFIO_DMA_CC_IOMMU` / P2P
   considerations; out of scope for milestone 1.

---

## (d) Sequenced task list

Ordered so **Milestone 1 = pass a single SR-IOV NIC VF and have the guest driver bind to it.**
Each task names the file/function it touches. 🔴 = highest risk/uncertainty, 🟠 = medium.

### Milestone 0 — scaffolding & host plumbing
1. Add deps `vfio-ioctls`, `vfio-bindings` to `src/vmm/Cargo.toml` (`[dependencies]`); update `Cargo.lock`,
   `deny.toml`, `CREDITS.md`.
2. Create module skeleton `src/vmm/src/devices/vfio/{mod.rs,device.rs}` and declare it in
   `src/vmm/src/devices/mod.rs:16` area. Empty `VfioPciDevice` struct + error enum.
3. 🟠 **Jailer**: generalize `mknod_and_own_dev` and add dynamic-node discovery for `/dev/vfio/vfio` +
   `/dev/vfio/<group>` in `src/jailer/src/env.rs` (`FOLDER_HIERARCHY:65`, `:422`, call site `:688-707`).
   Add a CLI/arg to pass the group id(s).
4. **Seccomp**: add the VFIO `ioctl` `op:eq` rules (+ verify `mmap`) to
   `resources/seccomp/x86_64-…json` and `aarch64-…json` near the KVM ioctl block (`:574`).

### Milestone 1 — single NIC/NVMe VF passthrough (MSI-X)
5. **Config/API plumbing** (mirror net):
   - `src/vmm/src/vmm_config/vfio.rs`: `VfioDeviceConfig { id, host_group/BDF or /dev/vfio path }` +
     `VfioBuilder` (model on `net.rs:20/:82`).
   - `src/vmm/src/resources.rs`: add `pub vfio: VfioBuilder` (~`:120`) + `build_vfio_device` (~`:370`);
     **require `pci_enabled`**.
   - API: swagger `firecracker.yaml` path `/vfio-devices/{id}` + `VfioDevice` def (~`:707/:1522`);
     `src/firecracker/src/api_server/request/vfio.rs` parser; route arm `parsed_request.rs:108`;
     `VmmAction::InsertVfioDevice` + dispatch + `insert_vfio_device` in `rpc_interface.rs` (`:94/:476/:540`).
6. 🔴 **VFIO core** (`devices/vfio/device.rs`): open container/group/device via `vfio-ioctls`; query
   `VFIO_DEVICE_GET_INFO` + per-region `VFIO_DEVICE_GET_REGION_INFO` + `VFIO_DEVICE_GET_IRQ_INFO`. Build an
   internal description of BARs (size/flags/mmap-able) and config space. **(G1)**
7. 🔴 **IOMMU DMA map** (`src/vmm/src/builder.rs`, new fn `map_guest_memory_for_vfio`, called after guest
   memory creation): iterate `vm.guest_memory()` RAM regions → `VfioContainer::vfio_dma_map(gpa, len,
   host_vaddr)`. Handle holes; unmap on teardown. **Top correctness risk. (G4)**
8. **BAR model extension** (`src/vmm/src/pci/configuration.rs`): add `set_bar_32` + generic
   `add_pci_bar(idx,size,BarRegionType,prefetchable)`; keep `set_bar_64` (`:93`). No relocation needed yet.
   **(G2)**
9. 🔴 **`impl PciDevice for VfioPciDevice`** (`devices/vfio/device.rs`, against `pci/mod.rs:32`):
   - `read/write_config_register`: serve a **virtualized** config space — real reads via `pread` on the VFIO
     config region, but intercept BARs (present allocated GPAs), Command reg, MSI-X cap. **(G5)**
   - allocate BAR GPAs from `ResourceAllocator.mmio64_memory`/`mmio32_memory`
     (`vstate/resources.rs:74/:79`).
10. 🔴 **`impl BusDevice for VfioPciDevice`** (BAR MMIO): for mmap-able BARs, `mmap` the region fd and add a
    **KVM memslot** (`KVM_SET_USER_MEMORY_REGION` via `vstate/vm.rs`) so guest MMIO is near-native; for
    non-mmap-able, trap `read/write` → `pread/pwrite` on the device fd. **(G2)**
11. 🔴 **MSI-X wiring** (`devices/vfio/device.rs` + reuse `pci/msix.rs`, `vstate/interrupts.rs`,
    `vstate/vm.rs`): own an `MsixConfig` whose `MsixVectorGroup` (`create_msix_group`, `vm.rs:697`) notifiers
    are pushed to hardware via `VFIO_DEVICE_SET_IRQS(MSIX)`. On guest table mask/unmask, update both KVM
    routing (`update`, `interrupts.rs:139`) and `VFIO_DEVICE_SET_IRQS`. **(G3)**
12. **Attach path**: `device_manager/pci_mngr.rs` new `attach_pci_vfio_device` (peer of `:141`) reusing
    `attach_common` (`:109`) for SBDF/bus/BAR/MSI-X registration; add `vfio_devices` map to `PciDevices`
    (`:48`). `device_manager/mod.rs` new `attach_vfio_device` (peer of `:279`). `builder.rs`
    `attach_vfio_devices` next to `:253`, gated on `pci_enabled` (`:216`).
13. **FLR on attach/reset**: `VFIO_DEVICE_RESET` in `VfioPciDevice::new` and on guest reset. **(G5)**
14. 🟠 **Snapshot**: in `device_manager/persist.rs` + `pci_mngr.rs:225` add a `vfio_devices` state vec, but
    **make snapshot/restore reject or skip VFIO** (host-bound fd + DMA mappings are not migratable). Document
    the limitation; fail closed.
15. **Validation**: host creates a NIC VF (`sriov_numvfs`), bind to `vfio-pci`; boot microVM with
    `vfio-devices` entry; confirm guest enumerates the PCI function, driver binds, MSI-X interrupts fire,
    DMA (RX/TX) works. Add an integration test under `tests/`.

### Milestone 2 — robustness / multi-function
16. 🔴 **Lift single-function limit** in `src/vmm/src/pci/bus.rs` (`function>0` rejects at `:184-187, :220,
    :344`) and `pci_segment.rs next_device_sbdf` (`:167`) to support multi-function devices.
17. **INTx (level) passthrough**: add `register_irqfd_with_resample` in `vstate/interrupts.rs`/`vm.rs` +
    `VFIO_DEVICE_SET_IRQS(INTX)` + resample handling. (Skip if all targets are MSI-X.)
18. **Prefetchable 64-bit BAR window** wired end-to-end: `BarPrefetchable::Yes` through
    `configuration.rs` + a prefetchable producer in `pci_segment.rs _CRS` (`:374-385`).
19. **Dynamic DMA map updates** for balloon/virtio-mem regions, or explicitly mark VFIO incompatible with
    them.

### Milestone 3 — GPU VF
20. 🔴 **Large/resizable BAR support**: ensure GPU multi-GB (and resizable) BARs allocate from
    `mmio64_memory` with prefetchable mapping; virtualize the PCIe Resizable-BAR capability; confirm `_CRS`
    advertises sufficient 64-bit space (`pci_segment.rs`, `layout.rs:124`). **(Phase 3.1)**
21. 🔴 **GPU reset quirks**: beyond FLR — secondary-bus/hot-reset
    (`VFIO_DEVICE_GET_PCI_HOT_RESET_INFO`/`VFIO_DEVICE_PCI_HOT_RESET`), which needs the whole IOMMU group →
    depends on Task 16 (multi-function). **(Phase 3.2)**
22. **High MSI-X vector counts**: validate `create_msix_group` + `set_gsi_routes` at scale (`vm.rs:697/:715`).
23. **Docs**: operator prereqs (SR-IOV mode, vGPU/GRID/MIG manager, vendor guest driver, licensing).
    No VMM code. **(Phase 3.4)**

### Riskiest / most uncertain (watch list)
- 🔴 **Task 7 (IOMMU DMA map)** — silent corruption if GPA→HVA mapping or holes are wrong; hardest to debug.
- 🔴 **Task 10 (BAR mmap + memslot)** — KVM memslot interaction with the existing memory map and slot limits.
- 🔴 **Task 9 (config-space virtualization)** — BAR interception + capability quirks are fiddly; the bus's
  **no-BAR-relocation** no-op (`configuration.rs:135`) means guest BAR moves will silently fail.
- 🔴 **Task 11 (MSI-X ↔ VFIO_DEVICE_SET_IRQS)** — ordering of routing vs irqfd vs hardware enable
  (the in-tree comment at `interrupts.rs:166` already warns about a KVM ordering hazard).
- 🟠 **Task 3 (jailer dynamic vfio nodes)** — per-group dynamic major/minor; multiple nodes per launch.
- 🔴 **Tasks 20–21 (GPU BARs + reset)** — the GPU-specific unknowns; reset quirks may force the
  multi-function/group work (Task 16) earlier than planned.

---

## Appendix — key file index

```
src/vmm/src/pci/mod.rs:32                          trait PciDevice  (the seam)
src/vmm/src/pci/bus.rs:79/110/154/306             PciBus / add_device / PciConfigIo / PciConfigMmio
src/vmm/src/pci/bus.rs:184,220,344                single-bus / single-function reject
src/vmm/src/pci/configuration.rs:93/125           set_bar_64 / BAR-write no-op (no relocation)
src/vmm/src/pci/msix.rs:70/279                     MsixConfig / write_table -> routing
src/vmm/src/devices/pci/pci_segment.rs:30/88       PciSegment / mmio32+mmio64 apertures
src/vmm/src/devices/virtio/transport/pci/device.rs:260/770/1019   VirtioPciDevice (template)
src/vmm/src/vstate/bus.rs:20                       trait BusDevice  (BAR MMIO seam)
src/vmm/src/vstate/interrupts.rs:45/139/134        MsixVector / update / notifier
src/vmm/src/vstate/vm.rs:667/697/715               register_msi / create_msix_group / set_gsi_routes
src/vmm/src/vstate/resources.rs:67                  ResourceAllocator (mmio32/mmio64/gsi)
src/vmm/src/arch/x86_64/layout.rs:118/124           32-bit (~750 MiB) / 64-bit (256 GiB) MMIO
src/vmm/src/builder.rs:216/253/695                  pci gate / attach point / attach_net_devices
src/vmm/src/device_manager/mod.rs:279               attach_virtio_device (transport choice)
src/vmm/src/device_manager/pci_mngr.rs:109/141/89   attach_common / attach_pci_virtio_device / register_bars
src/vmm/src/device_manager/persist.rs               snapshot (VFIO = not migratable)
src/vmm/src/vmm_config/net.rs:20/82                  config + builder template
src/firecracker/src/api_server/request/net.rs:11    request parser template
src/firecracker/swagger/firecracker.yaml:707/1522   API path + def template
src/jailer/src/env.rs:65/422/688                    FOLDER_HIERARCHY / mknod_and_own_dev / call sites
resources/seccomp/x86_64-unknown-linux-musl.json:574   ioctl allowlist (add VFIO reqs)
```
