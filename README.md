# Firecracker with PVM Pagetable Support

This fork ports Firecracker to support **PVM pagetable-based execution**, allowing
Firecracker-style microVMs to run on top of the PVM/KVM ecosystem while
preserving Firecracker's core goals: secure multi-tenant isolation, fast startup,
low overhead, and a minimal device model for container and function workloads.

The upstream Firecracker project is an open source virtual machine monitor
purpose-built for serverless infrastructure. This fork extends that model with
PVM support so Firecracker can target environments where traditional hardware
virtualization or nested virtualization is unavailable, restricted, or too
operationally expensive.

Read more about the upstream Firecracker Charter [here](CHARTER.md).

## What This Fork Adds

This fork adds support for running Firecracker with **PVM pagetable support**.

At a high level, the PVM integration enables:

- Firecracker microVM execution through a PVM-aware KVM backend.
- Pagetable handling compatible with PVM's paravirtualized guest model.
- Operation in cloud environments where nested virtualization is not exposed.
- A path for Firecracker-style isolation inside regular VMs, confidential
  compute guests, CI workers, and elastic cloud capacity.
- Continued use of Firecracker's minimal VMM, API-driven configuration model,
  jailer flow, virtio devices, and microVM-oriented security posture.

This is intended for operators who want Firecracker-like serverless execution
semantics without depending exclusively on bare-metal hardware virtualization.

## What is Firecracker?

Firecracker is an open source virtualization technology that is purpose-built
for creating and managing secure, multi-tenant container and function-based
services that provide serverless operational models. Firecracker runs workloads
in lightweight virtual machines, called microVMs, which combine the security and
isolation properties provided by virtualization technology with the speed and
flexibility of containers.

## Why PVM?

PVM is useful for Firecracker because many real-world deployment targets do not
reliably expose hardware virtualization features to guests. This is especially
common when running inside public cloud VMs, nested environments, confidential
compute guests, CI systems, and cost-optimized burst capacity.

PVM provides the following advantages:

- **KVM ecosystem compatibility**  
  PVM integrates with the KVM model while using software-backed virtualization
  paths where hardware assistance is unavailable.

- **No hard dependency on nested hardware virtualization**  
  Many cloud providers do not enable nested virtualization. PVM gives this fork
  a path to run Firecracker-style workloads inside those environments.

- **Better cloud elasticity**  
  Secure container and function platforms can expand into ordinary cloud VMs,
  spot instances, and preemptible instances instead of waiting for bare-metal
  capacity.

- **Support for confidential-compute-style environments**  
  PVM can be useful when running KVM-like virtualization flows inside TDX/SEV
  guests or other restricted execution environments.

- **Kernel CI and fast guest reboot workflows**  
  PVM guests can support lightweight kernel test loops in cheaper nested VM
  environments.

- **Lightweight container kernels**  
  PVM enables a path toward small, paravirtualized guest kernels that pair well
  with microVM and serverless workloads.

## PVM Design Overview

PVM introduces a paravirtualized execution model built around three major
components:

- **Switcher**  
  The code and data path responsible for handling guest entry and guest exit.

- **PVM hypervisor**  
  A KVM x86 vendor implementation that uses existing KVM software
  virtualization facilities, including shadow paging, APIC emulation, and the
  x86 instruction emulator.

- **PVM paravirtual guest**  
  A position-independent Linux guest kernel that runs in hardware CPL3 and uses
  existing paravirtualization operations for optimized guest/host interaction.

```text
                shadowed-user-pagetable shadowed-kernel-pagetable
                            +----------|-----------+
                            |   user   |  kernel   |
    h_ring3                 |  (umod)  |  (smod)   |
                            +---+------|--------+--+
                        syscall |    ^      ^   | hypercall/
            interrupt/exception |    |      |   | interrupt/exception
--------------------------------|----|------|---|------------------------------------
                                |    |sysret|   |
    h_ring0                     v    | /iret|   v
                              +------+------+----+
                              |     switcher     |
                              +---------+--------+
                            vm entry ^  | vm exit
                      (function call)|  v (function return)
      +..............................+..........................................+
      .                                                                         .
      .     +---------------+                      +--------------+             .
      .     |    kvm.ko     |                      |  kvm-pvm.ko  |             .
      .     +---------------+                      +--------------+             .
      .                         Virtualization                                  .
      .   memory virtualization                  CPU virtualization             .
      +.........................................................................+
                                 PVM hypervisor
```

### Switcher

The switcher reuses host entry paths to handle guest entry and guest exit. A
flag marks whether execution is currently in the guest world or transitioning
between host and guest. This allows the guest to look similar to a normal
userspace process from the host's perspective.

### Host MMU and Pagetable Handling

The switcher must be accessible by the guest, similar to the CPU entry area used
for userspace in KPTI. For simplicity, PVM reserves a range of PGDs for the
guest. The guest kernel is only allowed to run inside this reserved range.

During root shadow-page allocation, the host PGDs needed by the switcher are
cloned into the guest shadow pagetable. This is the key pagetable behavior this
fork ports into the Firecracker execution path.

### Event Delivery

PVM uses a new event delivery model instead of traditional IDT-based delivery.
The event model is similar in spirit to FRED and is designed to support the
PVM guest execution model cleanly.

## Overview

The main component of Firecracker is a virtual machine monitor, or VMM. The VMM
uses the Linux Kernel Virtual Machine interface to create and run microVMs.
Firecracker has a minimalist design: it excludes unnecessary devices and
guest-facing functionality to reduce memory footprint and attack surface.

This design improves security, decreases startup time, and increases hardware
utilization. Firecracker has also been integrated into container runtimes such
as [Kata Containers](https://github.com/kata-containers/kata-containers) and
[Flintlock](https://github.com/liquidmetal-dev/flintlock).

Upstream Firecracker was developed at Amazon Web Services to accelerate services
such as [AWS Lambda](https://aws.amazon.com/lambda/) and
[AWS Fargate](https://aws.amazon.com/fargate/). Firecracker is open sourced
under [Apache version 2.0](LICENSE).

To read more about upstream Firecracker, check out
[firecracker-microvm.io](https://firecracker-microvm.github.io).

## Getting Started

To get started with this fork, clone the repository and build Firecracker from
source.

```bash
git clone https://github.com/firecracker-microvm/firecracker
cd firecracker
tools/devtool build
toolchain="$(uname -m)-unknown-linux-musl"
```

The Firecracker binary will be placed at:

```text
build/cargo_target/${toolchain}/debug/firecracker
```

For more information on building, testing, and running Firecracker, see the
[quickstart guide](docs/getting-started.md).

> **Note**
> Replace the clone URL above with your fork URL if you are building the PVM
> pagetable-enabled fork directly.

## Running with PVM Support

A PVM-enabled Firecracker deployment requires a host kernel and KVM stack with
PVM support available. At minimum, validate that the host provides the expected
PVM KVM module and that the guest kernel/rootfs are built for the PVM execution
model.

Recommended operator checklist:

1. Boot a host kernel that includes the PVM KVM implementation.
2. Confirm that the PVM KVM module is available and loaded.
3. Build this Firecracker fork.
4. Boot a PVM-compatible guest kernel and rootfs.
5. Validate microVM lifecycle operations through the Firecracker API.
6. Run workload-level tests for boot, networking, block I/O, vsock, shutdown,
   and jailer-isolated execution.

## Tested Platforms

The following platform matrix is used for validation.

| Instance                                    | Host OS & Kernel  | Guest Rootfs | Guest Kernel |
| :------------------------------------------ | :---------------- | :----------- | :----------- |
| m5n.metal (Intel Cascade Lake)              | al2 linux_5.10    | ubuntu 24.04 | linux_5.10   |
| m6i.metal (Intel Ice Lake)                  | al2023 linux_6.1  | ubuntu 24.04 | linux_6.1    |
| m6i.metal (Intel Ice Lake)                  | al2023 linux_6.18 | ubuntu 24.04 | linux_6.1    |
| m7i.metal-24xl (Intel Sapphire Rapids)      | al2023/linux      | ubuntu 24.04 | linux_6.1    |
| m7i.metal-48xl (Intel Sapphire Rapids)      | al2023/linux      | ubuntu 24.04 | linux_6.1    |
| **m8i.metal-48xl (Intel Granite Rapids)**   | 6.1 or 6.18 host  | ubuntu 24.04 | linux_6.1    |
| **m8i.metal-96xl (Intel Granite Rapids)**   | 6.1 or 6.18 host  | ubuntu 24.04 | linux_6.1    |
| m6a.metal (AMD Milan)                       | al2023/linux      | ubuntu 24.04 | linux_6.1    |
| m7a.metal-48xl (AMD Genoa)                  | al2023/linux      | ubuntu 24.04 | linux_6.1    |
| m6g.metal (Graviton 2)                      | al2023/linux      | ubuntu 24.04 | linux_6.1    |
| m7g.metal (Graviton 3)                      | al2023/linux      | ubuntu 24.04 | linux_6.1    |
| m8g.metal-24xl (Graviton 4)                 | al2023/linux      | ubuntu 24.04 | linux_6.1    |
| m8g.metal-48xl (Graviton 4)                 | al2023/linux      | ubuntu 24.04 | linux_6.1    |
| AMD EPYC 9005 (SA5 Turin)                   | 6.12-pvm+ RHEL 10 | ubuntu 24.04 | linux_6.1    |

> **Granite Rapids note**
> AWS EC2 8th Gen Intel instances using Granite Rapids CPUs require a 6.1 or
> 6.18 host kernel due to limited Granite Rapids support in older 5.10 kernels.

## Features & Capabilities

Firecracker consists of a single micro Virtual Machine Manager process that
exposes an API endpoint to the host once started. The API is
[specified in OpenAPI format](src/firecracker/swagger/firecracker.yaml). Read
more about it in the [API docs](docs/api_requests).

The **API endpoint** can be used to:

- Configure the microVM by:
  - Setting the number of vCPUs.
  - Setting the memory size.
  - Configuring a [CPU template](docs/cpu_templates/cpu-templates.md).
- Add one or more network interfaces to the microVM.
- Add one or more read-write or read-only disks to the microVM.
- Trigger a block device re-scan while the guest is running.
- Change the backing file for a block device before or after guest boot.
- Configure virtio device rate limiters.
- Configure logging and metrics.
- `[BETA]` Configure the guest-facing metadata service data tree.
- Add a [vsock socket](docs/vsock.md) to the microVM.
- Add an [entropy device](docs/entropy.md) to the microVM.
- Add a [pmem device](docs/pmem.md) to the microVM.
- Configure and manage [memory hotplugging](docs/memory-hotplug.md).
- `[Developer Preview]` [Hot-plug and hot-unplug](docs/device-hotplug.md)
  virtio PCI devices while the VM is running.
- Start the microVM using a kernel image, root filesystem, and boot arguments.
- `[x86_64 only]` Stop the microVM.

**Built-in capabilities include:**

- PVM pagetable support in this fork.
- Demand fault paging and CPU oversubscription enabled by default.
- Advanced, thread-specific seccomp filters.
- [Jailer](docs/jailer.md) process for production isolation using cgroups,
  namespaces, and privilege dropping.

## Security

The overall security of Firecracker microVMs, including the ability to meet the
criteria for safe multi-tenant computing, depends on a properly configured Linux
host operating system. A configuration that upstream Firecracker recommends is
included in the [production host setup document](docs/prod-host-setup.md).

For this PVM-enabled fork, also review the security posture of the PVM host
kernel, the PVM KVM module, the guest kernel configuration, and the boundary
between Firecracker, the jailer, and the PVM backend.

## Known Issues and Limitations

- PVM support requires a compatible host kernel and KVM/PVM stack.
- PVM guests require a compatible guest kernel/rootfs configuration.
- The `pl031` RTC device on aarch64 does not support interrupts, so guest
  programs that use an RTC alarm, such as `hwclock`, will not work.
- Platform support should be validated per CPU generation, kernel version, and
  cloud provider.

## Performance

Firecracker's performance characteristics are listed as part of the
[specification documentation](SPECIFICATION.md). All specifications are part of
Firecracker's commitment to supporting container and function workloads in
serverless operational models and are enforced through continuous integration
testing.

For PVM deployments, benchmark both the upstream Firecracker workload profile
and the PVM-specific pagetable path. Recommended benchmarks include:

- microVM boot time
- guest reboot time
- memory footprint per microVM
- network throughput and latency
- block I/O throughput and latency
- syscall-heavy workload behavior
- cold start and warm start behavior for functions
- density under CPU oversubscription

## Design

Firecracker's upstream architecture is described in
[the design document](docs/design.md).

The PVM-specific design is based on the PVM hypervisor model described above and
the PVM paper published at SOSP 2023.

## Contributing

Firecracker is already running production workloads within AWS, but it is still
Day 1 on the journey guided by its [mission](CHARTER.md). Contributions are
welcome.

To contribute to Firecracker, check out the development setup section in the
[getting started guide](docs/getting-started.md) and then the Firecracker
[contribution guidelines](CONTRIBUTING.md).

For this fork, PVM-related contributions should include clear test coverage for:

- pagetable setup and teardown
- guest entry and exit
- PVM-compatible guest boot
- Firecracker API lifecycle operations
- jailer execution
- device model compatibility
- regression coverage against upstream Firecracker behavior

## Releases

New upstream Firecracker versions are released via the GitHub repository
[releases](https://github.com/firecracker-microvm/firecracker/releases) page,
typically every two or three months. A history of changes is recorded in the
[changelog](CHANGELOG.md).

The Firecracker release policy is detailed [here](docs/RELEASE_POLICY.md).

For this fork, release notes should call out the upstream Firecracker base
version, the PVM kernel requirements, guest kernel requirements, and any
PVM-specific compatibility notes.

## Policy for Security Disclosures

The security of Firecracker is a top priority. If you suspect you have uncovered
a vulnerability, contact the maintainers privately as outlined in the
[security policy document](SECURITY.md).

## FAQ & Contact

Frequently asked questions are collected in the upstream [FAQ doc](FAQ.md).

You can get in touch with the Firecracker community in the following ways:

- Security-related issues: see the [security policy document](SECURITY.md).
- Chat with the community in the
  [Slack workspace](https://join.slack.com/t/firecracker-microvm/shared_invite/zt-2tc0mfxpc-tU~HYAYSzLDl5XGGJU3YIg).
- Open a GitHub issue in this repository.
- Email the upstream maintainers at
  [firecracker-maintainers@amazon.com](mailto:firecracker-maintainers@amazon.com).

When communicating within the Firecracker community, please follow the
[code of conduct](CODE_OF_CONDUCT.md).
