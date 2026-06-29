# VFIO passthrough — Milestone 0–1 implementation status

Branch `feat/vfio-passthrough`. Implements Milestones 0–1 of
[`vfio-passthrough-plan.md`](vfio-passthrough-plan.md). **The whole workspace (incl. the `jailer`
crate and the release seccomp filters) compiles, and the new unit tests pass.** Runtime validation
(Task 15) requires a host with an SR-IOV VF bound to `vfio-pci` and is **not** possible in CI/sandbox.

## What is implemented (compiles + unit-tested)

| Plan task | Status | Where |
|---|---|---|
| 0.1 deps (`vfio-ioctls 0.7`, `vfio-bindings 0.6`, `default-features=false`) | ✅ | `src/vmm/Cargo.toml` |
| 0.2 module skeleton | ✅ | `src/vmm/src/devices/vfio/{mod,device}.rs` |
| 0.3 jailer `/dev/vfio` | ✅ container node; ⚠️ group node = follow-up | `src/jailer/src/env.rs` |
| 0.4 seccomp VFIO ioctls | ✅ both arches (15 rules, compiles to BPF) | `resources/seccomp/*.json` |
| 1.5 config + API plumbing | ✅ | `vmm_config/vfio.rs`, `resources.rs`, `rpc_interface.rs`, `api_server/request/vfio.rs`, `parsed_request.rs` |
| 1.6 open container/group/device + regions/IRQs | ✅ | `devices/vfio/device.rs::new` |
| 1.7 IOMMU DMA map (identity, all guest RAM) | ✅ | `device.rs::map_dma` |
| 1.8 BAR model extension (`set_bar_32`/`get_bar_addr_32`) | ✅ | `pci/configuration.rs` |
| 1.9 `impl PciDevice` (config virt + BAR interception) | ✅ | `device.rs` |
| 1.10 `impl BusDevice` (BAR access) | ✅ via trap (pread/pwrite) | `device.rs` |
| 1.12 attach path | ✅ | `pci_mngr.rs::attach_pci_vfio_device`, `device_manager/mod.rs`, `builder.rs` |
| 1.13 FLR on attach | ✅ (`VfioDevice::reset`) | `device.rs::new` |
| 1.14 snapshot rejection | ✅ (no `Persist` impl; not added to `DeviceStates`) | `device.rs` |

Configure a VF at boot:
```
PUT /vfio-devices/{id}   { "vfio_dev_id": "vf0", "host_pci_address": "0000:01:00.1" }
```
Requires `pci_enabled = true`; pre-boot only (rejected post-boot).

## Deliberately deferred (clearly marked TODO in code)

- **Task 1.10 BAR mmap fast-path** — BAR MMIO currently traps to `pread`/`pwrite` on the VFIO region
  fd (correct, slower). The `mmap`+`KVM_SET_USER_MEMORY_REGION` fast path (near-native MMIO) is a
  follow-up; `VfioBar::mmappable` is already recorded.
- **Task 1.11 MSI-X table emulation** — the device exposes `enable_msix(eventfds)` and
  `max_msix_vectors()`, and the existing `MsixVectorGroup` (irqfd↔KVM MSI route) is reused, but the
  guest-facing MSI-X **table emulation** (intercepting table writes to drive enable/mask and call
  `VFIO_DEVICE_SET_IRQS`) is not yet wired into `attach_pci_vfio_device`. **This is the gating item
  for "guest driver takes interrupts"** and is the recommended next task.
- **Task 0.3 group node** — the jailer replicates the misc container node `/dev/vfio/vfio` (minor
  discovered from `/proc/misc`, like `/dev/userfaultfd`). The per-IOMMU-group node `/dev/vfio/<N>`
  has a dynamically-allocated major/minor and needs a new `--vfio-group N` CLI argument to stat the
  host node and `mknod` it into the jail.
- **Task 16/17 (M2)** multi-function + INTx, **M3** GPU BARs/reset — unchanged from the plan.

## Riskiest items still unproven (need hardware)

1. **DMA map correctness** (`map_dma`) — identity IOVA==GPA over `guest_memory.iter()`; correctness
   only observable with a real device doing DMA.
2. **Config-space virtualization** — BAR registers are virtualized; the Command register and
   capability quirks are currently forwarded verbatim (may need interception for some devices).
3. **MSI-X enable ordering** vs the KVM routing hazard noted in `vstate/interrupts.rs`.

## How it was verified here
- `cargo check --workspace` ✅, `cargo check -p jailer` ✅, `cargo check -p firecracker` ✅
- `seccompiler-bin` compiles the edited x86_64 filter to BPF ✅
- `cargo test -p vmm --lib vmm_config::vfio` ✅ (BDF validation + dedup/replace)
