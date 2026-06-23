# PVM Firecracker — Ansible playbook

Run **Firecracker microVMs without KVM / hardware virtualization** on a regular
cloud VM, using **PVM** (Pagetable-based Virtual Machine). This playbook codifies
the whole manual build: a PVM-patched Firecracker, a PVM host kernel, a PVM guest
kernel, an Ubuntu rootfs, GRUB config, reboot, and an end-to-end microVM smoke test.

## The one hard requirement

PVM needs the **`fsgsbase`** CPU feature exposed to the guest vCPU. Many cloud
hosts hide it (for live-migration compatibility), and PVM **cannot** run there.
Check first:

```bash
grep -o fsgsbase /proc/cpuinfo   # must print "fsgsbase"
```

The playbook asserts this in preflight and stops immediately if it's missing.

Other requirements: x86_64, RHEL family (Rocky/Alma/RHEL **10** tested), root via SSH,
~10 GB free disk, internet access. First run compiles two kernels + Firecracker
(~20–40 min on 32 vCPUs).

## Usage

```bash
# 1. point inventory.ini at your host
# 2. run it
ansible-playbook playbook.yml

# stage everything but don't reboot / test (e.g. you want to control the reboot)
ansible-playbook playbook.yml -e pvm_reboot=false -e pvm_run_smoke_test=false

# re-run only part of it (every expensive step is idempotent via `creates:`)
ansible-playbook playbook.yml --tags firecracker
ansible-playbook playbook.yml --tags host_kernel,guest_kernel
```

## What it does (stages / tags)

| Tag | Stage |
|-----|-------|
| `preflight`     | assert x86_64 + RHEL + **fsgsbase**, make work dirs |
| `packages`      | install build + runtime dependencies |
| `firecracker`   | install Rust, clone the fork, **re-apply the PVM source patches**, build, install `firecracker`/`jailer` |
| `host_kernel`   | build + install the `pvm-612` host kernel (`CONFIG_KVM_PVM=m`, `pti=off` fixes) |
| `guest_kernel`  | build the PVM guest `vmlinux` (stripped) |
| `rootfs`        | build an Ubuntu ext4 rootfs with an injected SSH key |
| `boot`          | `pti=off`, auto-load `kvm-pvm`, set default kernel, **reboot**, verify `/dev/kvm` |
| `smoke_test`    | boot a microVM, ping + SSH into the guest, assert it's up |

## Key variables (`roles/pvm_firecracker/defaults/main.yml`)

| Variable | Default | Notes |
|----------|---------|-------|
| `pvm_reboot` | `true` | reboot into the PVM kernel and verify |
| `pvm_set_default_kernel` | `true` | make the PVM kernel the persistent default |
| `pvm_run_smoke_test` | `true` | boot a microVM and prove it |
| `firecracker_repo` / `firecracker_version` | rusternetes-labs / `main` | patches re-applied regardless |
| `pvm_linux_branch` | `pvm-612` | Linux 6.12.33 base |
| `rootfs_squashfs_url` | FC CI Ubuntu 24.04 | userland is PVM-independent |
| `make_jobs` | all vCPUs | parallel build |

## Result

After a successful run the target has, in `/root/pvm/`: `firecracker`, `jailer`,
`vmlinux-pvm`, `rootfs.ext4`, `id_rsa`, `vmconfig.json`, `run-test.sh`. It boots the
PVM kernel by default with `/dev/kvm` provided by `kvm-pvm`, and:

```bash
/root/pvm/run-test.sh   # boots a microVM (full Ubuntu guest) over PVM, no KVM hardware
```

## Notes & caveats

- **Custom kernel + reboot.** Have console access to your cloud VM in case the
  kernel fails to boot. `pvm_reboot=false` lets you stage everything and reboot
  yourself. (`saved_entry` keeps your stock kernel as a fallback.)
- **gnu build target.** Firecracker is built for `x86_64-unknown-linux-gnu`, which
  uses an empty seccomp filter (no `libseccomp` at runtime). For a hardened
  musl/static build, build via the upstream `tools/devtool` in a container.
- **Idempotent.** Safe to re-run; finished steps are skipped. To force a kernel
  rebuild, remove its `creates:` marker (e.g. the `bzImage`) or wipe `build_root`.
- Live-migration support from the fork is intentionally out of scope (not needed
  to run microVMs without KVM).
```
