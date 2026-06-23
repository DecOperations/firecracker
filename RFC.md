[**linux-kernel.vger.kernel.org archive mirror**](https://lore.kernel.org/lkml/?t=20240306110558)
 [help](https://lore.kernel.org/lkml/_/text/help/) / [color](https://lore.kernel.org/lkml/_/text/color/) / [mirror](https://lore.kernel.org/lkml/_/text/mirror/) / [Atom feed](https://lore.kernel.org/lkml/new.atom)

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e5380166ee3c0ce945348e361d39bf5ca577a1fbe) **[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor**
**@ 2024-02-26 14:35 Lai Jiangshan**
  2024-02-26 14:35 ` [[RFC PATCH 01/73] KVM: Documentation: Add the specification for PVM](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mdb114605fe7991cfe69b8719c9ca244d6e37e4b1) Lai Jiangshan
                   ` [(74 more replies)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rdb114605fe7991cfe69b8719c9ca244d6e37e4b1)
  [0 siblings, 75 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r5380166ee3c0ce945348e361d39bf5ca577a1fbe)
From: Lai Jiangshan @ 2024-02-26 14:35 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-1-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-1-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143411)
  Cc: Lai Jiangshan, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143411), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Hou Wenlong

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

This RFC series proposes a new virtualization framework built upon the
KVM hypervisor that does not require hardware-assisted virtualization
techniques. PVM (Pagetable-based virtual machine) is implemented as a
new vendor for KVM x86, which is compatible with the KVM virtualization
software stack, such as Kata Containers, a secure container technique in
a cloud-native environment.

The work also led to a paper being accepted at SOSP 2023 [sosp-2023-acm]
[sosp-2023-pdf], and Lai delivered a presentation at the symposium in
Germany in October 2023 [sosp-2023-slides]:

	PVM: Efficient Shadow Paging for Deploying Secure Containers in
	Cloud-native Environment

PVM has been adopted by Alibaba Cloud and Ant Group in production to
host tens of thousands of secure containers daily, and it has also been
adopted by the Openanolis community.

Motivation
==========
A team in Ant Group, co-creator of Kata Containers along with Intel,
deploy the VM-based containers in our public cloud VM to satisfy dynamic
resource requests and various needs to isolate workloads. However, for
safety, nested virtualization is disabled in the L0 hypervisor, so we
cannot use KVM directly. Additionally, the current nested architecture
involves complex and expensive transitions between the L0 hypervisor and
L1 hypervisor.

So the over-arching goals of PVM are to completely decouple secure
container hosting from the host hypervisor and hardware virtualization
support to:
  1) enable nested virtualization within any IaaS clouds without affecting
  the security, flexibility, and complexity of the cloud platform;
  2) avoid costly exits to the host hypervisor and devise efficient world
  switching mechanisms.

Why PVM
=======
The PVM hypervisor has the following features:

- Compatible with KVM ecosystems.

- No requiremment for hardware assistance.  Many cloud provider doesn't
  enable nested virtualization.  And it can also enable KVM in TDX/SEV
  guests.

- Flexible. Businesses with secure containers can easily expand in the
  cloud when demand surges, instead of waiting to accquire bare metal.
  Cloud vendors often offer lower pricing for spot instances or
  preemptible VMs.

- Help for kernel CI with fast [re-]booting PVM guest kernels nested in
  cheeper VMs.

- Enable light-weight container kernels.

Design
======
The design detail can be found in our paper posted in SOSP2023.

The framework contains 3 main objects:

"Switcher" - The code and data that handling the VM enter and VM exit.

"PVM hypervisor" - A new vendor implementation for KVM x86, it uses
                   existed software emulation in KVM for virtualization,
                   e.g., shadow paging, APIC emulation, x86 instruction
                   emulator.

"PVM paravirtual guest" - A PIE linux kernel runs in hardware CPL3, and
                          use existed PVOPS to implement optimization.

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
      .   memory virtualization                  CPU Virtualization             .
      +.........................................................................+
                                 PVM hypervisor

1. Switcher: To simplify, we reuse host entries to handle VM enter and
             VM exit, A flag is introduced to mark that the guest world
             is switched or during the switch in the entries. Therefore,
             the guest almost looks like a normal userspace process in
             the host.

2. Host MMU: The switcher needs to be accessed by the guest, which is
             similar to the CPU entry area for userspace in KPTI.
             Therefore, for simplification, we reserved a range of PGDs
             for the guest, and the guest kernel can only be allowed to
             run in this range. During the root SP allocation, the
             host PGDs of the switcher will be cloned into the guest
             SPT.

3. Event delivery: A new event delivery is used instead of the IDT-based
                   event delivery. The event delivery in PVM is similar
                   to FRED.

Design Decisions
================
In designing PVM, many decisions have been made and explained in the
patches. "Integral entry", "Exclusive address space separation and PIE
guest", and "Simple spec design" are among important decisions besides
for "KVM ecosystems" and "Ring3+Pagetable for privilege seperation".

Integral entry
--------------
The PVM switcher is integrated into the host kernel's entry code,
providing the following advantages:

- Full control: In XENPV/Lguest, the host Linux (dom0) entry code is
  subordinate to the hypervisor/switcher, and the host Linux kernel
  loses control over the entry code. This can cause inconvenience if
  there is a need to update something when there is a bug in the
  switcher or hardware.  Integral entry gives the control back to the
  host kernel.

- Zero overhead incurred: The integrated entry code doesn't cause any
  overhead in host Linux entry path, thanks to the discreet design with
  PVM code in the switcher, where the PVM path is bypassed on host events.
  While in XENPV/Lguest, host events must be handled by the
  hypervisor/switcher before being processed.

- Integral design allows all aspects of the entry and switcher to be
  considered together.

This RFC patchset doesn't include the complete design for integral
entry. It requires fixing the issue with IST [atomic-ist-entry].
And it would be better with the conversion of some ASM code to C code
[asm-to-c] (The link provided is not the final version, and some partial
patchset had sent separately later on). The new version of the patches
for converting ASM code and fixing the IST problem will be updated
and sent separately later.

Without the complete integral entry code, this patchset still has
unresolved issues related to IST, KPTI, and so on.

Exclusive address space separation and PIE guest
------------------------------------------------
In the higher half of the address spaces (where the most significant
bits in the addresses are 1s), the address ranges that a PVM guest is
allowed are exclusive from the host kernel.

- The exclusivity of the address makes it possible to design the
  integral entry because the switcher needs to be mapped for all
  guests.

- The exclusivity of the address allows the host kernel to still utilize
  global pages and save TLB entries. (XENPV doesn't allow it)

- With exclusivity, the existing shadow page table code can be reused
  with very few changes. The shadow page table contains both the guest
  portions and the host portions.

- Exclusivity necessitates the use of a Position-Independent Executable
  (PIE) guest since the host kernel occupies the top 2GB of the address
  space.

- With PIE kernel, the PVM guest kernel in hardware ring3 can be located
  in the lower half of the address spaces in the future when Linear
  Address Space Separation (LASS) is enabled.

This RFC patchset doesn't contain PIE patches which are not specific to
PVM and our effort to make linux kernel PIE continues.

Simple spec design
------------------
Designing a new paravirtualized guest is not an ideal opportunity to
redesign the specification. However, in order to avoid the known flaws
of x86_64 and enable the paravirtualized ABI on hardware ring3, the x86
PVM specification has some moderate differences from the x86
specification.

- Remove/Ignore most indirect tables and 32-bit supervisor mode.

- Simplified event delivery and the removal of IST.

- Add some software synthetic instructions.

See more details in the patch1 which contains the whole x86 PVM
specification.

Status
======
Current some features are not supported or disabled in PVM.

- SMAP/SMEP can't be enabled directly, however, we can use PKU to
  emulate SMAP and use NX to emulate SMEP.

- 5-level paging is not fully implemented.

- Speculative control for guest is disabled.

- LDT is not supported.

- PMU virtualization is not implemented. Actually, we have reused
  the current code in pmu_intel.c and pmu_amd.c to implement it.

PVM has been adopted in Alibaba Cloud and Ant Group for hosting secure
containers, providing a more performant and cost-effective option for
cloud users.

Performance drawback
====================
The most significant drawback of PVM is shadowpaging. Shadowpaging
results in very bad performance when guest applications frequently
modify pagetable, including excessive processes forking.

However, many long-running cloud services, such as Java, modify
pagetables less frequently and can perform very well with shadowpaging.
In some cases, they can even outperform EPT since they can avoid EPT TLB
entries. Furthermore, PVM can utilize host PCIDs for guest processes,
providing a finer-grained approach compared to VPID/ASID.

To mitigate the performance problem, we designed several optimizations
for the shadow MMU (not included in the patchset) and also planning to
build a shadow EPT in L0 for L2 PVM guests.

See the paper for more optimizations and the performance details.

Future plans
============
Some optimizations are not covered in this series now.

- Parallel Page fault for SPT and Paravirtualized MMU Optimization.

- Post interrupt emulation.

- Relocate guest kernel into userspace address range.

- More flexible container solutions based on it.

Patches layout
==============
[01-02]: PVM ABI documentation and header
[03-04]: Switcher implementation
[05-49]: PVM hypervisor implementation
       - 05-13: KVM module involved changes
       - 14-49: PVM module implementation
                patch 15: Add a vmalloc helper to reserve a kernel
                          address range for guest.
                patch 19: Export 32-bit ignore syscall for PVM.

[50-73]: PVM guest implementation
       - 50-52: Pack relocation information into vmlinux and allow
                it to do relocation.
       - 53: Introduce Kconfig and cpu features.
       - 54-59: Relocate guest kernel to the allowed range.
       - 60-65: Event handling and hypercall.
       - 66-69: PVOPS implementation.
       - 70-73: Disable some features and syscalls.

Code base
=========
The code base is at branch [linux-pie] which is commit ceb6a6f023fd
("Linu 6.7-rc6") + PIE series [pie-patchset].

Complete code can be found at [linux-pvm].

Testing
=======
Testing with Kata Containers can be found at [pvm-get-started].

We also provide a VM image based on the `Official Ubuntu Cloud Image`,
which has containerd, kata, pvm hypervisor/guest, and configurations
prepared and you can use to test Kata Containers with PVM directly.
[pvm-get-started-nested-in-vm]

[sosp-2023-acm]: <https://dl.acm.org/doi/10.1145/3600006.3613158>
[sosp-2023-pdf]: <https://github.com/virt-pvm/misc/blob/main/sosp2023-pvm-paper.pdf>
[sosp-2023-slides]: <https://github.com/virt-pvm/misc/blob/main/sosp2023-pvm-slides.pptx>
[asm-to-c]: <https://lore.kernel.org/lkml/20211126101209.8613-1-jiangshanlai@gmail.com/>
[atomic-ist-entry]: <https://lore.kernel.org/lkml/20230403140605.540512-1-jiangshanlai@gmail.com/>
[pie-patchset]: <https://lore.kernel.org/lkml/cover.1682673542.git.houwenlong.hwl@antgroup.com>
[linux-pie]: <https://github.com/virt-pvm/linux/tree/pie>
[linux-pvm]: <https://github.com/virt-pvm/linux/tree/pvm>
[pvm-get-started]: <https://github.com/virt-pvm/misc/blob/main/pvm-get-started-with-kata.md>
[pvm-get-started-nested-in-vm]: <https://github.com/virt-pvm/misc/blob/main/pvm-get-started-with-kata.md#verify-kata-containers-with-pvm-using-vm-image>

Hou Wenlong (22):
  KVM: x86: Allow hypercall handling to not skip the instruction
  KVM: x86: Implement gpc refresh for guest usage
  KVM: x86/emulator: Reinject #GP if instruction emulation failed for
    PVM
  mm/vmalloc: Add a helper to reserve a contiguous and aligned kernel
    virtual area
  x86/entry: Export 32-bit ignore syscall entry and __ia32_enabled
    variable
  KVM: x86/PVM: Support for kvm_exit() tracepoint
  KVM: x86/PVM: Support for CPUID faulting
  x86/tools/relocs: Cleanup cmdline options
  x86/tools/relocs: Append relocations into input file
  x86/boot: Allow to do relocation for uncompressed kernel
  x86/pvm: Relocate kernel image to specific virtual address range
  x86/pvm: Relocate kernel image early in PVH entry
  x86/pvm: Make cpu entry area and vmalloc area variable
  x86/pvm: Relocate kernel address space layout
  x86/pvm: Allow to install a system interrupt handler
  x86/pvm: Add early kernel event entry and dispatch code
  x86/pvm: Enable PVM event delivery
  x86/pvm: Use new cpu feature to describe XENPV and PVM
  x86/pvm: Don't use SWAPGS for gsbase read/write
  x86/pvm: Adapt pushf/popf in this_cpu_cmpxchg16b_emu()
  x86/pvm: Use RDTSCP as default in vdso_read_cpunode()
  x86/pvm: Disable some unsupported syscalls and features

Lai Jiangshan (51):
  KVM: Documentation: Add the specification for PVM
  x86/ABI/PVM: Add PVM-specific ABI header file
  x86/entry: Implement switcher for PVM VM enter/exit
  x86/entry: Implement direct switching for the switcher
  KVM: x86: Set 'vcpu->arch.exception.injected' as true before vendor
    callback
  KVM: x86: Move VMX interrupt/nmi handling into kvm.ko
  KVM: x86/mmu: Adapt shadow MMU for PVM
  KVM: x86: Add PVM virtual MSRs into emulated_msrs_all[]
  KVM: x86: Introduce vendor feature to expose vendor-specific CPUID
  KVM: x86: Add NR_VCPU_SREG in SREG enum
  KVM: x86: Create stubs for PVM module as a new vendor
  KVM: x86/PVM: Implement host mmu initialization
  KVM: x86/PVM: Implement module initialization related callbacks
  KVM: x86/PVM: Implement VM/VCPU initialization related callbacks
  KVM: x86/PVM: Implement vcpu_load()/vcpu_put() related callbacks
  KVM: x86/PVM: Implement vcpu_run() callbacks
  KVM: x86/PVM: Handle some VM exits before enable interrupts
  KVM: x86/PVM: Handle event handling related MSR read/write operation
  KVM: x86/PVM: Introduce PVM mode switching
  KVM: x86/PVM: Implement APIC emulation related callbacks
  KVM: x86/PVM: Implement event delivery flags related callbacks
  KVM: x86/PVM: Implement event injection related callbacks
  KVM: x86/PVM: Handle syscall from user mode
  KVM: x86/PVM: Implement allowed range checking for #PF
  KVM: x86/PVM: Implement segment related callbacks
  KVM: x86/PVM: Implement instruction emulation for #UD and #GP
  KVM: x86/PVM: Enable guest debugging functions
  KVM: x86/PVM: Handle VM-exit due to hardware exceptions
  KVM: x86/PVM: Handle ERETU/ERETS synthetic instruction
  KVM: x86/PVM: Handle PVM_SYNTHETIC_CPUID synthetic instruction
  KVM: x86/PVM: Handle KVM hypercall
  KVM: x86/PVM: Use host PCID to reduce guest TLB flushing
  KVM: x86/PVM: Handle hypercalls for privilege instruction emulation
  KVM: x86/PVM: Handle hypercall for CR3 switching
  KVM: x86/PVM: Handle hypercall for loading GS selector
  KVM: x86/PVM: Allow to load guest TLS in host GDT
  KVM: x86/PVM: Enable direct switching
  KVM: x86/PVM: Implement TSC related callbacks
  KVM: x86/PVM: Add dummy PMU related callbacks
  KVM: x86/PVM: Handle the left supported MSRs in msrs_to_save_base[]
  KVM: x86/PVM: Implement system registers setting callbacks
  KVM: x86/PVM: Implement emulation for non-PVM mode
  x86/pvm: Add Kconfig option and the CPU feature bit for PVM guest
  x86/pvm: Detect PVM hypervisor support
  x86/pti: Force enabling KPTI for PVM guest
  x86/pvm: Add event entry/exit and dispatch code
  x86/pvm: Add hypercall support
  x86/kvm: Patch KVM hypercall as PVM hypercall
  x86/pvm: Implement cpu related PVOPS
  x86/pvm: Implement irq related PVOPS
  x86/pvm: Implement mmu related PVOPS

 Documentation/virt/kvm/x86/pvm-spec.rst  |  989 +++++++
 arch/x86/Kconfig                         |   32 +
 arch/x86/Makefile.postlink               |    9 +-
 arch/x86/entry/Makefile                  |    4 +
 arch/x86/entry/calling.h                 |   47 +-
 arch/x86/entry/common.c                  |    1 +
 arch/x86/entry/entry_64.S                |   75 +-
 arch/x86/entry/entry_64_pvm.S            |  189 ++
 arch/x86/entry/entry_64_switcher.S       |  270 ++
 arch/x86/entry/vsyscall/vsyscall_64.c    |    4 +
 arch/x86/include/asm/alternative.h       |   14 +
 arch/x86/include/asm/cpufeatures.h       |    2 +
 arch/x86/include/asm/disabled-features.h |    8 +-
 arch/x86/include/asm/idtentry.h          |   12 +-
 arch/x86/include/asm/init.h              |    5 +
 arch/x86/include/asm/kvm-x86-ops.h       |    2 +
 arch/x86/include/asm/kvm_host.h          |   30 +-
 arch/x86/include/asm/kvm_para.h          |    7 +
 arch/x86/include/asm/page_64.h           |    3 +
 arch/x86/include/asm/paravirt.h          |   14 +-
 arch/x86/include/asm/pgtable_64_types.h  |   14 +-
 arch/x86/include/asm/processor.h         |    5 +
 arch/x86/include/asm/ptrace.h            |    5 +
 arch/x86/include/asm/pvm_para.h          |  103 +
 arch/x86/include/asm/segment.h           |   14 +-
 arch/x86/include/asm/switcher.h          |  119 +
 arch/x86/include/uapi/asm/kvm_para.h     |    8 +-
 arch/x86/include/uapi/asm/pvm_para.h     |  131 +
 arch/x86/kernel/Makefile                 |    1 +
 arch/x86/kernel/asm-offsets_64.c         |   31 +
 arch/x86/kernel/cpu/common.c             |   11 +
 arch/x86/kernel/head64.c                 |   10 +
 arch/x86/kernel/head64_identity.c        |  108 +-
 arch/x86/kernel/head_64.S                |   34 +
 arch/x86/kernel/idt.c                    |    2 +
 arch/x86/kernel/kvm.c                    |    2 +
 arch/x86/kernel/ldt.c                    |    3 +
 arch/x86/kernel/nmi.c                    |    8 +-
 arch/x86/kernel/process_64.c             |   10 +-
 arch/x86/kernel/pvm.c                    |  579 ++++
 arch/x86/kernel/traps.c                  |    3 +
 arch/x86/kernel/vmlinux.lds.S            |   18 +
 arch/x86/kvm/Kconfig                     |    9 +
 arch/x86/kvm/Makefile                    |    5 +-
 arch/x86/kvm/cpuid.c                     |   26 +-
 arch/x86/kvm/cpuid.h                     |    3 +
 arch/x86/kvm/host_entry.S                |   50 +
 arch/x86/kvm/mmu/mmu.c                   |   36 +-
 arch/x86/kvm/mmu/paging_tmpl.h           |    3 +
 arch/x86/kvm/mmu/spte.c                  |    4 +
 arch/x86/kvm/pvm/host_mmu.c              |  119 +
 arch/x86/kvm/pvm/pvm.c                   | 3257 ++++++++++++++++++++++
 arch/x86/kvm/pvm/pvm.h                   |  169 ++
 arch/x86/kvm/svm/svm.c                   |    4 +
 arch/x86/kvm/trace.h                     |    7 +-
 arch/x86/kvm/vmx/vmenter.S               |   43 -
 arch/x86/kvm/vmx/vmx.c                   |   18 +-
 arch/x86/kvm/x86.c                       |   33 +-
 arch/x86/kvm/x86.h                       |   18 +
 arch/x86/mm/dump_pagetables.c            |    3 +-
 arch/x86/mm/kaslr.c                      |    8 +-
 arch/x86/mm/pti.c                        |    7 +
 arch/x86/platform/pvh/enlighten.c        |   22 +
 arch/x86/platform/pvh/head.S             |    4 +
 arch/x86/tools/relocs.c                  |   88 +-
 arch/x86/tools/relocs.h                  |   20 +-
 arch/x86/tools/relocs_common.c           |   38 +-
 arch/x86/xen/enlighten_pv.c              |    1 +
 include/linux/kvm_host.h                 |   10 +
 include/linux/vmalloc.h                  |    2 +
 include/uapi/Kbuild                      |    4 +
 mm/vmalloc.c                             |   10 +
 virt/kvm/pfncache.c                      |    2 +-
 73 files changed, 6793 insertions(+), 166 deletions(-)
 create mode 100644 Documentation/virt/kvm/x86/pvm-spec.rst
 create mode 100644 arch/x86/entry/entry_64_pvm.S
 create mode 100644 arch/x86/entry/entry_64_switcher.S
 create mode 100644 arch/x86/include/asm/pvm_para.h
 create mode 100644 arch/x86/include/asm/switcher.h
 create mode 100644 arch/x86/include/uapi/asm/pvm_para.h
 create mode 100644 arch/x86/kernel/pvm.c
 create mode 100644 arch/x86/kvm/host_entry.S
 create mode 100644 arch/x86/kvm/pvm/host_mmu.c
 create mode 100644 arch/x86/kvm/pvm/pvm.c
 create mode 100644 arch/x86/kvm/pvm/pvm.h

--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-1-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-1-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-1-jiangshanlai@gmail.com/#R)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-1-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-1-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r5380166ee3c0ce945348e361d39bf5ca577a1fbe)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#edb114605fe7991cfe69b8719c9ca244d6e37e4b1) **[RFC PATCH 01/73] KVM: Documentation: Add the specification for PVM**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
**@ 2024-02-26 14:35 ` Lai Jiangshan**
  2024-02-26 14:35 ` [[RFC PATCH 02/73] x86/ABI/PVM: Add PVM-specific ABI header file](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m298a49e9ed7494c1be0fd7a4563fb73d2c26cb2f) Lai Jiangshan
                   ` [(73 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r298a49e9ed7494c1be0fd7a4563fb73d2c26cb2f)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rdb114605fe7991cfe69b8719c9ca244d6e37e4b1)
From: Lai Jiangshan @ 2024-02-26 14:35 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-2-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-2-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143422)
  Cc: Lai Jiangshan, SU Hang, Hou Wenlong, Linus Torvalds,
	Peter Zijlstra, Sean Christopherson, Thomas Gleixner,
	Borislav Petkov, Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143422), Paolo Bonzini, x86, Kees Cook,
	Juergen Gross, Jonathan Corbet, [linux-doc](https://lore.kernel.org/linux-doc/?t=20240226143422)

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

Add the specification to describe the PVM ABI, which is a new
lightweight software-based virtualization for x86.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: SU Hang <darcy.sh@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [Documentation/virt/kvm/x86/pvm-spec.rst](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-2-jiangshanlai::40gmail.com:1Documentation:virt:kvm:x86:pvm-spec.rst) | 989 ++++++++++++++++++++++++
 1 file [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#edb114605fe7991cfe69b8719c9ca244d6e37e4b1), 989 insertions(+)
 create mode 100644 Documentation/virt/kvm/x86/pvm-spec.rst

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-2-jiangshanlai::40gmail.com:1Documentation:virt:kvm:x86:pvm-spec.rst) --git a/Documentation/virt/kvm/x86/pvm-spec.rst b/Documentation/virt/kvm/x86/pvm-spec.rst
new file mode 100644
index 000000000000..04d3cf93d99f
--- /dev/null
+++ b/Documentation/virt/kvm/x86/pvm-spec.rst @@ -0,0 +1,989 @@ +.. SPDX-License-Identifier: GPL-2.0
+
+=====================
+X86 PVM Specification
+=====================
+
+Underlying states
+-----------------
+
+**The PVM guest is only running on the underlying CPU with underlying
+CPL=3.**
+
+The term ``underlying`` refers to the actual hardware architecture
+state. For example ``underlying CR3`` is the physic ``CR3`` of the
+architecture. On the contrary, ``CR3`` or ``PVM CR3`` is the virtualized
+``PVM CR3`` register. Any x86 states or registers in this document are
+PVM states or registers unless any of "underlying", "physic", or
+"hardware" is used to describe the states or registers. The doc uses
+"underlying" mostly to describe the actual hardware architecture.
+
+When the PVM guest is only running on the underlying CPU, it not only
+runs with underlying CPL=3 but also with the following underlying states
+and registers:
+
++-------------------+--------------------------------------------------+
+| Registers         | Values                                           |
++===================+==================================================+
+| Underlying RFLAGS | IOPL=VM=VIF=VIP=0, IF=1, fixed-bit1=1.           |
++-------------------+--------------------------------------------------+
+| Underlying CR3    | implementation-defined value, typically          |
+|                   | shadows the ``PVM CR3`` with extra pages         |
+|                   | mapped including the switcher.                   |
++-------------------+--------------------------------------------------+
+| Underlying CR0    | PE=PG=WP=ET=NE=AM=MP=1, CD=NW=EM=TS=0            |
++-------------------+--------------------------------------------------+
+| Underlying CR4    | VME=PVI=0, PAE=FSGSBASE=1,                       |
+|                   | others=implementation-defined                    |
++-------------------+--------------------------------------------------+
+| Underlying EFER   | SCE=LMA=LME=1, NXE=implementation-defined.       |
++-------------------+--------------------------------------------------+
+| Underlying GDTR   | All Entries with DPL<3 in the table are          |
+|                   | hypervisor-defined values. The table must        |
+|                   | have entries with DPL=3 for the selectors:       |
+|                   | ``__USER32_CS``, ``__USER_CS``,                  |
+|                   | ``__USER_DS`` (``__USER32_CS`` is                |
+|                   | implementation-defined value,                    |
+|                   | ``__USER_CS``\ =\ ``__USER32_CS``\ +8,           |
+|                   | ``__USER_DS``\ =\ ``__USER32_CS``\ +16)          |
+|                   | and may have other hypervisor-defined            |
+|                   | DPL=3 data entries. Typically a                  |
+|                   | host-defined CPUNODE entry is also in the        |
+|                   | underlying ``GDT`` table for each host CPU       |
+|                   | and its content (segment limit) can be           |
+|                   | visible to the PVM guest.                        |
++-------------------+--------------------------------------------------+
+| Underlying TR     | implementation-defined, no IOPORT is             |
+|                   | allowed.                                         |
++-------------------+--------------------------------------------------+
+| Underlying LDTR   | must be NULL                                     |
++-------------------+--------------------------------------------------+
+| Underlying IDT    | implementation-defined. All gate entries         |
+|                   | are with DPL=0, except for the entries for       |
+|                   | vector=3,4 and a vector>32 (for legacy           |
+|                   | syscall, normally 0x80) with DPL=3.              |
++-------------------+--------------------------------------------------+
+| Underlying CS     | loaded with the selector ``__USER_CS`` or        |
+|                   | ``__USER32_CS``.                                 |
++-------------------+--------------------------------------------------+
+| Underlying SS     | loaded with the selector ``__USER_DS``.          |
++-------------------+--------------------------------------------------+
+| Underlying        | loaded with the selector NULL or                 |
+| DS/ES/FS/GS       | ``__USER_DS`` or other DPL=3 data entries        |
+|                   | in the underlying ``GDT`` table.                 |
++-------------------+--------------------------------------------------+
+| Underlying DR6    | 0xFFFF0FF0, until a hardware #DB is              |
+|                   | delivered and the hardware exits the guest       |
++-------------------+--------------------------------------------------+
+| Underlying DR7    | ``DR7_GD``\ =0; illegitimate linear              |
+|                   | address (see address space separation) in        |
+|                   | ``DR0-DR3`` causes the corresponding bits        |
+|                   | in the ``underlying DR7`` cleared.               |
++-------------------+--------------------------------------------------+
+
+In summary, the underlying states are typical x86 states to run
+unprivileged software with stricter limitations.
+
+PVM modes and states
+--------------------
+
+PVM has three PVM modes and they are modified IA32-e mode with PVM ABI.
+
+- PVM 64bit supervisor mode: modified X86 64bit supervisor mode with
+  PVM ABI
+
+- PVM 64bit user mode: X86 64bit user mode with PVM event handling
+
+- PVM 32bit compatible user mode: x86 compatible user mode with PVM
+  event handling
+
+| A VMM or hypervisor may also support non-PVM mode. They are non-IA32-e
+  mode or IA32-e compatible kernel mode.
+| The specification has nothing to do with any non-PVM mode. But if the
+  hypervisor or the VMM can not boot the software directly into PVM
+  mode, the hypervisor can implement non-PVM mode as bootstrap.
+| Bootstrapping is implementation-defined. Bootstrapping in non-PVM mode
+  should conform to pure X86 ABI until it enters X86 64bit supervisor
+  mode and then the PVM hypervisor changes privilege registers(``CR0``,
+  ``CR4,`` ``EFER``, ``MSR_STAR``) to conform to PVM mode and transits
+  it into PVM 64bit supervisor mode.
+
+States or registers on PVM modes
+~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
+
++-----------------------+----------------------------------------------+
+| Register              | Values                                       |
++=======================+==============================================+
+| ``CR3`` and           | PVM switches ``CR3`` with                    |
+| MSR_PVM_SWITCH_CR3    | MSR_PVM_SWITCH_CR3 when switching            |
+|                       | supervisor/user mode. Hypercall              |
+|                       | HC_LOAD_PGTBL can load ``CR3`` and           |
+|                       | MSR_PVM_SWITCH_CR3 in one call. It           |
+|                       | is recommended software to use               |
+|                       | different ``CR3`` for supervisor             |
+|                       | and user modes like KPTI.                    |
++-----------------------+----------------------------------------------+
+| ``CR0``               | PE=PG=WP=ET=NE=AM=MP=1,                      |
+|                       | CD=NW=EM=TS=0                                |
++-----------------------+----------------------------------------------+
+| ``CR4``               | VME/PVI=0; PAE/FSGSBASE=1;                   |
+|                       | UMIP/PKE/OSXSAVE/OSXMMEXCPT/OSFXSR=host.     |
+|                       | PCID is recommended to be set even           |
+|                       | if the ``underlying CR4.PCID`` is            |
+|                       | not set. SMAP=SMEP=0 and the                 |
+|                       | corresponding features are                   |
+|                       | disabled in CPUID leaves.                    |
++-----------------------+----------------------------------------------+
+| ``EFER``              | SCE=LMA=LME=1; NXE=underlying;               |
++-----------------------+----------------------------------------------+
+| ``RFLAGS``            | Mapped to the underlying RFLAGS except for   |
+|                       | the RFLAGS.IF. (The underlying RFLAGS.IF     |
+|                       | is always 1.)                                |
+|                       |                                              |
+|                       | IOPL=VM=VIF=VIP=0, fixed-bit1=1.             |
+|                       | AC is not recommended to be set in           |
+|                       | the supervisor mode.                         |
+|                       |                                              |
+|                       | The PVM interrupt flag is defined as:        |
+|                       |                                              |
+|                       | - the bit 9 of the PVCS::event_flags when in |
+|                       |   supervisor mode.                           |
+|                       | - 1, when in user mode.                      |
+|                       | - 0, when in supervisor mode if              |
+|                       |   MSR_PVM_VCPU_CTRL_STRUCT=0.                |
++-----------------------+----------------------------------------------+
+| ``GDTR``              | ignored (can be written and read             |
+|                       | to get the last written value but            |
+|                       | take no effect). The effective PVM           |
+|                       | ``GDT`` table can be considered to           |
+|                       | be a read-only table consisting of           |
+|                       | entries: emulated supervisor mode            |
+|                       | ``CS/SS`` and entries in                     |
+|                       | ``underlying GDT`` with DPL=3. The           |
+|                       | hypercall PVM_HC_LOAD_TLS can                |
+|                       | modify part of the                           |
+|                       | ``underlying GDT``.                          |
++-----------------------+----------------------------------------------+
+| ``TR``                | ignored. Replaced by PVM event               |
+|                       | handling                                     |
++-----------------------+----------------------------------------------+
+| ``IDT``               | ignored. Replaced by PVM event               |
+|                       | handling                                     |
++-----------------------+----------------------------------------------+
+| ``LDTR``              | ignored. No replacement so it can            |
+|                       | be considered disabled.                      |
++-----------------------+----------------------------------------------+
+| ``CS`` in             | emulated. the ``underlying CS`` is           |
+| supervisor mode       | ``__USER_CS``.                               |
++-----------------------+----------------------------------------------+
+| ``CS`` in             | mapped to the ``underlying CS``              |
+| user mode             | which is ``__USER_CS`` or                    |
+|                       | ``__USER32_CS``                              |
++-----------------------+----------------------------------------------+
+| ``SS`` in             | emulated. the ``underlying SS`` is           |
+| supervisor mode       | ``__USER_DS``.                               |
++-----------------------+----------------------------------------------+
+| ``SS`` in             | mapped to the ``underlying SS``              |
+| user mode             | whose value is ``__USER_DS``.                |
++-----------------------+----------------------------------------------+
+| DS/ES/FS/GS           | mapped to the                                |
+|                       | ``underlying DS/ES/FS/GS``, loaded           |
+|                       | with the selector NULL or                    |
+|                       | ``__USER_DS`` or other DPL=3 data            |
+|                       | entries in the ``underlying GDT``            |
+|                       | table.                                       |
++-----------------------+----------------------------------------------+
+| interrupt shadow mask | no interrupt shadow mask                     |
++-----------------------+----------------------------------------------+
+| NMI shadow mask       | set when #NMI is delivered and               |
+|                       | cleared when and only when                   |
+|                       | EVENT_RETURN_USER or                         |
+|                       | EVENT_RETURN_SUPERVISOR                      |
++-----------------------+----------------------------------------------+
+
+MSR_PVM_VCPU_CTRL_STRUCT
+~~~~~~~~~~~~~~~~~~~~~~~~
+
+.. code::
+
+   struct pvm_vcpu_struct {
+       u64 event_flags;
+       u32 event_errcode;
+       u32 event_vector;
+       u64 cr2;
+       u64 reserved0[5];
+
+       u16 user_cs, user_ss;
+       u32 reserved1;
+       u64 reserved2;
+       u64 user_gsbase;
+       u32 eflags;
+       u32 pkru;
+       u64 rip;
+       u64 rsp;
+       u64 rcx;
+       u64 r11;
+   }
+
+PVCS::event_flags
+^^^^^^^^^^^^^^^^^
+
+| ``PVCS::event_flags.IF``\ (bit 9): interrupt enable flag: The flag
+  is set to respond to maskable external interrupts; and cleared to
+  inhibit maskable external interrupts.
+|   The flag works only in supervisor mode. The VCPU always responds to
+    maskable external interrupts regardless of the value of this flag in
+    user mode. The flag is unchanged when the VCPU switches
+    user/supervisor modes, even when handling the synthetic instruction
+    EVENT_RETURN_USER. The guest is responsible for clearing the flag
+    before switching to user mode (issuing EVENT_RETURN_USER) to ensure
+    that the external interrupt is disabled when the VCPU is switched back
+    from user mode later.
+
+| ``PVCS::event_flags.IP``\ (bit 8): interrupt pending flag: The
+  hypervisor sets it if it fails to inject a maskable event to the VCPU
+  due to the interrupt-enable flag being cleared in supervisor mode.
+|   The guest is responsible for issuing a hypercall PVM_HC_IRQ_WIN when
+    the guest sees this bit after setting the PVCS::event_flags.IF.
+    The hypervisor clears this bit in handling
+    PVM_HC_IRQ_WIN/IRQ_HLT/EVENT_RETURN_USER/EVENT_RETURN_HYPERVISOR.
+
+Other bits are reserved (Software should set them to zero).
+
+PVCS::event_vector, PVCS::event_errcode
+^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
+
+If the vector event being delivered is from user mode or with vector >= 32
+from supervisor mode ``PVCS::event_vector`` is set to the vector number. And
+if the event has an error code, ``PVCS::event_errcode`` is set to the code.
+
+PVCS::cr2
+^^^^^^^^^
+
+If the event being delivered is a page fault (#PF), ``PVCS::cr2`` is set
+to be ``CR2`` (the faulting linear address).
+
+PVCS::user_cs, PVCS::user_ss, PVCS::user_gsbase, PVCS::pkru, PVCS::rsp, PVCS::eflags, PVCS::rip, PVCS::rcx, PVCS::r11
+^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
+
+| ``CS``, ``SS``, ``GSBASE``, ``PKRU``, ``RSP``, ``EFLAGS``, ``RIP``,
+  ``RCX``, and ``R11`` are saved to ``PVCS::user_cs``,
+  ``PVCS::user_ss``, ``PVCS::user_gsbase``, ``PVCS::pkru``,
+  ``PVCS::rsp``, ``PVCS::eflags``, ``PVCS::rip``, ``PVCS::rcx``,
+  ``PVCS::r11`` correspondingly when handling the synthetic instruction
+  EVENT_RETURN_USER or vice vers when the architecture is switching to
+  supervisor mode on any event in user mode.
+| The value of ``PVCS::user_gsbase`` is semi-canonicalized before being
+  set to the ``underlying GSBASE`` by adjusting bits 63:N to get the
+  value of bit N--1, where N is the host's linear address width (48 or
+  57).
+| The value of ``PVCS::eflags`` is standardized before setting to the
+  ``underlying RFLAGS``. IOPL, VM, VIF, and VIP are cleared, and IF and
+  FIXED1 are set.
+| If an event with vector>=32 happens in supervisor mode, ``RSP``,
+  ``EFLAGS``, ``RIP``, ``RCX``, and ``R11`` are saved to ``PVCS::rsp``,
+  ``PVCS::eflags``, ``PVCS::rip``, ``PVCS::rcx``, ``PVCS::r11``
+  correspondingly.
+
+TSC MSRs
+~~~~~~~~
+
+TSC ABI is not settled down yet.
+
+X86 MSR
+~~~~~~~
+
+MSR_GS_BASE/MSR_KERNEL_GS_BASE
+^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
+
+``MSR_GS_BASE`` is mapped to the ``underlying GSBASE``.
+
+The ``MSR_KERNEL_GS_BASE`` is recommended to be synced with
+``MSR_GS_BASE`` when in supervisor mode, and supervisor software is
+recommended to maintain its version of ``MSR_KERNEL_GS_BASE``, and
+``PVCS::user_gsbase`` is recommended to be used on this purpose.
+
+When the CPU is switching from user mode to supervisor mode,
+``PVCS::user_gsbase`` is updated as the value of ``MSR_GS_BASE`` (the
+``underlying GSBASE``), and the value of ``MSR_GS_BASE`` is reset to
+``MSR_KERNEL_GS_BASE`` atomically at the same time.
+
+When the CPU is switching from supervisor mode to user mode,
+``MSR_KERNEL_GS_BASE`` is normally set with the value of
+``MSR_GS_BASE`` (but the hypervisor is allowed to omit this operation
+because ``MSR_GS_BASE`` and ``MSR_KERNEL_GS_BASE`` are expected to be
+the same when in supervisor), and the ``MSR_GS_BASE`` is loaded with
+``PVCS::user_gsbase``.
+
+WRGSBASE is not recommended to be used in supervisor mode.
+
+MSR_SYSCALL_MASK
+^^^^^^^^^^^^^^^^
+
+Ignored, when syscall, ``RFLAGS`` is set to a default value.
+
+MSR_STAR
+^^^^^^^^
+
+| ``__USER_CS,`` ``__USER_DS`` derived from it must be the same as
+  host's ``__USER_CS,`` ``__USER_DS`` and have RPL=3. ``__KERNEL_CS``,
+  ``__KERNEL_DS`` derived from it must have RPL=0 and be the same value
+  as the current PVM ``CS`` ``SS`` registers hold respectively.
+  Otherwise #GP.
+| X86 forces RPL for derived ``__USER_CS,`` ``__USER_DS``,
+  ``__USER32_CS``, ``__KERNEL_CS``, (not ``__KERNEL_DS``) when using
+  them, so the RPLs can be an arbitrary value.
+
+MSR_CSTAR, MSR_IA32_SYSENTER_CS/EIP/ESP
+^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
+
+Ignored, the software should use INTn instead for compatibility
+syscalls.
+
+MSR_IA32_PKRS
+^^^^^^^^^^^^^
+
+See "`Protection Keys <#protection-keys>`__".
+
+PVM MSRs
+~~~~~~~~
+
+MSR_PVM_SWITCH_CR3
+^^^^^^^^^^^^^^^^^^
+
+Switched with ``CR3`` when mode switching. No TLB request is issued when
+mode switching.
+
+MSR_PVM_EVENT_ENTRY
+^^^^^^^^^^^^^^^^^^^
+
+| The value is the entry point for vector events from the PVM user mode.
+| The value+256 is the entry point for vector events (vector < 32) from
+  the PVM supervisor mode.
+| The value+512 is the entry point for vector events (vector >= 32) from
+  the PVM supervisor mode.
+
+MSR_PVM_SUPERVISOR_RSP
+^^^^^^^^^^^^^^^^^^^^^^
+
+When switching from supervisor mode to user mode, this MSR is
+automatically saved with ``RSP`` which is restored from it when
+switching back from user mode.
+
+MSR_PVM_SUPERVISOR_REDZONE
+^^^^^^^^^^^^^^^^^^^^^^^^^^
+
+When delivering the event from supervisor mode, a fixed-size area
+is reserved below the current ``RSP`` and can be safely used by
+guest. The size is specified in this MSR.
+
+MSR_PVM_LINEAR_ADDRESS_RANGE
+^^^^^^^^^^^^^^^^^^^^^^^^^^^^
+
+See "`Paging <#paging>`__".
+
+PML4_INDEX_START, PML4_INDEX_END, PML5_INDEX_START, and PML5_INDEX_END
+are encoded in the MSR and they are all 9 bits value with the most
+significant bit set:
+
+- bit 57-63 are all set; bit 48-56: PML5_INDEX_END, bit 56 must be set.
+- bit 41-47 are all set; bit 32-40: PML5_INDEX_START, bit 40 must be set.
+- bit 25-31 are all set; bit 16-24:PML4_INDEX_END, bit 24 must be set.
+- bit 9-15 are all set; bit 0-8:PML4_INDEX_START, bit 8 must be set.
+
+constraints:
+
+- 256 <= PML5_INDEX_START < PML5_INDEX_END < 511
+- 256 <= PML4_INDEX_START < PML4_INDEX_END < 511
+- PML5_INDEX_START = PML5_INDEX_END = 0x1FF if the
+  ``underlying CR4.LA57`` is not set.
+
+The three legitimate address ranges for PVM virtual addresses:
+
+::
+
+  [ (1UL << 48) * (0xFE00 | PML5_INDEX_START), (1UL << 48) * (0xFE00 | PML5_INDEX_END) )
+  [ (1UL << 39) * (0x1FFFE00 | PML4_INDEX_START), (1UL << 39) * (0x1FFFE00 | PML4_INDEX_END) )
+  Lower half address (canonical address with bit63=0)
+
+The MSR is initialized as the widest ranges when the CPU is reset. The
+ranges should be sub-ranges of these initialized ranges when writing to
+the MSR or migration.
+
+| Pagetable walking is confined to these legitimate address ranges.
+| Note:
+
+- the top 2G is not in the range, so the guest supervisor software should
+  be PIE kernel.
+- Breakpoints (``DR0-DR3``) out of these ranges are not activated in the
+  underlying DR7.
+
+MSR_PVM_RETU_RIP, MSR_PVM_RETS_RIP
+^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
+
+The bare SYSCALL instruction staring at ``MSR_PVM_RETU_RIP`` or
+``MSR_PVM_RETS_RIP`` is synthetic instructions to return to
+user/supervisor mode. See "`PVM Synthetic
+Instructions <#pvm-synthetic-instructions>`__" and "`Events and Mode
+Changing <#events-and-mode-changing>`__".
+
+.. pvm-synthetic-instructions:
+
+PVM Synthetic Instructions
+~~~~~~~~~~~~~~~~~~~~~~~~~~
+
+PVM_SYNTHETIC_CPUID: invlpg 0xffffffffff4d5650;cpuid
+^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
+
+Works the same as the bare CPUID instruction generally, but it is
+ensured to be handled by the PVM hypervisor and reports the corresponding
+CPUID results for PVM.
+
+PVM_SYNTHETIC_CPUID is supposed to not trigger any trap in the real or virtual
+x86 kernel mode and is also guaranteed to trigger a trap in the underlying
+hardware user mode for the hypervisor emulating it. The hypervisor emulates
+both of the basic instructions, while the INVLPG is often emulated as an NOP
+since 0xffffffffff4d5650 is normally out of the allowed linear address ranges.
+
+EVENT_RETURN_SUPERVISOR: SYSCALL instruction starting at MSR_PVM_RETS_RIP
+^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
+
+EVENT_RETURN_SUPERVISOR instruction returns from supervisor mode to
+supervisor mode with the return state on the stack.
+
+EVENT_RETURN_USER: SYSCALL instruction starting at MSR_PVM_RETU_RIP
+^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
+
+EVENT_RETURN_USER instruction returns from supervisor mode to user
+mode with the return state on the PVCS.
+
+X86 Instructions with changed behavior
+~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
+
+CPUID
+^^^^^
+
+Guest CPUID instruction would get the host's CPUID information normally
+(when CPUID faulting is not enabled), and the synthetic instruction
+KVM_CPUID is recommended to be used instead in guest supervisor
+software.
+
+SGDT/SIDT/SLDT/STR/SMSW
+^^^^^^^^^^^^^^^^^^^^^^^
+
+Guest SGDT/SIDT/SLDT/STR/SMSW instructions would get the host's
+information. ``CR4.UMIP`` is in effect for guests only when the host
+enables it.
+
+LAR/LSL/VERR/VERW
+^^^^^^^^^^^^^^^^^
+
+Guest LAR/LSL/VERR/VERW instructions would get segment information from
+host ``GDT``.
+
+STAC/CLAC, SWAPGS, SYSEXIT, SYSRET
+^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
+
+These instructions are not allowed for PVM supervisor software, using
+them would result in unexpected behavior for the guest.
+
+SYSENTER
+^^^^^^^^
+
+Results in #GP.
+
+INT n
+^^^^^
+
+Only 0x80 and 0x3 are allowed in guests. Other INT n results in #GP.
+
+RDPKRU/WRPKRU
+^^^^^^^^^^^^^
+
+When the guest is in supervisor mode, RDPKRU/WRPKRU would access the
+``underlying PKRU`` register which is effectively PVM's
+``MSR_IA32_PKRS``, so the guest supervisor software should access user
+``PKRU`` via ``PVCS::pkru``.
+
+CPUID leaf
+~~~~~~~~~~
+
+- Features disabled in the host are also disabled in the guest except for
+  some specially handled features such as PCID and PKS.
+
+  - PCID can be enabled even host PCID is disabled or the hardware doesn't
+    support PCID.
+  - PKS can be enabled if the host ``CR4.PKE`` is set because guest PKS is
+    handled via hardware PKE.
+
+- Features that require the hypervisor's handling but are not yet
+  implemented are disabled in the guest.
+
+- Some features that require hardware-privileged instructions are
+  disabled in the guest.
+
+  - XSAVES/XRESTORES/MSR_IA32_XSS is not enabled.
+
+- Features that require distinguishing U/S pages are disabled in the
+  guest.
+
+  - SMEP/SMAP is disabled. LASS is also disabled.
+
+KVM and PVM specific CPUID leafs
+^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
+
+- When CPUID.EAX = KVM_CPUID_SIGNATURE (0x40000000) is entered, the
+  output CPUID.EAX will be at least 0x40000002 which is
+  KVM_CPUID_VENDOR_FEATURES (iff the hypervisor is a PVM hypervisor).
+- When CPUID.EAX = KVM_CPUID_VENDOR_FEATURES(0x40000002) is entered,
+  the output CPUID.EAX is PVM features; CPUID.EBX is 0x6d7670 ("pvm");
+  CPUID.ECX and CPUID.EDX are reserved (0).
+
+PVM booting sequence
+^^^^^^^^^^^^^^^^^^^^
+
+The PVM supervisor software has to relocate itself to conform its
+allowed address ranges (See MSR_PVM_LINEAR_ADDRESS_RANGE) and prepare
+itself for its special event handling mechanism on booting.
+
+PVM software can be booted via linux general booting entry points, so
+the software must detect whether itself is PVM as early as possible.
+
+Booting sequence for detecting PVM in 64 bit linux general booting entry:
+
+- check if the underlying EFLAGS.IF is 1
+- check if the underlying CS.CPL is 3
+- use the synthetic instruction KVM_CPUID to check KVM_CPUID_SIGNATURE
+  and KVM_CPUID_VENDOR_FEATURES including checking the signature.
+
+PVM is the first to define such booting sequence, so any later paravirt
+hypervisor that can boot a 64 bit linux guest with underlying
+EFLAGS.IF==1 and CS.CPL == 3 from the linux general booting entry points
+should support the synthetic instruction KVM_CPUID for compatibility.
+
+.. paging:
+
+Paging
+------
+
+PVM MMU has two registers for pagetables: ``CR3`` and ``MSR_PVM_SWITCH_CR3``
+and they are automatically switched on switching user/supervisor modes.
+When in supervisor mode, ``CR3`` holds the kernel pagetable and
+``MSR_PVM_SWITCH_CR3`` holds the user pagetable. These two pagetables work
+in the same way as the two pagetables for KPTI.
+
+The U/S bit in the paging struct is not always honored in PVM and is
+sometimes ignored. User mode software may or may not access the final
+page even if it is a supervisor page (in the view of X86). In fact, due
+to the lack of legacy segment-based isolation, both the user page and
+kernel page in PVM are shadowed as user pages in the underlying
+pagetable with only hypervisor pages with the U bit cleared in the
+underlying pagetable.
+
+It is recommended to have no supervisor pages in the user pagetable. (To
+make more use of the existing KPTI code, this rule can be relaxed as "it
+is recommended that any paging tree should be all supervisor pages or
+all user pages in the user pagetable except for the root PGD
+pagetable.")
+
+And the lack of legacy segment-based isolation is also the reason why
+PVM has two registers for pagetables and the automatically switching
+feature.
+
+Due to the ignoring U/S bit, some features are disabled in PVM.
+
+- SMEP is disabled and ``CR4.SMEP`` can not be set. The guest can use
+  the NX bit for the user pages in the supervisor pagetable to regain
+  the protection.
+
+- SMAP is disabled and ``CR4.SMAP`` can not be set. The guest can
+  emulate it via PKS.
+
+- PKS feature is changed. Protection Key protection doesn't consider
+  the U/S bit, it protects all the data access based on the key. The
+  software should distribute different keys for supervisor pages and
+  user pages.
+
+TLB
+~~~
+
+| TLB entries are considered to be tagged by the root page table (PGD)
+  pointer.
+
+- Hypercall HC_TLB_FLUSH_CURENT, HC_TLB_FLUSH, and HC_TLB_LOAD_PGTBL
+  flush TLB entries based on the tags (PGD of ``CR3`` and
+  ``MSR_PVM_SWITCH_CR3``).
+- ``CR3`` and ``MSR_PVM_SWITCH_CR3`` are swapped on switching
+  user/supervisor mode but no TLB flushing is performed.
+- Writing to ``CR3`` may not flush TLB for ``MSR_PVM_SWITCH_CR3``.
+- WRMSR or HC_WRMSR to ``MSR_PVM_SWITCH_CR3`` doesn't flush TLB.
+- ``CR4.PCID`` bit is recommended to be set even if the
+  ``underlying CR4.PCID`` is cleared so that the PVM TLB can be flushed
+  only on demand.
+
+Exclusive address ranges
+~~~~~~~~~~~~~~~~~~~~~~~~
+
+A portion of the upper half of the linear address is separated from
+the host kernel and the host doesn't use this separated portion. Only
+the address in this separated portion and the lower half is the
+guest-allowed linear address.
+
+.. protection-keys:
+
+Protection Keys
+~~~~~~~~~~~~~~~
+
+There are no distinctions between PVM user pages and PVM supervisor
+pages in the real hardware. Protection Keys protection protects all data
+accesses if enabled. ``CR4.PKE`` enables Protection Keys protection in
+user mode while ``CR4.PKS`` enables Protection Keys protection in
+supervisor mode.
+
+``CR4.PKS`` can only be enabled when ``CR4.PKE`` is enabled and
+``CR4.PKE`` can only be enabled when the underlying ``CR4.PKE`` is
+enabled.
+
+The ``underlying PKRU`` is the effective protection key register in both
+supervisor mode and user mode.
+
+The supervisor software should distribute different keys for supervisor
+mode and user mode so that the PVM ``PKRU`` and ``MSR_IA32_PKRS``\ (in
+guest supervisor view) are mapped to the different parts of the
+``underlying PKRU`` at the same time. With distributed different keys,
+``SUPERVISOR_KEYS_MASK`` can be defined in the guest supervisor.
+
+- The ``MSR_IA32_PKRS`` (in guest supervisor view) is the
+  ``underlying PKRU`` masked with ``SUPERVISOR_KEYS_MASK``, and it is
+  invisible to the hypervisor since ``SUPERVISOR_KEYS_MASK`` is
+  invisible to the hypervisor.
+- ``MSR_IA32_PKRS`` (in hypervisor view) is recommended to be set as the
+  same as ``MSR_IA32_PKRS`` (in guest supervisor view) before returning
+  to the user mode so that after the next switchback, the user part of
+  the ``underlying PKRU`` is access-denied and the supervisor part is
+  already set properly.
+
+If host/hardware ``CR4.PKE`` is set: the hypervisor/switcher will do
+these no matter what the value of ``CR4.PKE`` or ``CR4.PKS:``
+
+- supervisor -> user switching: load the ``underlying PKRU`` with
+  ``PVCS::pkru``
+
+- user -> supervisor switching: save the ``underlying PKRU`` to
+  ``PVCS::pkru``\ ， load the ``underlying PKRU`` with a default value
+  (0 or ``MSR_IA32_PKRS`` if ``CR4.PKS``).
+
+SMAP
+~~~~
+
+| PVM doesn't support SMAP, if the guest supervisor wants to protect
+  user access, it should use ``CR4.PKS``.
+
+- The software should distribute different keys for supervisor mode and
+  user mode.
+- ``MSR_IA32_PKRS`` should be set with the user keys as access-denied.
+- Events handlers in supervisor mode
+
+  - Save the old ``underlying PKRU`` and set it to ``MSR_IA32_PKRS`` on entry
+    so that the user part of the ``underlying PKRU`` is access-denied.
+  - Restore the ``underlying PKRU`` on exit.
+
+- When accessing to 'PVM user page' in supervisor mode
+
+  - Set the ``underlying PKRU`` to (``MSR_IA32_PKRS`` &
+    ``SUPERVISOR_KEYS_MASK``) \| ``PVCS::pkru``
+  - Restore the ``underlying PKRU`` when after it finishes the access.
+
+
+Events and Mode Changing
+------------------------
+
+Special Events
+~~~~~~~~~~~~~~
+
+No DoubleFault
+^^^^^^^^^^^^^^
+
+#DF is always promoted to TripleFault and brings down the PVM instance.
+
+Discarded #DB
+^^^^^^^^^^^^^
+
+When MOV/POP SS from a watched address is followed by any
+instruction-trap-induced supervisor mode entries, the MOV/POP SS that
+hits the watchpoint will be discarded instead.
+
+Vector events in user mode
+~~~~~~~~~~~~~~~~~~~~~~~~~~
+
+When vector events happen in user mode, the hypervisor is responsible
+for saving guest registers into ``PVCS``, including ``SS``, ``CS``,
+``PKRU``, ``GSBASE``, ``RSP``, ``RFLAGS``, ``RIP``, ``RCX``, and
+``R11``.
+
+The PVM hypervisor should also save the event vector into
+``PVCS::event_vector`` and the error code in ``PVCS::event_errcode``,
+and ``CR2`` into ``PVCS::cr2`` if it is pagefault event.
+
+No change to ``PVCS::event_flags.IF``\ (bit 9) during delivering any
+event in user mode, and the supervisor software is recommended to ensure
+it unset.
+
+Before returning to the guest supervisor, the PVM hypervisor will also
+load values to vCPU with the following actions:
+
+- Inexplicitly load ``CS/SS`` with the value the supervisor expects
+  from ``MSR_STAR``.
+
+  - The ``underlying CS/SS`` is loaded with host-defined ``__USER_CS``
+    and ``__USER_DS``.
+
+- Switch ``CR3`` with ``MSR_PVM_SWITCH_CR3`` without flushing TLB
+
+  - The ``underlying CR3`` is the actual shadow root page table for
+    the new ``PVM CR3``.
+
+- Load ``GSBASE`` with ``MSR_KERNEL_GS_BASE``.
+
+- Load ``RSP`` with ``MSR_PVM_KERNEL_RSP``.
+
+- Load ``RIP/RCX`` with ``MSR_PVM_EVENT_ENTRY``.
+
+- Load ``R11`` with (``X86_EFLAGS_IF`` \| ``X86_EFLAGS_FIXED``).
+
+- Load ``RFLAGS`` with ``X86_EFLAGS_FIXED``.
+
+  - The ``underlying RFLAGS`` is the same as ``R11`` which is
+    (``X86_EFLAGS_IF`` \| ``X86_EFLAGS_FIXED``).
+
+Vector events in supervisor mode
+~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
+
+The hypervisor handles vector events differently based on the vector
+and there is no IST stacks.
+
+The hypervisor handles vector events occurring in supervisor mode with
+vector number < 32 as these uninterruptible steps:
+
+- Subtract the fixed size (MSR_PVM_SUPERVISOR_REDZONE) from RSP.
+- Align RSP down to a 16-byte boundary.
+- Push R11
+- Push Rcx
+- Push SS
+- Push original RSP
+- Push RFLAGS
+
+  - ``RFLAGS.IF`` comes from ``PVCS::event_flags.IF`` (bit 9),
+     which means the pushed ``RFLAGS`` is ``(underlying RFLAGS ~
+     X86_EFLAGS_IF) | (PVCS::event_flags & X86_EFLAGS_IF)``
+
+- Push CS
+- Push RIP
+- Push vector (4 bytes), ERRCODE (4 bytes)
+- If it is pagefault, save CR2 into PVCS:cr2
+- No change to ``CS/SS.``
+- Load ``RSP`` with the result after the last push as described above.
+- Load ``R11`` with (``X86_EFLAGS_IF`` \| ``X86_EFLAGS_FIXED``).
+- Load ``RFLAGS`` with ``X86_EFLAGS_FIXED``.
+
+  - The ``underlying RFLAGS`` is the same as ``R11`` which is
+    (``X86_EFLAGS_IF`` \| ``X86_EFLAGS_FIXED``).
+  - PVCS::event_flags.IF will be cleared if it is previously set.
+
+- Load ``RIP/RCX`` with ``MSR_PVM_EVENT_ENTRY``\ +256
+
+The hypervisor handles vector events occurring in supervisor mode with
+vector number => 32 as these uninterruptible steps:
+
+- Save R11,RCX,RSP,EFLAGS,RIP to PVCS.
+- Save the vector number to PVCS:event_vector.
+- No change to ``CS/SS.``
+- Subtract the fixed size (MSR_PVM_SUPERVISOR_REDZONE) from RSP.
+- Load RSP with the current RSP value aligned down to a 16-byte boundary.
+- Load ``R11`` with (``X86_EFLAGS_IF`` \| ``X86_EFLAGS_FIXED``).
+- Load ``RFLAGS`` with ``X86_EFLAGS_FIXED``
+
+  - The ``underlying RFLAGS`` is the same as ``R11`` which is
+    (``X86_EFLAGS_IF`` \| ``X86_EFLAGS_FIXED``).
+  - PVCS::event_flags.IF will be cleared if it is previously set.
+
+- Load ``RIP/RCX`` with ``MSR_PVM_EVENT_ENTRY``\ +512
+
+User SYSCALL event
+~~~~~~~~~~~~~~~~~~
+
+SYSCALL instruction in PVM user mode is a user SYSCALL event and the
+hypervisor handles it almost as the same as vector events in user mode
+except that no change to ``PVCS::event_vector``, ``PVCS::event_errcode``
+and ``PVCS::cr2`` and ``RIP/RCX`` is loaded with ``MSR_LSTAR``.
+
+Specifically, the hypervisor saves guest registers into ``PVCS``,
+including ``SS``, ``CS``, ``PKRU``, ``GSBASE``, ``RSP``, ``RFLAGS``,
+RIP, ``RCX``, and ``R11``, and loads values to vCPU with the following
+actions:
+
+- Inexplicitly load ``CS/SS`` with the value the supervisor expects
+  from ``MSR_STAR``.
+
+  - The ``underlying CS/SS`` is loaded with host-defined ``__USER_CS``
+    and ``__USER_DS``.
+
+- Switch ``CR3`` with ``MSR_PVM_SWITCH_CR3`` without flushing TLB
+
+  - The ``underlying CR3`` is the actual shadow root page table for
+    the new ``PVM CR3``.
+
+- Load ``GSBASE`` with ``MSR_KERNEL_GS_BASE``.
+- Load ``RSP`` with ``MSR_PVM_KERNEL_RSP``.
+- Load ``RIP/RCX`` with ``MSR_LSTAR``.
+- Load ``R11`` with (``X86_EFLAGS_IF`` \| ``X86_EFLAGS_FIXED``).
+- Load ``RFLAGS`` with ``X86_EFLAGS_FIXED``.
+
+  - The ``underlying RFLAGS`` is the same as ``R11`` which is
+    (``X86_EFLAGS_IF`` \| ``X86_EFLAGS_FIXED``).
+  - No change to ``PVCS::event_flags.IF``\ (bit 9) during delivering
+    the SYSCALL event, and the supervisor software is recommended to
+    ensure it unset.
+
+
+Synthetic Instruction: EVENT_RETURN_USER
+~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
+
+This synthetic instruction is the only way for the PVM supervisor to
+switch to user mode.
+
+It works as the opposite operations of the event in user mode: load
+``CS``, ``SS``, ``GSBASE``, ``PKRU``, ``RSP``, ``RFLAGS``, RIP, ``RCX``,
+and ``R11`` from the ``PVCS`` respectively with some conversions to
+``GSBASE`` and ``RFLAGS``; switch ``CR3`` and ``MSR_PVM_SWITCH_CR3`` and
+return to user mode. The origian ``RSP`` is saved into
+``MSR_PVM_SUPERVISOR_RSP``.
+
+No change to ``PVCS::event_flags.IF``\ (bit 9) during handling it
+and the supervisor software is recommended to ensure it unset.
+
+Synthetic Instruction: EVENT_RETURN_SUPERVISOR
+~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
+
+| Return to PVM supervisor mode.
+| Work almost the same as IRETQ instruction except for ``RCX``, ``R11`` and
+  ``ERRCODE`` are also in the stack.
+
+It expects the stack frame:
+
+.. code::
+
+   R11
+   RCX
+   SS
+   RSP
+   RFLAGS
+   CS
+   RIP
+   ERRCODE
+
+Return to the context with RIP, RFLAGS, RSP, RCX, and R11 restored from the
+stack.
+
+The ``CS/SS`` and ``ERRCODE`` in the stack are ignored and the current PVM
+``CS/SS`` are unchanged.
+
+Hypercall event in supervisor mode
+~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
+
+Except for the synthetic instructions, SYSCALL instructions in PVM
+supervisor mode is a HYPERCALL.
+
+``RAX`` is the request number of the HYPERCALL. Some hypercall request
+numbers are PVM-specific HYPERCALLs. Other values are KVM-specific
+HYPERCALL.
+
+HYPERCALL be issued in supervisor software
+^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
+
+PVM supervisor software saves ``R10``, ``R11`` onto the stack and copies
+``RCX`` into ``R10``, and then invokes the SYSCALL instruction. After
+the HYPERCALL(SYSCALL instruction) returns, the software should get
+``RCX`` from ``R10`` and restore ``R10`` and ``R11`` from the stack.
+
+Hypercall's behavior should treat ``R10`` as ``RCX`` (in PVM
+hypervisor):
+
+.. code::
+
+   RCX := R10
+   pvm or kvm hypercall handling.
+   R10 := RCX
+
+If not specific, the return result is in ``RAX``.
+
+PVM_HC_LOAD_PGTBL
+^^^^^^^^^^^^^^^^^
+
+| Parameters: *flags*, *supervisor_pgd*, *user_pgd*.
+| Loads the pagetables
+|  \* flags bit0: flush the new supervisor_pgd and user_pgd.
+|  \* flags bit1: 4-level(bit1=0) or 5-level(bit1=1 && LA57 is supported
+  in the VCPU's cpuid features) pagetable, the ``CR4.LA57`` bit is also
+  changed correspondingly.
+|  \* supervisor_pgd: set to ``CR3``
+|  \* user_pgd: set to ``MSR_PVM_SWITCH_CR3``
+
+PVM_HC_IRQ_WIN
+^^^^^^^^^^^^^^
+
+| No parameters.
+| Infos the hypervisor that IRQ is enabled.
+
+PVM_HC_IRQ_HLT
+^^^^^^^^^^^^^^
+
+| No parameters.
+| Emulates the combination of X86 instructions "STI; HLT;".
+
+PVM_HC_TLB_FLUSH
+^^^^^^^^^^^^^^^^
+
+| No parameters.
+| Flush all TLB
+
+PVM_HC_TLB_FLUSH_CURRENT
+^^^^^^^^^^^^^^^^^^^^^^^^
+
+| No parameters.
+| Flush the TLB associated with the current ``PVM CR3`` and
+  ``MSR_PVM_SWITCH_CR3``.
+
+PVM_HC_TLB_INVLPG
+^^^^^^^^^^^^^^^^^
+
+| Parameters: *addr*.
+| Emulates INVLPG and Flush the TLB entries of the address.
+
+PVM_HC_LOAD_GS
+^^^^^^^^^^^^^^
+
+| Parameters: *gs_sel*.
+| Load GS with the selector gs_sel, if it fails, load GS with the NULL
+  selector.
+| Return the resulting GS_BASE.
+
+PVM_HC_RDMSR
+^^^^^^^^^^^^
+
+| Parameters: msr_index
+| Returns the MSR value or zero if the MSR index is invalid
+
+PVM_HC_WRMSR
+^^^^^^^^^^^^
+
+| Parameters: msr_index, msr_value
+| return 0 or -EINVAL.
+
+PVM_HC_LOAD_TLS
+^^^^^^^^^^^^^^^
+
+| Parameters: gdt_entry0, gdt_entry1, gdt_entry2
+| Rectify gdt_entry0, gdt_entry1, and gdt_entry2 and set them
+  continuously in the HOST ``GDT``.
+| Return HOST ``GDT`` index for *gdt_entry0*. --
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mdb114605fe7991cfe69b8719c9ca244d6e37e4b1) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-2-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-2-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-2-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-2-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-2-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-2-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rdb114605fe7991cfe69b8719c9ca244d6e37e4b1)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e298a49e9ed7494c1be0fd7a4563fb73d2c26cb2f) **[RFC PATCH 02/73] x86/ABI/PVM: Add PVM-specific ABI header file**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
  2024-02-26 14:35 ` [[RFC PATCH 01/73] KVM: Documentation: Add the specification for PVM](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mdb114605fe7991cfe69b8719c9ca244d6e37e4b1) Lai Jiangshan
**@ 2024-02-26 14:35 ` Lai Jiangshan**
  2024-02-26 14:35 ` [[RFC PATCH 03/73] x86/entry: Implement switcher for PVM VM enter/exit](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m9f2524581c30e4d18403640ceddcb5f0aaccd684) Lai Jiangshan
                   ` [(72 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r9f2524581c30e4d18403640ceddcb5f0aaccd684)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r298a49e9ed7494c1be0fd7a4563fb73d2c26cb2f)
From: Lai Jiangshan @ 2024-02-26 14:35 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-3-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-3-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143429)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143429), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

Add a PVM-specific ABI header file to describe the ABI between the guest
and hypervisor, which contains the hypercall numbers, virtual MSRS
index, and event data structure definitions. This is in preparation for
PVM.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/include/uapi/asm/pvm_para.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-3-jiangshanlai::40gmail.com:1arch:x86:include:uapi:asm:pvm_para.h) | 131 +++++++++++++++++++++++++++
 [include/uapi/Kbuild](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-3-jiangshanlai::40gmail.com:1include:uapi:Kbuild)                  |   4 +
 2 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e298a49e9ed7494c1be0fd7a4563fb73d2c26cb2f), 135 insertions(+)
 create mode 100644 arch/x86/include/uapi/asm/pvm_para.h

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-3-jiangshanlai::40gmail.com:1arch:x86:include:uapi:asm:pvm_para.h) --git a/arch/x86/include/uapi/asm/pvm_para.h b/arch/x86/include/uapi/asm/pvm_para.h
new file mode 100644
index 000000000000..36aedfa2cabd
--- /dev/null
+++ b/arch/x86/include/uapi/asm/pvm_para.h @@ -0,0 +1,131 @@ +/* SPDX-License-Identifier: GPL-2.0 WITH Linux-syscall-note */
+#ifndef _UAPI_ASM_X86_PVM_PARA_H
+#define _UAPI_ASM_X86_PVM_PARA_H
+
+#include <linux/const.h>
+
+/*
+ * The CPUID instruction in PVM guest can't be trapped and emulated,
+ * so PVM guest should use the following two instructions instead:
+ * "invlpg 0xffffffffff4d5650; cpuid;"
+ *
+ * PVM_SYNTHETIC_CPUID is supposed to not trigger any trap in the real or
+ * virtual x86 kernel mode and is also guaranteed to trigger a trap in the
+ * underlying hardware user mode for the hypervisor emulating it. The
+ * hypervisor emulates both of the basic instructions, while the INVLPG is
+ * often emulated as an NOP since 0xffffffffff4d5650 is normally out of the
+ * allowed linear address ranges.
+ */
+#define PVM_SYNTHETIC_CPUID		0x0f,0x01,0x3c,0x25,0x50,\
+					0x56,0x4d,0xff,0x0f,0xa2
+#define PVM_SYNTHETIC_CPUID_ADDRESS	0xffffffffff4d5650
+
+/*
+ * The vendor signature 'PVM' is returned in ebx. It should be used to
+ * determine that a VM is running under PVM.
+ */
+#define PVM_CPUID_SIGNATURE		0x4d5650
+
+/*
+ * PVM virtual MSRS falls in the range 0x4b564df0-0x4b564dff, and it should not
+ * conflict with KVM, see arch/x86/include/uapi/asm/kvm_para.h
+ */
+#define PVM_VIRTUAL_MSR_MAX_NR		15
+#define PVM_VIRTUAL_MSR_BASE		0x4b564df0
+#define PVM_VIRTUAL_MSR_MAX		(PVM_VIRTUAL_MSR_BASE+PVM_VIRTUAL_MSR_MAX_NR)
+
+#define MSR_PVM_LINEAR_ADDRESS_RANGE	0x4b564df0
+#define MSR_PVM_VCPU_STRUCT		0x4b564df1
+#define MSR_PVM_SUPERVISOR_RSP		0x4b564df2
+#define MSR_PVM_SUPERVISOR_REDZONE	0x4b564df3
+#define MSR_PVM_EVENT_ENTRY		0x4b564df4
+#define MSR_PVM_RETU_RIP		0x4b564df5
+#define MSR_PVM_RETS_RIP		0x4b564df6
+#define MSR_PVM_SWITCH_CR3		0x4b564df7
+
+#define PVM_HC_SPECIAL_MAX_NR		(256)
+#define PVM_HC_SPECIAL_BASE		(0x17088200)
+#define PVM_HC_SPECIAL_MAX		(PVM_HC_SPECIAL_BASE+PVM_HC_SPECIAL_MAX_NR)
+
+#define PVM_HC_LOAD_PGTBL		(PVM_HC_SPECIAL_BASE+0)
+#define PVM_HC_IRQ_WIN			(PVM_HC_SPECIAL_BASE+1)
+#define PVM_HC_IRQ_HALT			(PVM_HC_SPECIAL_BASE+2)
+#define PVM_HC_TLB_FLUSH		(PVM_HC_SPECIAL_BASE+3)
+#define PVM_HC_TLB_FLUSH_CURRENT	(PVM_HC_SPECIAL_BASE+4)
+#define PVM_HC_TLB_INVLPG		(PVM_HC_SPECIAL_BASE+5)
+#define PVM_HC_LOAD_GS			(PVM_HC_SPECIAL_BASE+6)
+#define PVM_HC_RDMSR			(PVM_HC_SPECIAL_BASE+7)
+#define PVM_HC_WRMSR			(PVM_HC_SPECIAL_BASE+8)
+#define PVM_HC_LOAD_TLS			(PVM_HC_SPECIAL_BASE+9)
+
+/*
+ * PVM_EVENT_FLAGS_IP
+ *	- Interrupt enable flag. The flag is set to respond to maskable
+ *	  external interrupts; and cleared to inhibit maskable external
+ *	  interrupts.
+ *
+ * PVM_EVENT_FLAGS_IF
+ *	- interrupt pending flag. The hypervisor sets it if it fails to inject
+ *	  a maskable event to the VCPU due to the interrupt-enable flag being
+ *	  cleared in supervisor mode.
+ */
+#define PVM_EVENT_FLAGS_IP_BIT		8
+#define PVM_EVENT_FLAGS_IP		_BITUL(PVM_EVENT_FLAGS_IP_BIT)
+#define PVM_EVENT_FLAGS_IF_BIT		9
+#define PVM_EVENT_FLAGS_IF		_BITUL(PVM_EVENT_FLAGS_IF_BIT)
+
+#ifndef __ASSEMBLY__
+
+/*
+ * PVM event delivery saves the information about the event and the old context
+ * into the PVCS structure if the event is from user mode or from supervisor
+ * mode with vector >=32. And ERETU synthetic instruction reads the return
+ * state from the PVCS structure to restore the old context.
+ */
+struct pvm_vcpu_struct {
+	/*
+	 * This flag is only used in supervisor mode, with only bit 8 and
+	 * bit 9 being valid. The other bits are reserved.
+	 */
+	u64 event_flags;
+	u32 event_errcode;
+	u32 event_vector;
+	u64 cr2;
+	u64 reserved0[5];
+
+	/*
+	 * For the event from supervisor mode with vector >=32, only eflags,
+	 * rip, rsp, rcx and r11 are saved, and others keep untouched.
+	 */
+	u16 user_cs, user_ss;
+	u32 reserved1;
+	u64 reserved2;
+	u64 user_gsbase;
+	u32 eflags;
+	u32 pkru;
+	u64 rip;
+	u64 rsp;
+	u64 rcx;
+	u64 r11;
+};
+
+/*
+ * PVM event delivery saves the information about the event and the old context
+ * on the stack with the following frame format if the event is from supervisor
+ * mode with vector <32. And ERETS synthetic instruction reads the return state
+ * with the following frame format from the stack to restore the old context.
+ */
+struct pvm_supervisor_event {
+	unsigned long errcode; // vector in high32
+	unsigned long rip;
+	unsigned long cs;
+	unsigned long rflags;
+	unsigned long rsp;
+	unsigned long ss;
+	unsigned long rcx;
+	unsigned long r11;
+};
+
+#endif /* __ASSEMBLY__ */
+
+#endif /* _UAPI_ASM_X86_PVM_PARA_H */ [diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-3-jiangshanlai::40gmail.com:1include:uapi:Kbuild) --git a/include/uapi/Kbuild b/include/uapi/Kbuild
index 61ee6e59c930..991848db246b 100644
--- a/include/uapi/Kbuild
+++ b/include/uapi/Kbuild @@ -12,3 +12,7 @@ ifeq ($(wildcard $(objtree)/arch/$(SRCARCH)/include/generated/uapi/asm/kvm_para.
 no-export-headers += linux/kvm_para.h
 endif
 endif
+
+ifeq ($(wildcard $(srctree)/arch/$(SRCARCH)/include/uapi/asm/pvm_para.h),)
+no-export-headers += pvm_para.h
+endif --
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m298a49e9ed7494c1be0fd7a4563fb73d2c26cb2f) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-3-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-3-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-3-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-3-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-3-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-3-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r298a49e9ed7494c1be0fd7a4563fb73d2c26cb2f)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e9f2524581c30e4d18403640ceddcb5f0aaccd684) **[RFC PATCH 03/73] x86/entry: Implement switcher for PVM VM enter/exit**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
  2024-02-26 14:35 ` [[RFC PATCH 01/73] KVM: Documentation: Add the specification for PVM](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mdb114605fe7991cfe69b8719c9ca244d6e37e4b1) Lai Jiangshan
  2024-02-26 14:35 ` [[RFC PATCH 02/73] x86/ABI/PVM: Add PVM-specific ABI header file](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m298a49e9ed7494c1be0fd7a4563fb73d2c26cb2f) Lai Jiangshan
**@ 2024-02-26 14:35 ` Lai Jiangshan**
  2024-02-26 14:35 ` [[RFC PATCH 04/73] x86/entry: Implement direct switching for the switcher](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m3b243b78a7b5a0502322405fa52e63002c19c978) Lai Jiangshan
                   ` [(71 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r3b243b78a7b5a0502322405fa52e63002c19c978)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r9f2524581c30e4d18403640ceddcb5f0aaccd684)
From: Lai Jiangshan @ 2024-02-26 14:35 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-4-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-4-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143440)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143440), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Andy Lutomirski, Dave Hansen, H. Peter Anvin, Oleg Nesterov,
	Mike Rapoport (IBM), Rick Edgecombe, Arnd Bergmann, Brian Gerst,
	Mateusz Guzik, Kirill A. Shutemov, Jacob Pan

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

Since the PVM guest runs in hardware CPL3, the host/guest world
switching is similar to userspace/kernelspace switching. Therefore, PVM
has decided to reuse the host entries for host/guest world switching. In
order to differentiate PVM guests from normal userspace processes, a new
flag is introduced to mark that the guest is active. The host entries
are then modified to use this flag for handling forwarding. The modified
host entries and VM enter path are collectively called the "switcher."

In the host entries, if from CPL3 and the flag is set, then it is
regarded as VM exit and the handling will be forwarded to the
hypervisor.  Otherwise, the handling belongs to the host like before. If
from CPL0, the handling belongs to the host too. Paranoid entries should
save and restore the guest CR3, similar to the save and restore
procedure for user CR3 in KPTI.

So the switcher is not compatiable with KPTI currently.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/entry/Makefile](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-4-jiangshanlai::40gmail.com:1arch:x86:entry:Makefile)            |   3 +
 [arch/x86/entry/calling.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-4-jiangshanlai::40gmail.com:1arch:x86:entry:calling.h)           |  47 ++++++++++-
 [arch/x86/entry/entry_64.S](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-4-jiangshanlai::40gmail.com:1arch:x86:entry:entry_64.S)          |  68 ++++++++++++++-
 [arch/x86/entry/entry_64_switcher.S](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-4-jiangshanlai::40gmail.com:1arch:x86:entry:entry_64_switcher.S) | 127 +++++++++++++++++++++++++++++
 [arch/x86/include/asm/processor.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-4-jiangshanlai::40gmail.com:1arch:x86:include:asm:processor.h)   |   5 ++
 [arch/x86/include/asm/ptrace.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-4-jiangshanlai::40gmail.com:1arch:x86:include:asm:ptrace.h)      |   3 +
 [arch/x86/include/asm/switcher.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-4-jiangshanlai::40gmail.com:1arch:x86:include:asm:switcher.h)    |  59 ++++++++++++++
 [arch/x86/kernel/asm-offsets_64.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-4-jiangshanlai::40gmail.com:1arch:x86:kernel:asm-offsets_64.c)   |   8 ++
 [arch/x86/kernel/traps.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-4-jiangshanlai::40gmail.com:1arch:x86:kernel:traps.c)            |   3 +
 9 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e9f2524581c30e4d18403640ceddcb5f0aaccd684), 315 insertions(+), 8 deletions(-)
 create mode 100644 arch/x86/entry/entry_64_switcher.S
 create mode 100644 arch/x86/include/asm/switcher.h

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-4-jiangshanlai::40gmail.com:1arch:x86:entry:Makefile) --git a/arch/x86/entry/Makefile b/arch/x86/entry/Makefile
index ca2fe186994b..55dd3f193d99 100644
--- a/arch/x86/entry/Makefile
+++ b/arch/x86/entry/Makefile @@ -21,3 +21,6 @@ obj-$(CONFIG_PREEMPTION)	+= thunk_$(BITS).o
 obj-$(CONFIG_IA32_EMULATION)	+= entry_64_compat.o syscall_32.o
 obj-$(CONFIG_X86_X32_ABI)	+= syscall_x32.o

+ifeq ($(CONFIG_X86_64),y)
+	obj-y += 		entry_64_switcher.o
+endif [diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-4-jiangshanlai::40gmail.com:1arch:x86:entry:calling.h) --git a/arch/x86/entry/calling.h b/arch/x86/entry/calling.h
index c99f36339236..83758019162d 100644
--- a/arch/x86/entry/calling.h
+++ b/arch/x86/entry/calling.h @@ -142,6 +142,10 @@ For 32-bit we have the following conventions - kernel is built with
 	.endif
 .endm

+.macro SET_NOFLUSH_BIT	reg:req
+	bts	$X86_CR3_PCID_NOFLUSH_BIT, \reg
+.endm
+
 #ifdef CONFIG_PAGE_TABLE_ISOLATION

 /*
@@ -154,10 +158,6 @@ For 32-bit we have the following conventions - kernel is built with
 #define PTI_USER_PCID_MASK		(1 << PTI_USER_PCID_BIT)
 #define PTI_USER_PGTABLE_AND_PCID_MASK  (PTI_USER_PCID_MASK | PTI_USER_PGTABLE_MASK)

-.macro SET_NOFLUSH_BIT	reg:req
-	bts	$X86_CR3_PCID_NOFLUSH_BIT, \reg
-.endm
 .macro ADJUST_KERNEL_CR3 reg:req
 	ALTERNATIVE "", "SET_NOFLUSH_BIT \reg", X86_FEATURE_PCID
 	/* Clear PCID and "PAGE_TABLE_ISOLATION bit", point CR3 at kernel pagetables: */
@@ -284,6 +284,45 @@ For 32-bit we have the following conventions - kernel is built with

 #endif

+#define TSS_extra(field) PER_CPU_VAR(cpu_tss_rw+TSS_EX_##field)
+
+/*
+ * Switcher would be disabled when KPTI is enabled.
+ *
+ * Ideally, switcher would switch to HOST_CR3 in IST before gsbase is fixed,
+ * in which case it would use the offset from the IST stack top to the TSS
+ * in CEA to get the pointer of the TSS.  But SEV guest modifies TSS.IST on
+ * the fly and makes the code non-workable in SEV guest even the switcher
+ * is not used.
+ *
+ * So switcher is marked disabled when KPTI is enabled rather than when
+ * in SEV guest.
+ *
+ * To enable switcher with KPTI, something like Integrated Entry code with
+ * atomic-IST-entry has to be introduced beforehand.
+ *
+ * The current SWITCHER_SAVE_AND_SWITCH_TO_HOST_CR3 is called after gsbase
+ * is fixed.
+ */
+.macro SWITCHER_SAVE_AND_SWITCH_TO_HOST_CR3 scratch_reg:req save_reg:req
+	ALTERNATIVE "", "jmp .Lend_\@", X86_FEATURE_PTI
+	cmpq	$0, TSS_extra(host_rsp)
+	jz	.Lend_\@
+	movq	%cr3, \save_reg
+	movq	TSS_extra(host_cr3), \scratch_reg
+	movq	\scratch_reg, %cr3
+.Lend_\@:
+.endm
+
+.macro SWITCHER_RESTORE_CR3 scratch_reg:req save_reg:req
+	ALTERNATIVE "", "jmp .Lend_\@", X86_FEATURE_PTI
+	cmpq	$0, TSS_extra(host_rsp)
+	jz	.Lend_\@
+	ALTERNATIVE "", "SET_NOFLUSH_BIT \save_reg", X86_FEATURE_PCID
+	movq	\save_reg, %cr3
+.Lend_\@:
+.endm
+
 /*
  * IBRS kernel mitigation for Spectre_v2.
  *
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-4-jiangshanlai::40gmail.com:1arch:x86:entry:entry_64.S) --git a/arch/x86/entry/entry_64.S b/arch/x86/entry/entry_64.S
index 57fae15b3136..65bfebebeab6 100644
--- a/arch/x86/entry/entry_64.S
+++ b/arch/x86/entry/entry_64.S @@ -278,10 +278,11 @@ SYM_CODE_END(xen_error_entry)

 /**
  * idtentry_body - Macro to emit code calling the C function
+ * @vector:		Vector number
  * @cfunc:		C function to be called
  * @has_error_code:	Hardware pushed error code on stack
  */
-.macro idtentry_body cfunc has_error_code:req +.macro idtentry_body vector cfunc has_error_code:req

 	/*
 	 * Call error_entry() and switch to the task stack if from userspace.
@@ -297,6 +298,10 @@ SYM_CODE_END(xen_error_entry)
 	ENCODE_FRAME_POINTER
 	UNWIND_HINT_REGS

+	cmpq	$0, TSS_extra(host_rsp)
+	jne	.Lpvm_idtentry_body_\@
+.L_host_idtenrty_\@:
+
 	movq	%rsp, %rdi			/* pt_regs pointer into 1st argument*/

 	.if \has_error_code == 1
@@ -310,6 +315,25 @@ SYM_CODE_END(xen_error_entry)
 	REACHABLE

 	jmp	error_return
+
+.Lpvm_idtentry_body_\@:
+	testb	$3, CS(%rsp)
+	/* host exception nested in IST handler while the switcher is active */
+	jz	.L_host_idtenrty_\@
+
+	.if \vector < 256
+	movl	$\vector, ORIG_RAX+4(%rsp)
+	.else // X86_TRAP_OTHER
+	/*
+	 * Here are the macros for common_interrupt(), spurious_interrupt(),
+	 * and XENPV entries with the titular vector X86_TRAP_OTHER. XENPV
+	 * entries can't reach here while common_interrupt() and
+	 * spurious_interrupt() have the real vector at ORIG_RAX.
+	 */
+	movl	ORIG_RAX(%rsp), %eax
+	movl	%eax, ORIG_RAX+4(%rsp)
+	.endif
+	jmp	switcher_return_from_guest
 .endm

 /**
@@ -354,7 +378,7 @@ SYM_CODE_START(\asmsym)
 .Lfrom_usermode_no_gap_\@:
 	.endif

-	idtentry_body \cfunc \has_error_code +	idtentry_body \vector \cfunc \has_error_code

 _ASM_NOKPROBE(\asmsym)
 SYM_CODE_END(\asmsym)
@@ -427,7 +451,7 @@ SYM_CODE_START(\asmsym)

 	/* Switch to the regular task stack and use the noist entry point */
 .Lfrom_usermode_switch_stack_\@:
-	idtentry_body noist_\cfunc, has_error_code=0 +	idtentry_body \vector, noist_\cfunc, has_error_code=0

 _ASM_NOKPROBE(\asmsym)
 SYM_CODE_END(\asmsym)
@@ -507,7 +531,7 @@ SYM_CODE_START(\asmsym)

 	/* Switch to the regular task stack */
 .Lfrom_usermode_switch_stack_\@:
-	idtentry_body user_\cfunc, has_error_code=1 +	idtentry_body \vector, user_\cfunc, has_error_code=1

 _ASM_NOKPROBE(\asmsym)
 SYM_CODE_END(\asmsym)
@@ -919,6 +943,16 @@ SYM_CODE_START(paranoid_entry)
 	FENCE_SWAPGS_KERNEL_ENTRY
 .Lparanoid_gsbase_done:

+	/*
+	 * Switch back to kernel cr3 when switcher is active.
+	 * Switcher can't be used when KPTI is enabled by far, so only one of
+	 * SAVE_AND_SWITCH_TO_KERNEL_CR3 and SWITCHER_SAVE_AND_SWITCH_TO_KERNEL_CR3
+	 * takes effect.  SWITCHER_SAVE_AND_SWITCH_TO_KERNEL_CR3 requires
+	 * kernel GSBASE.
+	 * See the comments above SWITCHER_SAVE_AND_SWITCH_TO_HOST_CR3.
+	 */
+	SWITCHER_SAVE_AND_SWITCH_TO_HOST_CR3 scratch_reg=%rax save_reg=%r14
+
 	/*
 	 * Once we have CR3 and %GS setup save and set SPEC_CTRL. Just like
 	 * CR3 above, keep the old value in a callee saved register.
@@ -970,6 +1004,15 @@ SYM_CODE_START_LOCAL(paranoid_exit)
 	 */
 	RESTORE_CR3	scratch_reg=%rax save_reg=%r14

+	/*
+	 * Switch back to origin cr3 when switcher is active.
+	 * Switcher can't be used when KPTI is enabled by far, so only
+	 * one of RESTORE_CR3 and SWITCHER_RESTORE_CR3 takes effect.
+	 *
+	 * See the comments above SWITCHER_SAVE_AND_SWITCH_TO_HOST_CR3.
+	 */
+	SWITCHER_RESTORE_CR3 scratch_reg=%rax save_reg=%r14
+
 	/* Handle the three GSBASE cases */
 	ALTERNATIVE "jmp .Lparanoid_exit_checkgs", "", X86_FEATURE_FSGSBASE

@@ -1158,6 +1201,8 @@ SYM_CODE_START(asm_exc_nmi)
 	FENCE_SWAPGS_USER_ENTRY
 	SWITCH_TO_KERNEL_CR3 scratch_reg=%rdx
 	movq	%rsp, %rdx
+	cmpq	$0, TSS_extra(host_rsp)
+	jne	.Lnmi_from_pvm_guest
 	movq	PER_CPU_VAR(pcpu_hot + X86_top_of_stack), %rsp
 	UNWIND_HINT_IRET_REGS base=%rdx offset=8
 	pushq	5*8(%rdx)	/* pt_regs->ss */
@@ -1188,6 +1233,21 @@ SYM_CODE_START(asm_exc_nmi)
 	 */
 	jmp	swapgs_restore_regs_and_return_to_usermode

+.Lnmi_from_pvm_guest:
+	movq	PER_CPU_VAR(cpu_tss_rw + TSS_sp0), %rsp
+	UNWIND_HINT_IRET_REGS base=%rdx offset=8
+	pushq	5*8(%rdx)	/* pt_regs->ss */
+	pushq	4*8(%rdx)	/* pt_regs->rsp */
+	pushq	3*8(%rdx)	/* pt_regs->flags */
+	pushq	2*8(%rdx)	/* pt_regs->cs */
+	pushq	1*8(%rdx)	/* pt_regs->rip */
+	UNWIND_HINT_IRET_REGS
+	pushq	$0		/* pt_regs->orig_ax */
+	movl	$2, 4(%rsp)	/* pt_regs->orig_ax, pvm vector */
+	PUSH_AND_CLEAR_REGS rdx=(%rdx)
+	ENCODE_FRAME_POINTER
+	jmp	switcher_return_from_guest
+
 .Lnmi_from_kernel:
 	/*
 	 * Here's what our stack frame will look like:
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-4-jiangshanlai::40gmail.com:1arch:x86:entry:entry_64_switcher.S) --git a/arch/x86/entry/entry_64_switcher.S b/arch/x86/entry/entry_64_switcher.S
new file mode 100644
index 000000000000..2b99a46421cc
--- /dev/null
+++ b/arch/x86/entry/entry_64_switcher.S @@ -0,0 +1,127 @@ +/* SPDX-License-Identifier: GPL-2.0 */
+#include <linux/linkage.h>
+#include <linux/export.h>
+#include <asm/segment.h>
+#include <asm/asm-offsets.h>
+#include <asm/msr.h>
+#include <asm/percpu.h>
+#include <asm/asm.h>
+#include <asm/nospec-branch.h>
+#include <asm/switcher.h>
+
+#include "calling.h"
+
+.code64
+.section .entry.text, "ax"
+
+.macro MITIGATION_EXIT
+	/* Same as user entry. */
+	IBRS_EXIT
+.endm
+
+.macro MITIGATION_ENTER
+	/*
+	 * IMPORTANT: RSB filling and SPEC_CTRL handling must be done before
+	 * the first unbalanced RET after vmexit!
+	 *
+	 * For retpoline or IBRS, RSB filling is needed to prevent poisoned RSB
+	 * entries and (in some cases) RSB underflow.
+	 *
+	 * eIBRS has its own protection against poisoned RSB, so it doesn't
+	 * need the RSB filling sequence.  But it does need to be enabled, and a
+	 * single call to retire, before the first unbalanced RET.
+	 */
+	FILL_RETURN_BUFFER %rcx, RSB_CLEAR_LOOPS, X86_FEATURE_RSB_VMEXIT,\
+			   X86_FEATURE_RSB_VMEXIT_LITE
+
+	IBRS_ENTER
+.endm
+
+/*
+ * switcher_enter_guest - Do a transition to guest mode
+ *
+ * Called with guest registers on the top of the sp0 stack and the switcher
+ * states on cpu_tss_rw.tss_ex.
+ *
+ * Returns:
+ *	pointer to pt_regs (on top of sp0 or IST stack) with guest registers.
+ */
+SYM_FUNC_START(switcher_enter_guest)
+	pushq	%rbp
+	pushq	%r15
+	pushq	%r14
+	pushq	%r13
+	pushq	%r12
+	pushq	%rbx
+
+	/* Save host RSP and mark the switcher active */
+	movq	%rsp, TSS_extra(host_rsp)
+
+	/* Switch to host sp0  */
+	movq	PER_CPU_VAR(cpu_tss_rw + TSS_sp0), %rdi
+	subq	$FRAME_SIZE, %rdi
+	movq	%rdi, %rsp
+
+	UNWIND_HINT_REGS
+
+	MITIGATION_EXIT
+
+	/* switch to guest cr3 on sp0 stack */
+	movq	TSS_extra(enter_cr3), %rax
+	movq	%rax, %cr3
+	/* Load guest registers. */
+	POP_REGS
+	addq	$8, %rsp
+
+	/* Switch to guest GSBASE and return to guest */
+	swapgs
+	jmp	native_irq_return_iret
+
+SYM_INNER_LABEL(switcher_return_from_guest, SYM_L_GLOBAL)
+	/* switch back to host cr3 when still on sp0/ist stack */
+	movq	TSS_extra(host_cr3), %rax
+	movq	%rax, %cr3
+
+	MITIGATION_ENTER
+
+	/* Restore to host RSP and mark the switcher inactive */
+	movq	%rsp, %rax
+	movq	TSS_extra(host_rsp), %rsp
+	movq	$0, TSS_extra(host_rsp)
+
+	popq	%rbx
+	popq	%r12
+	popq	%r13
+	popq	%r14
+	popq	%r15
+	popq	%rbp
+	RET
+SYM_FUNC_END(switcher_enter_guest)
+EXPORT_SYMBOL_GPL(switcher_enter_guest)
+
+SYM_CODE_START(entry_SYSCALL_64_switcher)
+	UNWIND_HINT_ENTRY
+	ENDBR
+
+	swapgs
+	/* tss.sp2 is scratch space. */
+	movq	%rsp, PER_CPU_VAR(cpu_tss_rw + TSS_sp2)
+	movq	PER_CPU_VAR(cpu_tss_rw + TSS_sp0), %rsp
+
+SYM_INNER_LABEL(entry_SYSCALL_64_switcher_safe_stack, SYM_L_GLOBAL)
+	ANNOTATE_NOENDBR
+
+	/* Construct struct pt_regs on stack */
+	pushq	$__USER_DS				/* pt_regs->ss */
+	pushq	PER_CPU_VAR(cpu_tss_rw + TSS_sp2)	/* pt_regs->sp */
+	pushq	%r11					/* pt_regs->flags */
+	pushq	$__USER_CS				/* pt_regs->cs */
+	pushq	%rcx					/* pt_regs->ip */
+
+	pushq	$0					/* pt_regs->orig_ax */
+	movl	$SWITCH_EXIT_REASONS_SYSCALL, 4(%rsp)
+
+	PUSH_AND_CLEAR_REGS
+	jmp	switcher_return_from_guest
+SYM_CODE_END(entry_SYSCALL_64_switcher)
+EXPORT_SYMBOL_GPL(entry_SYSCALL_64_switcher) [diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-4-jiangshanlai::40gmail.com:1arch:x86:include:asm:processor.h) --git a/arch/x86/include/asm/processor.h b/arch/x86/include/asm/processor.h
index 83dc4122c38d..4115267e7a3e 100644
--- a/arch/x86/include/asm/processor.h
+++ b/arch/x86/include/asm/processor.h @@ -29,6 +29,7 @@ struct vm86;
 #include <asm/vmxfeatures.h>
 #include <asm/vdso/processor.h>
 #include <asm/shstk.h>
+#include <asm/switcher.h>

 #include <linux/personality.h>
 #include <linux/cache.h>
@@ -382,6 +383,10 @@ struct tss_struct {
 	 */
 	struct x86_hw_tss	x86_tss;

+#ifdef CONFIG_X86_64
+	struct tss_extra	tss_ex;
+#endif
+
 	struct x86_io_bitmap	io_bitmap;
 } __aligned(PAGE_SIZE);

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-4-jiangshanlai::40gmail.com:1arch:x86:include:asm:ptrace.h) --git a/arch/x86/include/asm/ptrace.h b/arch/x86/include/asm/ptrace.h
index f4db78b09c8f..9eeeb5fdd387 100644
--- a/arch/x86/include/asm/ptrace.h
+++ b/arch/x86/include/asm/ptrace.h @@ -5,6 +5,7 @@
 #include <asm/segment.h>
 #include <asm/page_types.h>
 #include <uapi/asm/ptrace.h>
+#include <asm/switcher.h>

 #ifndef __ASSEMBLY__
 #ifdef __i386__
@@ -194,6 +195,8 @@ static __always_inline bool ip_within_syscall_gap(struct pt_regs *regs)
 	ret = ret || (regs->ip >= (unsigned long)entry_SYSRETL_compat_unsafe_stack &&
 		      regs->ip <  (unsigned long)entry_SYSRETL_compat_end);
 #endif
+	ret = ret || (regs->ip >= (unsigned long)entry_SYSCALL_64_switcher &&
+		      regs->ip <  (unsigned long)entry_SYSCALL_64_switcher_safe_stack);

 	return ret;
 }
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-4-jiangshanlai::40gmail.com:1arch:x86:include:asm:switcher.h) --git a/arch/x86/include/asm/switcher.h b/arch/x86/include/asm/switcher.h
new file mode 100644
index 000000000000..dbf1970ca62f
--- /dev/null
+++ b/arch/x86/include/asm/switcher.h @@ -0,0 +1,59 @@ +/* SPDX-License-Identifier: GPL-2.0 */
+#ifndef _ASM_X86_SWITCHER_H
+#define _ASM_X86_SWITCHER_H
+
+#ifdef CONFIG_X86_64
+#include <asm/processor-flags.h>
+
+#define SWITCH_EXIT_REASONS_SYSCALL		1024
+#define SWITCH_EXIT_REASONS_FAILED_VMETNRY	1025
+
+/* Bits allowed to be set in the underlying eflags */
+#define SWITCH_ENTER_EFLAGS_ALLOWED	(X86_EFLAGS_FIXED | X86_EFLAGS_IF |\
+					 X86_EFLAGS_TF | X86_EFLAGS_RF |\
+					 X86_EFLAGS_AC | X86_EFLAGS_OF |\
+					 X86_EFLAGS_DF | X86_EFLAGS_SF |\
+					 X86_EFLAGS_ZF | X86_EFLAGS_AF |\
+					 X86_EFLAGS_PF | X86_EFLAGS_CF |\
+					 X86_EFLAGS_ID | X86_EFLAGS_NT)
+
+/* Bits must be set in the underlying eflags */
+#define SWITCH_ENTER_EFLAGS_FIXED	(X86_EFLAGS_FIXED | X86_EFLAGS_IF)
+
+#ifndef __ASSEMBLY__
+#include <linux/cache.h>
+
+struct pt_regs;
+
+/*
+ * Extra per CPU control structure lives in the struct tss_struct.
+ *
+ * The page-size-aligned struct tss_struct has enough room to accommodate
+ * this extra data without increasing its size.
+ *
+ * The extra data is also in the first page of struct tss_struct whose
+ * read-write mapping (percpu cpu_tss_rw) is in the KPTI's user pagetable,
+ * so that it can even be accessible via cpu_tss_rw in the entry code.
+ */
+struct tss_extra {
+	/* Saved host CR3 to be loaded after VM exit. */
+	unsigned long host_cr3;
+	/*
+	 * Saved host stack to be loaded after VM exit. This also serves as a
+	 * flag to indicate that it is entering the guest world in the switcher
+	 * or has been in the guest world in the host entries.
+	 */
+	unsigned long host_rsp;
+	/* Prepared guest CR3 to be loaded before VM enter. */
+	unsigned long enter_cr3;
+} ____cacheline_aligned;
+
+extern struct pt_regs *switcher_enter_guest(void);
+extern const char entry_SYSCALL_64_switcher[];
+extern const char entry_SYSCALL_64_switcher_safe_stack[];
+extern const char entry_SYSRETQ_switcher_unsafe_stack[];
+#endif /* __ASSEMBLY__ */
+
+#endif /* CONFIG_X86_64 */
+
+#endif /* _ASM_X86_SWITCHER_H */ [diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-4-jiangshanlai::40gmail.com:1arch:x86:kernel:asm-offsets_64.c) --git a/arch/x86/kernel/asm-offsets_64.c b/arch/x86/kernel/asm-offsets_64.c
index f39baf90126c..1485cbda6dc4 100644
--- a/arch/x86/kernel/asm-offsets_64.c
+++ b/arch/x86/kernel/asm-offsets_64.c @@ -60,5 +60,13 @@ int main(void)
 	OFFSET(FIXED_stack_canary, fixed_percpu_data, stack_canary);
 	BLANK();
 #endif
+
+#define ENTRY(entry) OFFSET(TSS_EX_ ## entry, tss_struct, tss_ex.entry)
+	ENTRY(host_cr3);
+	ENTRY(host_rsp);
+	ENTRY(enter_cr3);
+	BLANK();
+#undef ENTRY
+
 	return 0;
 }
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-4-jiangshanlai::40gmail.com:1arch:x86:kernel:traps.c) --git a/arch/x86/kernel/traps.c b/arch/x86/kernel/traps.c
index c876f1d36a81..c4f2b629b422 100644
--- a/arch/x86/kernel/traps.c
+++ b/arch/x86/kernel/traps.c @@ -773,6 +773,9 @@ DEFINE_IDTENTRY_RAW(exc_int3)
 asmlinkage __visible noinstr struct pt_regs *sync_regs(struct pt_regs *eregs)
 {
 	struct pt_regs *regs = (struct pt_regs *)this_cpu_read(pcpu_hot.top_of_stack) - 1;
+
+	if (this_cpu_read(cpu_tss_rw.tss_ex.host_rsp))
+		return eregs;
 	if (regs != eregs)
 		*regs = *eregs;
 	return regs;
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m9f2524581c30e4d18403640ceddcb5f0aaccd684) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-4-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-4-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-4-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-4-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-4-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-4-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r9f2524581c30e4d18403640ceddcb5f0aaccd684)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e3b243b78a7b5a0502322405fa52e63002c19c978) **[RFC PATCH 04/73] x86/entry: Implement direct switching for the switcher**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(2 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r9f2524581c30e4d18403640ceddcb5f0aaccd684)
  2024-02-26 14:35 ` [[RFC PATCH 03/73] x86/entry: Implement switcher for PVM VM enter/exit](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m9f2524581c30e4d18403640ceddcb5f0aaccd684) Lai Jiangshan
**@ 2024-02-26 14:35 ` Lai Jiangshan**
  2024-02-26 14:35 ` [[RFC PATCH 05/73] KVM: x86: Set 'vcpu->arch.exception.injected' as true before vendor callback](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ma4640c2ddc5dbd24e9892d48e636c76b70be6ee7) Lai Jiangshan
                   ` [(70 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ra4640c2ddc5dbd24e9892d48e636c76b70be6ee7)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r3b243b78a7b5a0502322405fa52e63002c19c978)
From: Lai Jiangshan @ 2024-02-26 14:35 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-5-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-5-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143447)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143447), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Andy Lutomirski, Dave Hansen, H. Peter Anvin, Oleg Nesterov,
	Brian Gerst

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

During VM running, all VM exits in the switcher will be forwarded to the
hypervisor and then returned to the switcher to re-enter the VM after
handling the VM exit. In some situations, the switcher can handle the VM
exit directly without involving the hypervisor. This is referred to as
direct switching, and it can reduce the overhead of guest/host state
switching. Currently, for simplicity, only the syscall event from user
mode and ERETU synthetic instruction are allowed for direct switching.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/entry/entry_64_switcher.S](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-5-jiangshanlai::40gmail.com:1arch:x86:entry:entry_64_switcher.S) | 145 ++++++++++++++++++++++++++++-
 [arch/x86/include/asm/ptrace.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-5-jiangshanlai::40gmail.com:1arch:x86:include:asm:ptrace.h)      |   2 +
 [arch/x86/include/asm/switcher.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-5-jiangshanlai::40gmail.com:1arch:x86:include:asm:switcher.h)    |  60 ++++++++++++
 [arch/x86/kernel/asm-offsets_64.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-5-jiangshanlai::40gmail.com:1arch:x86:kernel:asm-offsets_64.c)   |  23 +++++
 4 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e3b243b78a7b5a0502322405fa52e63002c19c978), 229 insertions(+), 1 deletion(-)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-5-jiangshanlai::40gmail.com:1arch:x86:entry:entry_64_switcher.S) --git a/arch/x86/entry/entry_64_switcher.S b/arch/x86/entry/entry_64_switcher.S
index 2b99a46421cc..6f166d15635c 100644
--- a/arch/x86/entry/entry_64_switcher.S
+++ b/arch/x86/entry/entry_64_switcher.S @@ -75,7 +75,7 @@ SYM_FUNC_START(switcher_enter_guest)

 	/* Switch to guest GSBASE and return to guest */
 	swapgs
-	jmp	native_irq_return_iret +	jmp	.L_switcher_return_to_guest

 SYM_INNER_LABEL(switcher_return_from_guest, SYM_L_GLOBAL)
 	/* switch back to host cr3 when still on sp0/ist stack */
@@ -99,6 +99,23 @@ SYM_INNER_LABEL(switcher_return_from_guest, SYM_L_GLOBAL)
 SYM_FUNC_END(switcher_enter_guest)
 EXPORT_SYMBOL_GPL(switcher_enter_guest)

+.macro canonical_rcx
+	/*
+	 * If width of "canonical tail" ever becomes variable, this will need
+	 * to be updated to remain correct on both old and new CPUs.
+	 *
+	 * Change top bits to match most significant bit (47th or 56th bit
+	 * depending on paging mode) in the address.
+	 */
+#ifdef CONFIG_X86_5LEVEL
+	ALTERNATIVE "shl $(64 - 48), %rcx; sar $(64 - 48), %rcx",\
+		    "shl $(64 - 57), %rcx; sar $(64 - 57), %rcx", X86_FEATURE_LA57
+#else
+	shl	$(64 - (__VIRTUAL_MASK_SHIFT+1)), %rcx
+	sar	$(64 - (__VIRTUAL_MASK_SHIFT+1)), %rcx
+#endif
+.endm
+
 SYM_CODE_START(entry_SYSCALL_64_switcher)
 	UNWIND_HINT_ENTRY
 	ENDBR
@@ -117,7 +134,133 @@ SYM_INNER_LABEL(entry_SYSCALL_64_switcher_safe_stack, SYM_L_GLOBAL)
 	pushq	%r11					/* pt_regs->flags */
 	pushq	$__USER_CS				/* pt_regs->cs */
 	pushq	%rcx					/* pt_regs->ip */
+	pushq	%rdi					/* put rdi on ORIG_RAX */
+
+	/* check if it can do direct switch from umod to smod */
+	testq	$SWITCH_FLAGS_NO_DS_TO_SMOD, TSS_extra(switch_flags)
+	jnz	.L_switcher_check_return_umod_instruction
+
+	/* Now it must be umod, start to do direct switch from umod to smod */
+	movq	TSS_extra(pvcs), %rdi
+	movl	%r11d, PVCS_eflags(%rdi)
+	movq	%rcx, PVCS_rip(%rdi)
+	movq	%rcx, PVCS_rcx(%rdi)
+	movq	%r11, PVCS_r11(%rdi)
+	movq	RSP-ORIG_RAX(%rsp), %rcx
+	movq	%rcx, PVCS_rsp(%rdi)
+
+	/* switch umod to smod (switch_flags & cr3) */
+	xorb	$SWITCH_FLAGS_MOD_TOGGLE, TSS_extra(switch_flags)
+	movq	TSS_extra(smod_cr3), %rcx
+	movq	%rcx, %cr3
+
+	/* load smod registers from TSS_extra to sp0 stack or %r11 */
+	movq	TSS_extra(smod_rsp), %rcx
+	movq	%rcx, RSP-ORIG_RAX(%rsp)
+	movq	TSS_extra(smod_entry), %rcx
+	movq	%rcx, RIP-ORIG_RAX(%rsp)
+	movq	TSS_extra(smod_gsbase), %r11
+
+	/* switch host gsbase to guest gsbase, TSS_extra can't be use afterward */
+	swapgs
+
+	/* save guest gsbase as user_gsbase and switch to smod_gsbase */
+	rdgsbase %rcx
+	movq	%rcx, PVCS_user_gsbase(%rdi)
+	wrgsbase %r11
+
+	/* restore umod rdi and smod rflags/r11, rip/rcx and rsp for sysretq */
+	popq	%rdi
+	movq	$SWITCH_ENTER_EFLAGS_FIXED, %r11
+	movq	RIP-RIP(%rsp), %rcx
+
+.L_switcher_sysretq:
+	UNWIND_HINT_IRET_REGS
+	/* now everything is ready for sysretq except for %rsp */
+	movq	RSP-RIP(%rsp), %rsp
+	/* No instruction can be added between seting the guest %rsp and doing sysretq */
+SYM_INNER_LABEL(entry_SYSRETQ_switcher_unsafe_stack, SYM_L_GLOBAL)
+	sysretq
+
+.L_switcher_check_return_umod_instruction:
+	UNWIND_HINT_IRET_REGS offset=8
+
+	/* check if it can do direct switch from smod to umod */
+	testq	$SWITCH_FLAGS_NO_DS_TO_UMOD, TSS_extra(switch_flags)
+	jnz	.L_switcher_return_to_hypervisor
+
+	/*
+	 * Now it must be smod, check if it is the return-umod instruction.
+	 * Switcher and the PVM specification defines a SYSCALL instrucion
+	 * at TSS_extra(retu_rip) - 2 in smod as the return-umod instruction.
+	 */
+	cmpq	%rcx, TSS_extra(retu_rip)
+	jne	.L_switcher_return_to_hypervisor
+
+	/* only handle for the most common cs/ss */
+	movq	TSS_extra(pvcs), %rdi
+	cmpl	$((__USER_DS << 16) | __USER_CS), PVCS_user_cs(%rdi)
+	jne	.L_switcher_return_to_hypervisor
+
+	/* Switcher and the PVM specification requires the smod RSP to be saved */
+	movq	RSP-ORIG_RAX(%rsp), %rcx
+	movq	%rcx, TSS_extra(smod_rsp)
+
+	/* switch smod to umod (switch_flags & cr3) */
+	xorb	$SWITCH_FLAGS_MOD_TOGGLE, TSS_extra(switch_flags)
+	movq	TSS_extra(umod_cr3), %rcx
+	movq	%rcx, %cr3
+
+	/* switch host gsbase to guest gsbase, TSS_extra can't be use afterward */
+	swapgs
+
+	/* write umod gsbase */
+	movq	PVCS_user_gsbase(%rdi), %rcx
+	canonical_rcx
+	wrgsbase %rcx
+
+	/* load sp, flags, ip to sp0 stack and cx, r11, rdi to registers */
+	movq	PVCS_rsp(%rdi), %rcx
+	movq	%rcx, RSP-ORIG_RAX(%rsp)
+	movl	PVCS_eflags(%rdi), %r11d
+	movq	%r11, EFLAGS-ORIG_RAX(%rsp)
+	movq	PVCS_rip(%rdi), %rcx
+	movq	%rcx, RIP-ORIG_RAX(%rsp)
+	movq	PVCS_rcx(%rdi), %rcx
+	movq	PVCS_r11(%rdi), %r11
+	popq	%rdi		// saved rdi (on ORIG_RAX)
+
+.L_switcher_return_to_guest:
+	/*
+	 * Now the RSP points to an IRET frame with guest state on the
+	 * top of the sp0 stack.  Check if it can do sysretq.
+	 */
+	UNWIND_HINT_IRET_REGS
+
+	andq	$SWITCH_ENTER_EFLAGS_ALLOWED, EFLAGS-RIP(%rsp)
+	orq	$SWITCH_ENTER_EFLAGS_FIXED, EFLAGS-RIP(%rsp)
+	testq	$(X86_EFLAGS_RF|X86_EFLAGS_TF), EFLAGS-RIP(%rsp)
+	jnz	native_irq_return_iret
+	cmpq	%r11, EFLAGS-RIP(%rsp)
+	jne	native_irq_return_iret
+
+	cmpq	%rcx, RIP-RIP(%rsp)
+	jne	native_irq_return_iret
+	/*
+	 * On Intel CPUs, SYSRET with non-canonical RCX/RIP will #GP
+	 * in kernel space.  This essentially lets the guest take over
+	 * the host, since guest controls RSP.
+	 */
+	canonical_rcx
+	cmpq	%rcx, RIP-RIP(%rsp)
+	je	.L_switcher_sysretq
+
+	/* RCX matches for RIP only before RCX is canonicalized, restore RCX and do IRET. */
+	movq	RIP-RIP(%rsp), %rcx
+	jmp	native_irq_return_iret

+.L_switcher_return_to_hypervisor:
+	popq	%rdi					/* saved rdi */
 	pushq	$0					/* pt_regs->orig_ax */
 	movl	$SWITCH_EXIT_REASONS_SYSCALL, 4(%rsp)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-5-jiangshanlai::40gmail.com:1arch:x86:include:asm:ptrace.h) --git a/arch/x86/include/asm/ptrace.h b/arch/x86/include/asm/ptrace.h
index 9eeeb5fdd387..322697877a2d 100644
--- a/arch/x86/include/asm/ptrace.h
+++ b/arch/x86/include/asm/ptrace.h @@ -198,6 +198,8 @@ static __always_inline bool ip_within_syscall_gap(struct pt_regs *regs)
 	ret = ret || (regs->ip >= (unsigned long)entry_SYSCALL_64_switcher &&
 		      regs->ip <  (unsigned long)entry_SYSCALL_64_switcher_safe_stack);

+	ret = ret || (regs->ip == (unsigned long)entry_SYSRETQ_switcher_unsafe_stack);
+
 	return ret;
 }
 #endif
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-5-jiangshanlai::40gmail.com:1arch:x86:include:asm:switcher.h) --git a/arch/x86/include/asm/switcher.h b/arch/x86/include/asm/switcher.h
index dbf1970ca62f..35a60f4044c4 100644
--- a/arch/x86/include/asm/switcher.h
+++ b/arch/x86/include/asm/switcher.h @@ -8,6 +8,40 @@
 #define SWITCH_EXIT_REASONS_SYSCALL		1024
 #define SWITCH_EXIT_REASONS_FAILED_VMETNRY	1025

+/*
+ * SWITCH_FLAGS control the way how the switcher code works,
+ *	mostly dictate whether it should directly do the guest ring
+ *	switch or just go back to hypervisor.
+ *
+ * SMOD and UMOD
+ *	Current vcpu mode. Use two parity bits to simplify direct-switch
+ *	flags checking.
+ *
+ * NO_DS_CR3
+ *	Not to direct switch due to smod_cr3 or umod_cr3 not having been
+ *	prepared.
+ */
+#define SWITCH_FLAGS_SMOD			_BITULL(0)
+#define SWITCH_FLAGS_UMOD			_BITULL(1)
+#define SWITCH_FLAGS_NO_DS_CR3			_BITULL(2)
+
+#define SWITCH_FLAGS_MOD_TOGGLE			(SWITCH_FLAGS_SMOD | SWITCH_FLAGS_UMOD)
+
+/*
+ * Direct switching disabling bits are all the bits other than
+ * SWITCH_FLAGS_SMOD or SWITCH_FLAGS_UMOD. Bits 8-64 are defined by the driver
+ * using the switcher. Direct switching is enabled if all the disabling bits
+ * are cleared.
+ *
+ * SWITCH_FLAGS_NO_DS_TO_SMOD: not to direct switch to smod due to any
+ * disabling bit or smod bit being set.
+ *
+ * SWITCH_FLAGS_NO_DS_TO_UMOD: not to direct switch to umod due to any
+ * disabling bit or umod bit being set.
+ */
+#define SWITCH_FLAGS_NO_DS_TO_SMOD		(~SWITCH_FLAGS_UMOD)
+#define SWITCH_FLAGS_NO_DS_TO_UMOD		(~SWITCH_FLAGS_SMOD)
+
 /* Bits allowed to be set in the underlying eflags */
 #define SWITCH_ENTER_EFLAGS_ALLOWED	(X86_EFLAGS_FIXED | X86_EFLAGS_IF |\
 					 X86_EFLAGS_TF | X86_EFLAGS_RF |\
@@ -24,6 +58,7 @@
 #include <linux/cache.h>

 struct pt_regs;
+struct pvm_vcpu_struct;

 /*
  * Extra per CPU control structure lives in the struct tss_struct.
@@ -46,6 +81,31 @@ struct tss_extra {
 	unsigned long host_rsp;
 	/* Prepared guest CR3 to be loaded before VM enter. */
 	unsigned long enter_cr3;
+
+	/*
+	 * Direct switching flag indicates whether direct switching
+	 * is allowed.
+	 */
+	unsigned long switch_flags ____cacheline_aligned;
+	/*
+	 * Guest supervisor mode hardware CR3 for direct switching of guest
+	 * user mode syscall.
+	 */
+	unsigned long smod_cr3;
+	/*
+	 * Guest user mode hardware CR3 for direct switching of guest ERETU
+	 * synthetic instruction.
+	 */
+	unsigned long umod_cr3;
+	/*
+	 * The current PVCS for saving and restoring guest user mode context
+	 * in direct switching.
+	 */
+	struct pvm_vcpu_struct *pvcs;
+	unsigned long retu_rip;
+	unsigned long smod_entry;
+	unsigned long smod_gsbase;
+	unsigned long smod_rsp;
 } ____cacheline_aligned;

 extern struct pt_regs *switcher_enter_guest(void);
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-5-jiangshanlai::40gmail.com:1arch:x86:kernel:asm-offsets_64.c) --git a/arch/x86/kernel/asm-offsets_64.c b/arch/x86/kernel/asm-offsets_64.c
index 1485cbda6dc4..8230bd27f0b3 100644
--- a/arch/x86/kernel/asm-offsets_64.c
+++ b/arch/x86/kernel/asm-offsets_64.c @@ -4,6 +4,7 @@
 #endif

 #include <asm/ia32.h>
+#include <asm/pvm_para.h>

 #if defined(CONFIG_KVM_GUEST)
 #include <asm/kvm_para.h>
@@ -65,6 +66,28 @@ int main(void)
 	ENTRY(host_cr3);
 	ENTRY(host_rsp);
 	ENTRY(enter_cr3);
+	ENTRY(switch_flags);
+	ENTRY(smod_cr3);
+	ENTRY(umod_cr3);
+	ENTRY(pvcs);
+	ENTRY(retu_rip);
+	ENTRY(smod_entry);
+	ENTRY(smod_gsbase);
+	ENTRY(smod_rsp);
+	BLANK();
+#undef ENTRY
+
+#define ENTRY(entry) OFFSET(PVCS_ ## entry, pvm_vcpu_struct, entry)
+	ENTRY(event_flags);
+	ENTRY(event_errcode);
+	ENTRY(user_cs);
+	ENTRY(user_ss);
+	ENTRY(user_gsbase);
+	ENTRY(rsp);
+	ENTRY(eflags);
+	ENTRY(rip);
+	ENTRY(rcx);
+	ENTRY(r11);
 	BLANK();
 #undef ENTRY

--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m3b243b78a7b5a0502322405fa52e63002c19c978) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-5-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-5-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-5-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-5-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-5-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-5-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r3b243b78a7b5a0502322405fa52e63002c19c978)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ea4640c2ddc5dbd24e9892d48e636c76b70be6ee7) **[RFC PATCH 05/73] KVM: x86: Set 'vcpu->arch.exception.injected' as true before vendor callback**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(3 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r3b243b78a7b5a0502322405fa52e63002c19c978)
  2024-02-26 14:35 ` [[RFC PATCH 04/73] x86/entry: Implement direct switching for the switcher](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m3b243b78a7b5a0502322405fa52e63002c19c978) Lai Jiangshan
**@ 2024-02-26 14:35 ` Lai Jiangshan**
  2024-02-26 14:35 ` [[RFC PATCH 06/73] KVM: x86: Move VMX interrupt/nmi handling into kvm.ko](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m05a0809c1a3c5bc57be7bf03a668f0d8f1e00eaa) Lai Jiangshan
                   ` [(69 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r05a0809c1a3c5bc57be7bf03a668f0d8f1e00eaa)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ra4640c2ddc5dbd24e9892d48e636c76b70be6ee7)
From: Lai Jiangshan @ 2024-02-26 14:35 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-6-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-6-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143450)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143450), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

For PVM, the exception is injected and delivered directly in the
callback before VM enter, so it will clear
'vcpu->arch.exception.injected'.  Therefore, if
'vcpu->arch.exception.injected' is set to true after the vendor
callback, it may inject the same exception repeatedly in PVM. To address
this, move the setting of 'vcpu->arch.exception.injected' to true before
the vendor callback in kvm_inject_exception(). This adjustment has no
influence on VMX/SVM, as they don't change it in their callbacks.

No functional change.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/kvm/x86.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-6-jiangshanlai::40gmail.com:1arch:x86:kvm:x86.c) | 2 +-
 1 file [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ea4640c2ddc5dbd24e9892d48e636c76b70be6ee7), 1 insertion(+), 1 deletion(-)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-6-jiangshanlai::40gmail.com:1arch:x86:kvm:x86.c) --git a/arch/x86/kvm/x86.c b/arch/x86/kvm/x86.c
index 1a3aaa7dafae..35ad6dd5eaf6 100644
--- a/arch/x86/kvm/x86.c
+++ b/arch/x86/kvm/x86.c @@ -10137,6 +10137,7 @@ static void kvm_inject_exception(struct kvm_vcpu *vcpu)
 				vcpu->arch.exception.error_code,
 				vcpu->arch.exception.injected);

+	vcpu->arch.exception.injected = true;
 	static_call(kvm_x86_inject_exception)(vcpu);
 }

@@ -10288,7 +10289,6 @@ static int kvm_check_and_inject_events(struct kvm_vcpu *vcpu,
 		kvm_inject_exception(vcpu);

 		vcpu->arch.exception.pending = false;
-		vcpu->arch.exception.injected = true;

 		can_inject = false;
 	}
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ma4640c2ddc5dbd24e9892d48e636c76b70be6ee7) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-6-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-6-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-6-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-6-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-6-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-6-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ra4640c2ddc5dbd24e9892d48e636c76b70be6ee7)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e05a0809c1a3c5bc57be7bf03a668f0d8f1e00eaa) **[RFC PATCH 06/73] KVM: x86: Move VMX interrupt/nmi handling into kvm.ko**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(4 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ra4640c2ddc5dbd24e9892d48e636c76b70be6ee7)
  2024-02-26 14:35 ` [[RFC PATCH 05/73] KVM: x86: Set 'vcpu->arch.exception.injected' as true before vendor callback](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ma4640c2ddc5dbd24e9892d48e636c76b70be6ee7) Lai Jiangshan
**@ 2024-02-26 14:35 ` Lai Jiangshan**
  2024-02-26 14:35 ` [[RFC PATCH 07/73] KVM: x86/mmu: Adapt shadow MMU for PVM](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#me39c9679e5b0232d0df0d7ab320253a7791c3210) Lai Jiangshan
                   ` [(68 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#re39c9679e5b0232d0df0d7ab320253a7791c3210)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r05a0809c1a3c5bc57be7bf03a668f0d8f1e00eaa)
From: Lai Jiangshan @ 2024-02-26 14:35 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-7-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-7-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143457)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143457), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin, Mike Rapoport (IBM), Yu-cheng Yu,
	Rick Edgecombe, Paul E. McKenney, Mark Rutland

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

Similar to VMX, hardware interrupts/NMI during guest running in PVM will
trigger VM exit and should be handled by host interrupt/NMI handlers.
Therefore, move VMX interrupt/NMI handling into kvm.ko for common usage.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Co-developed-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/include/asm/idtentry.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-7-jiangshanlai::40gmail.com:1arch:x86:include:asm:idtentry.h) | 12 ++++----
 [arch/x86/kernel/nmi.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-7-jiangshanlai::40gmail.com:1arch:x86:kernel:nmi.c)           |  8 +++---
 [arch/x86/kvm/Makefile](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-7-jiangshanlai::40gmail.com:1arch:x86:kvm:Makefile)           |  2 +-
 [arch/x86/kvm/host_entry.S](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-7-jiangshanlai::40gmail.com:1arch:x86:kvm:host_entry.S)       | 50 +++++++++++++++++++++++++++++++++
 [arch/x86/kvm/vmx/vmenter.S](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-7-jiangshanlai::40gmail.com:1arch:x86:kvm:vmx:vmenter.S)      | 43 ----------------------------
 [arch/x86/kvm/vmx/vmx.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-7-jiangshanlai::40gmail.com:1arch:x86:kvm:vmx:vmx.c)          | 14 ++-------
 [arch/x86/kvm/x86.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-7-jiangshanlai::40gmail.com:1arch:x86:kvm:x86.c)              |  3 ++
 [arch/x86/kvm/x86.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-7-jiangshanlai::40gmail.com:1arch:x86:kvm:x86.h)              | 18 ++++++++++++
 8 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e05a0809c1a3c5bc57be7bf03a668f0d8f1e00eaa), 85 insertions(+), 65 deletions(-)
 create mode 100644 arch/x86/kvm/host_entry.S

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-7-jiangshanlai::40gmail.com:1arch:x86:include:asm:idtentry.h) --git a/arch/x86/include/asm/idtentry.h b/arch/x86/include/asm/idtentry.h
index 13639e57e1f8..8aab0b50431a 100644
--- a/arch/x86/include/asm/idtentry.h
+++ b/arch/x86/include/asm/idtentry.h @@ -586,14 +586,14 @@ DECLARE_IDTENTRY_RAW(X86_TRAP_MC,	xenpv_exc_machine_check);

 /* NMI */

-#if IS_ENABLED(CONFIG_KVM_INTEL) +#if IS_ENABLED(CONFIG_KVM)
 /*
- * Special entry point for VMX which invokes this on the kernel stack, even for
- * 64-bit, i.e. without using an IST.  asm_exc_nmi() requires an IST to work
- * correctly vs. the NMI 'executing' marker.  Used for 32-bit kernels as well
- * to avoid more ifdeffery. + * Special entry point for VMX/PVM which invokes this on the kernel stack, even
+ * for 64-bit, i.e. without using an IST.  asm_exc_nmi() requires an IST to
+ * work correctly vs. the NMI 'executing' marker.  Used for 32-bit kernels as
+ * well to avoid more ifdeffery.
  */
-DECLARE_IDTENTRY(X86_TRAP_NMI,		exc_nmi_kvm_vmx); +DECLARE_IDTENTRY(X86_TRAP_NMI,		exc_nmi_kvm);
 #endif

 DECLARE_IDTENTRY_NMI(X86_TRAP_NMI,	exc_nmi);
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-7-jiangshanlai::40gmail.com:1arch:x86:kernel:nmi.c) --git a/arch/x86/kernel/nmi.c b/arch/x86/kernel/nmi.c
index 17e955ab69fe..265e6b38cc58 100644
--- a/arch/x86/kernel/nmi.c
+++ b/arch/x86/kernel/nmi.c @@ -568,13 +568,13 @@ DEFINE_IDTENTRY_RAW(exc_nmi)
 		mds_user_clear_cpu_buffers();
 }

-#if IS_ENABLED(CONFIG_KVM_INTEL)
-DEFINE_IDTENTRY_RAW(exc_nmi_kvm_vmx) +#if IS_ENABLED(CONFIG_KVM)
+DEFINE_IDTENTRY_RAW(exc_nmi_kvm)
 {
 	exc_nmi(regs);
 }
-#if IS_MODULE(CONFIG_KVM_INTEL)
-EXPORT_SYMBOL_GPL(asm_exc_nmi_kvm_vmx); +#if IS_MODULE(CONFIG_KVM)
+EXPORT_SYMBOL_GPL(asm_exc_nmi_kvm);
 #endif
 #endif

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-7-jiangshanlai::40gmail.com:1arch:x86:kvm:Makefile) --git a/arch/x86/kvm/Makefile b/arch/x86/kvm/Makefile
index 80e3fe184d17..97bad203b1b1 100644
--- a/arch/x86/kvm/Makefile
+++ b/arch/x86/kvm/Makefile @@ -9,7 +9,7 @@ endif

 include $(srctree)/virt/kvm/Makefile.kvm

-kvm-y			+= x86.o emulate.o i8259.o irq.o lapic.o \ +kvm-y			+= x86.o emulate.o i8259.o irq.o lapic.o host_entry.o\
 			   i8254.o ioapic.o irq_comm.o cpuid.o pmu.o mtrr.o\
 			   hyperv.o debugfs.o mmu/mmu.o mmu/page_track.o\
 			   mmu/spte.o
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-7-jiangshanlai::40gmail.com:1arch:x86:kvm:host_entry.S) --git a/arch/x86/kvm/host_entry.S b/arch/x86/kvm/host_entry.S
new file mode 100644
index 000000000000..6bdf0df06eb0
--- /dev/null
+++ b/arch/x86/kvm/host_entry.S @@ -0,0 +1,50 @@ +/* SPDX-License-Identifier: GPL-2.0 */
+#include <linux/linkage.h>
+#include <asm/asm.h>
+#include <asm/nospec-branch.h>
+#include <asm/segment.h>
+
+.macro KVM_DO_EVENT_IRQOFF call_insn call_target
+	/*
+	 * Unconditionally create a stack frame, getting the correct RSP on the
+	 * stack (for x86-64) would take two instructions anyways, and RBP can
+	 * be used to restore RSP to make objtool happy (see below).
+	 */
+	push %_ASM_BP
+	mov %_ASM_SP, %_ASM_BP
+
+#ifdef CONFIG_X86_64
+	/*
+	 * Align RSP to a 16-byte boundary (to emulate CPU behavior) before
+	 * creating the synthetic interrupt stack frame for the IRQ/NMI.
+	 */
+	and  $-16, %rsp
+	push $__KERNEL_DS
+	push %rbp
+#endif
+	pushf
+	push $__KERNEL_CS
+	\call_insn \call_target
+
+	/*
+	 * "Restore" RSP from RBP, even though IRET has already unwound RSP to
+	 * the correct value.  objtool doesn't know the callee will IRET and,
+	 * without the explicit restore, thinks the stack is getting walloped.
+	 * Using an unwind hint is problematic due to x86-64's dynamic alignment.
+	 */
+	mov %_ASM_BP, %_ASM_SP
+	pop %_ASM_BP
+	RET
+.endm
+
+.section .noinstr.text, "ax"
+
+SYM_FUNC_START(kvm_do_host_nmi_irqoff)
+	KVM_DO_EVENT_IRQOFF call asm_exc_nmi_kvm
+SYM_FUNC_END(kvm_do_host_nmi_irqoff)
+
+.section .text, "ax"
+
+SYM_FUNC_START(kvm_do_host_interrupt_irqoff)
+	KVM_DO_EVENT_IRQOFF CALL_NOSPEC _ASM_ARG1
+SYM_FUNC_END(kvm_do_host_interrupt_irqoff) [diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-7-jiangshanlai::40gmail.com:1arch:x86:kvm:vmx:vmenter.S) --git a/arch/x86/kvm/vmx/vmenter.S b/arch/x86/kvm/vmx/vmenter.S
index 906ecd001511..12b7b99a9dd8 100644
--- a/arch/x86/kvm/vmx/vmenter.S
+++ b/arch/x86/kvm/vmx/vmenter.S @@ -31,39 +31,6 @@
 #define VCPU_R15	__VCPU_REGS_R15 * WORD_SIZE
 #endif

-.macro VMX_DO_EVENT_IRQOFF call_insn call_target
-	/*
-	 * Unconditionally create a stack frame, getting the correct RSP on the
-	 * stack (for x86-64) would take two instructions anyways, and RBP can
-	 * be used to restore RSP to make objtool happy (see below).
-	 */
-	push %_ASM_BP
-	mov %_ASM_SP, %_ASM_BP
-#ifdef CONFIG_X86_64
-	/*
-	 * Align RSP to a 16-byte boundary (to emulate CPU behavior) before
-	 * creating the synthetic interrupt stack frame for the IRQ/NMI.
-	 */
-	and  $-16, %rsp
-	push $__KERNEL_DS
-	push %rbp
-#endif
-	pushf
-	push $__KERNEL_CS
-	\call_insn \call_target
-	/*
-	 * "Restore" RSP from RBP, even though IRET has already unwound RSP to
-	 * the correct value.  objtool doesn't know the callee will IRET and,
-	 * without the explicit restore, thinks the stack is getting walloped.
-	 * Using an unwind hint is problematic due to x86-64's dynamic alignment.
-	 */
-	mov %_ASM_BP, %_ASM_SP
-	pop %_ASM_BP
-	RET
-.endm
 .section .noinstr.text, "ax"

 /**
@@ -299,10 +266,6 @@ SYM_INNER_LABEL_ALIGN(vmx_vmexit, SYM_L_GLOBAL)

 SYM_FUNC_END(__vmx_vcpu_run)

-SYM_FUNC_START(vmx_do_nmi_irqoff)
-	VMX_DO_EVENT_IRQOFF call asm_exc_nmi_kvm_vmx
-SYM_FUNC_END(vmx_do_nmi_irqoff)
 #ifndef CONFIG_CC_HAS_ASM_GOTO_OUTPUT

 /**
@@ -354,9 +317,3 @@ SYM_FUNC_START(vmread_error_trampoline)
 	RET
 SYM_FUNC_END(vmread_error_trampoline)
 #endif
-.section .text, "ax"
-SYM_FUNC_START(vmx_do_interrupt_irqoff)
-	VMX_DO_EVENT_IRQOFF CALL_NOSPEC _ASM_ARG1
-SYM_FUNC_END(vmx_do_interrupt_irqoff) [diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-7-jiangshanlai::40gmail.com:1arch:x86:kvm:vmx:vmx.c) --git a/arch/x86/kvm/vmx/vmx.c b/arch/x86/kvm/vmx/vmx.c
index be20a60047b1..fca47304506e 100644
--- a/arch/x86/kvm/vmx/vmx.c
+++ b/arch/x86/kvm/vmx/vmx.c @@ -6920,9 +6920,6 @@ static void vmx_apicv_pre_state_restore(struct kvm_vcpu *vcpu)
 	memset(vmx->pi_desc.pir, 0, sizeof(vmx->pi_desc.pir));
 }

-void vmx_do_interrupt_irqoff(unsigned long entry);
-void vmx_do_nmi_irqoff(void);
 static void handle_nm_fault_irqoff(struct kvm_vcpu *vcpu)
 {
 	/*
@@ -6968,9 +6965,7 @@ static void handle_external_interrupt_irqoff(struct kvm_vcpu *vcpu)
 	    "unexpected VM-Exit interrupt info: 0x%x", intr_info))
 		return;

-	kvm_before_interrupt(vcpu, KVM_HANDLING_IRQ);
-	vmx_do_interrupt_irqoff(gate_offset(desc));
-	kvm_after_interrupt(vcpu); +	kvm_do_interrupt_irqoff(vcpu, gate_offset(desc));

 	vcpu->arch.at_instruction_boundary = true;
 }
@@ -7260,11 +7255,8 @@ static noinstr void vmx_vcpu_enter_exit(struct kvm_vcpu *vcpu,
 		vmx->idt_vectoring_info = vmcs_read32(IDT_VECTORING_INFO_FIELD);

 	if ((u16)vmx->exit_reason.basic == EXIT_REASON_EXCEPTION_NMI &&
-	    is_nmi(vmx_get_intr_info(vcpu))) {
-		kvm_before_interrupt(vcpu, KVM_HANDLING_NMI);
-		vmx_do_nmi_irqoff();
-		kvm_after_interrupt(vcpu);
-	} +	    is_nmi(vmx_get_intr_info(vcpu)))
+		kvm_do_nmi_irqoff(vcpu);

 out:
 	guest_state_exit_irqoff();
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-7-jiangshanlai::40gmail.com:1arch:x86:kvm:x86.c) --git a/arch/x86/kvm/x86.c b/arch/x86/kvm/x86.c
index 35ad6dd5eaf6..96f3913f7fc5 100644
--- a/arch/x86/kvm/x86.c
+++ b/arch/x86/kvm/x86.c @@ -13784,6 +13784,9 @@ int kvm_sev_es_string_io(struct kvm_vcpu *vcpu, unsigned int size,
 }
 EXPORT_SYMBOL_GPL(kvm_sev_es_string_io);

+EXPORT_SYMBOL_GPL(kvm_do_host_nmi_irqoff);
+EXPORT_SYMBOL_GPL(kvm_do_host_interrupt_irqoff);
+
 EXPORT_TRACEPOINT_SYMBOL_GPL(kvm_entry);
 EXPORT_TRACEPOINT_SYMBOL_GPL(kvm_exit);
 EXPORT_TRACEPOINT_SYMBOL_GPL(kvm_fast_mmio);
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-7-jiangshanlai::40gmail.com:1arch:x86:kvm:x86.h) --git a/arch/x86/kvm/x86.h b/arch/x86/kvm/x86.h
index 5184fde1dc54..4d1430f8874b 100644
--- a/arch/x86/kvm/x86.h
+++ b/arch/x86/kvm/x86.h @@ -491,6 +491,24 @@ static inline void kvm_machine_check(void)
 #endif
 }

+void kvm_do_host_nmi_irqoff(void);
+void kvm_do_host_interrupt_irqoff(unsigned long entry);
+
+static __always_inline void kvm_do_nmi_irqoff(struct kvm_vcpu *vcpu)
+{
+	kvm_before_interrupt(vcpu, KVM_HANDLING_NMI);
+	kvm_do_host_nmi_irqoff();
+	kvm_after_interrupt(vcpu);
+}
+
+static inline void kvm_do_interrupt_irqoff(struct kvm_vcpu *vcpu,
+					   unsigned long entry)
+{
+	kvm_before_interrupt(vcpu, KVM_HANDLING_IRQ);
+	kvm_do_host_interrupt_irqoff(entry);
+	kvm_after_interrupt(vcpu);
+}
+
 void kvm_load_guest_xsave_state(struct kvm_vcpu *vcpu);
 void kvm_load_host_xsave_state(struct kvm_vcpu *vcpu);
 int kvm_spec_ctrl_test_value(u64 value);
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m05a0809c1a3c5bc57be7bf03a668f0d8f1e00eaa) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-7-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-7-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-7-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-7-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-7-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-7-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r05a0809c1a3c5bc57be7bf03a668f0d8f1e00eaa)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ee39c9679e5b0232d0df0d7ab320253a7791c3210) **[RFC PATCH 07/73] KVM: x86/mmu: Adapt shadow MMU for PVM**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(5 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r05a0809c1a3c5bc57be7bf03a668f0d8f1e00eaa)
  2024-02-26 14:35 ` [[RFC PATCH 06/73] KVM: x86: Move VMX interrupt/nmi handling into kvm.ko](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m05a0809c1a3c5bc57be7bf03a668f0d8f1e00eaa) Lai Jiangshan
**@ 2024-02-26 14:35 ` Lai Jiangshan**
  2024-02-26 14:35 ` [[RFC PATCH 08/73] KVM: x86: Allow hypercall handling to not skip the instruction](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ma85007e874e0c3c0f689d477d793302d04c044e6) Lai Jiangshan
                   ` [(67 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ra85007e874e0c3c0f689d477d793302d04c044e6)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#re39c9679e5b0232d0df0d7ab320253a7791c3210)
From: Lai Jiangshan @ 2024-02-26 14:35 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-8-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-8-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143500)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143500), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

In PVM, shadow MMU is used for guest MMU virtualization. However, it
needs some changes to adapt for PVM:

1. In PVM, hardware CR4.LA57 is not changed, so the paging level of
   shadow MMU should be same as host. If the guest paging level is 4 and
   host paging level is 5, then it performs like shadow NPT MMU and
   'root_role.passthrough' is set as true.

2. PVM guest needs to access the host switcher, so some host mapping PGD
   entries will be cloned into the guest shadow paging table during the
   root SP allocation. These cloned host PGD entries are not marked as MMU
   present, so they can't be cleared by write-protecting. Additionally, in
   order to avoid modifying those cloned host PGD entries in the #PF
   handling path, a new callback is introduced to check the fault of the
   guest virtual address before walking the guest page table. This ensures
   that the guest cannot overwrite the host entries in the root SP.

3. If the guest paging level is 4 and the host paging level is 5, then the
   last PGD entry in the root SP is allowed to be overwritten if the guest
   tries to build a new allowed mapping under this PGD entry. In this case,
   the host P4D entries in the table pointed to by the last PGD entry
   should also be cloned during the new P4D SP allocation. These cloned P4D
   entries are also not marked as MMU present. A new bit in the
   'kvm_mmu_page_role' is used to mark this special SP. When zapping this
   SP, its parent PTE will be set to the original host PGD PTEs instead of
   clearing it.

4. The user bit in the SPTE of guest mapping should be forced to be set
   for PVM, as the guest is always running in hardware CPL3.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/include/asm/kvm-x86-ops.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-8-jiangshanlai::40gmail.com:1arch:x86:include:asm:kvm-x86-ops.h) |  1 +
 [arch/x86/include/asm/kvm_host.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-8-jiangshanlai::40gmail.com:1arch:x86:include:asm:kvm_host.h)    |  6 ++++-
 [arch/x86/kvm/mmu/mmu.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-8-jiangshanlai::40gmail.com:1arch:x86:kvm:mmu:mmu.c)             | 35 +++++++++++++++++++++++++++++-
 [arch/x86/kvm/mmu/paging_tmpl.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-8-jiangshanlai::40gmail.com:1arch:x86:kvm:mmu:paging_tmpl.h)     |  3 +++
 [arch/x86/kvm/mmu/spte.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-8-jiangshanlai::40gmail.com:1arch:x86:kvm:mmu:spte.c)            |  4 ++++
 5 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ee39c9679e5b0232d0df0d7ab320253a7791c3210), 47 insertions(+), 2 deletions(-)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-8-jiangshanlai::40gmail.com:1arch:x86:include:asm:kvm-x86-ops.h) --git a/arch/x86/include/asm/kvm-x86-ops.h b/arch/x86/include/asm/kvm-x86-ops.h
index 26b628d84594..32e5473b499d 100644
--- a/arch/x86/include/asm/kvm-x86-ops.h
+++ b/arch/x86/include/asm/kvm-x86-ops.h @@ -93,6 +93,7 @@ KVM_X86_OP_OPTIONAL_RET0(set_tss_addr)
 KVM_X86_OP_OPTIONAL_RET0(set_identity_map_addr)
 KVM_X86_OP_OPTIONAL_RET0(get_mt_mask)
 KVM_X86_OP(load_mmu_pgd)
+KVM_X86_OP_OPTIONAL_RET0(disallowed_va)
 KVM_X86_OP(has_wbinvd_exit)
 KVM_X86_OP(get_l2_tsc_offset)
 KVM_X86_OP(get_l2_tsc_multiplier)
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-8-jiangshanlai::40gmail.com:1arch:x86:include:asm:kvm_host.h) --git a/arch/x86/include/asm/kvm_host.h b/arch/x86/include/asm/kvm_host.h
index d7036982332e..c76bafe9c7e2 100644
--- a/arch/x86/include/asm/kvm_host.h
+++ b/arch/x86/include/asm/kvm_host.h @@ -346,7 +346,8 @@ union kvm_mmu_page_role {
 		unsigned ad_disabled:1;
 		unsigned guest_mode:1;
 		unsigned passthrough:1;
-		unsigned :5; +		unsigned host_mmu_la57_top_p4d:1;
+		unsigned :4;

 		/*
 		 * This is left at the top of the word so that
@@ -1429,6 +1430,7 @@ struct kvm_arch {
 	 * the thread holds the MMU lock in write mode.
 	 */
 	spinlock_t tdp_mmu_pages_lock;
+	u64 *host_mmu_root_pgd;
 #endif /* CONFIG_X86_64 */

 	/*
@@ -1679,6 +1681,8 @@ struct kvm_x86_ops {
 	void (*load_mmu_pgd)(struct kvm_vcpu *vcpu, hpa_t root_hpa,
 			     int root_level);

+	bool (*disallowed_va)(struct kvm_vcpu *vcpu, u64 la);
+
 	bool (*has_wbinvd_exit)(void);

 	u64 (*get_l2_tsc_offset)(struct kvm_vcpu *vcpu);
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-8-jiangshanlai::40gmail.com:1arch:x86:kvm:mmu:mmu.c) --git a/arch/x86/kvm/mmu/mmu.c b/arch/x86/kvm/mmu/mmu.c
index c57e181bba21..80406666d7da 100644
--- a/arch/x86/kvm/mmu/mmu.c
+++ b/arch/x86/kvm/mmu/mmu.c @@ -1745,6 +1745,18 @@ static unsigned kvm_page_table_hashfn(gfn_t gfn)
 	return hash_64(gfn, KVM_MMU_HASH_SHIFT);
 }

+#define HOST_ROOT_LEVEL (pgtable_l5_enabled() ? PT64_ROOT_5LEVEL : PT64_ROOT_4LEVEL)
+
+static inline bool pvm_mmu_p4d_at_la57_pgd511(struct kvm *kvm, u64 *sptep)
+{
+	if (!pgtable_l5_enabled())
+		return false;
+	if (!kvm->arch.host_mmu_root_pgd)
+		return false;
+
+	return sptep_to_sp(sptep)->role.level == 5 && spte_index(sptep) == 511;
+}
+
 static void mmu_page_add_parent_pte(struct kvm_mmu_memory_cache *cache,
 				    struct kvm_mmu_page *sp, u64 *parent_pte)
 {
@@ -1764,7 +1776,10 @@ static void drop_parent_pte(struct kvm *kvm, struct kvm_mmu_page *sp,
 			    u64 *parent_pte)
 {
 	mmu_page_remove_parent_pte(kvm, sp, parent_pte);
-	mmu_spte_clear_no_track(parent_pte); +	if (!unlikely(sp->role.host_mmu_la57_top_p4d))
+		mmu_spte_clear_no_track(parent_pte);
+	else
+		__update_clear_spte_fast(parent_pte, kvm->arch.host_mmu_root_pgd[511]);
 }

 static void mark_unsync(u64 *spte);
@@ -2253,6 +2268,15 @@ static struct kvm_mmu_page *kvm_mmu_alloc_shadow_page(struct kvm *kvm,
 	list_add(&sp->link, &kvm->arch.active_mmu_pages);
 	kvm_account_mmu_page(kvm, sp);

+	/* install host mmu entries when PVM */
+	if (kvm->arch.host_mmu_root_pgd && role.level == HOST_ROOT_LEVEL) {
+		memcpy(sp->spt, kvm->arch.host_mmu_root_pgd, PAGE_SIZE);
+	} else if (role.host_mmu_la57_top_p4d) {
+		u64 *p4d = __va(kvm->arch.host_mmu_root_pgd[511] & SPTE_BASE_ADDR_MASK);
+
+		memcpy(sp->spt, p4d, PAGE_SIZE);
+	}
+
 	sp->gfn = gfn;
 	sp->role = role;
 	hlist_add_head(&sp->hash_link, sp_list);
@@ -2354,6 +2378,9 @@ static struct kvm_mmu_page *kvm_mmu_get_child_sp(struct kvm_vcpu *vcpu,
 		return ERR_PTR(-EEXIST);

 	role = kvm_mmu_child_role(sptep, direct, access);
+	if (unlikely(pvm_mmu_p4d_at_la57_pgd511(vcpu->kvm, sptep)))
+		role.host_mmu_la57_top_p4d = 1;
+
 	return kvm_mmu_get_shadow_page(vcpu, gfn, role);
 }

@@ -5271,6 +5298,12 @@ static void kvm_init_shadow_mmu(struct kvm_vcpu *vcpu,
 	/* KVM uses PAE paging whenever the guest isn't using 64-bit paging. */
 	root_role.level = max_t(u32, root_role.level, PT32E_ROOT_LEVEL);

+	/* Shadow MMU level should be the same as host for PVM */
+	if (vcpu->kvm->arch.host_mmu_root_pgd && root_role.level != HOST_ROOT_LEVEL) {
+		root_role.level = HOST_ROOT_LEVEL;
+		root_role.passthrough = 1;
+	}
+
 	/*
 	 * KVM forces EFER.NX=1 when TDP is disabled, reflect it in the MMU role.
 	 * KVM uses NX when TDP is disabled to handle a variety of scenarios,
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-8-jiangshanlai::40gmail.com:1arch:x86:kvm:mmu:paging_tmpl.h) --git a/arch/x86/kvm/mmu/paging_tmpl.h b/arch/x86/kvm/mmu/paging_tmpl.h
index c85255073f67..8ea3dca940ad 100644
--- a/arch/x86/kvm/mmu/paging_tmpl.h
+++ b/arch/x86/kvm/mmu/paging_tmpl.h @@ -336,6 +336,9 @@ static int FNAME(walk_addr_generic)(struct guest_walker *walker,
 			goto error;
 		--walker->level;
 	}
+
+	if (static_call(kvm_x86_disallowed_va)(vcpu, addr))
+		goto error;
 #endif
 	walker->max_level = walker->level;

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-8-jiangshanlai::40gmail.com:1arch:x86:kvm:mmu:spte.c) --git a/arch/x86/kvm/mmu/spte.c b/arch/x86/kvm/mmu/spte.c
index 4a599130e9c9..e302f7b5c696 100644
--- a/arch/x86/kvm/mmu/spte.c
+++ b/arch/x86/kvm/mmu/spte.c @@ -186,6 +186,10 @@ bool make_spte(struct kvm_vcpu *vcpu, struct kvm_mmu_page *sp,
 	if (pte_access & ACC_USER_MASK)
 		spte |= shadow_user_mask;

+	/* PVM guest is always running in hardware CPL3. */
+	if (vcpu->kvm->arch.host_mmu_root_pgd)
+		spte |= shadow_user_mask;
+
 	if (level > PG_LEVEL_4K)
 		spte |= PT_PAGE_SIZE_MASK;

--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#me39c9679e5b0232d0df0d7ab320253a7791c3210) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-8-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-8-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-8-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-8-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-8-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-8-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#re39c9679e5b0232d0df0d7ab320253a7791c3210)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ea85007e874e0c3c0f689d477d793302d04c044e6) **[RFC PATCH 08/73] KVM: x86: Allow hypercall handling to not skip the instruction**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(6 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#re39c9679e5b0232d0df0d7ab320253a7791c3210)
  2024-02-26 14:35 ` [[RFC PATCH 07/73] KVM: x86/mmu: Adapt shadow MMU for PVM](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#me39c9679e5b0232d0df0d7ab320253a7791c3210) Lai Jiangshan
**@ 2024-02-26 14:35 ` Lai Jiangshan**
  2024-02-26 14:35 ` [[RFC PATCH 09/73] KVM: x86: Add PVM virtual MSRs into emulated_msrs_all[]](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m012c1a1c873dc334ab545cbff69fc79bcc23f3da) Lai Jiangshan
                   ` [(66 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r012c1a1c873dc334ab545cbff69fc79bcc23f3da)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ra85007e874e0c3c0f689d477d793302d04c044e6)
From: Lai Jiangshan @ 2024-02-26 14:35 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-9-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-9-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143504)
  Cc: Hou Wenlong, Lai Jiangshan, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143504), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Hou Wenlong <houwenlong.hwl@antgroup.com>

In PVM, the syscall instruction is used as the hypercall instruction.
Since the syscall instruction is a trap that indicates the instruction
has been executed, there is no need to skip the hypercall instruction.

Suggested-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
---
 [arch/x86/include/asm/kvm_host.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-9-jiangshanlai::40gmail.com:1arch:x86:include:asm:kvm_host.h) | 12 +++++++++++-
 [arch/x86/kvm/x86.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-9-jiangshanlai::40gmail.com:1arch:x86:kvm:x86.c)              | 10 +++++++---
 2 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ea85007e874e0c3c0f689d477d793302d04c044e6), 18 insertions(+), 4 deletions(-)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-9-jiangshanlai::40gmail.com:1arch:x86:include:asm:kvm_host.h) --git a/arch/x86/include/asm/kvm_host.h b/arch/x86/include/asm/kvm_host.h
index c76bafe9c7e2..d17d85106d6f 100644
--- a/arch/x86/include/asm/kvm_host.h
+++ b/arch/x86/include/asm/kvm_host.h @@ -2077,7 +2077,17 @@ static inline void kvm_clear_apicv_inhibit(struct kvm *kvm,
 	kvm_set_or_clear_apicv_inhibit(kvm, reason, false);
 }

-int kvm_emulate_hypercall(struct kvm_vcpu *vcpu); +int kvm_handle_hypercall(struct kvm_vcpu *vcpu, bool skip);
+
+static inline int kvm_emulate_hypercall(struct kvm_vcpu *vcpu)
+{
+	return kvm_handle_hypercall(vcpu, true);
+}
+
+static inline int kvm_emulate_hypercall_noskip(struct kvm_vcpu *vcpu)
+{
+	return kvm_handle_hypercall(vcpu, false);
+}

 int kvm_mmu_page_fault(struct kvm_vcpu *vcpu, gpa_t cr2_or_gpa, u64 error_code,
 		       void *insn, int insn_len);
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-9-jiangshanlai::40gmail.com:1arch:x86:kvm:x86.c) --git a/arch/x86/kvm/x86.c b/arch/x86/kvm/x86.c
index 96f3913f7fc5..8ec7a36cdf3e 100644
--- a/arch/x86/kvm/x86.c
+++ b/arch/x86/kvm/x86.c @@ -9933,7 +9933,7 @@ static int complete_hypercall_exit(struct kvm_vcpu *vcpu)
 	return kvm_skip_emulated_instruction(vcpu);
 }

-int kvm_emulate_hypercall(struct kvm_vcpu *vcpu) +int kvm_handle_hypercall(struct kvm_vcpu *vcpu, bool skip)
 {
 	unsigned long nr, a0, a1, a2, a3, ret;
 	int op_64_bit;
@@ -10034,9 +10034,13 @@ int kvm_emulate_hypercall(struct kvm_vcpu *vcpu)
 	kvm_rax_write(vcpu, ret);

 	++vcpu->stat.hypercalls;
-	return kvm_skip_emulated_instruction(vcpu); +
+	if (skip)
+		return kvm_skip_emulated_instruction(vcpu);
+
+	return 1;
 }
-EXPORT_SYMBOL_GPL(kvm_emulate_hypercall); +EXPORT_SYMBOL_GPL(kvm_handle_hypercall);

 static int emulator_fix_hypercall(struct x86_emulate_ctxt *ctxt)
 {
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ma85007e874e0c3c0f689d477d793302d04c044e6) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-9-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-9-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-9-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-9-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-9-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-9-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ra85007e874e0c3c0f689d477d793302d04c044e6)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e012c1a1c873dc334ab545cbff69fc79bcc23f3da) **[RFC PATCH 09/73] KVM: x86: Add PVM virtual MSRs into emulated_msrs_all[]**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(7 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ra85007e874e0c3c0f689d477d793302d04c044e6)
  2024-02-26 14:35 ` [[RFC PATCH 08/73] KVM: x86: Allow hypercall handling to not skip the instruction](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ma85007e874e0c3c0f689d477d793302d04c044e6) Lai Jiangshan
**@ 2024-02-26 14:35 ` Lai Jiangshan**
  2024-02-26 14:35 ` [[RFC PATCH 10/73] KVM: x86: Introduce vendor feature to expose vendor-specific CPUID](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m0c0e5f5df8feaba74f67cf6883aa5a1c22c3a5a1) Lai Jiangshan
                   ` [(65 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r0c0e5f5df8feaba74f67cf6883aa5a1c22c3a5a1)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r012c1a1c873dc334ab545cbff69fc79bcc23f3da)
From: Lai Jiangshan @ 2024-02-26 14:35 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-10-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-10-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143507)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143507), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

Add PVM virtual MSRs to emulated_msrs_all[], enabling the saving and
restoration of VM states.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/kvm/svm/svm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-10-jiangshanlai::40gmail.com:1arch:x86:kvm:svm:svm.c) |  4 ++++
 [arch/x86/kvm/vmx/vmx.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-10-jiangshanlai::40gmail.com:1arch:x86:kvm:vmx:vmx.c) |  4 ++++
 [arch/x86/kvm/x86.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-10-jiangshanlai::40gmail.com:1arch:x86:kvm:x86.c)     | 10 ++++++++++
 3 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e012c1a1c873dc334ab545cbff69fc79bcc23f3da), 18 insertions(+)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-10-jiangshanlai::40gmail.com:1arch:x86:kvm:svm:svm.c) --git a/arch/x86/kvm/svm/svm.c b/arch/x86/kvm/svm/svm.c
index f3bb30b40876..91ab7cbbe813 100644
--- a/arch/x86/kvm/svm/svm.c
+++ b/arch/x86/kvm/svm/svm.c @@ -31,6 +31,7 @@

 #include <asm/apic.h>
 #include <asm/perf_event.h>
+#include <asm/pvm_para.h>
 #include <asm/tlbflush.h>
 #include <asm/desc.h>
 #include <asm/debugreg.h>
@@ -4281,6 +4282,9 @@ static bool svm_has_emulated_msr(struct kvm *kvm, u32 index)
 	case MSR_IA32_MCG_EXT_CTL:
 	case KVM_FIRST_EMULATED_VMX_MSR ... KVM_LAST_EMULATED_VMX_MSR:
 		return false;
+	case PVM_VIRTUAL_MSR_BASE ... PVM_VIRTUAL_MSR_MAX:
+		/* This is PVM only. */
+		return false;
 	case MSR_IA32_SMBASE:
 		if (!IS_ENABLED(CONFIG_KVM_SMM))
 			return false;
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-10-jiangshanlai::40gmail.com:1arch:x86:kvm:vmx:vmx.c) --git a/arch/x86/kvm/vmx/vmx.c b/arch/x86/kvm/vmx/vmx.c
index fca47304506e..e20a566f6d83 100644
--- a/arch/x86/kvm/vmx/vmx.c
+++ b/arch/x86/kvm/vmx/vmx.c @@ -43,6 +43,7 @@
 #include <asm/irq_remapping.h>
 #include <asm/reboot.h>
 #include <asm/perf_event.h>
+#include <asm/pvm_para.h>
 #include <asm/mmu_context.h>
 #include <asm/mshyperv.h>
 #include <asm/mwait.h>
@@ -7004,6 +7005,9 @@ static bool vmx_has_emulated_msr(struct kvm *kvm, u32 index)
 	case MSR_AMD64_TSC_RATIO:
 		/* This is AMD only.  */
 		return false;
+	case PVM_VIRTUAL_MSR_BASE ... PVM_VIRTUAL_MSR_MAX:
+		/* This is PVM only. */
+		return false;
 	default:
 		return true;
 	}
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-10-jiangshanlai::40gmail.com:1arch:x86:kvm:x86.c) --git a/arch/x86/kvm/x86.c b/arch/x86/kvm/x86.c
index 8ec7a36cdf3e..be8fdae942d1 100644
--- a/arch/x86/kvm/x86.c
+++ b/arch/x86/kvm/x86.c @@ -84,6 +84,7 @@
 #include <asm/intel_pt.h>
 #include <asm/emulate_prefix.h>
 #include <asm/sgx.h>
+#include <asm/pvm_para.h>
 #include <clocksource/hyperv_timer.h>

 #define CREATE_TRACE_POINTS
@@ -1525,6 +1526,15 @@ static const u32 emulated_msrs_all[] = {
 	MSR_KVM_ASYNC_PF_EN, MSR_KVM_STEAL_TIME,
 	MSR_KVM_PV_EOI_EN, MSR_KVM_ASYNC_PF_INT, MSR_KVM_ASYNC_PF_ACK,

+	MSR_PVM_LINEAR_ADDRESS_RANGE,
+	MSR_PVM_VCPU_STRUCT,
+	MSR_PVM_SUPERVISOR_RSP,
+	MSR_PVM_SUPERVISOR_REDZONE,
+	MSR_PVM_EVENT_ENTRY,
+	MSR_PVM_RETU_RIP,
+	MSR_PVM_RETS_RIP,
+	MSR_PVM_SWITCH_CR3,
+
 	MSR_IA32_TSC_ADJUST,
 	MSR_IA32_TSC_DEADLINE,
 	MSR_IA32_ARCH_CAPABILITIES,
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m012c1a1c873dc334ab545cbff69fc79bcc23f3da) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-10-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-10-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-10-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-10-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-10-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-10-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r012c1a1c873dc334ab545cbff69fc79bcc23f3da)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e0c0e5f5df8feaba74f67cf6883aa5a1c22c3a5a1) **[RFC PATCH 10/73] KVM: x86: Introduce vendor feature to expose vendor-specific CPUID**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(8 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r012c1a1c873dc334ab545cbff69fc79bcc23f3da)
  2024-02-26 14:35 ` [[RFC PATCH 09/73] KVM: x86: Add PVM virtual MSRs into emulated_msrs_all[]](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m012c1a1c873dc334ab545cbff69fc79bcc23f3da) Lai Jiangshan
**@ 2024-02-26 14:35 ` Lai Jiangshan**
  2024-02-26 14:35 ` [[RFC PATCH 11/73] KVM: x86: Implement gpc refresh for guest usage](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m72b6ae0685b99573cb31030b77b7b790135df13d) Lai Jiangshan
                   ` [(64 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r72b6ae0685b99573cb31030b77b7b790135df13d)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r0c0e5f5df8feaba74f67cf6883aa5a1c22c3a5a1)
From: Lai Jiangshan @ 2024-02-26 14:35 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-11-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-11-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143510)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143510), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Wanpeng Li, Vitaly Kuznetsov, Dave Hansen, H. Peter Anvin

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

For the PVM guest, it needs to detect PVM support early, even before IDT
setup, so the cpuid instruction is used. Moreover, in order to
differentiate PVM from VMX/SVM, a new CPUID is introduced to expose
vendor-specific features. Currently, only PVM uses it.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/include/uapi/asm/kvm_para.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-11-jiangshanlai::40gmail.com:1arch:x86:include:uapi:asm:kvm_para.h) |  8 +++++++-
 [arch/x86/kvm/cpuid.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-11-jiangshanlai::40gmail.com:1arch:x86:kvm:cpuid.c)                 | 26 +++++++++++++++++++++++++-
 [arch/x86/kvm/cpuid.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-11-jiangshanlai::40gmail.com:1arch:x86:kvm:cpuid.h)                 |  3 +++
 3 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e0c0e5f5df8feaba74f67cf6883aa5a1c22c3a5a1), 35 insertions(+), 2 deletions(-)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-11-jiangshanlai::40gmail.com:1arch:x86:include:uapi:asm:kvm_para.h) --git a/arch/x86/include/uapi/asm/kvm_para.h b/arch/x86/include/uapi/asm/kvm_para.h
index 6e64b27b2c1e..f999f1d32423 100644
--- a/arch/x86/include/uapi/asm/kvm_para.h
+++ b/arch/x86/include/uapi/asm/kvm_para.h @@ -5,7 +5,9 @@
 #include <linux/types.h>

 /* This CPUID returns the signature 'KVMKVMKVM' in ebx, ecx, and edx.  It
- * should be used to determine that a VM is running under KVM. + * should be used to determine that a VM is running under KVM. And it
+ * returns KVM_CPUID_FEATURES in eax if vendor feature is not enabled,
+ * otherwise KVM_CPUID_VENDOR_FEATURES.
  */
 #define KVM_CPUID_SIGNATURE	0x40000000
 #define KVM_SIGNATURE "KVMKVMKVM\0\0\0"
@@ -16,6 +18,10 @@
  * in edx.
  */
 #define KVM_CPUID_FEATURES	0x40000001
+/* This CPUID returns the vendor feature bitmaps in eax and the vendor
+ * signature in ebx.
+ */
+#define KVM_CPUID_VENDOR_FEATURES	0x40000002
 #define KVM_FEATURE_CLOCKSOURCE		0
 #define KVM_FEATURE_NOP_IO_DELAY	1
 #define KVM_FEATURE_MMU_OP		2
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-11-jiangshanlai::40gmail.com:1arch:x86:kvm:cpuid.c) --git a/arch/x86/kvm/cpuid.c b/arch/x86/kvm/cpuid.c
index dda6fc4cfae8..31ae843a6180 100644
--- a/arch/x86/kvm/cpuid.c
+++ b/arch/x86/kvm/cpuid.c @@ -36,6 +36,16 @@
 u32 kvm_cpu_caps[NR_KVM_CPU_CAPS] __read_mostly;
 EXPORT_SYMBOL_GPL(kvm_cpu_caps);

+u32 kvm_cpuid_vendor_features;
+EXPORT_SYMBOL_GPL(kvm_cpuid_vendor_features);
+u32 kvm_cpuid_vendor_signature;
+EXPORT_SYMBOL_GPL(kvm_cpuid_vendor_signature);
+
+static inline bool has_kvm_cpuid_vendor_features(void)
+{
+	return !!kvm_cpuid_vendor_signature;
+}
+
 u32 xstate_required_size(u64 xstate_bv, bool compacted)
 {
 	int feature_bit = 0;
@@ -1132,7 +1142,10 @@ static inline int __do_cpuid_func(struct kvm_cpuid_array *array, u32 function)
 		break;
 	case KVM_CPUID_SIGNATURE: {
 		const u32 *sigptr = (const u32 *)KVM_SIGNATURE;
-		entry->eax = KVM_CPUID_FEATURES; +		if (!has_kvm_cpuid_vendor_features())
+			entry->eax = KVM_CPUID_FEATURES;
+		else
+			entry->eax = KVM_CPUID_VENDOR_FEATURES;
 		entry->ebx = sigptr[0];
 		entry->ecx = sigptr[1];
 		entry->edx = sigptr[2];
@@ -1160,6 +1173,17 @@ static inline int __do_cpuid_func(struct kvm_cpuid_array *array, u32 function)
 		entry->ecx = 0;
 		entry->edx = 0;
 		break;
+	case KVM_CPUID_VENDOR_FEATURES:
+		if (!has_kvm_cpuid_vendor_features()) {
+			entry->eax = 0;
+			entry->ebx = 0;
+		} else {
+			entry->eax = kvm_cpuid_vendor_features;
+			entry->ebx = kvm_cpuid_vendor_signature;
+		}
+		entry->ecx = 0;
+		entry->edx = 0;
+		break;
 	case 0x80000000:
 		entry->eax = min(entry->eax, 0x80000022);
 		/*
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-11-jiangshanlai::40gmail.com:1arch:x86:kvm:cpuid.h) --git a/arch/x86/kvm/cpuid.h b/arch/x86/kvm/cpuid.h
index 0b90532b6e26..b93e5fec4808 100644
--- a/arch/x86/kvm/cpuid.h
+++ b/arch/x86/kvm/cpuid.h @@ -8,6 +8,9 @@
 #include <asm/processor.h>
 #include <uapi/asm/kvm_para.h>

+extern u32 kvm_cpuid_vendor_features;
+extern u32 kvm_cpuid_vendor_signature;
+
 extern u32 kvm_cpu_caps[NR_KVM_CPU_CAPS] __read_mostly;
 void kvm_set_cpu_caps(void);

--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m0c0e5f5df8feaba74f67cf6883aa5a1c22c3a5a1) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-11-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-11-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-11-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-11-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-11-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-11-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r0c0e5f5df8feaba74f67cf6883aa5a1c22c3a5a1)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e72b6ae0685b99573cb31030b77b7b790135df13d) **[RFC PATCH 11/73] KVM: x86: Implement gpc refresh for guest usage**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(9 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r0c0e5f5df8feaba74f67cf6883aa5a1c22c3a5a1)
  2024-02-26 14:35 ` [[RFC PATCH 10/73] KVM: x86: Introduce vendor feature to expose vendor-specific CPUID](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m0c0e5f5df8feaba74f67cf6883aa5a1c22c3a5a1) Lai Jiangshan
**@ 2024-02-26 14:35 ` Lai Jiangshan**
  2024-02-26 14:35 ` [[RFC PATCH 12/73] KVM: x86: Add NR_VCPU_SREG in SREG enum](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#me826405a148de8c11b9dad7f7ca0417305d7bd6d) Lai Jiangshan
                   ` [(63 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#re826405a148de8c11b9dad7f7ca0417305d7bd6d)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r72b6ae0685b99573cb31030b77b7b790135df13d)
From: Lai Jiangshan @ 2024-02-26 14:35 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-12-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-12-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143513)
  Cc: Hou Wenlong, Lai Jiangshan, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143513), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Hou Wenlong <houwenlong.hwl@antgroup.com>

PVM uses pfncache to share the PVCS structure between the guest and
host. The flag of pfncache of PVCS is initialized as
KVM_GUEST_AND_HOST_USE_PFN because the PVCS is used inside the
vcpu_run() callback, even in the switcher, where the vcpu is in guest
mode. However, there is no real usage for GUEST_USE_PFN, so the request
in mmu_notifier only kicks the vcpu out of guest mode and no refresh is
done before the next vcpu_run(). Therefore, a new request type is
introduced to request the refresh, and a new callback is used to service
the request.

Suggested-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
---
 [arch/x86/include/asm/kvm-x86-ops.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-12-jiangshanlai::40gmail.com:1arch:x86:include:asm:kvm-x86-ops.h) |  1 +
 [arch/x86/include/asm/kvm_host.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-12-jiangshanlai::40gmail.com:1arch:x86:include:asm:kvm_host.h)    |  3 +++
 [arch/x86/kvm/mmu/mmu.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-12-jiangshanlai::40gmail.com:1arch:x86:kvm:mmu:mmu.c)             |  1 +
 [arch/x86/kvm/x86.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-12-jiangshanlai::40gmail.com:1arch:x86:kvm:x86.c)                 |  3 +++
 [include/linux/kvm_host.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-12-jiangshanlai::40gmail.com:1include:linux:kvm_host.h)           | 10 ++++++++++
 [virt/kvm/pfncache.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-12-jiangshanlai::40gmail.com:1virt:kvm:pfncache.c)                |  2 +-
 6 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e72b6ae0685b99573cb31030b77b7b790135df13d), 19 insertions(+), 1 deletion(-)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-12-jiangshanlai::40gmail.com:1arch:x86:include:asm:kvm-x86-ops.h) --git a/arch/x86/include/asm/kvm-x86-ops.h b/arch/x86/include/asm/kvm-x86-ops.h
index 32e5473b499d..0d9b21988943 100644
--- a/arch/x86/include/asm/kvm-x86-ops.h
+++ b/arch/x86/include/asm/kvm-x86-ops.h @@ -94,6 +94,7 @@ KVM_X86_OP_OPTIONAL_RET0(set_identity_map_addr)
 KVM_X86_OP_OPTIONAL_RET0(get_mt_mask)
 KVM_X86_OP(load_mmu_pgd)
 KVM_X86_OP_OPTIONAL_RET0(disallowed_va)
+KVM_X86_OP_OPTIONAL(vcpu_gpc_refresh);
 KVM_X86_OP(has_wbinvd_exit)
 KVM_X86_OP(get_l2_tsc_offset)
 KVM_X86_OP(get_l2_tsc_multiplier)
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-12-jiangshanlai::40gmail.com:1arch:x86:include:asm:kvm_host.h) --git a/arch/x86/include/asm/kvm_host.h b/arch/x86/include/asm/kvm_host.h
index d17d85106d6f..9223d34cb8e3 100644
--- a/arch/x86/include/asm/kvm_host.h
+++ b/arch/x86/include/asm/kvm_host.h @@ -1683,6 +1683,8 @@ struct kvm_x86_ops {

 	bool (*disallowed_va)(struct kvm_vcpu *vcpu, u64 la);

+	void (*vcpu_gpc_refresh)(struct kvm_vcpu *vcpu);
+
 	bool (*has_wbinvd_exit)(void);

 	u64 (*get_l2_tsc_offset)(struct kvm_vcpu *vcpu);
@@ -1839,6 +1841,7 @@ static inline int kvm_arch_flush_remote_tlbs(struct kvm *kvm)
 }

 #define __KVM_HAVE_ARCH_FLUSH_REMOTE_TLBS_RANGE
+#define __KVM_HAVE_GUEST_USE_PFN_USAGE

 #define kvm_arch_pmi_in_guest(vcpu)\
 	((vcpu) && (vcpu)->arch.handling_intr_from_guest)
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-12-jiangshanlai::40gmail.com:1arch:x86:kvm:mmu:mmu.c) --git a/arch/x86/kvm/mmu/mmu.c b/arch/x86/kvm/mmu/mmu.c
index 80406666d7da..7bd88f7ace51 100644
--- a/arch/x86/kvm/mmu/mmu.c
+++ b/arch/x86/kvm/mmu/mmu.c @@ -6741,6 +6741,7 @@ void kvm_arch_flush_shadow_memslot(struct kvm *kvm,
 				   struct kvm_memory_slot *slot)
 {
 	kvm_mmu_zap_all_fast(kvm);
+	kvm_make_all_cpus_request(kvm, KVM_REQ_GPC_REFRESH);
 }

 void kvm_mmu_invalidate_mmio_sptes(struct kvm *kvm, u64 gen)
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-12-jiangshanlai::40gmail.com:1arch:x86:kvm:x86.c) --git a/arch/x86/kvm/x86.c b/arch/x86/kvm/x86.c
index be8fdae942d1..89bf368085a9 100644
--- a/arch/x86/kvm/x86.c
+++ b/arch/x86/kvm/x86.c @@ -10786,6 +10786,9 @@ static int vcpu_enter_guest(struct kvm_vcpu *vcpu)

 		if (kvm_check_request(KVM_REQ_UPDATE_CPU_DIRTY_LOGGING, vcpu))
 			static_call(kvm_x86_update_cpu_dirty_logging)(vcpu);
+
+		if (kvm_check_request(KVM_REQ_GPC_REFRESH, vcpu))
+			static_call_cond(kvm_x86_vcpu_gpc_refresh)(vcpu);
 	}

 	if (kvm_check_request(KVM_REQ_EVENT, vcpu) || req_int_win ||
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-12-jiangshanlai::40gmail.com:1include:linux:kvm_host.h) --git a/include/linux/kvm_host.h b/include/linux/kvm_host.h
index 4944136efaa2..b7c490e74704 100644
--- a/include/linux/kvm_host.h
+++ b/include/linux/kvm_host.h @@ -167,6 +167,7 @@ static inline bool is_error_page(struct page *page)
 #define KVM_REQ_VM_DEAD			(1 | KVM_REQUEST_WAIT | KVM_REQUEST_NO_WAKEUP)
 #define KVM_REQ_UNBLOCK			2
 #define KVM_REQ_DIRTY_RING_SOFT_FULL	3
+#define KVM_REQ_GPC_REFRESH		(5 | KVM_REQUEST_WAIT | KVM_REQUEST_NO_WAKEUP)
 #define KVM_REQUEST_ARCH_BASE		8

 /*
@@ -1367,6 +1368,15 @@ int kvm_gpc_refresh(struct gfn_to_pfn_cache *gpc, unsigned long len);
  */
 void kvm_gpc_deactivate(struct gfn_to_pfn_cache *gpc);

+static inline unsigned int kvm_gpc_refresh_request(void)
+{
+#ifdef __KVM_HAVE_GUEST_USE_PFN_USAGE
+	return KVM_REQ_GPC_REFRESH;
+#else
+	return KVM_REQ_OUTSIDE_GUEST_MODE;
+#endif
+}
+
 void kvm_sigset_activate(struct kvm_vcpu *vcpu);
 void kvm_sigset_deactivate(struct kvm_vcpu *vcpu);

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-12-jiangshanlai::40gmail.com:1virt:kvm:pfncache.c) --git a/virt/kvm/pfncache.c b/virt/kvm/pfncache.c
index 2d6aba677830..f7b7a2f75ec7 100644
--- a/virt/kvm/pfncache.c
+++ b/virt/kvm/pfncache.c @@ -59,7 +59,7 @@ void gfn_to_pfn_cache_invalidate_start(struct kvm *kvm, unsigned long start,
 		 * KVM needs to ensure the vCPU is fully out of guest context
 		 * before allowing the invalidation to continue.
 		 */
-		unsigned int req = KVM_REQ_OUTSIDE_GUEST_MODE; +		unsigned int req = kvm_gpc_refresh_request();
 		bool called;

 		/*
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m72b6ae0685b99573cb31030b77b7b790135df13d) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-12-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-12-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-12-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-12-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-12-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-12-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r72b6ae0685b99573cb31030b77b7b790135df13d)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ee826405a148de8c11b9dad7f7ca0417305d7bd6d) **[RFC PATCH 12/73] KVM: x86: Add NR_VCPU_SREG in SREG enum**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(10 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r72b6ae0685b99573cb31030b77b7b790135df13d)
  2024-02-26 14:35 ` [[RFC PATCH 11/73] KVM: x86: Implement gpc refresh for guest usage](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m72b6ae0685b99573cb31030b77b7b790135df13d) Lai Jiangshan
**@ 2024-02-26 14:35 ` Lai Jiangshan**
  2024-02-26 14:35 ` [[RFC PATCH 13/73] KVM: x86/emulator: Reinject #GP if instruction emulation failed for PVM](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m1ec193e69ae78265dd8d5a2ea61788103ea240e1) Lai Jiangshan
                   ` [(62 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r1ec193e69ae78265dd8d5a2ea61788103ea240e1)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#re826405a148de8c11b9dad7f7ca0417305d7bd6d)
From: Lai Jiangshan @ 2024-02-26 14:35 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-13-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-13-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143517)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143517), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

Add NR_VCPU_SREG to describe the size of the SREG enum, this allows for
the definition of the size of an array.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/include/asm/kvm_host.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-13-jiangshanlai::40gmail.com:1arch:x86:include:asm:kvm_host.h) | 1 +
 1 file [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ee826405a148de8c11b9dad7f7ca0417305d7bd6d), 1 insertion(+)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-13-jiangshanlai::40gmail.com:1arch:x86:include:asm:kvm_host.h) --git a/arch/x86/include/asm/kvm_host.h b/arch/x86/include/asm/kvm_host.h
index 9223d34cb8e3..a90807f676b9 100644
--- a/arch/x86/include/asm/kvm_host.h
+++ b/arch/x86/include/asm/kvm_host.h @@ -204,6 +204,7 @@ enum {
 	VCPU_SREG_GS,
 	VCPU_SREG_TR,
 	VCPU_SREG_LDTR,
+	NR_VCPU_SREG,
 };

 enum exit_fastpath_completion {
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#me826405a148de8c11b9dad7f7ca0417305d7bd6d) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-13-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-13-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-13-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-13-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-13-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-13-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#re826405a148de8c11b9dad7f7ca0417305d7bd6d)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e1ec193e69ae78265dd8d5a2ea61788103ea240e1) **[RFC PATCH 13/73] KVM: x86/emulator: Reinject #GP if instruction emulation failed for PVM**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(11 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#re826405a148de8c11b9dad7f7ca0417305d7bd6d)
  2024-02-26 14:35 ` [[RFC PATCH 12/73] KVM: x86: Add NR_VCPU_SREG in SREG enum](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#me826405a148de8c11b9dad7f7ca0417305d7bd6d) Lai Jiangshan
**@ 2024-02-26 14:35 ` Lai Jiangshan**
  2024-02-26 14:35 ` [[RFC PATCH 14/73] KVM: x86: Create stubs for PVM module as a new vendor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m3bec210bc1ebe2b4a3a15d38d96cb43675c08340) Lai Jiangshan
                   ` [(61 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r3bec210bc1ebe2b4a3a15d38d96cb43675c08340)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r1ec193e69ae78265dd8d5a2ea61788103ea240e1)
From: Lai Jiangshan @ 2024-02-26 14:35 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-14-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-14-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143520)
  Cc: Hou Wenlong, Lai Jiangshan, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143520), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Hou Wenlong <houwenlong.hwl@antgroup.com>

The privilege instruction in PVM guest supervisor mode will trigger a
instruction triggers a #GP in PVM guest supervisor mode and is not
implemented in the emulator, the emulator will currently exit to
userspace, and VMM may not be able to handle it. This can be triggered
by guest userspace, e.g., a guest userspace process can corrupt the
XSTATE header in a signal frame and the XRSTOR in the guest kernel will
trigger a #GP, but XRSTOR is not implemented in the emulator now.
Therefore, a new emulate type for PVM is added to instruct the emulator
to reinject the #GP into the guest.

Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
---
 [arch/x86/include/asm/kvm_host.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-14-jiangshanlai::40gmail.com:1arch:x86:include:asm:kvm_host.h) | 8 ++++++++
 [arch/x86/kvm/x86.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-14-jiangshanlai::40gmail.com:1arch:x86:kvm:x86.c)              | 5 +++--
 2 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e1ec193e69ae78265dd8d5a2ea61788103ea240e1), 11 insertions(+), 2 deletions(-)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-14-jiangshanlai::40gmail.com:1arch:x86:include:asm:kvm_host.h) --git a/arch/x86/include/asm/kvm_host.h b/arch/x86/include/asm/kvm_host.h
index a90807f676b9..3e6f27865528 100644
--- a/arch/x86/include/asm/kvm_host.h
+++ b/arch/x86/include/asm/kvm_host.h @@ -1954,6 +1954,13 @@ u64 vcpu_tsc_khz(struct kvm_vcpu *vcpu);
  *			     the gfn, i.e. retrying the instruction will hit a
  *			     !PRESENT fault, which results in a new shadow page
  *			     and sends KVM back to square one.
+ *
+ * EMULTYPE_PVM_GP - Set when emulating an intercepted #GP for PVM. Privilege
+ *		     instruction in PVM guest supervisor mode will trigger a
+ *		     #GP and be emulated by PVM. But if a non-privilege
+ *		     instruction triggers a #GP in PVM guest supervisor mode
+ *		     and is not implemented in the emulator, the emulator
+ *		     should reinject the #GP into guest.
  */
 #define EMULTYPE_NO_DECODE	    (1 << 0)
 #define EMULTYPE_TRAP_UD	    (1 << 1)
@@ -1964,6 +1971,7 @@ u64 vcpu_tsc_khz(struct kvm_vcpu *vcpu);
 #define EMULTYPE_PF		    (1 << 6)
 #define EMULTYPE_COMPLETE_USER_EXIT (1 << 7)
 #define EMULTYPE_WRITE_PF_TO_SP	    (1 << 8)
+#define EMULTYPE_PVM_GP		    (1 << 9)

 int kvm_emulate_instruction(struct kvm_vcpu *vcpu, int emulation_type);
 int kvm_emulate_instruction_from_buffer(struct kvm_vcpu *vcpu,
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-14-jiangshanlai::40gmail.com:1arch:x86:kvm:x86.c) --git a/arch/x86/kvm/x86.c b/arch/x86/kvm/x86.c
index 89bf368085a9..29413cb2f090 100644
--- a/arch/x86/kvm/x86.c
+++ b/arch/x86/kvm/x86.c @@ -8664,7 +8664,7 @@ static int handle_emulation_failure(struct kvm_vcpu *vcpu, int emulation_type)
 	++vcpu->stat.insn_emulation_fail;
 	trace_kvm_emulate_insn_failed(vcpu);

-	if (emulation_type & EMULTYPE_VMWARE_GP) { +	if (emulation_type & (EMULTYPE_VMWARE_GP | EMULTYPE_PVM_GP)) {
 		kvm_queue_exception_e(vcpu, GP_VECTOR, 0);
 		return 1;
 	}
@@ -8902,7 +8902,8 @@ static bool kvm_vcpu_check_code_breakpoint(struct kvm_vcpu *vcpu,
 	 * and without a prefix.
 	 */
 	if (emulation_type & (EMULTYPE_NO_DECODE | EMULTYPE_SKIP |
-			      EMULTYPE_TRAP_UD | EMULTYPE_VMWARE_GP | EMULTYPE_PF)) +			      EMULTYPE_TRAP_UD | EMULTYPE_VMWARE_GP |
+			      EMULTYPE_PVM_GP | EMULTYPE_PF))
 		return false;

 	if (unlikely(vcpu->guest_debug & KVM_GUESTDBG_USE_HW_BP) &&
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m1ec193e69ae78265dd8d5a2ea61788103ea240e1) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-14-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-14-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-14-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-14-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-14-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-14-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r1ec193e69ae78265dd8d5a2ea61788103ea240e1)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e3bec210bc1ebe2b4a3a15d38d96cb43675c08340) **[RFC PATCH 14/73] KVM: x86: Create stubs for PVM module as a new vendor**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(12 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r1ec193e69ae78265dd8d5a2ea61788103ea240e1)
  2024-02-26 14:35 ` [[RFC PATCH 13/73] KVM: x86/emulator: Reinject #GP if instruction emulation failed for PVM](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m1ec193e69ae78265dd8d5a2ea61788103ea240e1) Lai Jiangshan
**@ 2024-02-26 14:35 ` Lai Jiangshan**
  2024-02-26 14:35 ` [[RFC PATCH 15/73] mm/vmalloc: Add a helper to reserve a contiguous and aligned kernel virtual area](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ma2decc0d9dda7301af1d323411463772c3ee3e15) Lai Jiangshan
                   ` [(60 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ra2decc0d9dda7301af1d323411463772c3ee3e15)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r3bec210bc1ebe2b4a3a15d38d96cb43675c08340)
From: Lai Jiangshan @ 2024-02-26 14:35 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-15-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-15-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143523)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143523), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

Add a new Kconfig option and create stub files for what will eventually
be a new module named PVM (Pagetable-based PV Virtual Machine). PVM will
function as a vendor module, similar to VMX/SVM for KVM, but it doesn't
require hardware virtualization assistance.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/kvm/Kconfig](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-15-jiangshanlai::40gmail.com:1arch:x86:kvm:Kconfig)   |  9 +++++++++
 [arch/x86/kvm/Makefile](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-15-jiangshanlai::40gmail.com:1arch:x86:kvm:Makefile)  |  3 +++
 [arch/x86/kvm/pvm/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-15-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) | 26 ++++++++++++++++++++++++++
 3 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e3bec210bc1ebe2b4a3a15d38d96cb43675c08340), 38 insertions(+)
 create mode 100644 arch/x86/kvm/pvm/pvm.c

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-15-jiangshanlai::40gmail.com:1arch:x86:kvm:Kconfig) --git a/arch/x86/kvm/Kconfig b/arch/x86/kvm/Kconfig
index 950c12868d30..49a8b3489a0a 100644
--- a/arch/x86/kvm/Kconfig
+++ b/arch/x86/kvm/Kconfig @@ -118,6 +118,15 @@ config KVM_AMD_SEV
 	  Provides support for launching Encrypted VMs (SEV) and Encrypted VMs
 	  with Encrypted State (SEV-ES) on AMD processors.

+config KVM_PVM
+	tristate "Pagetable-based PV Virtual Machine"
+	depends on KVM && X86_64
+	help
+	  Provides Pagetable-based PV Virtual Machine for KVM.
+
+	  To compile this as a module, choose M here: the module
+	  will be called kvm-pvm.
+
 config KVM_SMM
 	bool "System Management Mode emulation"
 	default y
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-15-jiangshanlai::40gmail.com:1arch:x86:kvm:Makefile) --git a/arch/x86/kvm/Makefile b/arch/x86/kvm/Makefile
index 97bad203b1b1..036458a27d5e 100644
--- a/arch/x86/kvm/Makefile
+++ b/arch/x86/kvm/Makefile @@ -33,9 +33,12 @@ ifdef CONFIG_HYPERV
 kvm-amd-y		+= svm/svm_onhyperv.o
 endif

+kvm-pvm-y 		+= pvm/pvm.o
+
 obj-$(CONFIG_KVM)	+= kvm.o
 obj-$(CONFIG_KVM_INTEL)	+= kvm-intel.o
 obj-$(CONFIG_KVM_AMD)	+= kvm-amd.o
+obj-$(CONFIG_KVM_PVM) 	+= kvm-pvm.o

 AFLAGS_svm/vmenter.o    := -iquote $(obj)
 $(obj)/svm/vmenter.o: $(obj)/kvm-asm-offsets.h
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-15-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) --git a/arch/x86/kvm/pvm/pvm.c b/arch/x86/kvm/pvm/pvm.c
new file mode 100644
index 000000000000..1dfa1ae57c8c
--- /dev/null
+++ b/arch/x86/kvm/pvm/pvm.c @@ -0,0 +1,26 @@ +// SPDX-License-Identifier: GPL-2.0-only
+/*
+ * Pagetable-based Virtual Machine driver for Linux
+ *
+ * Copyright (C) 2020 Ant Group
+ * Copyright (C) 2020 Alibaba Group
+ *
+ * This work is licensed under the terms of the GNU GPL, version 2.  See
+ * the COPYING file in the top-level directory.
+ *
+ */
+#include <linux/module.h>
+
+MODULE_AUTHOR("AntGroup");
+MODULE_LICENSE("GPL");
+
+static void pvm_exit(void)
+{
+}
+module_exit(pvm_exit);
+
+static int __init pvm_init(void)
+{
+	return 0;
+}
+module_init(pvm_init); --
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m3bec210bc1ebe2b4a3a15d38d96cb43675c08340) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-15-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-15-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-15-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-15-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-15-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-15-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r3bec210bc1ebe2b4a3a15d38d96cb43675c08340)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ea2decc0d9dda7301af1d323411463772c3ee3e15) **[RFC PATCH 15/73] mm/vmalloc: Add a helper to reserve a contiguous and aligned kernel virtual area**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(13 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r3bec210bc1ebe2b4a3a15d38d96cb43675c08340)
  2024-02-26 14:35 ` [[RFC PATCH 14/73] KVM: x86: Create stubs for PVM module as a new vendor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m3bec210bc1ebe2b4a3a15d38d96cb43675c08340) Lai Jiangshan
**@ 2024-02-26 14:35 ` Lai Jiangshan**
  2024-02-27 14:56   ` [Christoph Hellwig](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m934efcc0c018f5e064353a9160954ce7103b69e3)
  2024-02-26 14:35 ` [[RFC PATCH 16/73] KVM: x86/PVM: Implement host mmu initialization](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m252184d3f37bddd3067ba3ba05f9140dd3e5aebf) Lai Jiangshan
                   ` [(59 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r252184d3f37bddd3067ba3ba05f9140dd3e5aebf)
  [74 siblings, 1 reply; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ra2decc0d9dda7301af1d323411463772c3ee3e15)
From: Lai Jiangshan @ 2024-02-26 14:35 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-16-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-16-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143527)
  Cc: Hou Wenlong, Lai Jiangshan, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143527), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Andrew Morton, Uladzislau Rezki, Christoph Hellwig,
	Lorenzo Stoakes, [linux-mm](https://lore.kernel.org/linux-mm/?t=20240226143527)

From: Hou Wenlong <houwenlong.hwl@antgroup.com>

PVM needs to reserve a contiguous and aligned kernel virtual area for
the guest kernel. Therefor, add a helper to achieve this. It is a
temporary method currently, and a better method is needed in the future.

Suggested-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
---
 [include/linux/vmalloc.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-16-jiangshanlai::40gmail.com:1include:linux:vmalloc.h) |  2 ++
 [mm/vmalloc.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-16-jiangshanlai::40gmail.com:1mm:vmalloc.c)            | 10 ++++++++++
 2 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ea2decc0d9dda7301af1d323411463772c3ee3e15), 12 insertions(+)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-16-jiangshanlai::40gmail.com:1include:linux:vmalloc.h) --git a/include/linux/vmalloc.h b/include/linux/vmalloc.h
index c720be70c8dd..1821494b51d6 100644
--- a/include/linux/vmalloc.h
+++ b/include/linux/vmalloc.h @@ -204,6 +204,8 @@ static inline size_t get_vm_area_size(const struct vm_struct *area)
 }

 extern struct vm_struct *get_vm_area(unsigned long size, unsigned long flags);
+extern struct vm_struct *get_vm_area_align(unsigned long size, unsigned long align,
+					   unsigned long flags);
 extern struct vm_struct *get_vm_area_caller(unsigned long size,
 					unsigned long flags, const void *caller);
 extern struct vm_struct *__get_vm_area_caller(unsigned long size,
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-16-jiangshanlai::40gmail.com:1mm:vmalloc.c) --git a/mm/vmalloc.c b/mm/vmalloc.c
index d12a17fc0c17..6e4b95f24bd8 100644
--- a/mm/vmalloc.c
+++ b/mm/vmalloc.c @@ -2642,6 +2642,16 @@ struct vm_struct *get_vm_area(unsigned long size, unsigned long flags)
 				  __builtin_return_address(0));
 }

+struct vm_struct *get_vm_area_align(unsigned long size, unsigned long align,
+				    unsigned long flags)
+{
+	return __get_vm_area_node(size, align, PAGE_SHIFT, flags,
+				  VMALLOC_START, VMALLOC_END,
+				  NUMA_NO_NODE, GFP_KERNEL,
+				  __builtin_return_address(0));
+}
+EXPORT_SYMBOL_GPL(get_vm_area_align);
+
 struct vm_struct *get_vm_area_caller(unsigned long size, unsigned long flags,
 				const void *caller)
 {
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ma2decc0d9dda7301af1d323411463772c3ee3e15) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-16-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-16-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-16-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-16-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-16-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-16-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ra2decc0d9dda7301af1d323411463772c3ee3e15)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e252184d3f37bddd3067ba3ba05f9140dd3e5aebf) **[RFC PATCH 16/73] KVM: x86/PVM: Implement host mmu initialization**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(14 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ra2decc0d9dda7301af1d323411463772c3ee3e15)
  2024-02-26 14:35 ` [[RFC PATCH 15/73] mm/vmalloc: Add a helper to reserve a contiguous and aligned kernel virtual area](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ma2decc0d9dda7301af1d323411463772c3ee3e15) Lai Jiangshan
**@ 2024-02-26 14:35 ` Lai Jiangshan**
  2024-02-26 14:35 ` [[RFC PATCH 17/73] KVM: x86/PVM: Implement module initialization related callbacks](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mcffc6d9b931c065a515cc5ebaa312b7cf6ed76ea) Lai Jiangshan
                   ` [(58 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rcffc6d9b931c065a515cc5ebaa312b7cf6ed76ea)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r252184d3f37bddd3067ba3ba05f9140dd3e5aebf)
From: Lai Jiangshan @ 2024-02-26 14:35 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-17-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-17-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143533)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143533), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

For PVM, it utilizes shadow paging as MMU virtualization for the guest.
As the switcher supports guest/host world switches, the host kernel
mapping should be cloned into the guest shadow paging table, similar to
PTI. For simplicity, only the PGD level is cloned. Additionally, the
guest Linux kernel runs in high address space, so PVM will reserve a
kernel virtual area in the host vmalloc area for the guest, also at the
PGD level.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/kvm/Makefile](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-17-jiangshanlai::40gmail.com:1arch:x86:kvm:Makefile)       |   2 +-
 [arch/x86/kvm/pvm/host_mmu.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-17-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:host_mmu.c) | 119 ++++++++++++++++++++++++++++++++++++
 [arch/x86/kvm/pvm/pvm.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-17-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.h)      |  23 +++++++
 3 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e252184d3f37bddd3067ba3ba05f9140dd3e5aebf), 143 insertions(+), 1 deletion(-)
 create mode 100644 arch/x86/kvm/pvm/host_mmu.c
 create mode 100644 arch/x86/kvm/pvm/pvm.h

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-17-jiangshanlai::40gmail.com:1arch:x86:kvm:Makefile) --git a/arch/x86/kvm/Makefile b/arch/x86/kvm/Makefile
index 036458a27d5e..706ccf3eca45 100644
--- a/arch/x86/kvm/Makefile
+++ b/arch/x86/kvm/Makefile @@ -33,7 +33,7 @@ ifdef CONFIG_HYPERV
 kvm-amd-y		+= svm/svm_onhyperv.o
 endif

-kvm-pvm-y 		+= pvm/pvm.o +kvm-pvm-y 		+= pvm/pvm.o pvm/host_mmu.o

 obj-$(CONFIG_KVM)	+= kvm.o
 obj-$(CONFIG_KVM_INTEL)	+= kvm-intel.o
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-17-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:host_mmu.c) --git a/arch/x86/kvm/pvm/host_mmu.c b/arch/x86/kvm/pvm/host_mmu.c
new file mode 100644
index 000000000000..35e97f4f7055
--- /dev/null
+++ b/arch/x86/kvm/pvm/host_mmu.c @@ -0,0 +1,119 @@ +// SPDX-License-Identifier: GPL-2.0-only
+/*
+ * PVM host mmu implementation
+ *
+ * Copyright (C) 2020 Ant Group
+ *
+ * This work is licensed under the terms of the GNU GPL, version 2.  See
+ * the COPYING file in the top-level directory.
+ *
+ */
+
+#define pr_fmt(fmt) KBUILD_MODNAME ": " fmt
+
+#include <linux/vmalloc.h>
+
+#include <asm/cpufeature.h>
+#include <asm/vsyscall.h>
+#include <asm/pgtable.h>
+
+#include "mmu.h"
+#include "mmu/spte.h"
+#include "pvm.h"
+
+static struct vm_struct *pvm_va_range_l4;
+
+u32 pml4_index_start;
+u32 pml4_index_end;
+u32 pml5_index_start;
+u32 pml5_index_end;
+
+static int __init guest_address_space_init(void)
+{
+	if (IS_ENABLED(CONFIG_KASAN_VMALLOC)) {
+		pr_warn("CONFIG_KASAN_VMALLOC is not compatible with PVM");
+		return -1;
+	}
+
+	pvm_va_range_l4 = get_vm_area_align(DEFAULT_RANGE_L4_SIZE, PT_L4_SIZE,
+			  VM_ALLOC|VM_NO_GUARD);
+	if (!pvm_va_range_l4)
+		return -1;
+
+	pml4_index_start = __PT_INDEX((u64)pvm_va_range_l4->addr, 4, 9);
+	pml4_index_end = __PT_INDEX((u64)pvm_va_range_l4->addr + (u64)pvm_va_range_l4->size, 4, 9);
+	pml5_index_start = 0x1ff;
+	pml5_index_end = 0x1ff;
+	return 0;
+}
+
+static __init void clone_host_mmu(u64 *spt, u64 *host, int index_start, int index_end)
+{
+	int i;
+
+	for (i = PTRS_PER_PGD/2; i < PTRS_PER_PGD; i++) {
+		/* clone only the range that doesn't belong to guest */
+		if (i >= index_start && i < index_end)
+			continue;
+
+		/* remove userbit from host mmu, which also disable VSYSCALL page */
+		spt[i] = host[i] & ~(_PAGE_USER | SPTE_MMU_PRESENT_MASK);
+	}
+}
+
+u64 *host_mmu_root_pgd;
+u64 *host_mmu_la57_top_p4d;
+
+int __init host_mmu_init(void)
+{
+	u64 *host_pgd;
+
+	if (guest_address_space_init() < 0)
+		return -ENOMEM;
+
+	if (!boot_cpu_has(X86_FEATURE_PTI))
+		host_pgd = (void *)current->mm->pgd;
+	else
+		host_pgd = (void *)kernel_to_user_pgdp(current->mm->pgd);
+
+	host_mmu_root_pgd = (void *)__get_free_page(GFP_KERNEL | __GFP_ZERO);
+
+	if (!host_mmu_root_pgd) {
+		host_mmu_destroy();
+		return -ENOMEM;
+	}
+	if (pgtable_l5_enabled()) {
+		host_mmu_la57_top_p4d = (void *)__get_free_page(GFP_KERNEL | __GFP_ZERO);
+		if (!host_mmu_la57_top_p4d) {
+			host_mmu_destroy();
+			return -ENOMEM;
+		}
+
+		clone_host_mmu(host_mmu_root_pgd, host_pgd, pml5_index_start, pml5_index_end);
+		clone_host_mmu(host_mmu_la57_top_p4d, __va(host_pgd[511] & SPTE_BASE_ADDR_MASK),
+				pml4_index_start, pml4_index_end);
+	} else {
+		clone_host_mmu(host_mmu_root_pgd, host_pgd, pml4_index_start, pml4_index_end);
+	}
+
+	if (pgtable_l5_enabled()) {
+		pr_warn("Supporting for LA57 host is not fully implemented yet.\n");
+		host_mmu_destroy();
+		return -EOPNOTSUPP;
+	}
+
+	return 0;
+}
+
+void host_mmu_destroy(void)
+{
+	if (pvm_va_range_l4)
+		free_vm_area(pvm_va_range_l4);
+	if (host_mmu_root_pgd)
+		free_page((unsigned long)(void *)host_mmu_root_pgd);
+	if (host_mmu_la57_top_p4d)
+		free_page((unsigned long)(void *)host_mmu_la57_top_p4d);
+	pvm_va_range_l4 = NULL;
+	host_mmu_root_pgd = NULL;
+	host_mmu_la57_top_p4d = NULL;
+} [diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-17-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.h) --git a/arch/x86/kvm/pvm/pvm.h b/arch/x86/kvm/pvm/pvm.h
new file mode 100644
index 000000000000..7a3732986a6d
--- /dev/null
+++ b/arch/x86/kvm/pvm/pvm.h @@ -0,0 +1,23 @@ +/* SPDX-License-Identifier: GPL-2.0 */
+#ifndef __KVM_X86_PVM_H
+#define __KVM_X86_PVM_H
+
+#define PT_L4_SHIFT		39
+#define PT_L4_SIZE		(1UL << PT_L4_SHIFT)
+#define DEFAULT_RANGE_L4_SIZE	(32 * PT_L4_SIZE)
+
+#define PT_L5_SHIFT		48
+#define PT_L5_SIZE		(1UL << PT_L5_SHIFT)
+#define DEFAULT_RANGE_L5_SIZE	(32 * PT_L5_SIZE)
+
+extern u32 pml4_index_start;
+extern u32 pml4_index_end;
+extern u32 pml5_index_start;
+extern u32 pml5_index_end;
+
+extern u64 *host_mmu_root_pgd;
+
+void host_mmu_destroy(void);
+int host_mmu_init(void);
+
+#endif /* __KVM_X86_PVM_H */ --
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m252184d3f37bddd3067ba3ba05f9140dd3e5aebf) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-17-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-17-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-17-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-17-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-17-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-17-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r252184d3f37bddd3067ba3ba05f9140dd3e5aebf)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ecffc6d9b931c065a515cc5ebaa312b7cf6ed76ea) **[RFC PATCH 17/73] KVM: x86/PVM: Implement module initialization related callbacks**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(15 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r252184d3f37bddd3067ba3ba05f9140dd3e5aebf)
  2024-02-26 14:35 ` [[RFC PATCH 16/73] KVM: x86/PVM: Implement host mmu initialization](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m252184d3f37bddd3067ba3ba05f9140dd3e5aebf) Lai Jiangshan
**@ 2024-02-26 14:35 ` Lai Jiangshan**
  2024-02-26 14:35 ` [[RFC PATCH 18/73] KVM: x86/PVM: Implement VM/VCPU](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m999e9100dc7bf390c8b2892c071ccfe18cfc9c7c) " Lai Jiangshan
                   ` [(57 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r999e9100dc7bf390c8b2892c071ccfe18cfc9c7c)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rcffc6d9b931c065a515cc5ebaa312b7cf6ed76ea)
From: Lai Jiangshan @ 2024-02-26 14:35 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-18-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-18-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143534)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143534), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

Implement hardware enable/disable and setup/unsetup callbacks for PVM
module initialization.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/kvm/pvm/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-18-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) | 226 +++++++++++++++++++++++++++++++++++++++++
 [arch/x86/kvm/pvm/pvm.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-18-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.h) |  20 ++++
 2 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ecffc6d9b931c065a515cc5ebaa312b7cf6ed76ea), 246 insertions(+)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-18-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) --git a/arch/x86/kvm/pvm/pvm.c b/arch/x86/kvm/pvm/pvm.c
index 1dfa1ae57c8c..83aa2c9f42f6 100644
--- a/arch/x86/kvm/pvm/pvm.c
+++ b/arch/x86/kvm/pvm/pvm.c @@ -9,18 +9,244 @@
  * the COPYING file in the top-level directory.
  *
  */
+#define pr_fmt(fmt) KBUILD_MODNAME ": " fmt
+
 #include <linux/module.h>

+#include <asm/pvm_para.h>
+
+#include "cpuid.h"
+#include "x86.h"
+#include "pvm.h"
+
 MODULE_AUTHOR("AntGroup");
 MODULE_LICENSE("GPL");

+static bool __read_mostly is_intel;
+
+static unsigned long host_idt_base;
+
+static void pvm_setup_mce(struct kvm_vcpu *vcpu)
+{
+}
+
+static bool pvm_has_emulated_msr(struct kvm *kvm, u32 index)
+{
+	switch (index) {
+	case MSR_IA32_MCG_EXT_CTL:
+	case KVM_FIRST_EMULATED_VMX_MSR ... KVM_LAST_EMULATED_VMX_MSR:
+		return false;
+	case MSR_AMD64_VIRT_SPEC_CTRL:
+	case MSR_AMD64_TSC_RATIO:
+		/* This is AMD SVM only. */
+		return false;
+	case MSR_IA32_SMBASE:
+		/* Currenlty we only run guest in long mode. */
+		return false;
+	default:
+		break;
+	}
+
+	return true;
+}
+
+static bool cpu_has_pvm_wbinvd_exit(void)
+{
+	return true;
+}
+
+static int hardware_enable(void)
+{
+	/* Nothing to do */
+	return 0;
+}
+
+static void hardware_disable(void)
+{
+	/* Nothing to do */
+}
+
+static int pvm_check_processor_compat(void)
+{
+	/* Nothing to do */
+	return 0;
+}
+
+static __init void pvm_set_cpu_caps(void)
+{
+	if (boot_cpu_has(X86_FEATURE_NX))
+		kvm_enable_efer_bits(EFER_NX);
+	if (boot_cpu_has(X86_FEATURE_FXSR_OPT))
+		kvm_enable_efer_bits(EFER_FFXSR);
+
+	kvm_set_cpu_caps();
+
+	/* Unloading kvm-intel.ko doesn't clean up kvm_caps.supported_mce_cap. */
+	kvm_caps.supported_mce_cap = MCG_CTL_P | MCG_SER_P;
+
+	kvm_caps.supported_xss = 0;
+
+	/* PVM supervisor mode runs on hardware ring3, so no xsaves. */
+	kvm_cpu_cap_clear(X86_FEATURE_XSAVES);
+
+	/*
+	 * PVM supervisor mode runs on hardware ring3, so SMEP and SMAP can not
+	 * be supported directly through hardware.  But they can be emulated
+	 * through other hardware feature when needed.
+	 */
+
+	/*
+	 * PVM doesn't support SMAP, but the similar protection might be
+	 * emulated via PKU in the future.
+	 */
+	kvm_cpu_cap_clear(X86_FEATURE_SMAP);
+
+	/*
+	 * PVM doesn't support SMEP.  When NX is supported and the guest can
+	 * use NX on the user pagetable to emulate the same protection as SMEP.
+	 */
+	kvm_cpu_cap_clear(X86_FEATURE_SMEP);
+
+	/*
+	 * Unlike VMX/SVM which can switches paging mode atomically, PVM
+	 * implements guest LA57 through host LA57 shadow paging.
+	 */
+	if (!pgtable_l5_enabled())
+		kvm_cpu_cap_clear(X86_FEATURE_LA57);
+
+	/*
+	 * Even host pcid is not enabled, guest pcid can be enabled to reduce
+	 * the heavy guest tlb flushing.  Guest CR4.PCIDE is not directly
+	 * mapped to the hardware and is virtualized by PVM so that it can be
+	 * enabled unconditionally.
+	 */
+	kvm_cpu_cap_set(X86_FEATURE_PCID);
+
+	/* Don't expose MSR_IA32_SPEC_CTRL to guest */
+	kvm_cpu_cap_clear(X86_FEATURE_SPEC_CTRL);
+	kvm_cpu_cap_clear(X86_FEATURE_AMD_STIBP);
+	kvm_cpu_cap_clear(X86_FEATURE_AMD_IBRS);
+	kvm_cpu_cap_clear(X86_FEATURE_AMD_SSBD);
+
+	/* PVM hypervisor hasn't implemented LAM so far */
+	kvm_cpu_cap_clear(X86_FEATURE_LAM);
+
+	/* Don't expose MSR_IA32_DEBUGCTLMSR related features. */
+	kvm_cpu_cap_clear(X86_FEATURE_BUS_LOCK_DETECT);
+}
+
+static __init int hardware_setup(void)
+{
+	struct desc_ptr dt;
+
+	store_idt(&dt);
+	host_idt_base = dt.address;
+
+	pvm_set_cpu_caps();
+
+	kvm_configure_mmu(false, 0, 0, 0);
+
+	enable_apicv = 0;
+
+	return 0;
+}
+
+static void hardware_unsetup(void)
+{
+}
+
+struct kvm_x86_nested_ops pvm_nested_ops = {};
+
+static struct kvm_x86_ops pvm_x86_ops __initdata = {
+	.name = KBUILD_MODNAME,
+
+	.check_processor_compatibility = pvm_check_processor_compat,
+
+	.hardware_unsetup = hardware_unsetup,
+	.hardware_enable = hardware_enable,
+	.hardware_disable = hardware_disable,
+	.has_emulated_msr = pvm_has_emulated_msr,
+
+	.has_wbinvd_exit = cpu_has_pvm_wbinvd_exit,
+
+	.nested_ops = &pvm_nested_ops,
+
+	.setup_mce = pvm_setup_mce,
+};
+
+static struct kvm_x86_init_ops pvm_init_ops __initdata = {
+	.hardware_setup = hardware_setup,
+
+	.runtime_ops = &pvm_x86_ops,
+};
+
 static void pvm_exit(void)
 {
+	kvm_exit();
+	kvm_x86_vendor_exit();
+	host_mmu_destroy();
+	allow_smaller_maxphyaddr = false;
+	kvm_cpuid_vendor_signature = 0;
 }
 module_exit(pvm_exit);

+static int __init hardware_cap_check(void)
+{
+	/*
+	 * switcher can't be used when KPTI. See the comments above
+	 * SWITCHER_SAVE_AND_SWITCH_TO_HOST_CR3
+	 */
+	if (boot_cpu_has(X86_FEATURE_PTI)) {
+		pr_warn("Support for host KPTI is not included yet.\n");
+		return -EOPNOTSUPP;
+	}
+	if (!boot_cpu_has(X86_FEATURE_FSGSBASE)) {
+		pr_warn("FSGSBASE is required per PVM specification.\n");
+		return -EOPNOTSUPP;
+	}
+	if (!boot_cpu_has(X86_FEATURE_RDTSCP)) {
+		pr_warn("RDTSCP is required to support for getcpu in guest vdso.\n");
+		return -EOPNOTSUPP;
+	}
+	if (!boot_cpu_has(X86_FEATURE_CX16)) {
+		pr_warn("CMPXCHG16B is required for guest.\n");
+		return -EOPNOTSUPP;
+	}
+
+	return 0;
+}
+
 static int __init pvm_init(void)
 {
+	int r;
+
+	r = hardware_cap_check();
+	if (r)
+		return r;
+
+	r = host_mmu_init();
+	if (r)
+		return r;
+
+	is_intel = boot_cpu_data.x86_vendor == X86_VENDOR_INTEL;
+
+	r = kvm_x86_vendor_init(&pvm_init_ops);
+	if (r)
+		goto exit_host_mmu;
+
+	r = kvm_init(sizeof(struct vcpu_pvm), __alignof__(struct vcpu_pvm), THIS_MODULE);
+	if (r)
+		goto exit_vendor;
+
+	allow_smaller_maxphyaddr = true;
+	kvm_cpuid_vendor_signature = PVM_CPUID_SIGNATURE;
+
 	return 0;
+
+exit_vendor:
+	kvm_x86_vendor_exit();
+exit_host_mmu:
+	host_mmu_destroy();
+	return r;
 }
 module_init(pvm_init);
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-18-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.h) --git a/arch/x86/kvm/pvm/pvm.h b/arch/x86/kvm/pvm/pvm.h
index 7a3732986a6d..6149cf5975a4 100644
--- a/arch/x86/kvm/pvm/pvm.h
+++ b/arch/x86/kvm/pvm/pvm.h @@ -2,6 +2,8 @@
 #ifndef __KVM_X86_PVM_H
 #define __KVM_X86_PVM_H

+#include <linux/kvm_host.h>
+
 #define PT_L4_SHIFT		39
 #define PT_L4_SIZE		(1UL << PT_L4_SHIFT)
 #define DEFAULT_RANGE_L4_SIZE	(32 * PT_L4_SIZE)
@@ -20,4 +22,22 @@ extern u64 *host_mmu_root_pgd;
 void host_mmu_destroy(void);
 int host_mmu_init(void);

+struct vcpu_pvm {
+	struct kvm_vcpu vcpu;
+};
+
+struct kvm_pvm {
+	struct kvm kvm;
+};
+
+static __always_inline struct kvm_pvm *to_kvm_pvm(struct kvm *kvm)
+{
+	return container_of(kvm, struct kvm_pvm, kvm);
+}
+
+static __always_inline struct vcpu_pvm *to_pvm(struct kvm_vcpu *vcpu)
+{
+	return container_of(vcpu, struct vcpu_pvm, vcpu);
+}
+
 #endif /* __KVM_X86_PVM_H */
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mcffc6d9b931c065a515cc5ebaa312b7cf6ed76ea) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-18-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-18-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-18-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-18-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-18-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-18-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rcffc6d9b931c065a515cc5ebaa312b7cf6ed76ea)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e999e9100dc7bf390c8b2892c071ccfe18cfc9c7c) **[RFC PATCH 18/73] KVM: x86/PVM: Implement VM/VCPU initialization related callbacks**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(16 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rcffc6d9b931c065a515cc5ebaa312b7cf6ed76ea)
  2024-02-26 14:35 ` [[RFC PATCH 17/73] KVM: x86/PVM: Implement module initialization related callbacks](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mcffc6d9b931c065a515cc5ebaa312b7cf6ed76ea) Lai Jiangshan
**@ 2024-02-26 14:35 ` Lai Jiangshan**
  2024-02-26 14:35 ` [[RFC PATCH 19/73] x86/entry: Export 32-bit ignore syscall entry and __ia32_enabled variable](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mb34ef91f220b438260491c4cf826cb0c72c41609) Lai Jiangshan
                   ` [(56 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rb34ef91f220b438260491c4cf826cb0c72c41609)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r999e9100dc7bf390c8b2892c071ccfe18cfc9c7c)
From: Lai Jiangshan @ 2024-02-26 14:35 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-19-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-19-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143537)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143537), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

In the vm_init() callback, the cloned host root page table is recorded
into the 'kvm' structure, allowing for the cloning of host PGD entries
during SP allocation. In the vcpu_create() callback, the pfn cache for
'PVCS' is initialized and deactivated in the vcpu_free() callback.
Additionally, the vcpu_reset() callback needs to perform a common x86
reset and specific PVM reset.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/kvm/pvm/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-19-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) | 120 +++++++++++++++++++++++++++++++++++++++++
 [arch/x86/kvm/pvm/pvm.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-19-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.h) |  34 ++++++++++++
 2 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e999e9100dc7bf390c8b2892c071ccfe18cfc9c7c), 154 insertions(+)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-19-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) --git a/arch/x86/kvm/pvm/pvm.c b/arch/x86/kvm/pvm/pvm.c
index 83aa2c9f42f6..d4cc52bf6b3f 100644
--- a/arch/x86/kvm/pvm/pvm.c
+++ b/arch/x86/kvm/pvm/pvm.c @@ -55,6 +55,117 @@ static bool cpu_has_pvm_wbinvd_exit(void)
 	return true;
 }

+static void reset_segment(struct kvm_segment *var, int seg)
+{
+	memset(var, 0, sizeof(*var));
+	var->limit = 0xffff;
+	var->present = 1;
+
+	switch (seg) {
+	case VCPU_SREG_CS:
+		var->s = 1;
+		var->type = 0xb; /* Code Segment */
+		var->selector = 0xf000;
+		var->base = 0xffff0000;
+		break;
+	case VCPU_SREG_LDTR:
+		var->s = 0;
+		var->type = DESC_LDT;
+		break;
+	case VCPU_SREG_TR:
+		var->s = 0;
+		var->type = DESC_TSS | 0x2; // TSS32 busy
+		break;
+	default:
+		var->s = 1;
+		var->type = 3; /* Read/Write Data Segment */
+		break;
+	}
+}
+
+static void __pvm_vcpu_reset(struct kvm_vcpu *vcpu)
+{
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+
+	if (is_intel)
+		vcpu->arch.microcode_version = 0x100000000ULL;
+	else
+		vcpu->arch.microcode_version = 0x01000065;
+
+	pvm->msr_ia32_feature_control_valid_bits = FEAT_CTL_LOCKED;
+}
+
+static void pvm_vcpu_reset(struct kvm_vcpu *vcpu, bool init_event)
+{
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+	int i;
+
+	kvm_gpc_deactivate(&pvm->pvcs_gpc);
+
+	if (!init_event)
+		__pvm_vcpu_reset(vcpu);
+
+	/*
+	 * For PVM, cpuid faulting relies on hardware capability, but it is set
+	 * as supported by default in kvm_arch_vcpu_create(). Therefore, it
+	 * should be cleared if the host doesn't support it.
+	 */
+	if (!boot_cpu_has(X86_FEATURE_CPUID_FAULT))
+		vcpu->arch.msr_platform_info &= ~MSR_PLATFORM_INFO_CPUID_FAULT;
+
+	// X86 resets
+	for (i = 0; i < ARRAY_SIZE(pvm->segments); i++)
+		reset_segment(&pvm->segments[i], i);
+	kvm_set_cr8(vcpu, 0);
+	pvm->idt_ptr.address = 0;
+	pvm->idt_ptr.size = 0xffff;
+	pvm->gdt_ptr.address = 0;
+	pvm->gdt_ptr.size = 0xffff;
+
+	// PVM resets
+	pvm->switch_flags = SWITCH_FLAGS_INIT;
+	pvm->hw_cs = __USER_CS;
+	pvm->hw_ss = __USER_DS;
+	pvm->int_shadow = 0;
+	pvm->nmi_mask = false;
+
+	pvm->msr_vcpu_struct = 0;
+	pvm->msr_supervisor_rsp = 0;
+	pvm->msr_event_entry = 0;
+	pvm->msr_retu_rip_plus2 = 0;
+	pvm->msr_rets_rip_plus2 = 0;
+	pvm->msr_switch_cr3 = 0;
+}
+
+static int pvm_vcpu_create(struct kvm_vcpu *vcpu)
+{
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+
+	BUILD_BUG_ON(offsetof(struct vcpu_pvm, vcpu) != 0);
+
+	pvm->switch_flags = SWITCH_FLAGS_INIT;
+	kvm_gpc_init(&pvm->pvcs_gpc, vcpu->kvm, vcpu, KVM_GUEST_AND_HOST_USE_PFN);
+
+	return 0;
+}
+
+static void pvm_vcpu_free(struct kvm_vcpu *vcpu)
+{
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+
+	kvm_gpc_deactivate(&pvm->pvcs_gpc);
+}
+
+static void pvm_vcpu_after_set_cpuid(struct kvm_vcpu *vcpu)
+{
+}
+
+static int pvm_vm_init(struct kvm *kvm)
+{
+	kvm->arch.host_mmu_root_pgd = host_mmu_root_pgd;
+	return 0;
+}
+
 static int hardware_enable(void)
 {
 	/* Nothing to do */
@@ -169,6 +280,15 @@ static struct kvm_x86_ops pvm_x86_ops __initdata = {

 	.has_wbinvd_exit = cpu_has_pvm_wbinvd_exit,

+	.vm_size = sizeof(struct kvm_pvm),
+	.vm_init = pvm_vm_init,
+
+	.vcpu_create = pvm_vcpu_create,
+	.vcpu_free = pvm_vcpu_free,
+	.vcpu_reset = pvm_vcpu_reset,
+
+	.vcpu_after_set_cpuid = pvm_vcpu_after_set_cpuid,
+
 	.nested_ops = &pvm_nested_ops,

 	.setup_mce = pvm_setup_mce,
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-19-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.h) --git a/arch/x86/kvm/pvm/pvm.h b/arch/x86/kvm/pvm/pvm.h
index 6149cf5975a4..599bbbb284dc 100644
--- a/arch/x86/kvm/pvm/pvm.h
+++ b/arch/x86/kvm/pvm/pvm.h @@ -3,6 +3,9 @@
 #define __KVM_X86_PVM_H

 #include <linux/kvm_host.h>
+#include <asm/switcher.h>
+
+#define SWITCH_FLAGS_INIT	(SWITCH_FLAGS_SMOD)

 #define PT_L4_SHIFT		39
 #define PT_L4_SIZE		(1UL << PT_L4_SHIFT)
@@ -24,6 +27,37 @@ int host_mmu_init(void);

 struct vcpu_pvm {
 	struct kvm_vcpu vcpu;
+
+	unsigned long switch_flags;
+
+	u32 hw_cs, hw_ss;
+
+	int int_shadow;
+	bool nmi_mask;
+
+	struct gfn_to_pfn_cache pvcs_gpc;
+
+	/*
+	 * Only bits masked by msr_ia32_feature_control_valid_bits can be set in
+	 * msr_ia32_feature_control. FEAT_CTL_LOCKED is always included
+	 * in msr_ia32_feature_control_valid_bits.
+	 */
+	u64 msr_ia32_feature_control;
+	u64 msr_ia32_feature_control_valid_bits;
+
+	// PVM paravirt MSRs
+	unsigned long msr_vcpu_struct;
+	unsigned long msr_supervisor_rsp;
+	unsigned long msr_supervisor_redzone;
+	unsigned long msr_event_entry;
+	unsigned long msr_retu_rip_plus2;
+	unsigned long msr_rets_rip_plus2;
+	unsigned long msr_switch_cr3;
+	unsigned long msr_linear_address_range;
+
+	struct kvm_segment segments[NR_VCPU_SREG];
+	struct desc_ptr idt_ptr;
+	struct desc_ptr gdt_ptr;
 };

 struct kvm_pvm {
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m999e9100dc7bf390c8b2892c071ccfe18cfc9c7c) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-19-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-19-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-19-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-19-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-19-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-19-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r999e9100dc7bf390c8b2892c071ccfe18cfc9c7c)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#eb34ef91f220b438260491c4cf826cb0c72c41609) **[RFC PATCH 19/73] x86/entry: Export 32-bit ignore syscall entry and __ia32_enabled variable**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(17 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r999e9100dc7bf390c8b2892c071ccfe18cfc9c7c)
  2024-02-26 14:35 ` [[RFC PATCH 18/73] KVM: x86/PVM: Implement VM/VCPU](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m999e9100dc7bf390c8b2892c071ccfe18cfc9c7c) " Lai Jiangshan
**@ 2024-02-26 14:35 ` Lai Jiangshan**
  2024-02-26 14:35 ` [[RFC PATCH 20/73] KVM: x86/PVM: Implement vcpu_load()/vcpu_put() related callbacks](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m977febae904fda3cf07f049403d3fd9a28dd7017) Lai Jiangshan
                   ` [(55 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r977febae904fda3cf07f049403d3fd9a28dd7017)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rb34ef91f220b438260491c4cf826cb0c72c41609)
From: Lai Jiangshan @ 2024-02-26 14:35 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-20-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-20-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143540)
  Cc: Hou Wenlong, Lai Jiangshan, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143540), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Andy Lutomirski, Dave Hansen, H. Peter Anvin

From: Hou Wenlong <houwenlong.hwl@antgroup.com>

For PVM hypervisor, it ignores 32-bit syscall for guest currenlty.
Therefore, export 32-bit ignore syscall entry and __ia32_enabled
variable for PVM module.

Suggested-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
---
 [arch/x86/entry/common.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-20-jiangshanlai::40gmail.com:1arch:x86:entry:common.c)   | 1 +
 [arch/x86/entry/entry_64.S](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-20-jiangshanlai::40gmail.com:1arch:x86:entry:entry_64.S) | 1 +
 2 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#eb34ef91f220b438260491c4cf826cb0c72c41609), 2 insertions(+)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-20-jiangshanlai::40gmail.com:1arch:x86:entry:common.c) --git a/arch/x86/entry/common.c b/arch/x86/entry/common.c
index 6356060caaf3..00ff701aa1be 100644
--- a/arch/x86/entry/common.c
+++ b/arch/x86/entry/common.c @@ -141,6 +141,7 @@ static __always_inline int syscall_32_enter(struct pt_regs *regs)

 #ifdef CONFIG_IA32_EMULATION
 bool __ia32_enabled __ro_after_init = !IS_ENABLED(CONFIG_IA32_EMULATION_DEFAULT_DISABLED);
+EXPORT_SYMBOL_GPL(__ia32_enabled);

 static int ia32_emulation_override_cmdline(char *arg)
 {
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-20-jiangshanlai::40gmail.com:1arch:x86:entry:entry_64.S) --git a/arch/x86/entry/entry_64.S b/arch/x86/entry/entry_64.S
index 65bfebebeab6..5b25ea4a16ae 100644
--- a/arch/x86/entry/entry_64.S
+++ b/arch/x86/entry/entry_64.S @@ -1527,6 +1527,7 @@ SYM_CODE_START(entry_SYSCALL32_ignore)
 	mov	$-ENOSYS, %eax
 	sysretl
 SYM_CODE_END(entry_SYSCALL32_ignore)
+EXPORT_SYMBOL_GPL(entry_SYSCALL32_ignore)

 .pushsection .text, "ax"
 	__FUNC_ALIGN
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mb34ef91f220b438260491c4cf826cb0c72c41609) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-20-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-20-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-20-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-20-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-20-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-20-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rb34ef91f220b438260491c4cf826cb0c72c41609)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e977febae904fda3cf07f049403d3fd9a28dd7017) **[RFC PATCH 20/73] KVM: x86/PVM: Implement vcpu_load()/vcpu_put() related callbacks**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(18 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rb34ef91f220b438260491c4cf826cb0c72c41609)
  2024-02-26 14:35 ` [[RFC PATCH 19/73] x86/entry: Export 32-bit ignore syscall entry and __ia32_enabled variable](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mb34ef91f220b438260491c4cf826cb0c72c41609) Lai Jiangshan
**@ 2024-02-26 14:35 ` Lai Jiangshan**
  2024-02-26 14:35 ` [[RFC PATCH 21/73] KVM: x86/PVM: Implement vcpu_run() callbacks](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m6a1a901f25c380d80ddb67622cbc167b148a8f94) Lai Jiangshan
                   ` [(54 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r6a1a901f25c380d80ddb67622cbc167b148a8f94)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r977febae904fda3cf07f049403d3fd9a28dd7017)
From: Lai Jiangshan @ 2024-02-26 14:35 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-21-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-21-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143544)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143544), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

When preparing to switch to the guest, some guest states that only
matter to userspace can be loaded ahead before VM enter. In PVM, guest
segment registers and user return MSRs are loaded into hardware at that
time. Since LDT and IO bitmap are not supported in PVM guests, they are
cleared as well. When preparing to switch to the host in vcpu_put(),
host states are restored.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/kvm/pvm/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-21-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) | 235 +++++++++++++++++++++++++++++++++++++++++
 [arch/x86/kvm/pvm/pvm.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-21-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.h) |   5 +
 2 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e977febae904fda3cf07f049403d3fd9a28dd7017), 240 insertions(+)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-21-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) --git a/arch/x86/kvm/pvm/pvm.c b/arch/x86/kvm/pvm/pvm.c
index d4cc52bf6b3f..52b3b47ffe42 100644
--- a/arch/x86/kvm/pvm/pvm.c
+++ b/arch/x86/kvm/pvm/pvm.c @@ -13,6 +13,8 @@

 #include <linux/module.h>

+#include <asm/gsseg.h>
+#include <asm/io_bitmap.h>
 #include <asm/pvm_para.h>

 #include "cpuid.h"
@@ -26,6 +28,211 @@ static bool __read_mostly is_intel;

 static unsigned long host_idt_base;

+static inline void __save_gs_base(struct vcpu_pvm *pvm)
+{
+	// switcher will do a real hw swapgs, so use hw MSR_KERNEL_GS_BASE
+	rdmsrl(MSR_KERNEL_GS_BASE, pvm->segments[VCPU_SREG_GS].base);
+}
+
+static inline void __load_gs_base(struct vcpu_pvm *pvm)
+{
+	// switcher will do a real hw swapgs, so use hw MSR_KERNEL_GS_BASE
+	wrmsrl(MSR_KERNEL_GS_BASE, pvm->segments[VCPU_SREG_GS].base);
+}
+
+static inline void __save_fs_base(struct vcpu_pvm *pvm)
+{
+	rdmsrl(MSR_FS_BASE, pvm->segments[VCPU_SREG_FS].base);
+}
+
+static inline void __load_fs_base(struct vcpu_pvm *pvm)
+{
+	wrmsrl(MSR_FS_BASE, pvm->segments[VCPU_SREG_FS].base);
+}
+
+/*
+ * Test whether DS, ES, FS and GS need to be reloaded.
+ *
+ * Reading them only returns the selectors, but writing them (if
+ * nonzero) loads the full descriptor from the GDT or LDT.
+ *
+ * We therefore need to write new values to the segment registers
+ * on every host-guest state switch unless both the new and old
+ * values are zero.
+ */
+static inline bool need_reload_sel(u16 sel1, u16 sel2)
+{
+	return unlikely(sel1 | sel2);
+}
+
+/*
+ * Save host DS/ES/FS/GS selector, FS base, and inactive GS base.
+ * And load guest DS/ES/FS/GS selector, FS base, and GS base.
+ *
+ * Note, when the guest state is loaded and it is in hypervisor, the guest
+ * GS base is loaded in the hardware MSR_KERNEL_GS_BASE which is loaded
+ * with host inactive GS base when the guest state is NOT loaded.
+ */
+static void segments_save_host_and_switch_to_guest(struct vcpu_pvm *pvm)
+{
+	u16 pvm_ds_sel, pvm_es_sel, pvm_fs_sel, pvm_gs_sel;
+
+	/* Save host segments */
+	savesegment(ds, pvm->host_ds_sel);
+	savesegment(es, pvm->host_es_sel);
+	current_save_fsgs();
+
+	/* Load guest segments */
+	pvm_ds_sel = pvm->segments[VCPU_SREG_DS].selector;
+	pvm_es_sel = pvm->segments[VCPU_SREG_ES].selector;
+	pvm_fs_sel = pvm->segments[VCPU_SREG_FS].selector;
+	pvm_gs_sel = pvm->segments[VCPU_SREG_GS].selector;
+
+	if (need_reload_sel(pvm_ds_sel, pvm->host_ds_sel))
+		loadsegment(ds, pvm_ds_sel);
+	if (need_reload_sel(pvm_es_sel, pvm->host_es_sel))
+		loadsegment(es, pvm_es_sel);
+	if (need_reload_sel(pvm_fs_sel, current->thread.fsindex))
+		loadsegment(fs, pvm_fs_sel);
+	if (need_reload_sel(pvm_gs_sel, current->thread.gsindex))
+		load_gs_index(pvm_gs_sel);
+
+	__load_gs_base(pvm);
+	__load_fs_base(pvm);
+}
+
+/*
+ * Save guest DS/ES/FS/GS selector, FS base, and GS base.
+ * And load host DS/ES/FS/GS selector, FS base, and inactive GS base.
+ */
+static void segments_save_guest_and_switch_to_host(struct vcpu_pvm *pvm)
+{
+	u16 pvm_ds_sel, pvm_es_sel, pvm_fs_sel, pvm_gs_sel;
+
+	/* Save guest segments */
+	savesegment(ds, pvm_ds_sel);
+	savesegment(es, pvm_es_sel);
+	savesegment(fs, pvm_fs_sel);
+	savesegment(gs, pvm_gs_sel);
+	pvm->segments[VCPU_SREG_DS].selector = pvm_ds_sel;
+	pvm->segments[VCPU_SREG_ES].selector = pvm_es_sel;
+	pvm->segments[VCPU_SREG_FS].selector = pvm_fs_sel;
+	pvm->segments[VCPU_SREG_GS].selector = pvm_gs_sel;
+
+	__save_fs_base(pvm);
+	__save_gs_base(pvm);
+
+	/* Load host segments */
+	if (need_reload_sel(pvm_ds_sel, pvm->host_ds_sel))
+		loadsegment(ds, pvm->host_ds_sel);
+	if (need_reload_sel(pvm_es_sel, pvm->host_es_sel))
+		loadsegment(es, pvm->host_es_sel);
+	if (need_reload_sel(pvm_fs_sel, current->thread.fsindex))
+		loadsegment(fs, current->thread.fsindex);
+	if (need_reload_sel(pvm_gs_sel, current->thread.gsindex))
+		load_gs_index(current->thread.gsindex);
+
+	wrmsrl(MSR_KERNEL_GS_BASE, current->thread.gsbase);
+	wrmsrl(MSR_FS_BASE, current->thread.fsbase);
+}
+
+static void pvm_prepare_switch_to_guest(struct kvm_vcpu *vcpu)
+{
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+
+	if (pvm->loaded_cpu_state)
+		return;
+
+	pvm->loaded_cpu_state = 1;
+
+#ifdef CONFIG_X86_IOPL_IOPERM
+	/*
+	 * PVM doesn't load guest I/O bitmap into hardware.  Invalidate I/O
+	 * bitmap if the current task is using it.  This prevents any possible
+	 * leakage of an active I/O bitmap to the guest and forces I/O
+	 * instructions in guest to be trapped and emulated.
+	 *
+	 * The I/O bitmap will be restored when the current task exits to
+	 * user mode in arch_exit_to_user_mode_prepare().
+	 */
+	if (test_thread_flag(TIF_IO_BITMAP))
+		native_tss_invalidate_io_bitmap();
+#endif
+
+#ifdef CONFIG_MODIFY_LDT_SYSCALL
+	/* PVM doesn't support LDT. */
+	if (unlikely(current->mm->context.ldt))
+		clear_LDT();
+#endif
+
+	segments_save_host_and_switch_to_guest(pvm);
+
+	kvm_set_user_return_msr(0, (u64)entry_SYSCALL_64_switcher, -1ull);
+	kvm_set_user_return_msr(1, pvm->msr_tsc_aux, -1ull);
+	if (ia32_enabled()) {
+		if (is_intel)
+			kvm_set_user_return_msr(2, GDT_ENTRY_INVALID_SEG, -1ull);
+		else
+			kvm_set_user_return_msr(2, (u64)entry_SYSCALL32_ignore, -1ull);
+	}
+}
+
+static void pvm_prepare_switch_to_host(struct vcpu_pvm *pvm)
+{
+	if (!pvm->loaded_cpu_state)
+		return;
+
+	++pvm->vcpu.stat.host_state_reload;
+
+#ifdef CONFIG_MODIFY_LDT_SYSCALL
+	if (unlikely(current->mm->context.ldt))
+		kvm_load_ldt(GDT_ENTRY_LDT*8);
+#endif
+
+	segments_save_guest_and_switch_to_host(pvm);
+	pvm->loaded_cpu_state = 0;
+}
+
+/*
+ * Set all hardware states back to host.
+ * Except user return MSRs.
+ */
+static void pvm_switch_to_host(struct vcpu_pvm *pvm)
+{
+	preempt_disable();
+	pvm_prepare_switch_to_host(pvm);
+	preempt_enable();
+}
+
+DEFINE_PER_CPU(struct vcpu_pvm *, active_pvm_vcpu);
+
+/*
+ * Switches to specified vcpu, until a matching vcpu_put(), but assumes
+ * vcpu mutex is already taken.
+ */
+static void pvm_vcpu_load(struct kvm_vcpu *vcpu, int cpu)
+{
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+
+	if (__this_cpu_read(active_pvm_vcpu) == pvm && vcpu->cpu == cpu)
+		return;
+
+	__this_cpu_write(active_pvm_vcpu, pvm);
+
+	indirect_branch_prediction_barrier();
+}
+
+static void pvm_vcpu_put(struct kvm_vcpu *vcpu)
+{
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+
+	pvm_prepare_switch_to_host(pvm);
+}
+
+static void pvm_sched_in(struct kvm_vcpu *vcpu, int cpu)
+{
+}
+
 static void pvm_setup_mce(struct kvm_vcpu *vcpu)
 {
 }
@@ -100,6 +307,8 @@ static void pvm_vcpu_reset(struct kvm_vcpu *vcpu, bool init_event)
 	struct vcpu_pvm *pvm = to_pvm(vcpu);
 	int i;

+	pvm_switch_to_host(pvm);
+
 	kvm_gpc_deactivate(&pvm->pvcs_gpc);

 	if (!init_event)
@@ -183,6 +392,24 @@ static int pvm_check_processor_compat(void)
 	return 0;
 }

+/*
+ * When in PVM mode, the hardware MSR_LSTAR is set to the entry point
+ * provided by the host entry code (switcher), and the
+ * hypervisor can also change the hardware MSR_TSC_AUX to emulate
+ * the guest MSR_TSC_AUX.
+ */
+static __init void pvm_setup_user_return_msrs(void)
+{
+	kvm_add_user_return_msr(MSR_LSTAR);
+	kvm_add_user_return_msr(MSR_TSC_AUX);
+	if (ia32_enabled()) {
+		if (is_intel)
+			kvm_add_user_return_msr(MSR_IA32_SYSENTER_CS);
+		else
+			kvm_add_user_return_msr(MSR_CSTAR);
+	}
+}
+
 static __init void pvm_set_cpu_caps(void)
 {
 	if (boot_cpu_has(X86_FEATURE_NX))
@@ -253,6 +480,8 @@ static __init int hardware_setup(void)
 	store_idt(&dt);
 	host_idt_base = dt.address;

+	pvm_setup_user_return_msrs();
+
 	pvm_set_cpu_caps();

 	kvm_configure_mmu(false, 0, 0, 0);
@@ -287,8 +516,14 @@ static struct kvm_x86_ops pvm_x86_ops __initdata = {
 	.vcpu_free = pvm_vcpu_free,
 	.vcpu_reset = pvm_vcpu_reset,

+	.prepare_switch_to_guest = pvm_prepare_switch_to_guest,
+	.vcpu_load = pvm_vcpu_load,
+	.vcpu_put = pvm_vcpu_put,
+
 	.vcpu_after_set_cpuid = pvm_vcpu_after_set_cpuid,

+	.sched_in = pvm_sched_in,
+
 	.nested_ops = &pvm_nested_ops,

 	.setup_mce = pvm_setup_mce,
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-21-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.h) --git a/arch/x86/kvm/pvm/pvm.h b/arch/x86/kvm/pvm/pvm.h
index 599bbbb284dc..6584314487bc 100644
--- a/arch/x86/kvm/pvm/pvm.h
+++ b/arch/x86/kvm/pvm/pvm.h @@ -30,13 +30,18 @@ struct vcpu_pvm {

 	unsigned long switch_flags;

+	u16 host_ds_sel, host_es_sel;
+
 	u32 hw_cs, hw_ss;

+	int loaded_cpu_state;
 	int int_shadow;
 	bool nmi_mask;

 	struct gfn_to_pfn_cache pvcs_gpc;

+	// emulated x86 msrs
+	u64 msr_tsc_aux;
 	/*
 	 * Only bits masked by msr_ia32_feature_control_valid_bits can be set in
 	 * msr_ia32_feature_control. FEAT_CTL_LOCKED is always included
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m977febae904fda3cf07f049403d3fd9a28dd7017) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-21-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-21-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-21-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-21-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-21-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-21-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r977febae904fda3cf07f049403d3fd9a28dd7017)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e6a1a901f25c380d80ddb67622cbc167b148a8f94) **[RFC PATCH 21/73] KVM: x86/PVM: Implement vcpu_run() callbacks**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(19 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r977febae904fda3cf07f049403d3fd9a28dd7017)
  2024-02-26 14:35 ` [[RFC PATCH 20/73] KVM: x86/PVM: Implement vcpu_load()/vcpu_put() related callbacks](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m977febae904fda3cf07f049403d3fd9a28dd7017) Lai Jiangshan
**@ 2024-02-26 14:35 ` Lai Jiangshan**
  2024-02-26 14:35 ` [[RFC PATCH 22/73] KVM: x86/PVM: Handle some VM exits before enable interrupts](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m7008898d987993cb9ff2bb727d36d62a5bd13fd7) Lai Jiangshan
                   ` [(53 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r7008898d987993cb9ff2bb727d36d62a5bd13fd7)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r6a1a901f25c380d80ddb67622cbc167b148a8f94)
From: Lai Jiangshan @ 2024-02-26 14:35 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-22-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-22-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143547)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143547), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

In the vcpu_run() callback, the hypervisor needs to prepare for VM enter
in the switcher and record exit reasons after VM exit. The guest
registers are prepared on the host SP0 stack, and the guest/host
hardware CR3 is saved in the TSS for the switcher before VM enter.
Additionally, the guest xsave state is loaded into hardware before VM
enter. After VM exit, the guest registers are saved from the entry
stack, and host xsave states are restored.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/kvm/pvm/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-22-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) | 163 +++++++++++++++++++++++++++++++++++++++++
 [arch/x86/kvm/pvm/pvm.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-22-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.h) |   5 ++
 2 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e6a1a901f25c380d80ddb67622cbc167b148a8f94), 168 insertions(+)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-22-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) --git a/arch/x86/kvm/pvm/pvm.c b/arch/x86/kvm/pvm/pvm.c
index 52b3b47ffe42..00a50ed0c118 100644
--- a/arch/x86/kvm/pvm/pvm.c
+++ b/arch/x86/kvm/pvm/pvm.c @@ -16,8 +16,11 @@
 #include <asm/gsseg.h>
 #include <asm/io_bitmap.h>
 #include <asm/pvm_para.h>
+#include <asm/mmu_context.h>

 #include "cpuid.h"
+#include "lapic.h"
+#include "trace.h"
 #include "x86.h"
 #include "pvm.h"

@@ -204,6 +207,31 @@ static void pvm_switch_to_host(struct vcpu_pvm *pvm)
 	preempt_enable();
 }

+static void pvm_set_host_cr3_for_hypervisor(struct vcpu_pvm *pvm)
+{
+	unsigned long cr3;
+
+	if (static_cpu_has(X86_FEATURE_PCID))
+		cr3 = __get_current_cr3_fast() | X86_CR3_PCID_NOFLUSH;
+	else
+		cr3 = __get_current_cr3_fast();
+	this_cpu_write(cpu_tss_rw.tss_ex.host_cr3, cr3);
+}
+
+// Set tss_ex.host_cr3 for VMExit.
+// Set tss_ex.enter_cr3 for VMEnter.
+static void pvm_set_host_cr3(struct vcpu_pvm *pvm)
+{
+	pvm_set_host_cr3_for_hypervisor(pvm);
+	this_cpu_write(cpu_tss_rw.tss_ex.enter_cr3, pvm->vcpu.arch.mmu->root.hpa);
+}
+
+static void pvm_load_mmu_pgd(struct kvm_vcpu *vcpu, hpa_t root_hpa,
+			     int root_level)
+{
+	/* Nothing to do. Guest cr3 will be prepared in pvm_set_host_cr3(). */
+}
+
 DEFINE_PER_CPU(struct vcpu_pvm *, active_pvm_vcpu);

 /*
@@ -262,6 +290,136 @@ static bool cpu_has_pvm_wbinvd_exit(void)
 	return true;
 }

+static int pvm_vcpu_pre_run(struct kvm_vcpu *vcpu)
+{
+	return 1;
+}
+
+// Save guest registers from host sp0 or IST stack.
+static __always_inline void save_regs(struct kvm_vcpu *vcpu, struct pt_regs *guest)
+{
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+
+	vcpu->arch.regs[VCPU_REGS_RAX] = guest->ax;
+	vcpu->arch.regs[VCPU_REGS_RCX] = guest->cx;
+	vcpu->arch.regs[VCPU_REGS_RDX] = guest->dx;
+	vcpu->arch.regs[VCPU_REGS_RBX] = guest->bx;
+	vcpu->arch.regs[VCPU_REGS_RSP] = guest->sp;
+	vcpu->arch.regs[VCPU_REGS_RBP] = guest->bp;
+	vcpu->arch.regs[VCPU_REGS_RSI] = guest->si;
+	vcpu->arch.regs[VCPU_REGS_RDI] = guest->di;
+	vcpu->arch.regs[VCPU_REGS_R8] = guest->r8;
+	vcpu->arch.regs[VCPU_REGS_R9] = guest->r9;
+	vcpu->arch.regs[VCPU_REGS_R10] = guest->r10;
+	vcpu->arch.regs[VCPU_REGS_R11] = guest->r11;
+	vcpu->arch.regs[VCPU_REGS_R12] = guest->r12;
+	vcpu->arch.regs[VCPU_REGS_R13] = guest->r13;
+	vcpu->arch.regs[VCPU_REGS_R14] = guest->r14;
+	vcpu->arch.regs[VCPU_REGS_R15] = guest->r15;
+	vcpu->arch.regs[VCPU_REGS_RIP] = guest->ip;
+	pvm->rflags = guest->flags;
+	pvm->hw_cs = guest->cs;
+	pvm->hw_ss = guest->ss;
+}
+
+// load guest registers to host sp0 stack.
+static __always_inline void load_regs(struct kvm_vcpu *vcpu, struct pt_regs *guest)
+{
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+
+	guest->ss = pvm->hw_ss;
+	guest->sp = vcpu->arch.regs[VCPU_REGS_RSP];
+	guest->flags = (pvm->rflags & SWITCH_ENTER_EFLAGS_ALLOWED) | SWITCH_ENTER_EFLAGS_FIXED;
+	guest->cs = pvm->hw_cs;
+	guest->ip = vcpu->arch.regs[VCPU_REGS_RIP];
+	guest->orig_ax = -1;
+	guest->di = vcpu->arch.regs[VCPU_REGS_RDI];
+	guest->si = vcpu->arch.regs[VCPU_REGS_RSI];
+	guest->dx = vcpu->arch.regs[VCPU_REGS_RDX];
+	guest->cx = vcpu->arch.regs[VCPU_REGS_RCX];
+	guest->ax = vcpu->arch.regs[VCPU_REGS_RAX];
+	guest->r8 = vcpu->arch.regs[VCPU_REGS_R8];
+	guest->r9 = vcpu->arch.regs[VCPU_REGS_R9];
+	guest->r10 = vcpu->arch.regs[VCPU_REGS_R10];
+	guest->r11 = vcpu->arch.regs[VCPU_REGS_R11];
+	guest->bx = vcpu->arch.regs[VCPU_REGS_RBX];
+	guest->bp = vcpu->arch.regs[VCPU_REGS_RBP];
+	guest->r12 = vcpu->arch.regs[VCPU_REGS_R12];
+	guest->r13 = vcpu->arch.regs[VCPU_REGS_R13];
+	guest->r14 = vcpu->arch.regs[VCPU_REGS_R14];
+	guest->r15 = vcpu->arch.regs[VCPU_REGS_R15];
+}
+
+static noinstr void pvm_vcpu_run_noinstr(struct kvm_vcpu *vcpu)
+{
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+	struct pt_regs *sp0_regs = (struct pt_regs *)this_cpu_read(cpu_tss_rw.x86_tss.sp0) - 1;
+	struct pt_regs *ret_regs;
+
+	guest_state_enter_irqoff();
+
+	// Load guest registers into the host sp0 stack for switcher.
+	load_regs(vcpu, sp0_regs);
+
+	// Call into switcher and enter guest.
+	ret_regs = switcher_enter_guest();
+
+	// Get the guest registers from the host sp0 stack.
+	save_regs(vcpu, ret_regs);
+	pvm->exit_vector = (ret_regs->orig_ax >> 32);
+	pvm->exit_error_code = (u32)ret_regs->orig_ax;
+
+	guest_state_exit_irqoff();
+}
+
+/*
+ * PVM wrappers for kvm_load_{guest|host}_xsave_state().
+ *
+ * Currently PKU is disabled for shadowpaging and to avoid overhead,
+ * host CR4.PKE is unchanged for entering/exiting guest even when
+ * host CR4.PKE is enabled.
+ *
+ * These wrappers fix pkru when host CR4.PKE is enabled.
+ */
+static inline void pvm_load_guest_xsave_state(struct kvm_vcpu *vcpu)
+{
+	kvm_load_guest_xsave_state(vcpu);
+
+	if (cpu_feature_enabled(X86_FEATURE_PKU)) {
+		if (vcpu->arch.host_pkru)
+			write_pkru(0);
+	}
+}
+
+static inline void pvm_load_host_xsave_state(struct kvm_vcpu *vcpu)
+{
+	kvm_load_host_xsave_state(vcpu);
+
+	if (cpu_feature_enabled(X86_FEATURE_PKU)) {
+		if (rdpkru() != vcpu->arch.host_pkru)
+			write_pkru(vcpu->arch.host_pkru);
+	}
+}
+
+static fastpath_t pvm_vcpu_run(struct kvm_vcpu *vcpu)
+{
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+
+	trace_kvm_entry(vcpu);
+
+	pvm_load_guest_xsave_state(vcpu);
+
+	kvm_wait_lapic_expire(vcpu);
+
+	pvm_set_host_cr3(pvm);
+
+	pvm_vcpu_run_noinstr(vcpu);
+
+	pvm_load_host_xsave_state(vcpu);
+
+	return EXIT_FASTPATH_NONE;
+}
+
 static void reset_segment(struct kvm_segment *var, int seg)
 {
 	memset(var, 0, sizeof(*var));
@@ -520,6 +678,11 @@ static struct kvm_x86_ops pvm_x86_ops __initdata = {
 	.vcpu_load = pvm_vcpu_load,
 	.vcpu_put = pvm_vcpu_put,

+	.load_mmu_pgd = pvm_load_mmu_pgd,
+
+	.vcpu_pre_run = pvm_vcpu_pre_run,
+	.vcpu_run = pvm_vcpu_run,
+
 	.vcpu_after_set_cpuid = pvm_vcpu_after_set_cpuid,

 	.sched_in = pvm_sched_in,
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-22-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.h) --git a/arch/x86/kvm/pvm/pvm.h b/arch/x86/kvm/pvm/pvm.h
index 6584314487bc..349f4eac98ec 100644
--- a/arch/x86/kvm/pvm/pvm.h
+++ b/arch/x86/kvm/pvm/pvm.h @@ -28,10 +28,15 @@ int host_mmu_init(void);
 struct vcpu_pvm {
 	struct kvm_vcpu vcpu;

+	// guest rflags, turned into hw rflags when in switcher
+	unsigned long rflags;
+
 	unsigned long switch_flags;

 	u16 host_ds_sel, host_es_sel;

+	u32 exit_vector;
+	u32 exit_error_code;
 	u32 hw_cs, hw_ss;

 	int loaded_cpu_state;
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m6a1a901f25c380d80ddb67622cbc167b148a8f94) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-22-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-22-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-22-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-22-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-22-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-22-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r6a1a901f25c380d80ddb67622cbc167b148a8f94)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e7008898d987993cb9ff2bb727d36d62a5bd13fd7) **[RFC PATCH 22/73] KVM: x86/PVM: Handle some VM exits before enable interrupts**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(20 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r6a1a901f25c380d80ddb67622cbc167b148a8f94)
  2024-02-26 14:35 ` [[RFC PATCH 21/73] KVM: x86/PVM: Implement vcpu_run() callbacks](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m6a1a901f25c380d80ddb67622cbc167b148a8f94) Lai Jiangshan
**@ 2024-02-26 14:35 ` Lai Jiangshan**
  2024-02-26 14:35 ` [[RFC PATCH 23/73] KVM: x86/PVM: Handle event handling related MSR read/write operation](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5cb5213fee46dd2beab66f058b8ddbe3bb7cd3d3) Lai Jiangshan
                   ` [(52 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r5cb5213fee46dd2beab66f058b8ddbe3bb7cd3d3)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r7008898d987993cb9ff2bb727d36d62a5bd13fd7)
From: Lai Jiangshan @ 2024-02-26 14:35 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-23-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-23-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143550)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143550), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

Similar to VMX, NMI should be handled in non-instrumented code early
after VM exit. Additionally, #PF, #VE, #VC, and #DB need early handling
in non-instrumented code as well. Host interrupts and #MC need to be
handled before enabling interrupts.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/kvm/pvm/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-23-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) | 89 ++++++++++++++++++++++++++++++++++++++++++
 [arch/x86/kvm/pvm/pvm.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-23-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.h) |  8 ++++
 2 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e7008898d987993cb9ff2bb727d36d62a5bd13fd7), 97 insertions(+)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-23-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) --git a/arch/x86/kvm/pvm/pvm.c b/arch/x86/kvm/pvm/pvm.c
index 00a50ed0c118..29c6d8da7c19 100644
--- a/arch/x86/kvm/pvm/pvm.c
+++ b/arch/x86/kvm/pvm/pvm.c @@ -265,6 +265,58 @@ static void pvm_setup_mce(struct kvm_vcpu *vcpu)
 {
 }

+static int handle_exit_external_interrupt(struct kvm_vcpu *vcpu)
+{
+	++vcpu->stat.irq_exits;
+	return 1;
+}
+
+static int handle_exit_failed_vmentry(struct kvm_vcpu *vcpu)
+{
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+	u32 error_code = pvm->exit_error_code;
+
+	kvm_queue_exception_e(vcpu, GP_VECTOR, error_code);
+	return 1;
+}
+
+/*
+ * The guest has exited.  See if we can fix it or if we need userspace
+ * assistance.
+ */
+static int pvm_handle_exit(struct kvm_vcpu *vcpu, fastpath_t exit_fastpath)
+{
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+	u32 exit_reason = pvm->exit_vector;
+
+	if (exit_reason >= FIRST_EXTERNAL_VECTOR && exit_reason < NR_VECTORS)
+		return handle_exit_external_interrupt(vcpu);
+	else if (exit_reason == PVM_FAILED_VMENTRY_VECTOR)
+		return handle_exit_failed_vmentry(vcpu);
+
+	vcpu_unimpl(vcpu, "pvm: unexpected exit reason 0x%x\n", exit_reason);
+	vcpu->run->exit_reason = KVM_EXIT_INTERNAL_ERROR;
+	vcpu->run->internal.suberror =
+		KVM_INTERNAL_ERROR_UNEXPECTED_EXIT_REASON;
+	vcpu->run->internal.ndata = 2;
+	vcpu->run->internal.data[0] = exit_reason;
+	vcpu->run->internal.data[1] = vcpu->arch.last_vmentry_cpu;
+	return 0;
+}
+
+static void pvm_handle_exit_irqoff(struct kvm_vcpu *vcpu)
+{
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+	u32 vector = pvm->exit_vector;
+	gate_desc *desc = (gate_desc *)host_idt_base + vector;
+
+	if (vector >= FIRST_EXTERNAL_VECTOR && vector < NR_VECTORS &&
+	    vector != IA32_SYSCALL_VECTOR)
+		kvm_do_interrupt_irqoff(vcpu, gate_offset(desc));
+	else if (vector == MC_VECTOR)
+		kvm_machine_check();
+}
+
 static bool pvm_has_emulated_msr(struct kvm *kvm, u32 index)
 {
 	switch (index) {
@@ -369,6 +421,40 @@ static noinstr void pvm_vcpu_run_noinstr(struct kvm_vcpu *vcpu)
 	pvm->exit_vector = (ret_regs->orig_ax >> 32);
 	pvm->exit_error_code = (u32)ret_regs->orig_ax;

+	// handle noinstr vmexits reasons.
+	switch (pvm->exit_vector) {
+	case PF_VECTOR:
+		// if the exit due to #PF, check for async #PF.
+		pvm->exit_cr2 = read_cr2();
+		vcpu->arch.apf.host_apf_flags = kvm_read_and_reset_apf_flags();
+		break;
+	case NMI_VECTOR:
+		kvm_do_nmi_irqoff(vcpu);
+		break;
+	case VE_VECTOR:
+		// TODO: pvm host is TDX guest.
+		// tdx_get_ve_info(&pvm->host_ve);
+		break;
+	case X86_TRAP_VC:
+		/*
+		 * TODO: pvm host is SEV guest.
+		 * if (!vc_is_db(error_code)) {
+		 *      collect info and handle the first part for #VC
+		 *      break;
+		 * } else {
+		 *      get_debugreg(pvm->exit_dr6, 6);
+		 *      set_debugreg(DR6_RESERVED, 6);
+		 * }
+		 */
+		break;
+	case DB_VECTOR:
+		get_debugreg(pvm->exit_dr6, 6);
+		set_debugreg(DR6_RESERVED, 6);
+		break;
+	default:
+		break;
+	}
+
 	guest_state_exit_irqoff();
 }

@@ -682,9 +768,12 @@ static struct kvm_x86_ops pvm_x86_ops __initdata = {

 	.vcpu_pre_run = pvm_vcpu_pre_run,
 	.vcpu_run = pvm_vcpu_run,
+	.handle_exit = pvm_handle_exit,

 	.vcpu_after_set_cpuid = pvm_vcpu_after_set_cpuid,

+	.handle_exit_irqoff = pvm_handle_exit_irqoff,
+
 	.sched_in = pvm_sched_in,

 	.nested_ops = &pvm_nested_ops,
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-23-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.h) --git a/arch/x86/kvm/pvm/pvm.h b/arch/x86/kvm/pvm/pvm.h
index 349f4eac98ec..123cfe1c3c6a 100644
--- a/arch/x86/kvm/pvm/pvm.h
+++ b/arch/x86/kvm/pvm/pvm.h @@ -7,6 +7,8 @@

 #define SWITCH_FLAGS_INIT	(SWITCH_FLAGS_SMOD)

+#define PVM_FAILED_VMENTRY_VECTOR	SWITCH_EXIT_REASONS_FAILED_VMETNRY
+
 #define PT_L4_SHIFT		39
 #define PT_L4_SIZE		(1UL << PT_L4_SHIFT)
 #define DEFAULT_RANGE_L4_SIZE	(32 * PT_L4_SIZE)
@@ -35,6 +37,12 @@ struct vcpu_pvm {

 	u16 host_ds_sel, host_es_sel;

+	union {
+		unsigned long exit_extra;
+		unsigned long exit_cr2;
+		unsigned long exit_dr6;
+		struct ve_info exit_ve;
+	};
 	u32 exit_vector;
 	u32 exit_error_code;
 	u32 hw_cs, hw_ss;
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m7008898d987993cb9ff2bb727d36d62a5bd13fd7) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-23-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-23-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-23-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-23-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-23-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-23-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r7008898d987993cb9ff2bb727d36d62a5bd13fd7)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e5cb5213fee46dd2beab66f058b8ddbe3bb7cd3d3) **[RFC PATCH 23/73] KVM: x86/PVM: Handle event handling related MSR read/write operation**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(21 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r7008898d987993cb9ff2bb727d36d62a5bd13fd7)
  2024-02-26 14:35 ` [[RFC PATCH 22/73] KVM: x86/PVM: Handle some VM exits before enable interrupts](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m7008898d987993cb9ff2bb727d36d62a5bd13fd7) Lai Jiangshan
**@ 2024-02-26 14:35 ` Lai Jiangshan**
  2024-02-26 14:35 ` [[RFC PATCH 24/73] KVM: x86/PVM: Introduce PVM mode switching](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m11884219130ef982a74482ba96cdb82e086660b5) Lai Jiangshan
                   ` [(51 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r11884219130ef982a74482ba96cdb82e086660b5)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r5cb5213fee46dd2beab66f058b8ddbe3bb7cd3d3)
From: Lai Jiangshan @ 2024-02-26 14:35 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-24-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-24-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143554)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143554), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

In the PVM event handling specification, the guest needs to register the
event entry into the associated MSRs before delivering the event.
Therefore, handling them in the get_msr()/set_msr() callbacks is
necessary to prepare for event delivery later. Additionally, the user
mode syscall event still uses the original syscall event entry, but only
MSR_LSTAR is used; other MSRs are ignored.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/kvm/pvm/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-24-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) | 188 +++++++++++++++++++++++++++++++++++++++++
 [arch/x86/kvm/pvm/pvm.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-24-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.h) |   7 ++
 2 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e5cb5213fee46dd2beab66f058b8ddbe3bb7cd3d3), 195 insertions(+)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-24-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) --git a/arch/x86/kvm/pvm/pvm.c b/arch/x86/kvm/pvm/pvm.c
index 29c6d8da7c19..69f8fbbb6176 100644
--- a/arch/x86/kvm/pvm/pvm.c
+++ b/arch/x86/kvm/pvm/pvm.c @@ -31,6 +31,33 @@ static bool __read_mostly is_intel;

 static unsigned long host_idt_base;

+static inline u16 kernel_cs_by_msr(u64 msr_star)
+{
+	// [47..32]
+	// and force rpl=0
+	return ((msr_star >> 32) & ~0x3);
+}
+
+static inline u16 kernel_ds_by_msr(u64 msr_star)
+{
+	// [47..32] + 8
+	// and force rpl=0
+	return ((msr_star >> 32) & ~0x3) + 8;
+}
+
+static inline u16 user_cs32_by_msr(u64 msr_star)
+{
+	// [63..48] is user_cs32 and force rpl=3
+	return ((msr_star >> 48) | 0x3);
+}
+
+static inline u16 user_cs_by_msr(u64 msr_star)
+{
+	// [63..48] is user_cs32, and [63..48] + 16 is user_cs
+	// and force rpl=3
+	return ((msr_star >> 48) | 0x3) + 16;
+}
+
 static inline void __save_gs_base(struct vcpu_pvm *pvm)
 {
 	// switcher will do a real hw swapgs, so use hw MSR_KERNEL_GS_BASE
@@ -261,6 +288,161 @@ static void pvm_sched_in(struct kvm_vcpu *vcpu, int cpu)
 {
 }

+static int pvm_get_msr_feature(struct kvm_msr_entry *msr)
+{
+	return 1;
+}
+
+static void pvm_msr_filter_changed(struct kvm_vcpu *vcpu)
+{
+	/* Accesses to MSRs are emulated in hypervisor, nothing to do here. */
+}
+
+/*
+ * Reads an msr value (of 'msr_index') into 'msr_info'.
+ * Returns 0 on success, non-0 otherwise.
+ * Assumes vcpu_load() was already called.
+ */
+static int pvm_get_msr(struct kvm_vcpu *vcpu, struct msr_data *msr_info)
+{
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+	int ret = 0;
+
+	switch (msr_info->index) {
+	case MSR_STAR:
+		msr_info->data = pvm->msr_star;
+		break;
+	case MSR_LSTAR:
+		msr_info->data = pvm->msr_lstar;
+		break;
+	case MSR_SYSCALL_MASK:
+		msr_info->data = pvm->msr_syscall_mask;
+		break;
+	case MSR_CSTAR:
+		msr_info->data = pvm->unused_MSR_CSTAR;
+		break;
+	/*
+	 * Since SYSENTER is not supported for the guest, we return a bad
+	 * segment to the emulator when emulating the instruction for #GP.
+	 */
+	case MSR_IA32_SYSENTER_CS:
+		msr_info->data = GDT_ENTRY_INVALID_SEG;
+		break;
+	case MSR_IA32_SYSENTER_EIP:
+		msr_info->data = pvm->unused_MSR_IA32_SYSENTER_EIP;
+		break;
+	case MSR_IA32_SYSENTER_ESP:
+		msr_info->data = pvm->unused_MSR_IA32_SYSENTER_ESP;
+		break;
+	case MSR_PVM_VCPU_STRUCT:
+		msr_info->data = pvm->msr_vcpu_struct;
+		break;
+	case MSR_PVM_SUPERVISOR_RSP:
+		msr_info->data = pvm->msr_supervisor_rsp;
+		break;
+	case MSR_PVM_SUPERVISOR_REDZONE:
+		msr_info->data = pvm->msr_supervisor_redzone;
+		break;
+	case MSR_PVM_EVENT_ENTRY:
+		msr_info->data = pvm->msr_event_entry;
+		break;
+	case MSR_PVM_RETU_RIP:
+		msr_info->data = pvm->msr_retu_rip_plus2 - 2;
+		break;
+	case MSR_PVM_RETS_RIP:
+		msr_info->data = pvm->msr_rets_rip_plus2 - 2;
+		break;
+	default:
+		ret = kvm_get_msr_common(vcpu, msr_info);
+	}
+
+	return ret;
+}
+
+/*
+ * Writes msr value into the appropriate "register".
+ * Returns 0 on success, non-0 otherwise.
+ * Assumes vcpu_load() was already called.
+ */
+static int pvm_set_msr(struct kvm_vcpu *vcpu, struct msr_data *msr_info)
+{
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+	int ret = 0;
+	u32 msr_index = msr_info->index;
+	u64 data = msr_info->data;
+
+	switch (msr_index) {
+	case MSR_STAR:
+		/*
+		 * Guest KERNEL_CS/DS shouldn't be NULL and guest USER_CS/DS
+		 * must be the same as the host USER_CS/DS.
+		 */
+		if (!msr_info->host_initiated) {
+			if (!kernel_cs_by_msr(data))
+				return 1;
+			if (user_cs_by_msr(data) != __USER_CS)
+				return 1;
+		}
+		pvm->msr_star = data;
+		break;
+	case MSR_LSTAR:
+		if (is_noncanonical_address(msr_info->data, vcpu))
+			return 1;
+		pvm->msr_lstar = data;
+		break;
+	case MSR_SYSCALL_MASK:
+		pvm->msr_syscall_mask = data;
+		break;
+	case MSR_CSTAR:
+		pvm->unused_MSR_CSTAR = data;
+		break;
+	case MSR_IA32_SYSENTER_CS:
+		pvm->unused_MSR_IA32_SYSENTER_CS = data;
+		break;
+	case MSR_IA32_SYSENTER_EIP:
+		pvm->unused_MSR_IA32_SYSENTER_EIP = data;
+		break;
+	case MSR_IA32_SYSENTER_ESP:
+		pvm->unused_MSR_IA32_SYSENTER_ESP = data;
+		break;
+	case MSR_PVM_VCPU_STRUCT:
+		if (!PAGE_ALIGNED(data))
+			return 1;
+		if (!data)
+			kvm_gpc_deactivate(&pvm->pvcs_gpc);
+		else if (kvm_gpc_activate(&pvm->pvcs_gpc, data, PAGE_SIZE))
+			return 1;
+
+		pvm->msr_vcpu_struct = data;
+		break;
+	case MSR_PVM_SUPERVISOR_RSP:
+		pvm->msr_supervisor_rsp = msr_info->data;
+		break;
+	case MSR_PVM_SUPERVISOR_REDZONE:
+		pvm->msr_supervisor_redzone = msr_info->data;
+		break;
+	case MSR_PVM_EVENT_ENTRY:
+		if (is_noncanonical_address(data, vcpu) ||
+		    is_noncanonical_address(data + 256, vcpu) ||
+		    is_noncanonical_address(data + 512, vcpu)) {
+			kvm_make_request(KVM_REQ_TRIPLE_FAULT, vcpu);
+			return 1;
+		}
+		pvm->msr_event_entry = msr_info->data;
+		break;
+	case MSR_PVM_RETU_RIP:
+		pvm->msr_retu_rip_plus2 = msr_info->data + 2;
+		break;
+	case MSR_PVM_RETS_RIP:
+		pvm->msr_rets_rip_plus2 = msr_info->data + 2;
+		break;
+	default:
+		ret = kvm_set_msr_common(vcpu, msr_info);
+	}
+
+	return ret;
+}
+
 static void pvm_setup_mce(struct kvm_vcpu *vcpu)
 {
 }
@@ -764,6 +946,9 @@ static struct kvm_x86_ops pvm_x86_ops __initdata = {
 	.vcpu_load = pvm_vcpu_load,
 	.vcpu_put = pvm_vcpu_put,

+	.get_msr_feature = pvm_get_msr_feature,
+	.get_msr = pvm_get_msr,
+	.set_msr = pvm_set_msr,
 	.load_mmu_pgd = pvm_load_mmu_pgd,

 	.vcpu_pre_run = pvm_vcpu_pre_run,
@@ -779,6 +964,9 @@ static struct kvm_x86_ops pvm_x86_ops __initdata = {
 	.nested_ops = &pvm_nested_ops,

 	.setup_mce = pvm_setup_mce,
+
+	.msr_filter_changed = pvm_msr_filter_changed,
+	.complete_emulated_msr = kvm_complete_insn_gp,
 };

 static struct kvm_x86_init_ops pvm_init_ops __initdata = {
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-24-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.h) --git a/arch/x86/kvm/pvm/pvm.h b/arch/x86/kvm/pvm/pvm.h
index 123cfe1c3c6a..57ca2e901e0d 100644
--- a/arch/x86/kvm/pvm/pvm.h
+++ b/arch/x86/kvm/pvm/pvm.h @@ -54,6 +54,13 @@ struct vcpu_pvm {
 	struct gfn_to_pfn_cache pvcs_gpc;

 	// emulated x86 msrs
+	u64 msr_lstar;
+	u64 msr_syscall_mask;
+	u64 msr_star;
+	u64 unused_MSR_CSTAR;
+	u64 unused_MSR_IA32_SYSENTER_CS;
+	u64 unused_MSR_IA32_SYSENTER_EIP;
+	u64 unused_MSR_IA32_SYSENTER_ESP;
 	u64 msr_tsc_aux;
 	/*
 	 * Only bits masked by msr_ia32_feature_control_valid_bits can be set in
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5cb5213fee46dd2beab66f058b8ddbe3bb7cd3d3) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-24-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-24-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-24-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-24-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-24-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-24-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r5cb5213fee46dd2beab66f058b8ddbe3bb7cd3d3)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e11884219130ef982a74482ba96cdb82e086660b5) **[RFC PATCH 24/73] KVM: x86/PVM: Introduce PVM mode switching**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(22 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r5cb5213fee46dd2beab66f058b8ddbe3bb7cd3d3)
  2024-02-26 14:35 ` [[RFC PATCH 23/73] KVM: x86/PVM: Handle event handling related MSR read/write operation](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5cb5213fee46dd2beab66f058b8ddbe3bb7cd3d3) Lai Jiangshan
**@ 2024-02-26 14:35 ` Lai Jiangshan**
  2024-02-26 14:35 ` [[RFC PATCH 25/73] KVM: x86/PVM: Implement APIC emulation related callbacks](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#md1c28920b1bd98bd862098afd244cffaa8cb4ff5) Lai Jiangshan
                   ` [(50 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rd1c28920b1bd98bd862098afd244cffaa8cb4ff5)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r11884219130ef982a74482ba96cdb82e086660b5)
From: Lai Jiangshan @ 2024-02-26 14:35 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-25-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-25-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143557)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143557), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

In PVM ABI, CPL is not used directly. Instead, supervisor mode and user
mode are used to represent the original CPL0/CPL3 concept. It is assumed
that the kernel runs in supervisor mode and userspace runs in user mode.
From the x86 operating modes perspective, the PVM supervisor mode is a
modified 64-bit long mode. Therefore, 32-bit compatibility mode is not
allowed for the supervisor mode, and its hardware CS must be __USER_CS.

When switching to user mode, the stack and GS base of supervisor mode
are saved into the associated MSRs. When switching back from user mode,
the stack and GS base of supervisor mode are automatically restored from
the MSRs. Therefore, in PVM ABI, the value of MSR_KERNEL_GS_BASE in
supervisor mode is the same as the value of MSR_GS_BASE in supervisor
mode, which does not follow the x86 ABI.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/kvm/pvm/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-25-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) | 129 +++++++++++++++++++++++++++++++++++++++++
 [arch/x86/kvm/pvm/pvm.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-25-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.h) |   1 +
 2 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e11884219130ef982a74482ba96cdb82e086660b5), 130 insertions(+)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-25-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) --git a/arch/x86/kvm/pvm/pvm.c b/arch/x86/kvm/pvm/pvm.c
index 69f8fbbb6176..3735baee1d5f 100644
--- a/arch/x86/kvm/pvm/pvm.c
+++ b/arch/x86/kvm/pvm/pvm.c @@ -31,6 +31,22 @@ static bool __read_mostly is_intel;

 static unsigned long host_idt_base;

+static inline bool is_smod(struct vcpu_pvm *pvm)
+{
+	unsigned long switch_flags = pvm->switch_flags;
+
+	if ((switch_flags & SWITCH_FLAGS_MOD_TOGGLE) == SWITCH_FLAGS_SMOD)
+		return true;
+
+	WARN_ON_ONCE((switch_flags & SWITCH_FLAGS_MOD_TOGGLE) != SWITCH_FLAGS_UMOD);
+	return false;
+}
+
+static inline void pvm_switch_flags_toggle_mod(struct vcpu_pvm *pvm)
+{
+	pvm->switch_flags ^= SWITCH_FLAGS_MOD_TOGGLE;
+}
+
 static inline u16 kernel_cs_by_msr(u64 msr_star)
 {
 	// [47..32]
@@ -80,6 +96,82 @@ static inline void __load_fs_base(struct vcpu_pvm *pvm)
 	wrmsrl(MSR_FS_BASE, pvm->segments[VCPU_SREG_FS].base);
 }

+static u64 pvm_read_guest_gs_base(struct vcpu_pvm *pvm)
+{
+	preempt_disable();
+	if (pvm->loaded_cpu_state)
+		__save_gs_base(pvm);
+	preempt_enable();
+
+	return pvm->segments[VCPU_SREG_GS].base;
+}
+
+static u64 pvm_read_guest_fs_base(struct vcpu_pvm *pvm)
+{
+	preempt_disable();
+	if (pvm->loaded_cpu_state)
+		__save_fs_base(pvm);
+	preempt_enable();
+
+	return pvm->segments[VCPU_SREG_FS].base;
+}
+
+static u64 pvm_read_guest_kernel_gs_base(struct vcpu_pvm *pvm)
+{
+	return pvm->msr_kernel_gs_base;
+}
+
+static void pvm_write_guest_gs_base(struct vcpu_pvm *pvm, u64 data)
+{
+	preempt_disable();
+	pvm->segments[VCPU_SREG_GS].base = data;
+	if (pvm->loaded_cpu_state)
+		__load_gs_base(pvm);
+	preempt_enable();
+}
+
+static void pvm_write_guest_fs_base(struct vcpu_pvm *pvm, u64 data)
+{
+	preempt_disable();
+	pvm->segments[VCPU_SREG_FS].base = data;
+	if (pvm->loaded_cpu_state)
+		__load_fs_base(pvm);
+	preempt_enable();
+}
+
+static void pvm_write_guest_kernel_gs_base(struct vcpu_pvm *pvm, u64 data)
+{
+	pvm->msr_kernel_gs_base = data;
+}
+
+// switch_to_smod() and switch_to_umod() switch the mode (smod/umod) and
+// the CR3.  No vTLB flushing when switching the CR3 per PVM Spec.
+static inline void switch_to_smod(struct kvm_vcpu *vcpu)
+{
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+
+	pvm_switch_flags_toggle_mod(pvm);
+	kvm_mmu_new_pgd(vcpu, pvm->msr_switch_cr3);
+	swap(pvm->msr_switch_cr3, vcpu->arch.cr3);
+
+	pvm_write_guest_gs_base(pvm, pvm->msr_kernel_gs_base);
+	kvm_rsp_write(vcpu, pvm->msr_supervisor_rsp);
+
+	pvm->hw_cs = __USER_CS;
+	pvm->hw_ss = __USER_DS;
+}
+
+static inline void switch_to_umod(struct kvm_vcpu *vcpu)
+{
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+
+	pvm->msr_supervisor_rsp = kvm_rsp_read(vcpu);
+
+	pvm_switch_flags_toggle_mod(pvm);
+	kvm_mmu_new_pgd(vcpu, pvm->msr_switch_cr3);
+	swap(pvm->msr_switch_cr3, vcpu->arch.cr3);
+}
+
 /*
  * Test whether DS, ES, FS and GS need to be reloaded.
  *
@@ -309,6 +401,15 @@ static int pvm_get_msr(struct kvm_vcpu *vcpu, struct msr_data *msr_info)
 	int ret = 0;

 	switch (msr_info->index) {
+	case MSR_FS_BASE:
+		msr_info->data = pvm_read_guest_fs_base(pvm);
+		break;
+	case MSR_GS_BASE:
+		msr_info->data = pvm_read_guest_gs_base(pvm);
+		break;
+	case MSR_KERNEL_GS_BASE:
+		msr_info->data = pvm_read_guest_kernel_gs_base(pvm);
+		break;
 	case MSR_STAR:
 		msr_info->data = pvm->msr_star;
 		break;
@@ -352,6 +453,9 @@ static int pvm_get_msr(struct kvm_vcpu *vcpu, struct msr_data *msr_info)
 	case MSR_PVM_RETS_RIP:
 		msr_info->data = pvm->msr_rets_rip_plus2 - 2;
 		break;
+	case MSR_PVM_SWITCH_CR3:
+		msr_info->data = pvm->msr_switch_cr3;
+		break;
 	default:
 		ret = kvm_get_msr_common(vcpu, msr_info);
 	}
@@ -372,6 +476,15 @@ static int pvm_set_msr(struct kvm_vcpu *vcpu, struct msr_data *msr_info)
 	u64 data = msr_info->data;

 	switch (msr_index) {
+	case MSR_FS_BASE:
+		pvm_write_guest_fs_base(pvm, data);
+		break;
+	case MSR_GS_BASE:
+		pvm_write_guest_gs_base(pvm, data);
+		break;
+	case MSR_KERNEL_GS_BASE:
+		pvm_write_guest_kernel_gs_base(pvm, data);
+		break;
 	case MSR_STAR:
 		/*
 		 * Guest KERNEL_CS/DS shouldn't be NULL and guest USER_CS/DS
@@ -436,6 +549,9 @@ static int pvm_set_msr(struct kvm_vcpu *vcpu, struct msr_data *msr_info)
 	case MSR_PVM_RETS_RIP:
 		pvm->msr_rets_rip_plus2 = msr_info->data + 2;
 		break;
+	case MSR_PVM_SWITCH_CR3:
+		pvm->msr_switch_cr3 = msr_info->data;
+		break;
 	default:
 		ret = kvm_set_msr_common(vcpu, msr_info);
 	}
@@ -443,6 +559,13 @@ static int pvm_set_msr(struct kvm_vcpu *vcpu, struct msr_data *msr_info)
 	return ret;
 }

+static int pvm_get_cpl(struct kvm_vcpu *vcpu)
+{
+	if (is_smod(to_pvm(vcpu)))
+		return 0;
+	return 3;
+}
+
 static void pvm_setup_mce(struct kvm_vcpu *vcpu)
 {
 }
@@ -683,6 +806,11 @@ static fastpath_t pvm_vcpu_run(struct kvm_vcpu *vcpu)

 	pvm_vcpu_run_noinstr(vcpu);

+	if (is_smod(pvm)) {
+		if (pvm->hw_cs != __USER_CS || pvm->hw_ss != __USER_DS)
+			kvm_make_request(KVM_REQ_TRIPLE_FAULT, vcpu);
+	}
+
 	pvm_load_host_xsave_state(vcpu);

 	return EXIT_FASTPATH_NONE;
@@ -949,6 +1077,7 @@ static struct kvm_x86_ops pvm_x86_ops __initdata = {
 	.get_msr_feature = pvm_get_msr_feature,
 	.get_msr = pvm_get_msr,
 	.set_msr = pvm_set_msr,
+	.get_cpl = pvm_get_cpl,
 	.load_mmu_pgd = pvm_load_mmu_pgd,

 	.vcpu_pre_run = pvm_vcpu_pre_run,
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-25-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.h) --git a/arch/x86/kvm/pvm/pvm.h b/arch/x86/kvm/pvm/pvm.h
index 57ca2e901e0d..b0c633ce2987 100644
--- a/arch/x86/kvm/pvm/pvm.h
+++ b/arch/x86/kvm/pvm/pvm.h @@ -61,6 +61,7 @@ struct vcpu_pvm {
 	u64 unused_MSR_IA32_SYSENTER_CS;
 	u64 unused_MSR_IA32_SYSENTER_EIP;
 	u64 unused_MSR_IA32_SYSENTER_ESP;
+	u64 msr_kernel_gs_base;
 	u64 msr_tsc_aux;
 	/*
 	 * Only bits masked by msr_ia32_feature_control_valid_bits can be set in
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m11884219130ef982a74482ba96cdb82e086660b5) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-25-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-25-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-25-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-25-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-25-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-25-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r11884219130ef982a74482ba96cdb82e086660b5)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ed1c28920b1bd98bd862098afd244cffaa8cb4ff5) **[RFC PATCH 25/73] KVM: x86/PVM: Implement APIC emulation related callbacks**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(23 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r11884219130ef982a74482ba96cdb82e086660b5)
  2024-02-26 14:35 ` [[RFC PATCH 24/73] KVM: x86/PVM: Introduce PVM mode switching](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m11884219130ef982a74482ba96cdb82e086660b5) Lai Jiangshan
**@ 2024-02-26 14:35 ` Lai Jiangshan**
  2024-02-26 14:35 ` [[RFC PATCH 26/73] KVM: x86/PVM: Implement event delivery flags](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m832f2e4e35b2d9ed4bad146f00bd3812873304ef) " Lai Jiangshan
                   ` [(49 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r832f2e4e35b2d9ed4bad146f00bd3812873304ef)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rd1c28920b1bd98bd862098afd244cffaa8cb4ff5)
From: Lai Jiangshan @ 2024-02-26 14:35 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-26-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-26-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143600)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143600), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

For PVM, APIC virtualization for the guest is supported by reusing APIC
emulation.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/kvm/pvm/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-26-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) | 25 +++++++++++++++++++++++++
 1 file [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ed1c28920b1bd98bd862098afd244cffaa8cb4ff5), 25 insertions(+)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-26-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) --git a/arch/x86/kvm/pvm/pvm.c b/arch/x86/kvm/pvm/pvm.c
index 3735baee1d5f..ce047d211657 100644
--- a/arch/x86/kvm/pvm/pvm.c
+++ b/arch/x86/kvm/pvm/pvm.c @@ -566,6 +566,25 @@ static int pvm_get_cpl(struct kvm_vcpu *vcpu)
 	return 3;
 }

+static void pvm_deliver_interrupt(struct kvm_lapic *apic, int delivery_mode,
+				  int trig_mode, int vector)
+{
+	struct kvm_vcpu *vcpu = apic->vcpu;
+
+	kvm_lapic_set_irr(vector, apic);
+	kvm_make_request(KVM_REQ_EVENT, vcpu);
+	kvm_vcpu_kick(vcpu);
+}
+
+static void pvm_refresh_apicv_exec_ctrl(struct kvm_vcpu *vcpu)
+{
+}
+
+static bool pvm_apic_init_signal_blocked(struct kvm_vcpu *vcpu)
+{
+	return false;
+}
+
 static void pvm_setup_mce(struct kvm_vcpu *vcpu)
 {
 }
@@ -1083,19 +1102,25 @@ static struct kvm_x86_ops pvm_x86_ops __initdata = {
 	.vcpu_pre_run = pvm_vcpu_pre_run,
 	.vcpu_run = pvm_vcpu_run,
 	.handle_exit = pvm_handle_exit,
+	.refresh_apicv_exec_ctrl = pvm_refresh_apicv_exec_ctrl,
+	.deliver_interrupt = pvm_deliver_interrupt,

 	.vcpu_after_set_cpuid = pvm_vcpu_after_set_cpuid,

 	.handle_exit_irqoff = pvm_handle_exit_irqoff,

+	.request_immediate_exit = __kvm_request_immediate_exit,
+
 	.sched_in = pvm_sched_in,

 	.nested_ops = &pvm_nested_ops,

 	.setup_mce = pvm_setup_mce,

+	.apic_init_signal_blocked = pvm_apic_init_signal_blocked,
 	.msr_filter_changed = pvm_msr_filter_changed,
 	.complete_emulated_msr = kvm_complete_insn_gp,
+	.vcpu_deliver_sipi_vector = kvm_vcpu_deliver_sipi_vector,
 };

 static struct kvm_x86_init_ops pvm_init_ops __initdata = {
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#md1c28920b1bd98bd862098afd244cffaa8cb4ff5) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-26-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-26-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-26-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-26-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-26-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-26-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rd1c28920b1bd98bd862098afd244cffaa8cb4ff5)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e832f2e4e35b2d9ed4bad146f00bd3812873304ef) **[RFC PATCH 26/73] KVM: x86/PVM: Implement event delivery flags related callbacks**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(24 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rd1c28920b1bd98bd862098afd244cffaa8cb4ff5)
  2024-02-26 14:35 ` [[RFC PATCH 25/73] KVM: x86/PVM: Implement APIC emulation related callbacks](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#md1c28920b1bd98bd862098afd244cffaa8cb4ff5) Lai Jiangshan
**@ 2024-02-26 14:35 ` Lai Jiangshan**
  2024-02-26 14:35 ` [[RFC PATCH 27/73] KVM: x86/PVM: Implement event injection](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m33dc6d1661d230e4e082e894184c3b24dc45d32f) " Lai Jiangshan
                   ` [(48 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r33dc6d1661d230e4e082e894184c3b24dc45d32f)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r832f2e4e35b2d9ed4bad146f00bd3812873304ef)
From: Lai Jiangshan @ 2024-02-26 14:35 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-27-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-27-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143603)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143603), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

To reduce the number of VM exits for modifying the X86_EFLAGS_IF bit in
guest suprvisor mode, a shared structure is used between the guest and
hypervisor in PVM. This structure is stored in the guest memory. In this
way, the guest supervisor can change its X86_EFLAGS_IF bit without
causing a VM exit, as long as there is no IRQ window request. After a VM
exit occurs, the hypervisor updates the guest's X86_EFLAGS_IF bit from
the shared structure.

Since the SRET/URET synthetic instruction always induces a VM exit,
there is nothing to do in the enable_nmi_window() callback.
Additionally, SMM mode is not supported now.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/kvm/pvm/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-27-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) | 194 +++++++++++++++++++++++++++++++++++++++++
 1 file [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e832f2e4e35b2d9ed4bad146f00bd3812873304ef), 194 insertions(+)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-27-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) --git a/arch/x86/kvm/pvm/pvm.c b/arch/x86/kvm/pvm/pvm.c
index ce047d211657..3d2a3c472664 100644
--- a/arch/x86/kvm/pvm/pvm.c
+++ b/arch/x86/kvm/pvm/pvm.c @@ -585,6 +585,143 @@ static bool pvm_apic_init_signal_blocked(struct kvm_vcpu *vcpu)
 	return false;
 }

+static struct pvm_vcpu_struct *pvm_get_vcpu_struct(struct vcpu_pvm *pvm)
+{
+	struct gfn_to_pfn_cache *gpc = &pvm->pvcs_gpc;
+
+	read_lock_irq(&gpc->lock);
+	while (!kvm_gpc_check(gpc, PAGE_SIZE)) {
+		read_unlock_irq(&gpc->lock);
+
+		if (kvm_gpc_refresh(gpc, PAGE_SIZE))
+			return NULL;
+
+		read_lock_irq(&gpc->lock);
+	}
+
+	return (struct pvm_vcpu_struct *)(gpc->khva);
+}
+
+static void pvm_put_vcpu_struct(struct vcpu_pvm *pvm, bool dirty)
+{
+	struct gfn_to_pfn_cache *gpc = &pvm->pvcs_gpc;
+
+	read_unlock_irq(&gpc->lock);
+	if (dirty)
+		mark_page_dirty_in_slot(pvm->vcpu.kvm, gpc->memslot,
+					gpc->gpa >> PAGE_SHIFT);
+}
+
+static void pvm_vcpu_gpc_refresh(struct kvm_vcpu *vcpu)
+{
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+	struct gfn_to_pfn_cache *gpc = &pvm->pvcs_gpc;
+
+	if (!gpc->active)
+		return;
+
+	if (pvm_get_vcpu_struct(pvm))
+		pvm_put_vcpu_struct(pvm, false);
+	else
+		kvm_make_request(KVM_REQ_TRIPLE_FAULT, vcpu);
+}
+
+static void pvm_event_flags_update(struct kvm_vcpu *vcpu, unsigned long set,
+				   unsigned long clear)
+{
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+	static struct pvm_vcpu_struct *pvcs;
+	unsigned long old_flags, new_flags;
+
+	if (!pvm->msr_vcpu_struct)
+		return;
+
+	pvcs = pvm_get_vcpu_struct(pvm);
+	if (!pvcs)
+		return;
+
+	old_flags = pvcs->event_flags;
+	new_flags = (old_flags | set) & ~clear;
+	if (new_flags != old_flags)
+		pvcs->event_flags = new_flags;
+
+	pvm_put_vcpu_struct(pvm, new_flags != old_flags);
+}
+
+static unsigned long pvm_get_rflags(struct kvm_vcpu *vcpu)
+{
+	return to_pvm(vcpu)->rflags;
+}
+
+static void pvm_set_rflags(struct kvm_vcpu *vcpu, unsigned long rflags)
+{
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+	int need_update = !!((pvm->rflags ^ rflags) & X86_EFLAGS_IF);
+
+	pvm->rflags = rflags;
+
+	/*
+	 * The IF bit of 'pvcs->event_flags' should not be changed in user
+	 * mode. It is recommended for this bit to be cleared when switching to
+	 * user mode, so that when the guest switches back to supervisor mode,
+	 * the X86_EFLAGS_IF is already cleared.
+	 */
+	if (!need_update || !is_smod(pvm))
+		return;
+
+	if (rflags & X86_EFLAGS_IF)
+		pvm_event_flags_update(vcpu, X86_EFLAGS_IF, PVM_EVENT_FLAGS_IP);
+	else
+		pvm_event_flags_update(vcpu, 0, X86_EFLAGS_IF);
+}
+
+static bool pvm_get_if_flag(struct kvm_vcpu *vcpu)
+{
+	return pvm_get_rflags(vcpu) & X86_EFLAGS_IF;
+}
+
+static u32 pvm_get_interrupt_shadow(struct kvm_vcpu *vcpu)
+{
+	return to_pvm(vcpu)->int_shadow;
+}
+
+static void pvm_set_interrupt_shadow(struct kvm_vcpu *vcpu, int mask)
+{
+	/* PVM spec: ignore interrupt shadow when in PVM mode. */
+}
+
+static void enable_irq_window(struct kvm_vcpu *vcpu)
+{
+	pvm_event_flags_update(vcpu, PVM_EVENT_FLAGS_IP, 0);
+}
+
+static int pvm_interrupt_allowed(struct kvm_vcpu *vcpu, bool for_injection)
+{
+	return (pvm_get_rflags(vcpu) & X86_EFLAGS_IF) &&
+		!to_pvm(vcpu)->int_shadow;
+}
+
+static bool pvm_get_nmi_mask(struct kvm_vcpu *vcpu)
+{
+	return to_pvm(vcpu)->nmi_mask;
+}
+
+static void pvm_set_nmi_mask(struct kvm_vcpu *vcpu, bool masked)
+{
+	to_pvm(vcpu)->nmi_mask = masked;
+}
+
+static void enable_nmi_window(struct kvm_vcpu *vcpu)
+{
+}
+
+static int pvm_nmi_allowed(struct kvm_vcpu *vcpu, bool for_injection)
+{
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+
+	return !pvm->nmi_mask && !pvm->int_shadow;
+}
+
 static void pvm_setup_mce(struct kvm_vcpu *vcpu)
 {
 }
@@ -826,12 +963,29 @@ static fastpath_t pvm_vcpu_run(struct kvm_vcpu *vcpu)
 	pvm_vcpu_run_noinstr(vcpu);

 	if (is_smod(pvm)) {
+		struct pvm_vcpu_struct *pvcs = pvm->pvcs_gpc.khva;
+
+		/*
+		 * Load the X86_EFLAGS_IF bit from PVCS. In user mode, the
+		 * Interrupt Flag is considered to be set and cannot be
+		 * changed. Since it is already set in 'pvm->rflags', so
+		 * nothing to do. In supervisor mode, the Interrupt Flag is
+		 * reflected in 'pvcs->event_flags' and can be changed
+		 * directly without triggering a VM exit.
+		 */
+		pvm->rflags &= ~X86_EFLAGS_IF;
+		if (likely(pvm->msr_vcpu_struct))
+			pvm->rflags |= X86_EFLAGS_IF & pvcs->event_flags;
+
 		if (pvm->hw_cs != __USER_CS || pvm->hw_ss != __USER_DS)
 			kvm_make_request(KVM_REQ_TRIPLE_FAULT, vcpu);
 	}

 	pvm_load_host_xsave_state(vcpu);

+	mark_page_dirty_in_slot(vcpu->kvm, pvm->pvcs_gpc.memslot,
+				pvm->pvcs_gpc.gpa >> PAGE_SHIFT);
+
 	return EXIT_FASTPATH_NONE;
 }

@@ -965,6 +1119,27 @@ static int pvm_check_processor_compat(void)
 	return 0;
 }

+#ifdef CONFIG_KVM_SMM
+static int pvm_smi_allowed(struct kvm_vcpu *vcpu, bool for_injection)
+{
+	return 0;
+}
+
+static int pvm_enter_smm(struct kvm_vcpu *vcpu, union kvm_smram *smram)
+{
+	return 0;
+}
+
+static int pvm_leave_smm(struct kvm_vcpu *vcpu, const union kvm_smram *smram)
+{
+	return 0;
+}
+
+static void enable_smi_window(struct kvm_vcpu *vcpu)
+{
+}
+#endif
+
 /*
  * When in PVM mode, the hardware MSR_LSTAR is set to the entry point
  * provided by the host entry code (switcher), and the
@@ -1098,10 +1273,21 @@ static struct kvm_x86_ops pvm_x86_ops __initdata = {
 	.set_msr = pvm_set_msr,
 	.get_cpl = pvm_get_cpl,
 	.load_mmu_pgd = pvm_load_mmu_pgd,
+	.get_rflags = pvm_get_rflags,
+	.set_rflags = pvm_set_rflags,
+	.get_if_flag = pvm_get_if_flag,

 	.vcpu_pre_run = pvm_vcpu_pre_run,
 	.vcpu_run = pvm_vcpu_run,
 	.handle_exit = pvm_handle_exit,
+	.set_interrupt_shadow = pvm_set_interrupt_shadow,
+	.get_interrupt_shadow = pvm_get_interrupt_shadow,
+	.interrupt_allowed = pvm_interrupt_allowed,
+	.nmi_allowed = pvm_nmi_allowed,
+	.get_nmi_mask = pvm_get_nmi_mask,
+	.set_nmi_mask = pvm_set_nmi_mask,
+	.enable_nmi_window = enable_nmi_window,
+	.enable_irq_window = enable_irq_window,
 	.refresh_apicv_exec_ctrl = pvm_refresh_apicv_exec_ctrl,
 	.deliver_interrupt = pvm_deliver_interrupt,

@@ -1117,10 +1303,18 @@ static struct kvm_x86_ops pvm_x86_ops __initdata = {

 	.setup_mce = pvm_setup_mce,

+#ifdef CONFIG_KVM_SMM
+	.smi_allowed = pvm_smi_allowed,
+	.enter_smm = pvm_enter_smm,
+	.leave_smm = pvm_leave_smm,
+	.enable_smi_window = enable_smi_window,
+#endif
+
 	.apic_init_signal_blocked = pvm_apic_init_signal_blocked,
 	.msr_filter_changed = pvm_msr_filter_changed,
 	.complete_emulated_msr = kvm_complete_insn_gp,
 	.vcpu_deliver_sipi_vector = kvm_vcpu_deliver_sipi_vector,
+	.vcpu_gpc_refresh = pvm_vcpu_gpc_refresh,
 };

 static struct kvm_x86_init_ops pvm_init_ops __initdata = {
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m832f2e4e35b2d9ed4bad146f00bd3812873304ef) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-27-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-27-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-27-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-27-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-27-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-27-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r832f2e4e35b2d9ed4bad146f00bd3812873304ef)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e33dc6d1661d230e4e082e894184c3b24dc45d32f) **[RFC PATCH 27/73] KVM: x86/PVM: Implement event injection related callbacks**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(25 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r832f2e4e35b2d9ed4bad146f00bd3812873304ef)
  2024-02-26 14:35 ` [[RFC PATCH 26/73] KVM: x86/PVM: Implement event delivery flags](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m832f2e4e35b2d9ed4bad146f00bd3812873304ef) " Lai Jiangshan
**@ 2024-02-26 14:35 ` Lai Jiangshan**
  2024-02-26 14:35 ` [[RFC PATCH 28/73] KVM: x86/PVM: Handle syscall from user mode](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m047f1b27ead6cff47e8d3a5c1d3cc4ffe9ca24ad) Lai Jiangshan
                   ` [(47 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r047f1b27ead6cff47e8d3a5c1d3cc4ffe9ca24ad)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r33dc6d1661d230e4e082e894184c3b24dc45d32f)
From: Lai Jiangshan @ 2024-02-26 14:35 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-28-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-28-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143607)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143607), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

In PVM, events are injected and consumed directly. The PVM hypervisor
does not follow the IDT-based event delivery mechanism but instead
utilizes a new PVM-specific event delivery ABI, which is similar to FRED
event delivery.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/kvm/pvm/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-28-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) | 193 +++++++++++++++++++++++++++++++++++++++++
 [arch/x86/kvm/pvm/pvm.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-28-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.h) |   1 +
 2 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e33dc6d1661d230e4e082e894184c3b24dc45d32f), 194 insertions(+)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-28-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) --git a/arch/x86/kvm/pvm/pvm.c b/arch/x86/kvm/pvm/pvm.c
index 3d2a3c472664..57d987903791 100644
--- a/arch/x86/kvm/pvm/pvm.c
+++ b/arch/x86/kvm/pvm/pvm.c @@ -648,6 +648,150 @@ static void pvm_event_flags_update(struct kvm_vcpu *vcpu, unsigned long set,
 	pvm_put_vcpu_struct(pvm, new_flags != old_flags);
 }

+static void pvm_standard_event_entry(struct kvm_vcpu *vcpu, unsigned long entry)
+{
+	// Change rip, rflags, rcx and r11 per PVM event delivery specification,
+	// this allows to use sysret in VM enter.
+	kvm_rip_write(vcpu, entry);
+	kvm_set_rflags(vcpu, X86_EFLAGS_FIXED);
+	kvm_rcx_write(vcpu, entry);
+	kvm_r11_write(vcpu, X86_EFLAGS_IF | X86_EFLAGS_FIXED);
+}
+
+/* handle pvm user event per PVM Spec. */
+static int do_pvm_user_event(struct kvm_vcpu *vcpu, int vector,
+			     bool has_err_code, u64 err_code)
+{
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+	unsigned long entry = vector == PVM_SYSCALL_VECTOR ?
+			      pvm->msr_lstar : pvm->msr_event_entry;
+	struct pvm_vcpu_struct *pvcs;
+
+	pvcs = pvm_get_vcpu_struct(pvm);
+	if (!pvcs) {
+		kvm_make_request(KVM_REQ_TRIPLE_FAULT, vcpu);
+		return 1;
+	}
+
+	pvcs->user_cs = pvm->hw_cs;
+	pvcs->user_ss = pvm->hw_ss;
+	pvcs->eflags = kvm_get_rflags(vcpu);
+	pvcs->pkru = 0;
+	pvcs->user_gsbase = pvm_read_guest_gs_base(pvm);
+	pvcs->rip = kvm_rip_read(vcpu);
+	pvcs->rsp = kvm_rsp_read(vcpu);
+	pvcs->rcx = kvm_rcx_read(vcpu);
+	pvcs->r11 = kvm_r11_read(vcpu);
+
+	if (has_err_code)
+		pvcs->event_errcode = err_code;
+	if (vector != PVM_SYSCALL_VECTOR)
+		pvcs->event_vector = vector;
+
+	if (vector == PF_VECTOR)
+		pvcs->cr2 = vcpu->arch.cr2;
+
+	pvm_put_vcpu_struct(pvm, true);
+
+	switch_to_smod(vcpu);
+
+	pvm_standard_event_entry(vcpu, entry);
+
+	return 1;
+}
+
+static int do_pvm_supervisor_exception(struct kvm_vcpu *vcpu, int vector,
+				       bool has_error_code, u64 error_code)
+{
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+	unsigned long stack;
+	struct pvm_supervisor_event frame;
+	struct x86_exception e;
+	int ret;
+
+	memset(&frame, 0, sizeof(frame));
+	frame.cs = kernel_cs_by_msr(pvm->msr_star);
+	frame.ss = kernel_ds_by_msr(pvm->msr_star);
+	frame.rip = kvm_rip_read(vcpu);
+	frame.rflags = kvm_get_rflags(vcpu);
+	frame.rsp = kvm_rsp_read(vcpu);
+	frame.errcode = ((unsigned long)vector << 32) | error_code;
+	frame.r11 = kvm_r11_read(vcpu);
+	frame.rcx = kvm_rcx_read(vcpu);
+
+	stack = ((frame.rsp - pvm->msr_supervisor_redzone) & ~15UL) - sizeof(frame);
+
+	ret = kvm_write_guest_virt_system(vcpu, stack, &frame, sizeof(frame), &e);
+	if (ret) {
+		kvm_make_request(KVM_REQ_TRIPLE_FAULT, vcpu);
+		return 1;
+	}
+
+	if (vector == PF_VECTOR) {
+		struct pvm_vcpu_struct *pvcs;
+
+		pvcs = pvm_get_vcpu_struct(pvm);
+		if (!pvcs) {
+			kvm_make_request(KVM_REQ_TRIPLE_FAULT, vcpu);
+			return 1;
+		}
+
+		pvcs->cr2 = vcpu->arch.cr2;
+		pvm_put_vcpu_struct(pvm, true);
+	}
+
+	kvm_rsp_write(vcpu, stack);
+
+	pvm_standard_event_entry(vcpu, pvm->msr_event_entry + 256);
+
+	return 1;
+}
+
+static int do_pvm_supervisor_interrupt(struct kvm_vcpu *vcpu, int vector,
+				       bool has_error_code, u64 error_code)
+{
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+	unsigned long stack = kvm_rsp_read(vcpu);
+	struct pvm_vcpu_struct *pvcs;
+
+	pvcs = pvm_get_vcpu_struct(pvm);
+	if (!pvcs) {
+		kvm_make_request(KVM_REQ_TRIPLE_FAULT, vcpu);
+		return 1;
+	}
+	pvcs->eflags = kvm_get_rflags(vcpu);
+	pvcs->rip = kvm_rip_read(vcpu);
+	pvcs->rsp = stack;
+	pvcs->rcx = kvm_rcx_read(vcpu);
+	pvcs->r11 = kvm_r11_read(vcpu);
+
+	pvcs->event_vector = vector;
+	if (has_error_code)
+		pvcs->event_errcode = error_code;
+
+	pvm_put_vcpu_struct(pvm, true);
+
+	stack = (stack - pvm->msr_supervisor_redzone) & ~15UL;
+	kvm_rsp_write(vcpu, stack);
+
+	pvm_standard_event_entry(vcpu, pvm->msr_event_entry + 512);
+
+	return 1;
+}
+
+static int do_pvm_event(struct kvm_vcpu *vcpu, int vector,
+			bool has_error_code, u64 error_code)
+{
+	if (!is_smod(to_pvm(vcpu)))
+		return do_pvm_user_event(vcpu, vector, has_error_code, error_code);
+
+	if (vector < 32)
+		return do_pvm_supervisor_exception(vcpu, vector,
+						   has_error_code, error_code);
+
+	return do_pvm_supervisor_interrupt(vcpu, vector, has_error_code, error_code);
+}
+
 static unsigned long pvm_get_rflags(struct kvm_vcpu *vcpu)
 {
 	return to_pvm(vcpu)->rflags;
@@ -722,6 +866,51 @@ static int pvm_nmi_allowed(struct kvm_vcpu *vcpu, bool for_injection)
 	return !pvm->nmi_mask && !pvm->int_shadow;
 }

+/* Always inject the exception directly and consume the event. */
+static void pvm_inject_exception(struct kvm_vcpu *vcpu)
+{
+	unsigned int vector = vcpu->arch.exception.vector;
+	bool has_error_code = vcpu->arch.exception.has_error_code;
+	u32 error_code = vcpu->arch.exception.error_code;
+
+	kvm_deliver_exception_payload(vcpu, &vcpu->arch.exception);
+
+	if (do_pvm_event(vcpu, vector, has_error_code, error_code))
+		kvm_clear_exception_queue(vcpu);
+}
+
+/* Always inject the interrupt directly and consume the event. */
+static void pvm_inject_irq(struct kvm_vcpu *vcpu, bool reinjected)
+{
+	int irq = vcpu->arch.interrupt.nr;
+
+	trace_kvm_inj_virq(irq, vcpu->arch.interrupt.soft, false);
+
+	if (do_pvm_event(vcpu, irq, false, 0))
+		kvm_clear_interrupt_queue(vcpu);
+
+	++vcpu->stat.irq_injections;
+}
+
+/* Always inject the NMI directly and consume the event. */
+static void pvm_inject_nmi(struct kvm_vcpu *vcpu)
+{
+	if (do_pvm_event(vcpu, NMI_VECTOR, false, 0)) {
+		vcpu->arch.nmi_injected = false;
+		pvm_set_nmi_mask(vcpu, true);
+	}
+
+	++vcpu->stat.nmi_injections;
+}
+
+static void pvm_cancel_injection(struct kvm_vcpu *vcpu)
+{
+	/*
+	 * Nothing to do. Since exceptions/interrupts are delivered immediately
+	 * during event injection, so they cannot be cancelled and reinjected.
+	 */
+}
+
 static void pvm_setup_mce(struct kvm_vcpu *vcpu)
 {
 }
@@ -1282,6 +1471,10 @@ static struct kvm_x86_ops pvm_x86_ops __initdata = {
 	.handle_exit = pvm_handle_exit,
 	.set_interrupt_shadow = pvm_set_interrupt_shadow,
 	.get_interrupt_shadow = pvm_get_interrupt_shadow,
+	.inject_irq = pvm_inject_irq,
+	.inject_nmi = pvm_inject_nmi,
+	.inject_exception = pvm_inject_exception,
+	.cancel_injection = pvm_cancel_injection,
 	.interrupt_allowed = pvm_interrupt_allowed,
 	.nmi_allowed = pvm_nmi_allowed,
 	.get_nmi_mask = pvm_get_nmi_mask,
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-28-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.h) --git a/arch/x86/kvm/pvm/pvm.h b/arch/x86/kvm/pvm/pvm.h
index b0c633ce2987..39506ddbe5c5 100644
--- a/arch/x86/kvm/pvm/pvm.h
+++ b/arch/x86/kvm/pvm/pvm.h @@ -7,6 +7,7 @@

 #define SWITCH_FLAGS_INIT	(SWITCH_FLAGS_SMOD)

+#define PVM_SYSCALL_VECTOR		SWITCH_EXIT_REASONS_SYSCALL
 #define PVM_FAILED_VMENTRY_VECTOR	SWITCH_EXIT_REASONS_FAILED_VMETNRY

 #define PT_L4_SHIFT		39
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m33dc6d1661d230e4e082e894184c3b24dc45d32f) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-28-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-28-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-28-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-28-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-28-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-28-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r33dc6d1661d230e4e082e894184c3b24dc45d32f)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e047f1b27ead6cff47e8d3a5c1d3cc4ffe9ca24ad) **[RFC PATCH 28/73] KVM: x86/PVM: Handle syscall from user mode**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(26 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r33dc6d1661d230e4e082e894184c3b24dc45d32f)
  2024-02-26 14:35 ` [[RFC PATCH 27/73] KVM: x86/PVM: Implement event injection](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m33dc6d1661d230e4e082e894184c3b24dc45d32f) " Lai Jiangshan
**@ 2024-02-26 14:35 ` Lai Jiangshan**
  2024-02-26 14:35 ` [[RFC PATCH 29/73] KVM: x86/PVM: Implement allowed range checking for #PF](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m1e0e233f733da10c3280e7afaf54c4c86f476d0f) Lai Jiangshan
                   ` [(46 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r1e0e233f733da10c3280e7afaf54c4c86f476d0f)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r047f1b27ead6cff47e8d3a5c1d3cc4ffe9ca24ad)
From: Lai Jiangshan @ 2024-02-26 14:35 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-29-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-29-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143609)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143609), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

Similar to the vector event from user mode, the syscall event from user
mode follows the PVM event delivery ABI. Additionally, the 32-bit user
mode can only use "INT 0x80" for syscall.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/kvm/pvm/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-29-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) | 15 ++++++++++++++-
 1 file [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e047f1b27ead6cff47e8d3a5c1d3cc4ffe9ca24ad), 14 insertions(+), 1 deletion(-)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-29-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) --git a/arch/x86/kvm/pvm/pvm.c b/arch/x86/kvm/pvm/pvm.c
index 57d987903791..92eef226df28 100644
--- a/arch/x86/kvm/pvm/pvm.c
+++ b/arch/x86/kvm/pvm/pvm.c @@ -915,6 +915,15 @@ static void pvm_setup_mce(struct kvm_vcpu *vcpu)
 {
 }

+static int handle_exit_syscall(struct kvm_vcpu *vcpu)
+{
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+
+	if (!is_smod(pvm))
+		return do_pvm_user_event(vcpu, PVM_SYSCALL_VECTOR, false, 0);
+	return 1;
+}
+
 static int handle_exit_external_interrupt(struct kvm_vcpu *vcpu)
 {
 	++vcpu->stat.irq_exits;
@@ -939,7 +948,11 @@ static int pvm_handle_exit(struct kvm_vcpu *vcpu, fastpath_t exit_fastpath)
 	struct vcpu_pvm *pvm = to_pvm(vcpu);
 	u32 exit_reason = pvm->exit_vector;

-	if (exit_reason >= FIRST_EXTERNAL_VECTOR && exit_reason < NR_VECTORS) +	if (exit_reason == PVM_SYSCALL_VECTOR)
+		return handle_exit_syscall(vcpu);
+	else if (exit_reason == IA32_SYSCALL_VECTOR)
+		return do_pvm_event(vcpu, IA32_SYSCALL_VECTOR, false, 0);
+	else if (exit_reason >= FIRST_EXTERNAL_VECTOR && exit_reason < NR_VECTORS)
 		return handle_exit_external_interrupt(vcpu);
 	else if (exit_reason == PVM_FAILED_VMENTRY_VECTOR)
 		return handle_exit_failed_vmentry(vcpu);
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m047f1b27ead6cff47e8d3a5c1d3cc4ffe9ca24ad) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-29-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-29-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-29-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-29-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-29-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-29-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r047f1b27ead6cff47e8d3a5c1d3cc4ffe9ca24ad)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e1e0e233f733da10c3280e7afaf54c4c86f476d0f) **[RFC PATCH 29/73] KVM: x86/PVM: Implement allowed range checking for #PF**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(27 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r047f1b27ead6cff47e8d3a5c1d3cc4ffe9ca24ad)
  2024-02-26 14:35 ` [[RFC PATCH 28/73] KVM: x86/PVM: Handle syscall from user mode](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m047f1b27ead6cff47e8d3a5c1d3cc4ffe9ca24ad) Lai Jiangshan
**@ 2024-02-26 14:35 ` Lai Jiangshan**
  2024-02-26 14:35 ` [[RFC PATCH 30/73] KVM: x86/PVM: Implement segment related callbacks](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#md376ee490f7f7f78f1acbe712885be489e294ece) Lai Jiangshan
                   ` [(45 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rd376ee490f7f7f78f1acbe712885be489e294ece)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r1e0e233f733da10c3280e7afaf54c4c86f476d0f)
From: Lai Jiangshan @ 2024-02-26 14:35 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-30-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-30-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143613)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143613), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

In PVM, guest is only allowed to be running in the reserved virtual
address range provided by the hypervisor. So guest needs to get the
allowed range information from the MSR and the hypervisor needs to check
the fault address and prevent install mapping in the #PF handler.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/kvm/pvm/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-30-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) | 74 ++++++++++++++++++++++++++++++++++++++++++
 [arch/x86/kvm/pvm/pvm.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-30-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.h) |  5 +++
 2 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e1e0e233f733da10c3280e7afaf54c4c86f476d0f), 79 insertions(+)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-30-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) --git a/arch/x86/kvm/pvm/pvm.c b/arch/x86/kvm/pvm/pvm.c
index 92eef226df28..26b2201f7dde 100644
--- a/arch/x86/kvm/pvm/pvm.c
+++ b/arch/x86/kvm/pvm/pvm.c @@ -144,6 +144,28 @@ static void pvm_write_guest_kernel_gs_base(struct vcpu_pvm *pvm, u64 data)
 	pvm->msr_kernel_gs_base = data;
 }

+static __always_inline bool pvm_guest_allowed_va(struct kvm_vcpu *vcpu, u64 va)
+{
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+
+	if ((s64)va > 0)
+		return true;
+	if (pvm->l4_range_start <= va && va < pvm->l4_range_end)
+		return true;
+	if (pvm->l5_range_start <= va && va < pvm->l5_range_end)
+		return true;
+
+	return false;
+}
+
+static bool pvm_disallowed_va(struct kvm_vcpu *vcpu, u64 va)
+{
+	if (is_noncanonical_address(va, vcpu))
+		return true;
+
+	return !pvm_guest_allowed_va(vcpu, va);
+}
+
 // switch_to_smod() and switch_to_umod() switch the mode (smod/umod) and
 // the CR3.  No vTLB flushing when switching the CR3 per PVM Spec.
 static inline void switch_to_smod(struct kvm_vcpu *vcpu)
@@ -380,6 +402,48 @@ static void pvm_sched_in(struct kvm_vcpu *vcpu, int cpu)
 {
 }

+static void pvm_set_msr_linear_address_range(struct vcpu_pvm *pvm,
+					     u64 pml4_i_s, u64 pml4_i_e,
+					     u64 pml5_i_s, u64 pml5_i_e)
+{
+	pvm->msr_linear_address_range = ((0xfe00 | pml4_i_s) << 0) |
+					((0xfe00 | pml4_i_e) << 16) |
+					((0xfe00 | pml5_i_s) << 32) |
+					((0xfe00 | pml5_i_e) << 48);
+
+	pvm->l4_range_start = (0x1fffe00 | pml4_i_s) * PT_L4_SIZE;
+	pvm->l4_range_end = (0x1fffe00 | pml4_i_e) * PT_L4_SIZE;
+	pvm->l5_range_start = (0xfe00 | pml5_i_s) * PT_L5_SIZE;
+	pvm->l5_range_end = (0xfe00 | pml5_i_e) * PT_L5_SIZE;
+}
+
+static void pvm_set_default_msr_linear_address_range(struct vcpu_pvm *pvm)
+{
+	pvm_set_msr_linear_address_range(pvm, pml4_index_start, pml4_index_end,
+					 pml5_index_start, pml5_index_end);
+}
+
+static bool pvm_check_and_set_msr_linear_address_range(struct vcpu_pvm *pvm, u64 msr)
+{
+	u64 pml4_i_s = (msr >> 0) & 0x1ff;
+	u64 pml4_i_e = (msr >> 16) & 0x1ff;
+	u64 pml5_i_s = (msr >> 32) & 0x1ff;
+	u64 pml5_i_e = (msr >> 48) & 0x1ff;
+
+	/* PVM specification requires those bits to be all set. */
+	if ((msr & 0xff00ff00ff00ff00) != 0xff00ff00ff00ff00)
+		return false;
+
+	/* Guest ranges should be inside what the hypervisor can provide. */
+	if (pml4_i_s < pml4_index_start || pml4_i_e > pml4_index_end ||
+	    pml5_i_s < pml5_index_start || pml5_i_e > pml5_index_end)
+		return false;
+
+	pvm_set_msr_linear_address_range(pvm, pml4_i_s, pml4_i_e, pml5_i_s, pml5_i_e);
+
+	return true;
+}
+
 static int pvm_get_msr_feature(struct kvm_msr_entry *msr)
 {
 	return 1;
@@ -456,6 +520,9 @@ static int pvm_get_msr(struct kvm_vcpu *vcpu, struct msr_data *msr_info)
 	case MSR_PVM_SWITCH_CR3:
 		msr_info->data = pvm->msr_switch_cr3;
 		break;
+	case MSR_PVM_LINEAR_ADDRESS_RANGE:
+		msr_info->data = pvm->msr_linear_address_range;
+		break;
 	default:
 		ret = kvm_get_msr_common(vcpu, msr_info);
 	}
@@ -552,6 +619,10 @@ static int pvm_set_msr(struct kvm_vcpu *vcpu, struct msr_data *msr_info)
 	case MSR_PVM_SWITCH_CR3:
 		pvm->msr_switch_cr3 = msr_info->data;
 		break;
+	case MSR_PVM_LINEAR_ADDRESS_RANGE:
+		if (!pvm_check_and_set_msr_linear_address_range(pvm, msr_info->data))
+			return 1;
+		break;
 	default:
 		ret = kvm_set_msr_common(vcpu, msr_info);
 	}
@@ -1273,6 +1344,7 @@ static void pvm_vcpu_reset(struct kvm_vcpu *vcpu, bool init_event)
 	pvm->msr_retu_rip_plus2 = 0;
 	pvm->msr_rets_rip_plus2 = 0;
 	pvm->msr_switch_cr3 = 0;
+	pvm_set_default_msr_linear_address_range(pvm);
 }

 static int pvm_vcpu_create(struct kvm_vcpu *vcpu)
@@ -1520,6 +1592,8 @@ static struct kvm_x86_ops pvm_x86_ops __initdata = {
 	.msr_filter_changed = pvm_msr_filter_changed,
 	.complete_emulated_msr = kvm_complete_insn_gp,
 	.vcpu_deliver_sipi_vector = kvm_vcpu_deliver_sipi_vector,
+
+	.disallowed_va = pvm_disallowed_va,
 	.vcpu_gpc_refresh = pvm_vcpu_gpc_refresh,
 };

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-30-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.h) --git a/arch/x86/kvm/pvm/pvm.h b/arch/x86/kvm/pvm/pvm.h
index 39506ddbe5c5..bf3a6a1837c0 100644
--- a/arch/x86/kvm/pvm/pvm.h
+++ b/arch/x86/kvm/pvm/pvm.h @@ -82,6 +82,11 @@ struct vcpu_pvm {
 	unsigned long msr_switch_cr3;
 	unsigned long msr_linear_address_range;

+	u64 l4_range_start;
+	u64 l4_range_end;
+	u64 l5_range_start;
+	u64 l5_range_end;
+
 	struct kvm_segment segments[NR_VCPU_SREG];
 	struct desc_ptr idt_ptr;
 	struct desc_ptr gdt_ptr;
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m1e0e233f733da10c3280e7afaf54c4c86f476d0f) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-30-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-30-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-30-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-30-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-30-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-30-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r1e0e233f733da10c3280e7afaf54c4c86f476d0f)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ed376ee490f7f7f78f1acbe712885be489e294ece) **[RFC PATCH 30/73] KVM: x86/PVM: Implement segment related callbacks**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(28 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r1e0e233f733da10c3280e7afaf54c4c86f476d0f)
  2024-02-26 14:35 ` [[RFC PATCH 29/73] KVM: x86/PVM: Implement allowed range checking for #PF](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m1e0e233f733da10c3280e7afaf54c4c86f476d0f) Lai Jiangshan
**@ 2024-02-26 14:35 ` Lai Jiangshan**
  2024-02-26 14:35 ` [[RFC PATCH 31/73] KVM: x86/PVM: Implement instruction emulation for #UD and #GP](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5396337203ffa3dcf5a0740edb14f424af5c53dd) Lai Jiangshan
                   ` [(44 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r5396337203ffa3dcf5a0740edb14f424af5c53dd)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rd376ee490f7f7f78f1acbe712885be489e294ece)
From: Lai Jiangshan @ 2024-02-26 14:35 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-31-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-31-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143616)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143616), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

Segmentation in PVM guest is generally disabled and is only available
for instruction emulation. The segment descriptors of segment registers
are just cached and do not take effect in hardware. Since the PVM guest
is only allowed to run in x86 long mode, the value of guest CS/SS is
fixed and depends on the current mode.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/kvm/pvm/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-31-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) | 128 +++++++++++++++++++++++++++++++++++++++++
 1 file [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ed376ee490f7f7f78f1acbe712885be489e294ece), 128 insertions(+)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-31-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) --git a/arch/x86/kvm/pvm/pvm.c b/arch/x86/kvm/pvm/pvm.c
index 26b2201f7dde..6f91dffb6c50 100644
--- a/arch/x86/kvm/pvm/pvm.c
+++ b/arch/x86/kvm/pvm/pvm.c @@ -630,6 +630,52 @@ static int pvm_set_msr(struct kvm_vcpu *vcpu, struct msr_data *msr_info)
 	return ret;
 }

+static void pvm_get_segment(struct kvm_vcpu *vcpu,
+			    struct kvm_segment *var, int seg)
+{
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+
+	// Update CS or SS to reflect the current mode.
+	if (seg == VCPU_SREG_CS) {
+		if (is_smod(pvm)) {
+			pvm->segments[seg].selector = kernel_cs_by_msr(pvm->msr_star);
+			pvm->segments[seg].dpl = 0;
+			pvm->segments[seg].l = 1;
+			pvm->segments[seg].db = 0;
+		} else {
+			pvm->segments[seg].selector = pvm->hw_cs >> 3;
+			pvm->segments[seg].dpl = 3;
+			if (pvm->hw_cs == __USER_CS) {
+				pvm->segments[seg].l = 1;
+				pvm->segments[seg].db = 0;
+			} else { // __USER32_CS
+				pvm->segments[seg].l = 0;
+				pvm->segments[seg].db = 1;
+			}
+		}
+	} else if (seg == VCPU_SREG_SS) {
+		if (is_smod(pvm)) {
+			pvm->segments[seg].dpl = 0;
+			pvm->segments[seg].selector = kernel_ds_by_msr(pvm->msr_star);
+		} else {
+			pvm->segments[seg].dpl = 3;
+			pvm->segments[seg].selector = pvm->hw_ss >> 3;
+		}
+	}
+
+	// Update DS/ES/FS/GS states from the hardware when the states are loaded.
+	pvm_switch_to_host(pvm);
+	*var = pvm->segments[seg];
+}
+
+static u64 pvm_get_segment_base(struct kvm_vcpu *vcpu, int seg)
+{
+	struct kvm_segment var;
+
+	pvm_get_segment(vcpu, &var, seg);
+	return var.base;
+}
+
 static int pvm_get_cpl(struct kvm_vcpu *vcpu)
 {
 	if (is_smod(to_pvm(vcpu)))
@@ -637,6 +683,80 @@ static int pvm_get_cpl(struct kvm_vcpu *vcpu)
 	return 3;
 }

+static void pvm_set_segment(struct kvm_vcpu *vcpu, struct kvm_segment *var, int seg)
+{
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+	int cpl = pvm_get_cpl(vcpu);
+
+	// Unload DS/ES/FS/GS states from hardware before changing them.
+	// It also has to unload the VCPU when leaving PVM mode.
+	pvm_switch_to_host(pvm);
+	pvm->segments[seg] = *var;
+
+	switch (seg) {
+	case VCPU_SREG_CS:
+		if (var->dpl == 1 || var->dpl == 2)
+			goto invalid_change;
+		if (!kvm_vcpu_has_run(vcpu)) {
+			// CPL changing is only valid for the first changed
+			// after the vcpu is created (vm-migration).
+			if (cpl != var->dpl)
+				pvm_switch_flags_toggle_mod(pvm);
+		} else {
+			if (cpl != var->dpl)
+				goto invalid_change;
+			if (cpl == 0 && !var->l)
+				goto invalid_change;
+		}
+		break;
+	case VCPU_SREG_LDTR:
+		// pvm doesn't support LDT
+		if (var->selector)
+			goto invalid_change;
+		break;
+	default:
+		break;
+	}
+
+	return;
+
+invalid_change:
+	kvm_make_request(KVM_REQ_TRIPLE_FAULT, vcpu);
+}
+
+static void pvm_get_cs_db_l_bits(struct kvm_vcpu *vcpu, int *db, int *l)
+{
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+
+	if (pvm->hw_cs == __USER_CS) {
+		*db = 0;
+		*l = 1;
+	} else {
+		*db = 1;
+		*l = 0;
+	}
+}
+
+static void pvm_get_idt(struct kvm_vcpu *vcpu, struct desc_ptr *dt)
+{
+	*dt = to_pvm(vcpu)->idt_ptr;
+}
+
+static void pvm_set_idt(struct kvm_vcpu *vcpu, struct desc_ptr *dt)
+{
+	to_pvm(vcpu)->idt_ptr = *dt;
+}
+
+static void pvm_get_gdt(struct kvm_vcpu *vcpu, struct desc_ptr *dt)
+{
+	*dt = to_pvm(vcpu)->gdt_ptr;
+}
+
+static void pvm_set_gdt(struct kvm_vcpu *vcpu, struct desc_ptr *dt)
+{
+	to_pvm(vcpu)->gdt_ptr = *dt;
+}
+
 static void pvm_deliver_interrupt(struct kvm_lapic *apic, int delivery_mode,
 				  int trig_mode, int vector)
 {
@@ -1545,8 +1665,16 @@ static struct kvm_x86_ops pvm_x86_ops __initdata = {
 	.get_msr_feature = pvm_get_msr_feature,
 	.get_msr = pvm_get_msr,
 	.set_msr = pvm_set_msr,
+	.get_segment_base = pvm_get_segment_base,
+	.get_segment = pvm_get_segment,
+	.set_segment = pvm_set_segment,
 	.get_cpl = pvm_get_cpl,
+	.get_cs_db_l_bits = pvm_get_cs_db_l_bits,
 	.load_mmu_pgd = pvm_load_mmu_pgd,
+	.get_gdt = pvm_get_gdt,
+	.set_gdt = pvm_set_gdt,
+	.get_idt = pvm_get_idt,
+	.set_idt = pvm_set_idt,
 	.get_rflags = pvm_get_rflags,
 	.set_rflags = pvm_set_rflags,
 	.get_if_flag = pvm_get_if_flag,
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#md376ee490f7f7f78f1acbe712885be489e294ece) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-31-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-31-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-31-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-31-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-31-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-31-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rd376ee490f7f7f78f1acbe712885be489e294ece)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e5396337203ffa3dcf5a0740edb14f424af5c53dd) **[RFC PATCH 31/73] KVM: x86/PVM: Implement instruction emulation for #UD and #GP**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(29 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rd376ee490f7f7f78f1acbe712885be489e294ece)
  2024-02-26 14:35 ` [[RFC PATCH 30/73] KVM: x86/PVM: Implement segment related callbacks](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#md376ee490f7f7f78f1acbe712885be489e294ece) Lai Jiangshan
**@ 2024-02-26 14:35 ` Lai Jiangshan**
  2024-02-26 14:35 ` [[RFC PATCH 32/73] KVM: x86/PVM: Enable guest debugging functions](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m7ef1d017b302aff61b6ddea0d53bd06db238e693) Lai Jiangshan
                   ` [(43 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r7ef1d017b302aff61b6ddea0d53bd06db238e693)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r5396337203ffa3dcf5a0740edb14f424af5c53dd)
From: Lai Jiangshan @ 2024-02-26 14:35 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-32-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-32-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143619)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143619), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

The privilege instruction in supervisor mode will trigger a #GP and
induce VM exit. Therefore, PVM reuses the existing x86 emulator in PVM
to support privilege instruction emulation in supervisor mode.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/kvm/pvm/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-32-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) | 38 ++++++++++++++++++++++++++++++++++++++
 1 file [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e5396337203ffa3dcf5a0740edb14f424af5c53dd), 38 insertions(+)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-32-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) --git a/arch/x86/kvm/pvm/pvm.c b/arch/x86/kvm/pvm/pvm.c
index 6f91dffb6c50..4ec8c2c514ca 100644
--- a/arch/x86/kvm/pvm/pvm.c
+++ b/arch/x86/kvm/pvm/pvm.c @@ -402,6 +402,40 @@ static void pvm_sched_in(struct kvm_vcpu *vcpu, int cpu)
 {
 }

+static void pvm_patch_hypercall(struct kvm_vcpu *vcpu, unsigned char *hypercall)
+{
+	/* KVM_X86_QUIRK_FIX_HYPERCALL_INSN should not be enabled for pvm guest */
+
+	/* ud2; int3; */
+	hypercall[0] = 0x0F;
+	hypercall[1] = 0x0B;
+	hypercall[2] = 0xCC;
+}
+
+static int pvm_check_emulate_instruction(struct kvm_vcpu *vcpu, int emul_type,
+					 void *insn, int insn_len)
+{
+	return X86EMUL_CONTINUE;
+}
+
+static int skip_emulated_instruction(struct kvm_vcpu *vcpu)
+{
+	return kvm_emulate_instruction(vcpu, EMULTYPE_SKIP);
+}
+
+static int pvm_check_intercept(struct kvm_vcpu *vcpu,
+			       struct x86_instruction_info *info,
+			       enum x86_intercept_stage stage,
+			       struct x86_exception *exception)
+{
+	/*
+	 * HF_GUEST_MASK is not used even nested pvm is supported. L0 pvm
+	 * might even be unaware the L1 pvm.
+	 */
+	WARN_ON_ONCE(1);
+	return X86EMUL_CONTINUE;
+}
+
 static void pvm_set_msr_linear_address_range(struct vcpu_pvm *pvm,
 					     u64 pml4_i_s, u64 pml4_i_e,
 					     u64 pml5_i_s, u64 pml5_i_e)
@@ -1682,8 +1716,10 @@ static struct kvm_x86_ops pvm_x86_ops __initdata = {
 	.vcpu_pre_run = pvm_vcpu_pre_run,
 	.vcpu_run = pvm_vcpu_run,
 	.handle_exit = pvm_handle_exit,
+	.skip_emulated_instruction = skip_emulated_instruction,
 	.set_interrupt_shadow = pvm_set_interrupt_shadow,
 	.get_interrupt_shadow = pvm_get_interrupt_shadow,
+	.patch_hypercall = pvm_patch_hypercall,
 	.inject_irq = pvm_inject_irq,
 	.inject_nmi = pvm_inject_nmi,
 	.inject_exception = pvm_inject_exception,
@@ -1699,6 +1735,7 @@ static struct kvm_x86_ops pvm_x86_ops __initdata = {

 	.vcpu_after_set_cpuid = pvm_vcpu_after_set_cpuid,

+	.check_intercept = pvm_check_intercept,
 	.handle_exit_irqoff = pvm_handle_exit_irqoff,

 	.request_immediate_exit = __kvm_request_immediate_exit,
@@ -1721,6 +1758,7 @@ static struct kvm_x86_ops pvm_x86_ops __initdata = {
 	.complete_emulated_msr = kvm_complete_insn_gp,
 	.vcpu_deliver_sipi_vector = kvm_vcpu_deliver_sipi_vector,

+	.check_emulate_instruction = pvm_check_emulate_instruction,
 	.disallowed_va = pvm_disallowed_va,
 	.vcpu_gpc_refresh = pvm_vcpu_gpc_refresh,
 };
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5396337203ffa3dcf5a0740edb14f424af5c53dd) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-32-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-32-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-32-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-32-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-32-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-32-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r5396337203ffa3dcf5a0740edb14f424af5c53dd)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e7ef1d017b302aff61b6ddea0d53bd06db238e693) **[RFC PATCH 32/73] KVM: x86/PVM: Enable guest debugging functions**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(30 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r5396337203ffa3dcf5a0740edb14f424af5c53dd)
  2024-02-26 14:35 ` [[RFC PATCH 31/73] KVM: x86/PVM: Implement instruction emulation for #UD and #GP](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5396337203ffa3dcf5a0740edb14f424af5c53dd) Lai Jiangshan
**@ 2024-02-26 14:35 ` Lai Jiangshan**
  2024-02-26 14:35 ` [[RFC PATCH 33/73] KVM: x86/PVM: Handle VM-exit due to hardware exceptions](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#me5509b00e1fd0d054e9cd1221f60150678e7902a) Lai Jiangshan
                   ` [(42 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#re5509b00e1fd0d054e9cd1221f60150678e7902a)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r7ef1d017b302aff61b6ddea0d53bd06db238e693)
From: Lai Jiangshan @ 2024-02-26 14:35 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-33-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-33-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143623)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143623), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

The guest DR7 is loaded before VM enter to enable debugging functions
for the guest. If guest debugging is not enabled, the #DB and #BP
exceptions are reinjected into the guest directly; otherwise, they are
handled by the hypervisor.

However, DR7_GD is cleared since debug register read/write is a
privileged instruction, which always leads to a VM exit for #GP. The
address of breakpoints is limited to the allowed address range, similar
to the check in the #PF path.  Guest DR7 is loaded before VM enter to
enable debug function for guest.  If guest debug is not enabled, the #DB
and #BP are reinjected into guest directly, otherwise, they are handled
by hypervisor similar to VMX.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/kvm/pvm/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-33-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) | 96 ++++++++++++++++++++++++++++++++++++++++++
 [arch/x86/kvm/pvm/pvm.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-33-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.h) |  3 ++
 2 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e7ef1d017b302aff61b6ddea0d53bd06db238e693), 99 insertions(+)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-33-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) --git a/arch/x86/kvm/pvm/pvm.c b/arch/x86/kvm/pvm/pvm.c
index 4ec8c2c514ca..299305903005 100644
--- a/arch/x86/kvm/pvm/pvm.c
+++ b/arch/x86/kvm/pvm/pvm.c @@ -383,6 +383,8 @@ static void pvm_vcpu_load(struct kvm_vcpu *vcpu, int cpu)
 {
 	struct vcpu_pvm *pvm = to_pvm(vcpu);

+	pvm->host_debugctlmsr = get_debugctlmsr();
+
 	if (__this_cpu_read(active_pvm_vcpu) == pvm && vcpu->cpu == cpu)
 		return;

@@ -533,6 +535,9 @@ static int pvm_get_msr(struct kvm_vcpu *vcpu, struct msr_data *msr_info)
 	case MSR_IA32_SYSENTER_ESP:
 		msr_info->data = pvm->unused_MSR_IA32_SYSENTER_ESP;
 		break;
+	case MSR_IA32_DEBUGCTLMSR:
+		msr_info->data = 0;
+		break;
 	case MSR_PVM_VCPU_STRUCT:
 		msr_info->data = pvm->msr_vcpu_struct;
 		break;
@@ -619,6 +624,9 @@ static int pvm_set_msr(struct kvm_vcpu *vcpu, struct msr_data *msr_info)
 	case MSR_IA32_SYSENTER_ESP:
 		pvm->unused_MSR_IA32_SYSENTER_ESP = data;
 		break;
+	case MSR_IA32_DEBUGCTLMSR:
+		/* It is ignored now. */
+		break;
 	case MSR_PVM_VCPU_STRUCT:
 		if (!PAGE_ALIGNED(data))
 			return 1;
@@ -810,6 +818,10 @@ static bool pvm_apic_init_signal_blocked(struct kvm_vcpu *vcpu)
 	return false;
 }

+static void update_exception_bitmap(struct kvm_vcpu *vcpu)
+{
+}
+
 static struct pvm_vcpu_struct *pvm_get_vcpu_struct(struct vcpu_pvm *pvm)
 {
 	struct gfn_to_pfn_cache *gpc = &pvm->pvcs_gpc;
@@ -1235,6 +1247,72 @@ static int pvm_vcpu_pre_run(struct kvm_vcpu *vcpu)
 	return 1;
 }

+static void pvm_sync_dirty_debug_regs(struct kvm_vcpu *vcpu)
+{
+	WARN_ONCE(1, "pvm never sets KVM_DEBUGREG_WONT_EXIT\n");
+}
+
+static void pvm_set_dr7(struct kvm_vcpu *vcpu, unsigned long val)
+{
+	to_pvm(vcpu)->guest_dr7 = val;
+}
+
+static __always_inline unsigned long __dr7_enable_mask(int drnum)
+{
+	unsigned long bp_mask = 0;
+
+	bp_mask |= (DR_LOCAL_ENABLE << (drnum * DR_ENABLE_SIZE));
+	bp_mask |= (DR_GLOBAL_ENABLE << (drnum * DR_ENABLE_SIZE));
+
+	return bp_mask;
+}
+
+static __always_inline unsigned long __dr7_mask(int drnum)
+{
+	unsigned long bp_mask = 0xf;
+
+	bp_mask <<= (DR_CONTROL_SHIFT + drnum * DR_CONTROL_SIZE);
+	bp_mask |= __dr7_enable_mask(drnum);
+
+	return bp_mask;
+}
+
+/*
+ * Calculate the correct dr7 for the hardware to avoid the host
+ * being watched.
+ *
+ * It only needs to be calculated each time when vcpu->arch.eff_db or
+ * pvm->guest_dr7 is changed.  But now it is calculated each time on
+ * VM-enter since there is no proper callback for vcpu->arch.eff_db and
+ * it is slow path.
+ */
+static __always_inline unsigned long pvm_eff_dr7(struct kvm_vcpu *vcpu)
+{
+	unsigned long eff_dr7 = to_pvm(vcpu)->guest_dr7;
+	int i;
+
+	/*
+	 * DR7_GD should not be set to hardware. And it doesn't need to be
+	 * set to hardware since PVM guest is running on hardware ring3.
+	 * All access to debug registers will be trapped and the emulation
+	 * code can handle DR7_GD correctly for PVM.
+	 */
+	eff_dr7 &= ~DR7_GD;
+
+	/*
+	 * Disallow addresses that are not for the guest, especially addresses
+	 * on the host entry code.
+	 */
+	for (i = 0; i < KVM_NR_DB_REGS; i++) {
+		if (!pvm_guest_allowed_va(vcpu, vcpu->arch.eff_db[i]))
+			eff_dr7 &= ~__dr7_mask(i);
+		if (!pvm_guest_allowed_va(vcpu, vcpu->arch.eff_db[i] + 7))
+			eff_dr7 &= ~__dr7_mask(i);
+	}
+
+	return eff_dr7;
+}
+
 // Save guest registers from host sp0 or IST stack.
 static __always_inline void save_regs(struct kvm_vcpu *vcpu, struct pt_regs *guest)
 {
@@ -1301,6 +1379,9 @@ static noinstr void pvm_vcpu_run_noinstr(struct kvm_vcpu *vcpu)
 	// Load guest registers into the host sp0 stack for switcher.
 	load_regs(vcpu, sp0_regs);

+	if (unlikely(pvm->guest_dr7 & DR7_BP_EN_MASK))
+		set_debugreg(pvm_eff_dr7(vcpu), 7);
+
 	// Call into switcher and enter guest.
 	ret_regs = switcher_enter_guest();

@@ -1309,6 +1390,11 @@ static noinstr void pvm_vcpu_run_noinstr(struct kvm_vcpu *vcpu)
 	pvm->exit_vector = (ret_regs->orig_ax >> 32);
 	pvm->exit_error_code = (u32)ret_regs->orig_ax;

+	// dr7 requires to be zero when the controling of debug registers
+	// passes back to the host.
+	if (unlikely(pvm->guest_dr7 & DR7_BP_EN_MASK))
+		set_debugreg(0, 7);
+
 	// handle noinstr vmexits reasons.
 	switch (pvm->exit_vector) {
 	case PF_VECTOR:
@@ -1387,8 +1473,15 @@ static fastpath_t pvm_vcpu_run(struct kvm_vcpu *vcpu)

 	pvm_set_host_cr3(pvm);

+	if (pvm->host_debugctlmsr)
+		update_debugctlmsr(0);
+
 	pvm_vcpu_run_noinstr(vcpu);

+	/* MSR_IA32_DEBUGCTLMSR is zeroed before vmenter. Restore it if needed */
+	if (pvm->host_debugctlmsr)
+		update_debugctlmsr(pvm->host_debugctlmsr);
+
 	if (is_smod(pvm)) {
 		struct pvm_vcpu_struct *pvcs = pvm->pvcs_gpc.khva;

@@ -1696,6 +1789,7 @@ static struct kvm_x86_ops pvm_x86_ops __initdata = {
 	.vcpu_load = pvm_vcpu_load,
 	.vcpu_put = pvm_vcpu_put,

+	.update_exception_bitmap = update_exception_bitmap,
 	.get_msr_feature = pvm_get_msr_feature,
 	.get_msr = pvm_get_msr,
 	.set_msr = pvm_set_msr,
@@ -1709,6 +1803,8 @@ static struct kvm_x86_ops pvm_x86_ops __initdata = {
 	.set_gdt = pvm_set_gdt,
 	.get_idt = pvm_get_idt,
 	.set_idt = pvm_set_idt,
+	.set_dr7 = pvm_set_dr7,
+	.sync_dirty_debug_regs = pvm_sync_dirty_debug_regs,
 	.get_rflags = pvm_get_rflags,
 	.set_rflags = pvm_set_rflags,
 	.get_if_flag = pvm_get_if_flag,
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-33-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.h) --git a/arch/x86/kvm/pvm/pvm.h b/arch/x86/kvm/pvm/pvm.h
index bf3a6a1837c0..4cdcbed1c813 100644
--- a/arch/x86/kvm/pvm/pvm.h
+++ b/arch/x86/kvm/pvm/pvm.h @@ -37,6 +37,7 @@ struct vcpu_pvm {
 	unsigned long switch_flags;

 	u16 host_ds_sel, host_es_sel;
+	u64 host_debugctlmsr;

 	union {
 		unsigned long exit_extra;
@@ -52,6 +53,8 @@ struct vcpu_pvm {
 	int int_shadow;
 	bool nmi_mask;

+	unsigned long guest_dr7;
+
 	struct gfn_to_pfn_cache pvcs_gpc;

 	// emulated x86 msrs
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m7ef1d017b302aff61b6ddea0d53bd06db238e693) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-33-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-33-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-33-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-33-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-33-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-33-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r7ef1d017b302aff61b6ddea0d53bd06db238e693)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ee5509b00e1fd0d054e9cd1221f60150678e7902a) **[RFC PATCH 33/73] KVM: x86/PVM: Handle VM-exit due to hardware exceptions**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(31 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r7ef1d017b302aff61b6ddea0d53bd06db238e693)
  2024-02-26 14:35 ` [[RFC PATCH 32/73] KVM: x86/PVM: Enable guest debugging functions](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m7ef1d017b302aff61b6ddea0d53bd06db238e693) Lai Jiangshan
**@ 2024-02-26 14:35 ` Lai Jiangshan**
  2024-02-26 14:35 ` [[RFC PATCH 34/73] KVM: x86/PVM: Handle ERETU/ERETS synthetic instruction](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mc42f8da11f3737be4b493b5bcada0e082f08f631) Lai Jiangshan
                   ` [(41 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rc42f8da11f3737be4b493b5bcada0e082f08f631)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#re5509b00e1fd0d054e9cd1221f60150678e7902a)
From: Lai Jiangshan @ 2024-02-26 14:35 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-34-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-34-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143626)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143626), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

When the exceptions are of interest to the hypervisor for emulation or
debugging, they should be handled by the hypervisor first, for example,
handling #PF for shadow page table. If the exceptions are pure guest
exceptions, they should be reinjected into the guest directly. If the
exceptions belong to the host, they should already have been handled in
an atomic way before enabling interrupts.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/kvm/pvm/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-34-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) | 157 +++++++++++++++++++++++++++++++++++++++++
 1 file [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ee5509b00e1fd0d054e9cd1221f60150678e7902a), 157 insertions(+)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-34-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) --git a/arch/x86/kvm/pvm/pvm.c b/arch/x86/kvm/pvm/pvm.c
index 299305903005..c6fd01c19c3e 100644
--- a/arch/x86/kvm/pvm/pvm.c
+++ b/arch/x86/kvm/pvm/pvm.c @@ -20,6 +20,7 @@

 #include "cpuid.h"
 #include "lapic.h"
+#include "mmu.h"
 #include "trace.h"
 #include "x86.h"
 #include "pvm.h"
@@ -1161,6 +1162,160 @@ static int handle_exit_syscall(struct kvm_vcpu *vcpu)
 	return 1;
 }

+static int handle_exit_debug(struct kvm_vcpu *vcpu)
+{
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+	struct kvm_run *kvm_run = pvm->vcpu.run;
+
+	if (pvm->vcpu.guest_debug &
+	    (KVM_GUESTDBG_SINGLESTEP | KVM_GUESTDBG_USE_HW_BP)) {
+		kvm_run->exit_reason = KVM_EXIT_DEBUG;
+		kvm_run->debug.arch.dr6 = pvm->exit_dr6 | DR6_FIXED_1 | DR6_RTM;
+		kvm_run->debug.arch.dr7 = vcpu->arch.guest_debug_dr7;
+		kvm_run->debug.arch.pc = kvm_rip_read(vcpu);
+		kvm_run->debug.arch.exception = DB_VECTOR;
+		return 0;
+	}
+
+	kvm_queue_exception_p(vcpu, DB_VECTOR, pvm->exit_dr6);
+	return 1;
+}
+
+/* check if the previous instruction is "int3" on receiving #BP */
+static bool is_bp_trap(struct kvm_vcpu *vcpu)
+{
+	u8 byte = 0;
+	unsigned long rip;
+	struct x86_exception exception;
+	int r;
+
+	rip = kvm_rip_read(vcpu) - 1;
+	r = kvm_read_guest_virt(vcpu, rip, &byte, 1, &exception);
+
+	/* Just assume it to be int3 when failed to fetch the instruction. */
+	if (r)
+		return true;
+
+	return byte == 0xcc;
+}
+
+static int handle_exit_breakpoint(struct kvm_vcpu *vcpu)
+{
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+	struct kvm_run *kvm_run = pvm->vcpu.run;
+
+	/*
+	 * Breakpoint exception can be caused by int3 or int 3.  While "int3"
+	 * participates in guest debug, but "int 3" should not.
+	 */
+	if ((vcpu->guest_debug & KVM_GUESTDBG_USE_SW_BP) && is_bp_trap(vcpu)) {
+		kvm_rip_write(vcpu, kvm_rip_read(vcpu) - 1);
+		kvm_run->exit_reason = KVM_EXIT_DEBUG;
+		kvm_run->debug.arch.pc = kvm_rip_read(vcpu);
+		kvm_run->debug.arch.exception = BP_VECTOR;
+		return 0;
+	}
+
+	kvm_queue_exception(vcpu, BP_VECTOR);
+	return 1;
+}
+
+static int handle_exit_exception(struct kvm_vcpu *vcpu)
+{
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+	struct kvm_run *kvm_run = vcpu->run;
+	u32 vector, error_code;
+	int err;
+
+	vector = pvm->exit_vector;
+	error_code = pvm->exit_error_code;
+
+	switch (vector) {
+	// #PF, #GP, #UD, #DB and #BP are guest exceptions or hypervisor
+	// interested exceptions for emulation or debugging.
+	case PF_VECTOR:
+		// Remove hardware generated PFERR_USER_MASK when in supervisor
+		// mode to reflect the real mode in PVM.
+		if (is_smod(pvm))
+			error_code &= ~PFERR_USER_MASK;
+
+		// If it is a PK fault, set pkru=0 and re-enter the guest silently.
+		// See the comment before pvm_load_guest_xsave_state().
+		if (cpu_feature_enabled(X86_FEATURE_PKU) && (error_code & PFERR_PK_MASK))
+			return 1;
+
+		return kvm_handle_page_fault(vcpu, error_code, pvm->exit_cr2,
+					     NULL, 0);
+	case GP_VECTOR:
+		err = kvm_emulate_instruction(vcpu, EMULTYPE_PVM_GP);
+		if (!err)
+			return 0;
+
+		if (vcpu->arch.halt_request) {
+			vcpu->arch.halt_request = 0;
+			return kvm_emulate_halt_noskip(vcpu);
+		}
+		return 1;
+	case UD_VECTOR:
+		if (!is_smod(pvm)) {
+			kvm_queue_exception(vcpu, UD_VECTOR);
+			return 1;
+		}
+		return handle_ud(vcpu);
+	case DB_VECTOR:
+		return handle_exit_debug(vcpu);
+	case BP_VECTOR:
+		return handle_exit_breakpoint(vcpu);
+
+	// #DE, #OF, #BR, #NM, #MF, #XM, #TS, #NP, #SS and #AC are pure guest
+	// exceptions.
+	case DE_VECTOR:
+	case OF_VECTOR:
+	case BR_VECTOR:
+	case NM_VECTOR:
+	case MF_VECTOR:
+	case XM_VECTOR:
+		kvm_queue_exception(vcpu, vector);
+		return 1;
+	case AC_VECTOR:
+	case TS_VECTOR:
+	case NP_VECTOR:
+	case SS_VECTOR:
+		kvm_queue_exception_e(vcpu, vector, error_code);
+		return 1;
+
+	// #NMI, #VE, #VC, #MC and #DF are exceptions that belong to host.
+	// They should have been handled in atomic way when vmexit.
+	case NMI_VECTOR:
+		// NMI is handled by pvm_vcpu_run_noinstr().
+		return 1;
+	case VE_VECTOR:
+		// TODO: tdx_handle_virt_exception(regs, &pvm->exit_ve); break;
+		goto unknown_exit_reason;
+	case X86_TRAP_VC:
+		// TODO: handle the second part for #VC.
+		goto unknown_exit_reason;
+	case MC_VECTOR:
+		// MC is handled by pvm_handle_exit_irqoff().
+		// TODO: split kvm_machine_check() to avoid irq-enabled or
+		// schedule code (thread dead) in pvm_handle_exit_irqoff().
+		return 1;
+	case DF_VECTOR:
+		// DF is handled when exiting and can't reach here.
+		pr_warn_once("host bug, can't reach here");
+		break;
+	default:
+unknown_exit_reason:
+		pr_warn_once("unknown exit_reason vector:%d, error_code:%x, rip:0x%lx\n",
+			      vector, pvm->exit_error_code, kvm_rip_read(vcpu));
+		kvm_run->exit_reason = KVM_EXIT_EXCEPTION;
+		kvm_run->ex.exception = vector;
+		kvm_run->ex.error_code = error_code;
+		break;
+	}
+	return 0;
+}
+
 static int handle_exit_external_interrupt(struct kvm_vcpu *vcpu)
 {
 	++vcpu->stat.irq_exits;
@@ -1187,6 +1342,8 @@ static int pvm_handle_exit(struct kvm_vcpu *vcpu, fastpath_t exit_fastpath)

 	if (exit_reason == PVM_SYSCALL_VECTOR)
 		return handle_exit_syscall(vcpu);
+	else if (exit_reason >= 0 && exit_reason < FIRST_EXTERNAL_VECTOR)
+		return handle_exit_exception(vcpu);
 	else if (exit_reason == IA32_SYSCALL_VECTOR)
 		return do_pvm_event(vcpu, IA32_SYSCALL_VECTOR, false, 0);
 	else if (exit_reason >= FIRST_EXTERNAL_VECTOR && exit_reason < NR_VECTORS)
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#me5509b00e1fd0d054e9cd1221f60150678e7902a) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-34-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-34-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-34-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-34-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-34-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-34-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#re5509b00e1fd0d054e9cd1221f60150678e7902a)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ec42f8da11f3737be4b493b5bcada0e082f08f631) **[RFC PATCH 34/73] KVM: x86/PVM: Handle ERETU/ERETS synthetic instruction**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(32 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#re5509b00e1fd0d054e9cd1221f60150678e7902a)
  2024-02-26 14:35 ` [[RFC PATCH 33/73] KVM: x86/PVM: Handle VM-exit due to hardware exceptions](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#me5509b00e1fd0d054e9cd1221f60150678e7902a) Lai Jiangshan
**@ 2024-02-26 14:35 ` Lai Jiangshan**
  2024-02-26 14:35 ` [[RFC PATCH 35/73] KVM: x86/PVM: Handle PVM_SYNTHETIC_CPUID](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5dc2eca0376fb278548d57d6a9e719e2461aadd5) " Lai Jiangshan
                   ` [(40 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r5dc2eca0376fb278548d57d6a9e719e2461aadd5)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rc42f8da11f3737be4b493b5bcada0e082f08f631)
From: Lai Jiangshan @ 2024-02-26 14:35 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-35-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-35-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143629)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143629), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

PVM uses the ERETU synthetic instruction to return to user mode and the
ERETS instruction to return to supervisor mode. Similar to event
injection, information passing is different. For the ERETU, information
is passed by the shared PVCS structure, and for the ERETS, information
is passed by the current guest stack.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/kvm/pvm/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-35-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) | 74 ++++++++++++++++++++++++++++++++++++++++++
 1 file [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ec42f8da11f3737be4b493b5bcada0e082f08f631), 74 insertions(+)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-35-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) --git a/arch/x86/kvm/pvm/pvm.c b/arch/x86/kvm/pvm/pvm.c
index c6fd01c19c3e..514f0573f70f 100644
--- a/arch/x86/kvm/pvm/pvm.c
+++ b/arch/x86/kvm/pvm/pvm.c @@ -1153,12 +1153,86 @@ static void pvm_setup_mce(struct kvm_vcpu *vcpu)
 {
 }

+static int handle_synthetic_instruction_return_user(struct kvm_vcpu *vcpu)
+{
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+	struct pvm_vcpu_struct *pvcs;
+
+	// instruction to return user means nmi allowed.
+	pvm->nmi_mask = false;
+
+	/*
+	 * switch to user mode before kvm_set_rflags() to avoid PVM_EVENT_FLAGS_IF
+	 * to be set.
+	 */
+	switch_to_umod(vcpu);
+
+	pvcs = pvm_get_vcpu_struct(pvm);
+	if (!pvcs) {
+		kvm_make_request(KVM_REQ_TRIPLE_FAULT, vcpu);
+		return 1;
+	}
+
+	/*
+	 * pvm_set_rflags() doesn't clear PVM_EVENT_FLAGS_IP
+	 * for user mode, so clear it here.
+	 */
+	if (pvcs->event_flags & PVM_EVENT_FLAGS_IP) {
+		pvcs->event_flags &= ~PVM_EVENT_FLAGS_IP;
+		kvm_make_request(KVM_REQ_EVENT, vcpu);
+	}
+
+	pvm->hw_cs = pvcs->user_cs | USER_RPL;
+	pvm->hw_ss = pvcs->user_ss | USER_RPL;
+
+	pvm_write_guest_gs_base(pvm, pvcs->user_gsbase);
+	kvm_set_rflags(vcpu, pvcs->eflags | X86_EFLAGS_IF);
+	kvm_rip_write(vcpu, pvcs->rip);
+	kvm_rsp_write(vcpu, pvcs->rsp);
+	kvm_rcx_write(vcpu, pvcs->rcx);
+	kvm_r11_write(vcpu, pvcs->r11);
+
+	pvm_put_vcpu_struct(pvm, false);
+
+	return 1;
+}
+
+static int handle_synthetic_instruction_return_supervisor(struct kvm_vcpu *vcpu)
+{
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+	unsigned long rsp = kvm_rsp_read(vcpu);
+	struct pvm_supervisor_event frame;
+	struct x86_exception e;
+
+	if (kvm_read_guest_virt(vcpu, rsp, &frame, sizeof(frame), &e)) {
+		kvm_make_request(KVM_REQ_TRIPLE_FAULT, vcpu);
+		return 1;
+	}
+
+	// instruction to return supervisor means nmi allowed.
+	pvm->nmi_mask = false;
+
+	kvm_set_rflags(vcpu, frame.rflags);
+	kvm_rip_write(vcpu, frame.rip);
+	kvm_rsp_write(vcpu, frame.rsp);
+	kvm_rcx_write(vcpu, frame.rcx);
+	kvm_r11_write(vcpu, frame.r11);
+
+	return 1;
+}
+
 static int handle_exit_syscall(struct kvm_vcpu *vcpu)
 {
 	struct vcpu_pvm *pvm = to_pvm(vcpu);
+	unsigned long rip = kvm_rip_read(vcpu);

 	if (!is_smod(pvm))
 		return do_pvm_user_event(vcpu, PVM_SYSCALL_VECTOR, false, 0);
+
+	if (rip == pvm->msr_retu_rip_plus2)
+		return handle_synthetic_instruction_return_user(vcpu);
+	if (rip == pvm->msr_rets_rip_plus2)
+		return handle_synthetic_instruction_return_supervisor(vcpu);
 	return 1;
 }

--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mc42f8da11f3737be4b493b5bcada0e082f08f631) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-35-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-35-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-35-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-35-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-35-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-35-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rc42f8da11f3737be4b493b5bcada0e082f08f631)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e5dc2eca0376fb278548d57d6a9e719e2461aadd5) **[RFC PATCH 35/73] KVM: x86/PVM: Handle PVM_SYNTHETIC_CPUID synthetic instruction**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(33 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rc42f8da11f3737be4b493b5bcada0e082f08f631)
  2024-02-26 14:35 ` [[RFC PATCH 34/73] KVM: x86/PVM: Handle ERETU/ERETS synthetic instruction](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mc42f8da11f3737be4b493b5bcada0e082f08f631) Lai Jiangshan
**@ 2024-02-26 14:35 ` Lai Jiangshan**
  2024-02-26 14:35 ` [[RFC PATCH 36/73] KVM: x86/PVM: Handle KVM hypercall](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m3bb448f8fe42d888eb3384b3a04b0e38e77af341) Lai Jiangshan
                   ` [(39 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r3bb448f8fe42d888eb3384b3a04b0e38e77af341)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r5dc2eca0376fb278548d57d6a9e719e2461aadd5)
From: Lai Jiangshan @ 2024-02-26 14:35 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-36-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-36-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143633)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143633), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

The PVM guest utilizes the CPUID instruction for detecting PVM
hypervisor support. However, the CPUID instruction in the PVM guest is
not directly trapped and emulated. Instead, the PVM guest employs the
"invlpg 0xffffffffff4d5650; cpuid;" instructions to cause a #GP trap.
The hypervisor must identify this trap and handle the emulation of the
CPUID instruction within the #GP handling process.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/kvm/pvm/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-36-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) | 33 +++++++++++++++++++++++++++++++++
 1 file [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e5dc2eca0376fb278548d57d6a9e719e2461aadd5), 33 insertions(+)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-36-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) --git a/arch/x86/kvm/pvm/pvm.c b/arch/x86/kvm/pvm/pvm.c
index 514f0573f70f..a2602d9828a5 100644
--- a/arch/x86/kvm/pvm/pvm.c
+++ b/arch/x86/kvm/pvm/pvm.c @@ -1294,6 +1294,36 @@ static int handle_exit_breakpoint(struct kvm_vcpu *vcpu)
 	return 1;
 }

+static bool handle_synthetic_instruction_pvm_cpuid(struct kvm_vcpu *vcpu)
+{
+	/* invlpg 0xffffffffff4d5650; cpuid; */
+	static const char pvm_synthetic_cpuid_insns[] = { PVM_SYNTHETIC_CPUID };
+	char insns[10];
+	struct x86_exception e;
+
+	if (kvm_read_guest_virt(vcpu, kvm_get_linear_rip(vcpu),
+				insns, sizeof(insns), &e) == 0 &&
+	    memcmp(insns, pvm_synthetic_cpuid_insns, sizeof(insns)) == 0) {
+		u32 eax, ebx, ecx, edx;
+
+		if (unlikely(pvm_guest_allowed_va(vcpu, PVM_SYNTHETIC_CPUID_ADDRESS)))
+			kvm_mmu_invlpg(vcpu, PVM_SYNTHETIC_CPUID_ADDRESS);
+
+		eax = kvm_rax_read(vcpu);
+		ecx = kvm_rcx_read(vcpu);
+		kvm_cpuid(vcpu, &eax, &ebx, &ecx, &edx, false);
+		kvm_rax_write(vcpu, eax);
+		kvm_rbx_write(vcpu, ebx);
+		kvm_rcx_write(vcpu, ecx);
+		kvm_rdx_write(vcpu, edx);
+
+		kvm_rip_write(vcpu, kvm_rip_read(vcpu) + sizeof(insns));
+		return true;
+	}
+
+	return false;
+}
+
 static int handle_exit_exception(struct kvm_vcpu *vcpu)
 {
 	struct vcpu_pvm *pvm = to_pvm(vcpu);
@@ -1321,6 +1351,9 @@ static int handle_exit_exception(struct kvm_vcpu *vcpu)
 		return kvm_handle_page_fault(vcpu, error_code, pvm->exit_cr2,
 					     NULL, 0);
 	case GP_VECTOR:
+		if (is_smod(pvm) && handle_synthetic_instruction_pvm_cpuid(vcpu))
+			return 1;
+
 		err = kvm_emulate_instruction(vcpu, EMULTYPE_PVM_GP);
 		if (!err)
 			return 0;
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5dc2eca0376fb278548d57d6a9e719e2461aadd5) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-36-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-36-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-36-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-36-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-36-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-36-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r5dc2eca0376fb278548d57d6a9e719e2461aadd5)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e3bb448f8fe42d888eb3384b3a04b0e38e77af341) **[RFC PATCH 36/73] KVM: x86/PVM: Handle KVM hypercall**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(34 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r5dc2eca0376fb278548d57d6a9e719e2461aadd5)
  2024-02-26 14:35 ` [[RFC PATCH 35/73] KVM: x86/PVM: Handle PVM_SYNTHETIC_CPUID](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5dc2eca0376fb278548d57d6a9e719e2461aadd5) " Lai Jiangshan
**@ 2024-02-26 14:35 ` Lai Jiangshan**
  2024-02-26 14:35 ` [[RFC PATCH 37/73] KVM: x86/PVM: Use host PCID to reduce guest TLB flushing](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m2d1522a59d0e90ba1385d0417217234a95b13e8d) Lai Jiangshan
                   ` [(38 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r2d1522a59d0e90ba1385d0417217234a95b13e8d)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r3bb448f8fe42d888eb3384b3a04b0e38e77af341)
From: Lai Jiangshan @ 2024-02-26 14:35 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-37-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-37-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143636)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143636), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

PVM uses the syscall instruction as the hypercall instruction, so r10 is
used as a replacement for rcx since rcx is clobbered by the syscall.
Additionally, the syscall is a trap and does not need to skip the
hypercall instruction for PVM.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/kvm/pvm/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-37-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) | 15 ++++++++++++++-
 1 file [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e3bb448f8fe42d888eb3384b3a04b0e38e77af341), 14 insertions(+), 1 deletion(-)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-37-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) --git a/arch/x86/kvm/pvm/pvm.c b/arch/x86/kvm/pvm/pvm.c
index a2602d9828a5..242c355fda8f 100644
--- a/arch/x86/kvm/pvm/pvm.c
+++ b/arch/x86/kvm/pvm/pvm.c @@ -1221,6 +1221,18 @@ static int handle_synthetic_instruction_return_supervisor(struct kvm_vcpu *vcpu)
 	return 1;
 }

+static int handle_kvm_hypercall(struct kvm_vcpu *vcpu)
+{
+	int r;
+
+	// In PVM, r10 is the replacement for rcx in hypercall
+	kvm_rcx_write(vcpu, kvm_r10_read(vcpu));
+	r = kvm_emulate_hypercall_noskip(vcpu);
+	kvm_r10_write(vcpu, kvm_rcx_read(vcpu));
+
+	return r;
+}
+
 static int handle_exit_syscall(struct kvm_vcpu *vcpu)
 {
 	struct vcpu_pvm *pvm = to_pvm(vcpu);
@@ -1233,7 +1245,8 @@ static int handle_exit_syscall(struct kvm_vcpu *vcpu)
 		return handle_synthetic_instruction_return_user(vcpu);
 	if (rip == pvm->msr_rets_rip_plus2)
 		return handle_synthetic_instruction_return_supervisor(vcpu);
-	return 1; +
+	return handle_kvm_hypercall(vcpu);
 }

 static int handle_exit_debug(struct kvm_vcpu *vcpu)
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m3bb448f8fe42d888eb3384b3a04b0e38e77af341) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-37-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-37-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-37-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-37-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-37-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-37-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r3bb448f8fe42d888eb3384b3a04b0e38e77af341)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e2d1522a59d0e90ba1385d0417217234a95b13e8d) **[RFC PATCH 37/73] KVM: x86/PVM: Use host PCID to reduce guest TLB flushing**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(35 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r3bb448f8fe42d888eb3384b3a04b0e38e77af341)
  2024-02-26 14:35 ` [[RFC PATCH 36/73] KVM: x86/PVM: Handle KVM hypercall](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m3bb448f8fe42d888eb3384b3a04b0e38e77af341) Lai Jiangshan
**@ 2024-02-26 14:35 ` Lai Jiangshan**
  2024-02-26 14:35 ` [[RFC PATCH 38/73] KVM: x86/PVM: Handle hypercalls for privilege instruction emulation](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mbda22272cd4bb3a7a881e6ca2e65572f999dc4c7) Lai Jiangshan
                   ` [(37 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rbda22272cd4bb3a7a881e6ca2e65572f999dc4c7)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r2d1522a59d0e90ba1385d0417217234a95b13e8d)
From: Lai Jiangshan @ 2024-02-26 14:35 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-38-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-38-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143639)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143639), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

Since the host doesn't use all PCIDs, PVM can utilize the host PCID to
reduce guest TLB flushing. The PCID allocation algorithm in PVM is
similar to that of the host.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/kvm/pvm/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-38-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) | 228 ++++++++++++++++++++++++++++++++++++++++-
 [arch/x86/kvm/pvm/pvm.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-38-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.h) |   5 +
 2 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e2d1522a59d0e90ba1385d0417217234a95b13e8d), 232 insertions(+), 1 deletion(-)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-38-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) --git a/arch/x86/kvm/pvm/pvm.c b/arch/x86/kvm/pvm/pvm.c
index 242c355fda8f..2d3785e7f2f3 100644
--- a/arch/x86/kvm/pvm/pvm.c
+++ b/arch/x86/kvm/pvm/pvm.c @@ -349,6 +349,211 @@ static void pvm_switch_to_host(struct vcpu_pvm *pvm)
 	preempt_enable();
 }

+struct host_pcid_one {
+	/*
+	 * It is struct vcpu_pvm *pvm, but it is not allowed to be
+	 * dereferenced since it might be freed.
+	 */
+	void *pvm;
+	u64 root_hpa;
+};
+
+struct host_pcid_state {
+	struct host_pcid_one pairs[NUM_HOST_PCID_FOR_GUEST];
+	int evict_next_round_robin;
+};
+
+static DEFINE_PER_CPU(struct host_pcid_state, pvm_tlb_state);
+
+static void host_pcid_flush_all(struct vcpu_pvm *pvm)
+{
+	struct host_pcid_state *tlb_state = this_cpu_ptr(&pvm_tlb_state);
+	int i;
+
+	for (i = 0; i < NUM_HOST_PCID_FOR_GUEST; i++) {
+		if (tlb_state->pairs[i].pvm == pvm)
+			tlb_state->pairs[i].pvm = NULL;
+	}
+}
+
+static inline unsigned int host_pcid_to_index(unsigned int host_pcid)
+{
+	return host_pcid & ~HOST_PCID_TAG_FOR_GUEST;
+}
+
+static inline int index_to_host_pcid(int index)
+{
+	return index | HOST_PCID_TAG_FOR_GUEST;
+}
+
+/*
+ * Free the uncached guest pcid (not in mmu->root nor mmu->prev_root), so
+ * that the next allocation would not evict a clean one.
+ *
+ * It would be better if kvm.ko notifies us when a root_pgd is freed
+ * from the cache.
+ *
+ * Returns a freed index or -1 if nothing is freed.
+ */
+static int host_pcid_free_uncached(struct vcpu_pvm *pvm)
+{
+	/* It is allowed to do nothing. */
+	return -1;
+}
+
+/*
+ * Get a host pcid of the current pCPU for the specific guest pgd.
+ * PVM vTLB is guest pgd tagged.
+ */
+static int host_pcid_get(struct vcpu_pvm *pvm, u64 root_hpa, bool *flush)
+{
+	struct host_pcid_state *tlb_state = this_cpu_ptr(&pvm_tlb_state);
+	int i, j = -1;
+
+	/* find if it is allocated. */
+	for (i = 0; i < NUM_HOST_PCID_FOR_GUEST; i++) {
+		struct host_pcid_one *tlb = &tlb_state->pairs[i];
+
+		if (tlb->root_hpa == root_hpa && tlb->pvm == pvm)
+			return index_to_host_pcid(i);
+
+		/* if it has no owner, allocate it if not found. */
+		if (!tlb->pvm)
+			j = i;
+	}
+
+	/*
+	 * Fallback to:
+	 *    use the fallback recorded in the above loop.
+	 *    use a freed uncached.
+	 *    evict one (which might be still usable) by round-robin policy.
+	 */
+	if (j < 0)
+		j = host_pcid_free_uncached(pvm);
+	if (j < 0) {
+		j = tlb_state->evict_next_round_robin;
+		if (++tlb_state->evict_next_round_robin == NUM_HOST_PCID_FOR_GUEST)
+			tlb_state->evict_next_round_robin = 0;
+	}
+
+	/* associate the host pcid to the guest */
+	tlb_state->pairs[j].pvm = pvm;
+	tlb_state->pairs[j].root_hpa = root_hpa;
+
+	*flush = true;
+	return index_to_host_pcid(j);
+}
+
+static void host_pcid_free(struct vcpu_pvm *pvm, u64 root_hpa)
+{
+	struct host_pcid_state *tlb_state = this_cpu_ptr(&pvm_tlb_state);
+	int i;
+
+	for (i = 0; i < NUM_HOST_PCID_FOR_GUEST; i++) {
+		struct host_pcid_one *tlb = &tlb_state->pairs[i];
+
+		if (tlb->root_hpa == root_hpa && tlb->pvm == pvm) {
+			tlb->pvm = NULL;
+			return;
+		}
+	}
+}
+
+static inline void *host_pcid_owner(int host_pcid)
+{
+	return this_cpu_read(pvm_tlb_state.pairs[host_pcid_to_index(host_pcid)].pvm);
+}
+
+static inline u64 host_pcid_root(int host_pcid)
+{
+	return this_cpu_read(pvm_tlb_state.pairs[host_pcid_to_index(host_pcid)].root_hpa);
+}
+
+static void __pvm_hwtlb_flush_all(struct vcpu_pvm *pvm)
+{
+	if (static_cpu_has(X86_FEATURE_PCID))
+		host_pcid_flush_all(pvm);
+}
+
+static void pvm_flush_hwtlb(struct kvm_vcpu *vcpu)
+{
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+
+	get_cpu();
+	__pvm_hwtlb_flush_all(pvm);
+	put_cpu();
+}
+
+static void pvm_flush_hwtlb_guest(struct kvm_vcpu *vcpu)
+{
+	/*
+	 * flushing hwtlb for guest only when:
+	 *	change to the shadow page table.
+	 *	reused an used (guest) pcid.
+	 * change to the shadow page table always results flushing hwtlb
+	 * and PVM uses pgd tagged tlb.
+	 *
+	 * So no hwtlb needs to be flushed here.
+	 */
+}
+
+static void pvm_flush_hwtlb_current(struct kvm_vcpu *vcpu)
+{
+	/* No flush required if the current context is invalid. */
+	if (!VALID_PAGE(vcpu->arch.mmu->root.hpa))
+		return;
+
+	if (static_cpu_has(X86_FEATURE_PCID)) {
+		get_cpu();
+		host_pcid_free(to_pvm(vcpu), vcpu->arch.mmu->root.hpa);
+		put_cpu();
+	}
+}
+
+static void pvm_flush_hwtlb_gva(struct kvm_vcpu *vcpu, gva_t addr)
+{
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+	int max = MIN_HOST_PCID_FOR_GUEST + NUM_HOST_PCID_FOR_GUEST;
+	int i;
+
+	if (!static_cpu_has(X86_FEATURE_PCID))
+		return;
+
+	get_cpu();
+	if (!this_cpu_has(X86_FEATURE_INVPCID)) {
+		host_pcid_flush_all(pvm);
+		put_cpu();
+		return;
+	}
+
+	host_pcid_free_uncached(pvm);
+	for (i = MIN_HOST_PCID_FOR_GUEST; i < max; i++) {
+		if (host_pcid_owner(i) == pvm)
+			invpcid_flush_one(i, addr);
+	}
+
+	put_cpu();
+}
+
+static void pvm_set_host_cr3_for_guest_with_host_pcid(struct vcpu_pvm *pvm)
+{
+	u64 root_hpa = pvm->vcpu.arch.mmu->root.hpa;
+	bool flush = false;
+	u32 host_pcid = host_pcid_get(pvm, root_hpa, &flush);
+	u64 hw_cr3 = root_hpa | host_pcid;
+
+	if (!flush)
+		hw_cr3 |= CR3_NOFLUSH;
+	this_cpu_write(cpu_tss_rw.tss_ex.enter_cr3, hw_cr3);
+}
+
+static void pvm_set_host_cr3_for_guest_without_host_pcid(struct vcpu_pvm *pvm)
+{
+	u64 root_hpa = pvm->vcpu.arch.mmu->root.hpa;
+
+	this_cpu_write(cpu_tss_rw.tss_ex.enter_cr3, root_hpa);
+}
+
 static void pvm_set_host_cr3_for_hypervisor(struct vcpu_pvm *pvm)
 {
 	unsigned long cr3;
@@ -365,7 +570,11 @@ static void pvm_set_host_cr3_for_hypervisor(struct vcpu_pvm *pvm)
 static void pvm_set_host_cr3(struct vcpu_pvm *pvm)
 {
 	pvm_set_host_cr3_for_hypervisor(pvm);
-	this_cpu_write(cpu_tss_rw.tss_ex.enter_cr3, pvm->vcpu.arch.mmu->root.hpa); +
+	if (static_cpu_has(X86_FEATURE_PCID))
+		pvm_set_host_cr3_for_guest_with_host_pcid(pvm);
+	else
+		pvm_set_host_cr3_for_guest_without_host_pcid(pvm);
 }

 static void pvm_load_mmu_pgd(struct kvm_vcpu *vcpu, hpa_t root_hpa,
@@ -391,6 +600,9 @@ static void pvm_vcpu_load(struct kvm_vcpu *vcpu, int cpu)

 	__this_cpu_write(active_pvm_vcpu, pvm);

+	if (vcpu->cpu != cpu)
+		__pvm_hwtlb_flush_all(pvm);
+
 	indirect_branch_prediction_barrier();
 }

@@ -398,6 +610,7 @@ static void pvm_vcpu_put(struct kvm_vcpu *vcpu)
 {
 	struct vcpu_pvm *pvm = to_pvm(vcpu);

+	host_pcid_free_uncached(pvm);
 	pvm_prepare_switch_to_host(pvm);
 }

@@ -2086,6 +2299,11 @@ static struct kvm_x86_ops pvm_x86_ops __initdata = {
 	.set_rflags = pvm_set_rflags,
 	.get_if_flag = pvm_get_if_flag,

+	.flush_tlb_all = pvm_flush_hwtlb,
+	.flush_tlb_current = pvm_flush_hwtlb_current,
+	.flush_tlb_gva = pvm_flush_hwtlb_gva,
+	.flush_tlb_guest = pvm_flush_hwtlb_guest,
+
 	.vcpu_pre_run = pvm_vcpu_pre_run,
 	.vcpu_run = pvm_vcpu_run,
 	.handle_exit = pvm_handle_exit,
@@ -2152,8 +2370,16 @@ static void pvm_exit(void)
 }
 module_exit(pvm_exit);

+#define TLB_NR_DYN_ASIDS	6
+
 static int __init hardware_cap_check(void)
 {
+	BUILD_BUG_ON(MIN_HOST_PCID_FOR_GUEST <= TLB_NR_DYN_ASIDS);
+#ifdef CONFIG_PAGE_TABLE_ISOLATION
+	BUILD_BUG_ON((MIN_HOST_PCID_FOR_GUEST + NUM_HOST_PCID_FOR_GUEST) >=
+		     (1 << X86_CR3_PTI_PCID_USER_BIT));
+#endif
+
 	/*
 	 * switcher can't be used when KPTI. See the comments above
 	 * SWITCHER_SAVE_AND_SWITCH_TO_HOST_CR3
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-38-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.h) --git a/arch/x86/kvm/pvm/pvm.h b/arch/x86/kvm/pvm/pvm.h
index 4cdcbed1c813..31060831e009 100644
--- a/arch/x86/kvm/pvm/pvm.h
+++ b/arch/x86/kvm/pvm/pvm.h @@ -28,6 +28,11 @@ extern u64 *host_mmu_root_pgd;
 void host_mmu_destroy(void);
 int host_mmu_init(void);

+#define HOST_PCID_TAG_FOR_GUEST			(32)
+
+#define MIN_HOST_PCID_FOR_GUEST			HOST_PCID_TAG_FOR_GUEST
+#define NUM_HOST_PCID_FOR_GUEST			HOST_PCID_TAG_FOR_GUEST
+
 struct vcpu_pvm {
 	struct kvm_vcpu vcpu;

--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m2d1522a59d0e90ba1385d0417217234a95b13e8d) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-38-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-38-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-38-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-38-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-38-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-38-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r2d1522a59d0e90ba1385d0417217234a95b13e8d)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ebda22272cd4bb3a7a881e6ca2e65572f999dc4c7) **[RFC PATCH 38/73] KVM: x86/PVM: Handle hypercalls for privilege instruction emulation**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(36 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r2d1522a59d0e90ba1385d0417217234a95b13e8d)
  2024-02-26 14:35 ` [[RFC PATCH 37/73] KVM: x86/PVM: Use host PCID to reduce guest TLB flushing](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m2d1522a59d0e90ba1385d0417217234a95b13e8d) Lai Jiangshan
**@ 2024-02-26 14:35 ` Lai Jiangshan**
  2024-02-26 14:35 ` [[RFC PATCH 39/73] KVM: x86/PVM: Handle hypercall for CR3 switching](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m97850ee556b8a2ea2c4a71f23922a5669f15d854) Lai Jiangshan
                   ` [(36 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r97850ee556b8a2ea2c4a71f23922a5669f15d854)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rbda22272cd4bb3a7a881e6ca2e65572f999dc4c7)
From: Lai Jiangshan @ 2024-02-26 14:35 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-39-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-39-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143642)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143642), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

The privileged instructions in the PVM guest will be trapped and
emulated. To reduce the emulation overhead, some privileged instructions
in the hot path, such as RDMSR/WRMSR and TLB flushing related
instructions, will be replaced by hypercalls to improve performance.
The handling of those hypercalls is the same as the associated
privileged instruction emulation.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/kvm/pvm/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-39-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) | 114 ++++++++++++++++++++++++++++++++++++++++-
 1 file [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ebda22272cd4bb3a7a881e6ca2e65572f999dc4c7), 113 insertions(+), 1 deletion(-)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-39-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) --git a/arch/x86/kvm/pvm/pvm.c b/arch/x86/kvm/pvm/pvm.c
index 2d3785e7f2f3..8d8c783c72b5 100644
--- a/arch/x86/kvm/pvm/pvm.c
+++ b/arch/x86/kvm/pvm/pvm.c @@ -1434,6 +1434,96 @@ static int handle_synthetic_instruction_return_supervisor(struct kvm_vcpu *vcpu)
 	return 1;
 }

+static int handle_hc_interrupt_window(struct kvm_vcpu *vcpu)
+{
+	kvm_make_request(KVM_REQ_EVENT, vcpu);
+	pvm_event_flags_update(vcpu, 0, PVM_EVENT_FLAGS_IP);
+
+	++vcpu->stat.irq_window_exits;
+	return 1;
+}
+
+static int handle_hc_irq_halt(struct kvm_vcpu *vcpu)
+{
+	kvm_set_rflags(vcpu, kvm_get_rflags(vcpu) | X86_EFLAGS_IF);
+
+	return kvm_emulate_halt_noskip(vcpu);
+}
+
+static void pvm_flush_tlb_guest_current_kernel_user(struct kvm_vcpu *vcpu)
+{
+	/*
+	 * sync the current pgd and user_pgd (pvm->msr_switch_cr3)
+	 * which is a subset work of KVM_REQ_TLB_FLUSH_GUEST.
+	 */
+	kvm_make_request(KVM_REQ_TLB_FLUSH_GUEST, vcpu);
+}
+
+/*
+ * Hypercall: PVM_HC_TLB_FLUSH
+ *	Flush all TLBs.
+ */
+static int handle_hc_flush_tlb_all(struct kvm_vcpu *vcpu)
+{
+	kvm_make_request(KVM_REQ_TLB_FLUSH_GUEST, vcpu);
+
+	return 1;
+}
+
+/*
+ * Hypercall: PVM_HC_TLB_FLUSH_CURRENT
+ *	Flush all TLBs tagged with the current CR3 and MSR_PVM_SWITCH_CR3.
+ */
+static int handle_hc_flush_tlb_current_kernel_user(struct kvm_vcpu *vcpu)
+{
+	pvm_flush_tlb_guest_current_kernel_user(vcpu);
+
+	return 1;
+}
+
+/*
+ * Hypercall: PVM_HC_TLB_INVLPG
+ *	Flush TLBs associated with a single address for all tags.
+ */
+static int handle_hc_invlpg(struct kvm_vcpu *vcpu, unsigned long addr)
+{
+	kvm_mmu_invlpg(vcpu, addr);
+
+	return 1;
+}
+
+/*
+ * Hypercall: PVM_HC_RDMSR
+ *	Write MSR.
+ *	Return with RAX = the MSR value if succeeded.
+ *	Return with RAX = 0 if it failed.
+ */
+static int handle_hc_rdmsr(struct kvm_vcpu *vcpu, u32 index)
+{
+	u64 value = 0;
+
+	kvm_get_msr(vcpu, index, &value);
+	kvm_rax_write(vcpu, value);
+
+	return 1;
+}
+
+/*
+ * Hypercall: PVM_HC_WRMSR
+ *	Write MSR.
+ *	Return with RAX = 0 if succeeded.
+ *	Return with RAX = -EIO if it failed
+ */
+static int handle_hc_wrmsr(struct kvm_vcpu *vcpu, u32 index, u64 value)
+{
+	if (kvm_set_msr(vcpu, index, value))
+		kvm_rax_write(vcpu, -EIO);
+	else
+		kvm_rax_write(vcpu, 0);
+
+	return 1;
+}
+
 static int handle_kvm_hypercall(struct kvm_vcpu *vcpu)
 {
 	int r;
@@ -1450,6 +1540,7 @@ static int handle_exit_syscall(struct kvm_vcpu *vcpu)
 {
 	struct vcpu_pvm *pvm = to_pvm(vcpu);
 	unsigned long rip = kvm_rip_read(vcpu);
+	unsigned long a0, a1;

 	if (!is_smod(pvm))
 		return do_pvm_user_event(vcpu, PVM_SYSCALL_VECTOR, false, 0);
@@ -1459,7 +1550,28 @@ static int handle_exit_syscall(struct kvm_vcpu *vcpu)
 	if (rip == pvm->msr_rets_rip_plus2)
 		return handle_synthetic_instruction_return_supervisor(vcpu);

-	return handle_kvm_hypercall(vcpu); +	a0 = kvm_rbx_read(vcpu);
+	a1 = kvm_r10_read(vcpu);
+
+	// handle hypercall, check it for pvm hypercall and then kvm hypercall
+	switch (kvm_rax_read(vcpu)) {
+	case PVM_HC_IRQ_WIN:
+		return handle_hc_interrupt_window(vcpu);
+	case PVM_HC_IRQ_HALT:
+		return handle_hc_irq_halt(vcpu);
+	case PVM_HC_TLB_FLUSH:
+		return handle_hc_flush_tlb_all(vcpu);
+	case PVM_HC_TLB_FLUSH_CURRENT:
+		return handle_hc_flush_tlb_current_kernel_user(vcpu);
+	case PVM_HC_TLB_INVLPG:
+		return handle_hc_invlpg(vcpu, a0);
+	case PVM_HC_RDMSR:
+		return handle_hc_rdmsr(vcpu, a0);
+	case PVM_HC_WRMSR:
+		return handle_hc_wrmsr(vcpu, a0, a1);
+	default:
+		return handle_kvm_hypercall(vcpu);
+	}
 }

 static int handle_exit_debug(struct kvm_vcpu *vcpu)
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mbda22272cd4bb3a7a881e6ca2e65572f999dc4c7) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-39-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-39-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-39-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-39-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-39-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-39-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rbda22272cd4bb3a7a881e6ca2e65572f999dc4c7)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e97850ee556b8a2ea2c4a71f23922a5669f15d854) **[RFC PATCH 39/73] KVM: x86/PVM: Handle hypercall for CR3 switching**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(37 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rbda22272cd4bb3a7a881e6ca2e65572f999dc4c7)
  2024-02-26 14:35 ` [[RFC PATCH 38/73] KVM: x86/PVM: Handle hypercalls for privilege instruction emulation](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mbda22272cd4bb3a7a881e6ca2e65572f999dc4c7) Lai Jiangshan
**@ 2024-02-26 14:35 ` Lai Jiangshan**
  2024-02-26 14:35 ` [[RFC PATCH 40/73] KVM: x86/PVM: Handle hypercall for loading GS selector](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mabc67608b691853150bde97ac631b7a1d6d00eb4) Lai Jiangshan
                   ` [(35 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rabc67608b691853150bde97ac631b7a1d6d00eb4)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r97850ee556b8a2ea2c4a71f23922a5669f15d854)
From: Lai Jiangshan @ 2024-02-26 14:35 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-40-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-40-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143645)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143645), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

If the guest uses the same page table for supervisor mode and user mode,
then the user mode can access the supervisor mode address space.
Therefore, for safety, the guest needs to provide two different page
tables for one process, which is similar to KPTI. When switching CR3
during the process switching, the guest uses the hypercall to provide
the two page tables for the hypervisor, and then the hypervisor can
switch CR3 during the mode switch automatically. Additionally, an extra
flag is introduced to perform TLB flushing at the same time.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/kvm/pvm/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-40-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) | 41 ++++++++++++++++++++++++++++++++++++++++-
 1 file [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e97850ee556b8a2ea2c4a71f23922a5669f15d854), 40 insertions(+), 1 deletion(-)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-40-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) --git a/arch/x86/kvm/pvm/pvm.c b/arch/x86/kvm/pvm/pvm.c
index 8d8c783c72b5..ad08643c098a 100644
--- a/arch/x86/kvm/pvm/pvm.c
+++ b/arch/x86/kvm/pvm/pvm.c @@ -1459,6 +1459,42 @@ static void pvm_flush_tlb_guest_current_kernel_user(struct kvm_vcpu *vcpu)
 	kvm_make_request(KVM_REQ_TLB_FLUSH_GUEST, vcpu);
 }

+/*
+ * Hypercall: PVM_HC_LOAD_PGTBL
+ *	Load two PGDs into the current CR3 and MSR_PVM_SWITCH_CR3.
+ *
+ * Arguments:
+ *	flags:	bit0: flush the TLBs tagged with @pgd and @user_pgd.
+ *		bit1: 4 (bit1=0) or 5 (bit1=1 && cpuid_has(LA57)) level paging.
+ *	pgd: to be loaded into CR3.
+ *	user_pgd: to be loaded into MSR_PVM_SWITCH_CR3.
+ */
+static int handle_hc_load_pagetables(struct kvm_vcpu *vcpu, unsigned long flags,
+				     unsigned long pgd, unsigned long user_pgd)
+{
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+	unsigned long cr4 = vcpu->arch.cr4;
+
+	if (!(flags & 2))
+		cr4 &= ~X86_CR4_LA57;
+	else if (guest_cpuid_has(vcpu, X86_FEATURE_LA57))
+		cr4 |= X86_CR4_LA57;
+
+	if (cr4 != vcpu->arch.cr4) {
+		vcpu->arch.cr4 = cr4;
+		kvm_mmu_reset_context(vcpu);
+	}
+
+	kvm_mmu_new_pgd(vcpu, pgd);
+	vcpu->arch.cr3 = pgd;
+	pvm->msr_switch_cr3 = user_pgd;
+
+	if (flags & 1)
+		pvm_flush_tlb_guest_current_kernel_user(vcpu);
+
+	return 1;
+}
+
 /*
  * Hypercall: PVM_HC_TLB_FLUSH
  *	Flush all TLBs.
@@ -1540,7 +1576,7 @@ static int handle_exit_syscall(struct kvm_vcpu *vcpu)
 {
 	struct vcpu_pvm *pvm = to_pvm(vcpu);
 	unsigned long rip = kvm_rip_read(vcpu);
-	unsigned long a0, a1; +	unsigned long a0, a1, a2;

 	if (!is_smod(pvm))
 		return do_pvm_user_event(vcpu, PVM_SYSCALL_VECTOR, false, 0);
@@ -1552,6 +1588,7 @@ static int handle_exit_syscall(struct kvm_vcpu *vcpu)

 	a0 = kvm_rbx_read(vcpu);
 	a1 = kvm_r10_read(vcpu);
+	a2 = kvm_rdx_read(vcpu);

 	// handle hypercall, check it for pvm hypercall and then kvm hypercall
 	switch (kvm_rax_read(vcpu)) {
@@ -1559,6 +1596,8 @@ static int handle_exit_syscall(struct kvm_vcpu *vcpu)
 		return handle_hc_interrupt_window(vcpu);
 	case PVM_HC_IRQ_HALT:
 		return handle_hc_irq_halt(vcpu);
+	case PVM_HC_LOAD_PGTBL:
+		return handle_hc_load_pagetables(vcpu, a0, a1, a2);
 	case PVM_HC_TLB_FLUSH:
 		return handle_hc_flush_tlb_all(vcpu);
 	case PVM_HC_TLB_FLUSH_CURRENT:
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m97850ee556b8a2ea2c4a71f23922a5669f15d854) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-40-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-40-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-40-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-40-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-40-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-40-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r97850ee556b8a2ea2c4a71f23922a5669f15d854)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#eabc67608b691853150bde97ac631b7a1d6d00eb4) **[RFC PATCH 40/73] KVM: x86/PVM: Handle hypercall for loading GS selector**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(38 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r97850ee556b8a2ea2c4a71f23922a5669f15d854)
  2024-02-26 14:35 ` [[RFC PATCH 39/73] KVM: x86/PVM: Handle hypercall for CR3 switching](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m97850ee556b8a2ea2c4a71f23922a5669f15d854) Lai Jiangshan
**@ 2024-02-26 14:35 ` Lai Jiangshan**
  2024-02-26 14:35 ` [[RFC PATCH 41/73] KVM: x86/PVM: Allow to load guest TLS in host GDT](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mc76a78e8e1e7b7d584ff1ef56de2a847e07a6cd4) Lai Jiangshan
                   ` [(34 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rc76a78e8e1e7b7d584ff1ef56de2a847e07a6cd4)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rabc67608b691853150bde97ac631b7a1d6d00eb4)
From: Lai Jiangshan @ 2024-02-26 14:35 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-41-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-41-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143648)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143648), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

SWAPGS is not supported in PVM, so the native load_gs_index() cannot be
used in the guest. Therefore, a hypercall is introduced to load the GS
selector into the GS segment register, and the resulting GS base is
returned to the guest. This is prepared for supporting 32-bit processes.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/kvm/pvm/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-41-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) | 71 ++++++++++++++++++++++++++++++++++++++++++
 1 file [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#eabc67608b691853150bde97ac631b7a1d6d00eb4), 71 insertions(+)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-41-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) --git a/arch/x86/kvm/pvm/pvm.c b/arch/x86/kvm/pvm/pvm.c
index ad08643c098a..ee55e99fb204 100644
--- a/arch/x86/kvm/pvm/pvm.c
+++ b/arch/x86/kvm/pvm/pvm.c @@ -1528,6 +1528,75 @@ static int handle_hc_invlpg(struct kvm_vcpu *vcpu, unsigned long addr)
 	return 1;
 }

+/*
+ * Hypercall: PVM_HC_LOAD_GS
+ *	Load %gs with the selector %rdi and load the resulted base address
+ *	into RAX.
+ *
+ *	If %rdi is an invalid selector (including RPL != 3), NULL selector
+ *	will be used instead.
+ *
+ *	Return the resulted GS BASE in vCPU's RAX.
+ */
+static int handle_hc_load_gs(struct kvm_vcpu *vcpu, unsigned short sel)
+{
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+	unsigned long guest_kernel_gs_base;
+
+	/* Use NULL selector if RPL != 3. */
+	if (sel != 0 && (sel & 3) != 3)
+		sel = 0;
+
+	/* Protect the guest state on the hardware. */
+	preempt_disable();
+
+	/*
+	 * Switch to the guest state because the CPU is going to set the %gs to
+	 * the guest value.  Save the original guest MSR_GS_BASE if it is
+	 * already the guest state.
+	 */
+	if (!pvm->loaded_cpu_state)
+		pvm_prepare_switch_to_guest(vcpu);
+	else
+		__save_gs_base(pvm);
+
+	/*
+	 * Load sel into %gs, which also changes the hardware MSR_KERNEL_GS_BASE.
+	 *
+	 * Before load_gs_index(sel):
+	 *	hardware %gs:			old gs index
+	 *	hardware MSR_KERNEL_GS_BASE:	guest MSR_GS_BASE
+	 *
+	 * After load_gs_index(sel);
+	 *	hardware %gs:			resulted %gs, @sel or NULL
+	 *	hardware MSR_KERNEL_GS_BASE:	resulted GS BASE
+	 *
+	 * The resulted %gs is the new guest %gs and will be saved into
+	 * pvm->segments[VCPU_SREG_GS].selector later when the CPU is
+	 * switching to host or the guest %gs is read (pvm_get_segment()).
+	 *
+	 * The resulted hardware MSR_KERNEL_GS_BASE will be returned via RAX
+	 * to the guest and the hardware MSR_KERNEL_GS_BASE, which represents
+	 * the guest MSR_GS_BASE when in VM-Exit state, is restored back to
+	 * the guest MSR_GS_BASE.
+	 */
+	load_gs_index(sel);
+
+	/* Get the resulted guest MSR_KERNEL_GS_BASE. */
+	rdmsrl(MSR_KERNEL_GS_BASE, guest_kernel_gs_base);
+
+	/* Restore the guest MSR_GS_BASE into the hardware MSR_KERNEL_GS_BASE. */
+	__load_gs_base(pvm);
+
+	/* Finished access to the guest state on the hardware. */
+	preempt_enable();
+
+	/* Return RAX with the resulted GS BASE. */
+	kvm_rax_write(vcpu, guest_kernel_gs_base);
+
+	return 1;
+}
+
 /*
  * Hypercall: PVM_HC_RDMSR
  *	Write MSR.
@@ -1604,6 +1673,8 @@ static int handle_exit_syscall(struct kvm_vcpu *vcpu)
 		return handle_hc_flush_tlb_current_kernel_user(vcpu);
 	case PVM_HC_TLB_INVLPG:
 		return handle_hc_invlpg(vcpu, a0);
+	case PVM_HC_LOAD_GS:
+		return handle_hc_load_gs(vcpu, a0);
 	case PVM_HC_RDMSR:
 		return handle_hc_rdmsr(vcpu, a0);
 	case PVM_HC_WRMSR:
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mabc67608b691853150bde97ac631b7a1d6d00eb4) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-41-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-41-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-41-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-41-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-41-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-41-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rabc67608b691853150bde97ac631b7a1d6d00eb4)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ec76a78e8e1e7b7d584ff1ef56de2a847e07a6cd4) **[RFC PATCH 41/73] KVM: x86/PVM: Allow to load guest TLS in host GDT**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(39 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rabc67608b691853150bde97ac631b7a1d6d00eb4)
  2024-02-26 14:35 ` [[RFC PATCH 40/73] KVM: x86/PVM: Handle hypercall for loading GS selector](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mabc67608b691853150bde97ac631b7a1d6d00eb4) Lai Jiangshan
**@ 2024-02-26 14:35 ` Lai Jiangshan**
  2024-02-26 14:35 ` [[RFC PATCH 42/73] KVM: x86/PVM: Support for kvm_exit() tracepoint](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ma6bec3331b150ec2a3f44a0d098516cddd1978ca) Lai Jiangshan
                   ` [(33 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ra6bec3331b150ec2a3f44a0d098516cddd1978ca)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rc76a78e8e1e7b7d584ff1ef56de2a847e07a6cd4)
From: Lai Jiangshan @ 2024-02-26 14:35 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-42-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-42-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143652)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143652), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

The 32-bit process needs to use TLS in libc, so a hypercall is
introduced to load the guest TLS into the host GDT. The checking of the
guest TLS is the same as tls_desc_okay() in the arch/x86/kernel/tls.c
file.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/kvm/pvm/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-42-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) | 81 ++++++++++++++++++++++++++++++++++++++++++
 [arch/x86/kvm/pvm/pvm.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-42-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.h) |  1 +
 2 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ec76a78e8e1e7b7d584ff1ef56de2a847e07a6cd4), 82 insertions(+)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-42-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) --git a/arch/x86/kvm/pvm/pvm.c b/arch/x86/kvm/pvm/pvm.c
index ee55e99fb204..e68052f33186 100644
--- a/arch/x86/kvm/pvm/pvm.c
+++ b/arch/x86/kvm/pvm/pvm.c @@ -281,6 +281,26 @@ static void segments_save_guest_and_switch_to_host(struct vcpu_pvm *pvm)
 	wrmsrl(MSR_FS_BASE, current->thread.fsbase);
 }

+/*
+ * Load guest TLS entries into the GDT.
+ */
+static inline void host_gdt_set_tls(struct vcpu_pvm *pvm)
+{
+	struct desc_struct *gdt = get_current_gdt_rw();
+	unsigned int i;
+
+	for (i = 0; i < GDT_ENTRY_TLS_ENTRIES; i++)
+		gdt[GDT_ENTRY_TLS_MIN + i] = pvm->tls_array[i];
+}
+
+/*
+ * Load current task's TLS into the GDT.
+ */
+static inline void host_gdt_restore_tls(void)
+{
+	native_load_tls(&current->thread, smp_processor_id());
+}
+
 static void pvm_prepare_switch_to_guest(struct kvm_vcpu *vcpu)
 {
 	struct vcpu_pvm *pvm = to_pvm(vcpu);
@@ -304,6 +324,8 @@ static void pvm_prepare_switch_to_guest(struct kvm_vcpu *vcpu)
 		native_tss_invalidate_io_bitmap();
 #endif

+	host_gdt_set_tls(pvm);
+
 #ifdef CONFIG_MODIFY_LDT_SYSCALL
 	/* PVM doesn't support LDT. */
 	if (unlikely(current->mm->context.ldt))
@@ -334,6 +356,8 @@ static void pvm_prepare_switch_to_host(struct vcpu_pvm *pvm)
 		kvm_load_ldt(GDT_ENTRY_LDT*8);
 #endif

+	host_gdt_restore_tls();
+
 	segments_save_guest_and_switch_to_host(pvm);
 	pvm->loaded_cpu_state = 0;
 }
@@ -1629,6 +1653,60 @@ static int handle_hc_wrmsr(struct kvm_vcpu *vcpu, u32 index, u64 value)
 	return 1;
 }

+// Check if the tls desc is allowed on the host GDT.
+// The same logic as tls_desc_okay() in arch/x86/kernel/tls.c.
+static bool tls_desc_okay(struct desc_struct *desc)
+{
+	// Only allow present segments.
+	if (!desc->p)
+		return false;
+
+	// Only allow data segments.
+	if (desc->type & (1 << 3))
+		return false;
+
+	// Only allow 32-bit data segments.
+	if (!desc->d)
+		return false;
+
+	return true;
+}
+
+/*
+ * Hypercall: PVM_HC_LOAD_TLS
+ *	Load guest TLS desc into host GDT.
+ */
+static int handle_hc_load_tls(struct kvm_vcpu *vcpu, unsigned long tls_desc_0,
+			      unsigned long tls_desc_1, unsigned long tls_desc_2)
+{
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+	unsigned long *tls_array = (unsigned long *)&pvm->tls_array[0];
+	int i;
+
+	tls_array[0] = tls_desc_0;
+	tls_array[1] = tls_desc_1;
+	tls_array[2] = tls_desc_2;
+
+	for (i = 0; i < GDT_ENTRY_TLS_ENTRIES; i++) {
+		if (!tls_desc_okay(&pvm->tls_array[i])) {
+			pvm->tls_array[i] = (struct desc_struct){0};
+			continue;
+		}
+		/* Standarding TLS descs, same as fill_ldt(). */
+		pvm->tls_array[i].type |= 1;
+		pvm->tls_array[i].s = 1;
+		pvm->tls_array[i].dpl = 0x3;
+		pvm->tls_array[i].l = 0;
+	}
+
+	preempt_disable();
+	if (pvm->loaded_cpu_state)
+		host_gdt_set_tls(pvm);
+	preempt_enable();
+
+	return 1;
+}
+
 static int handle_kvm_hypercall(struct kvm_vcpu *vcpu)
 {
 	int r;
@@ -1679,6 +1757,8 @@ static int handle_exit_syscall(struct kvm_vcpu *vcpu)
 		return handle_hc_rdmsr(vcpu, a0);
 	case PVM_HC_WRMSR:
 		return handle_hc_wrmsr(vcpu, a0, a1);
+	case PVM_HC_LOAD_TLS:
+		return handle_hc_load_tls(vcpu, a0, a1, a2);
 	default:
 		return handle_kvm_hypercall(vcpu);
 	}
@@ -2296,6 +2376,7 @@ static void pvm_vcpu_reset(struct kvm_vcpu *vcpu, bool init_event)
 	pvm->hw_ss = __USER_DS;
 	pvm->int_shadow = 0;
 	pvm->nmi_mask = false;
+	memset(&pvm->tls_array[0], 0, sizeof(pvm->tls_array));

 	pvm->msr_vcpu_struct = 0;
 	pvm->msr_supervisor_rsp = 0;
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-42-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.h) --git a/arch/x86/kvm/pvm/pvm.h b/arch/x86/kvm/pvm/pvm.h
index 31060831e009..f28ab0b48f40 100644
--- a/arch/x86/kvm/pvm/pvm.h
+++ b/arch/x86/kvm/pvm/pvm.h @@ -98,6 +98,7 @@ struct vcpu_pvm {
 	struct kvm_segment segments[NR_VCPU_SREG];
 	struct desc_ptr idt_ptr;
 	struct desc_ptr gdt_ptr;
+	struct desc_struct tls_array[GDT_ENTRY_TLS_ENTRIES];
 };

 struct kvm_pvm {
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mc76a78e8e1e7b7d584ff1ef56de2a847e07a6cd4) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-42-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-42-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-42-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-42-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-42-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-42-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rc76a78e8e1e7b7d584ff1ef56de2a847e07a6cd4)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ea6bec3331b150ec2a3f44a0d098516cddd1978ca) **[RFC PATCH 42/73] KVM: x86/PVM: Support for kvm_exit() tracepoint**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(40 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rc76a78e8e1e7b7d584ff1ef56de2a847e07a6cd4)
  2024-02-26 14:35 ` [[RFC PATCH 41/73] KVM: x86/PVM: Allow to load guest TLS in host GDT](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mc76a78e8e1e7b7d584ff1ef56de2a847e07a6cd4) Lai Jiangshan
**@ 2024-02-26 14:35 ` Lai Jiangshan**
  2024-02-26 14:36 ` [[RFC PATCH 43/73] KVM: x86/PVM: Enable direct switching](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m25de9f37876e75c7b1168173fd21deb1976c578e) Lai Jiangshan
                   ` [(32 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r25de9f37876e75c7b1168173fd21deb1976c578e)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ra6bec3331b150ec2a3f44a0d098516cddd1978ca)
From: Lai Jiangshan @ 2024-02-26 14:35 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-43-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-43-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143655)
  Cc: Hou Wenlong, Lai Jiangshan, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143655), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Hou Wenlong <houwenlong.hwl@antgroup.com>

Similar to VMX/SVM, add necessary information to support kvm_exit()
tracepoint.

Suggested-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
---
 [arch/x86/kvm/pvm/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-43-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) | 41 +++++++++++++++++++++++++++++++++++++++++
 [arch/x86/kvm/pvm/pvm.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-43-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.h) | 35 +++++++++++++++++++++++++++++++++++
 [arch/x86/kvm/trace.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-43-jiangshanlai::40gmail.com:1arch:x86:kvm:trace.h)   |  7 ++++++-
 3 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ea6bec3331b150ec2a3f44a0d098516cddd1978ca), 82 insertions(+), 1 deletion(-)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-43-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) --git a/arch/x86/kvm/pvm/pvm.c b/arch/x86/kvm/pvm/pvm.c
index e68052f33186..6ac599587567 100644
--- a/arch/x86/kvm/pvm/pvm.c
+++ b/arch/x86/kvm/pvm/pvm.c @@ -1996,6 +1996,43 @@ static int pvm_handle_exit(struct kvm_vcpu *vcpu, fastpath_t exit_fastpath)
 	return 0;
 }

+static u32 pvm_get_syscall_exit_reason(struct kvm_vcpu *vcpu)
+{
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+	unsigned long rip = kvm_rip_read(vcpu);
+
+	if (is_smod(pvm)) {
+		if (rip == pvm->msr_retu_rip_plus2)
+			return PVM_EXIT_REASONS_ERETU;
+		else if (rip == pvm->msr_rets_rip_plus2)
+			return PVM_EXIT_REASONS_ERETS;
+		else
+			return PVM_EXIT_REASONS_HYPERCALL;
+	}
+
+	return PVM_EXIT_REASONS_SYSCALL;
+}
+
+static void pvm_get_exit_info(struct kvm_vcpu *vcpu, u32 *reason, u64 *info1, u64 *info2,
+			      u32 *intr_info, u32 *error_code)
+{
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+
+	if (pvm->exit_vector == PVM_SYSCALL_VECTOR)
+		*reason = pvm_get_syscall_exit_reason(vcpu);
+	else if (pvm->exit_vector == IA32_SYSCALL_VECTOR)
+		*reason = PVM_EXIT_REASONS_INT80;
+	else if (pvm->exit_vector >= FIRST_EXTERNAL_VECTOR &&
+		 pvm->exit_vector < NR_VECTORS)
+		*reason = PVM_EXIT_REASONS_INTERRUPT;
+	else
+		*reason = pvm->exit_vector;
+	*info1 = pvm->exit_vector;
+	*info2 = pvm->exit_error_code;
+	*intr_info = pvm->exit_vector;
+	*error_code = pvm->exit_error_code;
+}
+
 static void pvm_handle_exit_irqoff(struct kvm_vcpu *vcpu)
 {
 	struct vcpu_pvm *pvm = to_pvm(vcpu);
@@ -2298,6 +2335,8 @@ static fastpath_t pvm_vcpu_run(struct kvm_vcpu *vcpu)
 	mark_page_dirty_in_slot(vcpu->kvm, pvm->pvcs_gpc.memslot,
 				pvm->pvcs_gpc.gpa >> PAGE_SHIFT);

+	trace_kvm_exit(vcpu, KVM_ISA_PVM);
+
 	return EXIT_FASTPATH_NONE;
 }

@@ -2627,6 +2666,8 @@ static struct kvm_x86_ops pvm_x86_ops __initdata = {
 	.refresh_apicv_exec_ctrl = pvm_refresh_apicv_exec_ctrl,
 	.deliver_interrupt = pvm_deliver_interrupt,

+	.get_exit_info = pvm_get_exit_info,
+
 	.vcpu_after_set_cpuid = pvm_vcpu_after_set_cpuid,

 	.check_intercept = pvm_check_intercept,
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-43-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.h) --git a/arch/x86/kvm/pvm/pvm.h b/arch/x86/kvm/pvm/pvm.h
index f28ab0b48f40..2f8fdb0ae3df 100644
--- a/arch/x86/kvm/pvm/pvm.h
+++ b/arch/x86/kvm/pvm/pvm.h @@ -10,6 +10,41 @@
 #define PVM_SYSCALL_VECTOR		SWITCH_EXIT_REASONS_SYSCALL
 #define PVM_FAILED_VMENTRY_VECTOR	SWITCH_EXIT_REASONS_FAILED_VMETNRY

+#define PVM_EXIT_REASONS_SHIFT		16
+#define PVM_EXIT_REASONS_SYSCALL	(1UL << PVM_EXIT_REASONS_SHIFT)
+#define PVM_EXIT_REASONS_HYPERCALL	(2UL << PVM_EXIT_REASONS_SHIFT)
+#define PVM_EXIT_REASONS_ERETU		(3UL << PVM_EXIT_REASONS_SHIFT)
+#define PVM_EXIT_REASONS_ERETS		(4UL << PVM_EXIT_REASONS_SHIFT)
+#define PVM_EXIT_REASONS_INTERRUPT	(5UL << PVM_EXIT_REASONS_SHIFT)
+#define PVM_EXIT_REASONS_INT80		(6UL << PVM_EXIT_REASONS_SHIFT)
+
+#define PVM_EXIT_REASONS\
+	{ DE_VECTOR, "DE excp" },\
+	{ DB_VECTOR, "DB excp" },\
+	{ NMI_VECTOR, "NMI excp" },\
+	{ BP_VECTOR, "BP excp" },\
+	{ OF_VECTOR, "OF excp" },\
+	{ BR_VECTOR, "BR excp" },\
+	{ UD_VECTOR, "UD excp" },\
+	{ NM_VECTOR, "NM excp" },\
+	{ DF_VECTOR, "DF excp" },\
+	{ TS_VECTOR, "TS excp" },\
+	{ SS_VECTOR, "SS excp" },\
+	{ GP_VECTOR, "GP excp" },\
+	{ PF_VECTOR, "PF excp" },\
+	{ MF_VECTOR, "MF excp" },\
+	{ AC_VECTOR, "AC excp" },\
+	{ MC_VECTOR, "MC excp" },\
+	{ XM_VECTOR, "XM excp" },\
+	{ VE_VECTOR, "VE excp" },\
+	{ PVM_EXIT_REASONS_SYSCALL, "SYSCALL" },\
+	{ PVM_EXIT_REASONS_HYPERCALL, "HYPERCALL" },\
+	{ PVM_EXIT_REASONS_ERETU, "ERETU" },\
+	{ PVM_EXIT_REASONS_ERETS, "ERETS" },\
+	{ PVM_EXIT_REASONS_INTERRUPT, "INTERRUPT" },\
+	{ PVM_EXIT_REASONS_INT80, "INT80" },\
+	{ PVM_FAILED_VMENTRY_VECTOR, "FAILED_VMENTRY" }
+
 #define PT_L4_SHIFT		39
 #define PT_L4_SIZE		(1UL << PT_L4_SHIFT)
 #define DEFAULT_RANGE_L4_SIZE	(32 * PT_L4_SIZE)
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-43-jiangshanlai::40gmail.com:1arch:x86:kvm:trace.h) --git a/arch/x86/kvm/trace.h b/arch/x86/kvm/trace.h
index 83843379813e..3d6549679e98 100644
--- a/arch/x86/kvm/trace.h
+++ b/arch/x86/kvm/trace.h @@ -8,6 +8,8 @@
 #include <asm/clocksource.h>
 #include <asm/pvclock-abi.h>

+#include "pvm/pvm.h"
+
 #undef TRACE_SYSTEM
 #define TRACE_SYSTEM kvm

@@ -282,11 +284,14 @@ TRACE_EVENT(kvm_apic,

 #define KVM_ISA_VMX   1
 #define KVM_ISA_SVM   2
+#define KVM_ISA_PVM   3

 #define kvm_print_exit_reason(exit_reason, isa)\
 	(isa == KVM_ISA_VMX) ?\
 	__print_symbolic(exit_reason & 0xffff, VMX_EXIT_REASONS) :\
-	__print_symbolic(exit_reason, SVM_EXIT_REASONS),		\ +	((isa == KVM_ISA_SVM) ?\
+	__print_symbolic(exit_reason, SVM_EXIT_REASONS) :\
+	__print_symbolic(exit_reason, PVM_EXIT_REASONS)),\
 	(isa == KVM_ISA_VMX && exit_reason & ~0xffff) ? " " : "",\
 	(isa == KVM_ISA_VMX) ?\
 	__print_flags(exit_reason & ~0xffff, " ", VMX_EXIT_REASON_FLAGS) : ""
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ma6bec3331b150ec2a3f44a0d098516cddd1978ca) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-43-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-43-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-43-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-43-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-43-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-43-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ra6bec3331b150ec2a3f44a0d098516cddd1978ca)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e25de9f37876e75c7b1168173fd21deb1976c578e) **[RFC PATCH 43/73] KVM: x86/PVM: Enable direct switching**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(41 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ra6bec3331b150ec2a3f44a0d098516cddd1978ca)
  2024-02-26 14:35 ` [[RFC PATCH 42/73] KVM: x86/PVM: Support for kvm_exit() tracepoint](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ma6bec3331b150ec2a3f44a0d098516cddd1978ca) Lai Jiangshan
**@ 2024-02-26 14:36 ` Lai Jiangshan**
  2024-02-26 14:36 ` [[RFC PATCH 44/73] KVM: x86/PVM: Implement TSC related callbacks](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mbd856b434a7deadfc6ce93537dd8df2b513bbc77) Lai Jiangshan
                   ` [(31 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rbd856b434a7deadfc6ce93537dd8df2b513bbc77)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r25de9f37876e75c7b1168173fd21deb1976c578e)
From: Lai Jiangshan @ 2024-02-26 14:36 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-44-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-44-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143658)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143658), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

To enable direct switching, certain necessary information needs to be
prepared in TSS for the switcher. Since only syscall and RETU hypercalls
are allowed for now, CPL switching-related information is needed before
VM enters. Additionally, after VM exit, the states in the hypervisor
should be updated if direct switching has occurred.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/kvm/pvm/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-44-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) | 87 +++++++++++++++++++++++++++++++++++++++++-
 [arch/x86/kvm/pvm/pvm.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-44-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.h) | 15 ++++++++
 2 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e25de9f37876e75c7b1168173fd21deb1976c578e), 100 insertions(+), 2 deletions(-)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-44-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) --git a/arch/x86/kvm/pvm/pvm.c b/arch/x86/kvm/pvm/pvm.c
index 6ac599587567..138d0c255cb8 100644
--- a/arch/x86/kvm/pvm/pvm.c
+++ b/arch/x86/kvm/pvm/pvm.c @@ -559,23 +559,70 @@ static void pvm_flush_hwtlb_gva(struct kvm_vcpu *vcpu, gva_t addr)
 	put_cpu();
 }

+static bool check_switch_cr3(struct vcpu_pvm *pvm, u64 switch_host_cr3)
+{
+	u64 root = pvm->vcpu.arch.mmu->prev_roots[0].hpa;
+
+	if (pvm->vcpu.arch.mmu->prev_roots[0].pgd != pvm->msr_switch_cr3)
+		return false;
+	if (!VALID_PAGE(root))
+		return false;
+	if (host_pcid_owner(switch_host_cr3 & X86_CR3_PCID_MASK) != pvm)
+		return false;
+	if (host_pcid_root(switch_host_cr3 & X86_CR3_PCID_MASK) != root)
+		return false;
+	if (root != (switch_host_cr3 & CR3_ADDR_MASK))
+		return false;
+
+	return true;
+}
+
 static void pvm_set_host_cr3_for_guest_with_host_pcid(struct vcpu_pvm *pvm)
 {
 	u64 root_hpa = pvm->vcpu.arch.mmu->root.hpa;
 	bool flush = false;
 	u32 host_pcid = host_pcid_get(pvm, root_hpa, &flush);
 	u64 hw_cr3 = root_hpa | host_pcid;
+	u64 switch_host_cr3;

 	if (!flush)
 		hw_cr3 |= CR3_NOFLUSH;
 	this_cpu_write(cpu_tss_rw.tss_ex.enter_cr3, hw_cr3);
+
+	if (is_smod(pvm)) {
+		this_cpu_write(cpu_tss_rw.tss_ex.smod_cr3, hw_cr3 | CR3_NOFLUSH);
+		switch_host_cr3 = this_cpu_read(cpu_tss_rw.tss_ex.umod_cr3);
+	} else {
+		this_cpu_write(cpu_tss_rw.tss_ex.umod_cr3, hw_cr3 | CR3_NOFLUSH);
+		switch_host_cr3 = this_cpu_read(cpu_tss_rw.tss_ex.smod_cr3);
+	}
+
+	if (check_switch_cr3(pvm, switch_host_cr3))
+		pvm->switch_flags &= ~SWITCH_FLAGS_NO_DS_CR3;
+	else
+		pvm->switch_flags |= SWITCH_FLAGS_NO_DS_CR3;
 }

 static void pvm_set_host_cr3_for_guest_without_host_pcid(struct vcpu_pvm *pvm)
 {
 	u64 root_hpa = pvm->vcpu.arch.mmu->root.hpa;
+	u64 switch_root = 0;
+
+	if (pvm->vcpu.arch.mmu->prev_roots[0].pgd == pvm->msr_switch_cr3) {
+		switch_root = pvm->vcpu.arch.mmu->prev_roots[0].hpa;
+		pvm->switch_flags &= ~SWITCH_FLAGS_NO_DS_CR3;
+	} else {
+		pvm->switch_flags |= SWITCH_FLAGS_NO_DS_CR3;
+	}

 	this_cpu_write(cpu_tss_rw.tss_ex.enter_cr3, root_hpa);
+	if (is_smod(pvm)) {
+		this_cpu_write(cpu_tss_rw.tss_ex.smod_cr3, root_hpa);
+		this_cpu_write(cpu_tss_rw.tss_ex.umod_cr3, switch_root);
+	} else {
+		this_cpu_write(cpu_tss_rw.tss_ex.umod_cr3, root_hpa);
+		this_cpu_write(cpu_tss_rw.tss_ex.smod_cr3, switch_root);
+	}
 }

 static void pvm_set_host_cr3_for_hypervisor(struct vcpu_pvm *pvm)
@@ -591,6 +638,8 @@ static void pvm_set_host_cr3_for_hypervisor(struct vcpu_pvm *pvm)

 // Set tss_ex.host_cr3 for VMExit.
 // Set tss_ex.enter_cr3 for VMEnter.
+// Set tss_ex.smod_cr3 and tss_ex.umod_cr3 and set or clear
+// SWITCH_FLAGS_NO_DS_CR3 for direct switching.
 static void pvm_set_host_cr3(struct vcpu_pvm *pvm)
 {
 	pvm_set_host_cr3_for_hypervisor(pvm);
@@ -1058,6 +1107,11 @@ static bool pvm_apic_init_signal_blocked(struct kvm_vcpu *vcpu)

 static void update_exception_bitmap(struct kvm_vcpu *vcpu)
 {
+	/* disable direct switch when single step debugging */
+	if (vcpu->guest_debug & KVM_GUESTDBG_SINGLESTEP)
+		to_pvm(vcpu)->switch_flags |= SWITCH_FLAGS_SINGLE_STEP;
+	else
+		to_pvm(vcpu)->switch_flags &= ~SWITCH_FLAGS_SINGLE_STEP;
 }

 static struct pvm_vcpu_struct *pvm_get_vcpu_struct(struct vcpu_pvm *pvm)
@@ -1288,10 +1342,12 @@ static void pvm_set_rflags(struct kvm_vcpu *vcpu, unsigned long rflags)
 	if (!need_update || !is_smod(pvm))
 		return;

-	if (rflags & X86_EFLAGS_IF) +	if (rflags & X86_EFLAGS_IF) {
+		pvm->switch_flags &= ~SWITCH_FLAGS_IRQ_WIN;
 		pvm_event_flags_update(vcpu, X86_EFLAGS_IF, PVM_EVENT_FLAGS_IP);
-	else +	} else {
 		pvm_event_flags_update(vcpu, 0, X86_EFLAGS_IF);
+	}
 }

 static bool pvm_get_if_flag(struct kvm_vcpu *vcpu)
@@ -1311,6 +1367,7 @@ static void pvm_set_interrupt_shadow(struct kvm_vcpu *vcpu, int mask)

 static void enable_irq_window(struct kvm_vcpu *vcpu)
 {
+	to_pvm(vcpu)->switch_flags |= SWITCH_FLAGS_IRQ_WIN;
 	pvm_event_flags_update(vcpu, PVM_EVENT_FLAGS_IP, 0);
 }

@@ -1332,6 +1389,7 @@ static void pvm_set_nmi_mask(struct kvm_vcpu *vcpu, bool masked)

 static void enable_nmi_window(struct kvm_vcpu *vcpu)
 {
+	to_pvm(vcpu)->switch_flags |= SWITCH_FLAGS_NMI_WIN;
 }

 static int pvm_nmi_allowed(struct kvm_vcpu *vcpu, bool for_injection)
@@ -1361,6 +1419,8 @@ static void pvm_inject_irq(struct kvm_vcpu *vcpu, bool reinjected)

 	trace_kvm_inj_virq(irq, vcpu->arch.interrupt.soft, false);

+	to_pvm(vcpu)->switch_flags &= ~SWITCH_FLAGS_IRQ_WIN;
+
 	if (do_pvm_event(vcpu, irq, false, 0))
 		kvm_clear_interrupt_queue(vcpu);

@@ -1397,6 +1457,7 @@ static int handle_synthetic_instruction_return_user(struct kvm_vcpu *vcpu)

 	// instruction to return user means nmi allowed.
 	pvm->nmi_mask = false;
+	pvm->switch_flags &= ~(SWITCH_FLAGS_IRQ_WIN | SWITCH_FLAGS_NMI_WIN);

 	/*
 	 * switch to user mode before kvm_set_rflags() to avoid PVM_EVENT_FLAGS_IF
@@ -1448,6 +1509,7 @@ static int handle_synthetic_instruction_return_supervisor(struct kvm_vcpu *vcpu)

 	// instruction to return supervisor means nmi allowed.
 	pvm->nmi_mask = false;
+	pvm->switch_flags &= ~SWITCH_FLAGS_NMI_WIN;

 	kvm_set_rflags(vcpu, frame.rflags);
 	kvm_rip_write(vcpu, frame.rip);
@@ -1461,6 +1523,7 @@ static int handle_synthetic_instruction_return_supervisor(struct kvm_vcpu *vcpu)
 static int handle_hc_interrupt_window(struct kvm_vcpu *vcpu)
 {
 	kvm_make_request(KVM_REQ_EVENT, vcpu);
+	to_pvm(vcpu)->switch_flags &= ~SWITCH_FLAGS_IRQ_WIN;
 	pvm_event_flags_update(vcpu, 0, PVM_EVENT_FLAGS_IP);

 	++vcpu->stat.irq_window_exits;
@@ -2199,6 +2262,7 @@ static __always_inline void load_regs(struct kvm_vcpu *vcpu, struct pt_regs *gue

 static noinstr void pvm_vcpu_run_noinstr(struct kvm_vcpu *vcpu)
 {
+	struct tss_extra *tss_ex = this_cpu_ptr(&cpu_tss_rw.tss_ex);
 	struct vcpu_pvm *pvm = to_pvm(vcpu);
 	struct pt_regs *sp0_regs = (struct pt_regs *)this_cpu_read(cpu_tss_rw.x86_tss.sp0) - 1;
 	struct pt_regs *ret_regs;
@@ -2208,12 +2272,25 @@ static noinstr void pvm_vcpu_run_noinstr(struct kvm_vcpu *vcpu)
 	// Load guest registers into the host sp0 stack for switcher.
 	load_regs(vcpu, sp0_regs);

+	// Prepare context for direct switching.
+	tss_ex->switch_flags = pvm->switch_flags;
+	tss_ex->pvcs = pvm->pvcs_gpc.khva;
+	tss_ex->retu_rip = pvm->msr_retu_rip_plus2;
+	tss_ex->smod_entry = pvm->msr_lstar;
+	tss_ex->smod_gsbase = pvm->msr_kernel_gs_base;
+	tss_ex->smod_rsp = pvm->msr_supervisor_rsp;
+
 	if (unlikely(pvm->guest_dr7 & DR7_BP_EN_MASK))
 		set_debugreg(pvm_eff_dr7(vcpu), 7);

 	// Call into switcher and enter guest.
 	ret_regs = switcher_enter_guest();

+	// Get the resulted mode and PVM MSRs which might be changed
+	// when direct switching.
+	pvm->switch_flags = tss_ex->switch_flags;
+	pvm->msr_supervisor_rsp = tss_ex->smod_rsp;
+
 	// Get the guest registers from the host sp0 stack.
 	save_regs(vcpu, ret_regs);
 	pvm->exit_vector = (ret_regs->orig_ax >> 32);
@@ -2293,6 +2370,7 @@ static inline void pvm_load_host_xsave_state(struct kvm_vcpu *vcpu)
 static fastpath_t pvm_vcpu_run(struct kvm_vcpu *vcpu)
 {
 	struct vcpu_pvm *pvm = to_pvm(vcpu);
+	bool is_smod_befor_run = is_smod(pvm);

 	trace_kvm_entry(vcpu);

@@ -2307,6 +2385,11 @@ static fastpath_t pvm_vcpu_run(struct kvm_vcpu *vcpu)

 	pvm_vcpu_run_noinstr(vcpu);

+	if (is_smod_befor_run != is_smod(pvm)) {
+		swap(pvm->vcpu.arch.mmu->root, pvm->vcpu.arch.mmu->prev_roots[0]);
+		swap(pvm->msr_switch_cr3, pvm->vcpu.arch.cr3);
+	}
+
 	/* MSR_IA32_DEBUGCTLMSR is zeroed before vmenter. Restore it if needed */
 	if (pvm->host_debugctlmsr)
 		update_debugctlmsr(pvm->host_debugctlmsr);
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-44-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.h) --git a/arch/x86/kvm/pvm/pvm.h b/arch/x86/kvm/pvm/pvm.h
index 2f8fdb0ae3df..e49d9dc70a94 100644
--- a/arch/x86/kvm/pvm/pvm.h
+++ b/arch/x86/kvm/pvm/pvm.h @@ -5,6 +5,21 @@
 #include <linux/kvm_host.h>
 #include <asm/switcher.h>

+/*
+ * Extra switch flags:
+ *
+ * IRQ_WIN:
+ *	There is an irq window request, and the vcpu should not directly
+ *	switch to context with IRQ enabled, e.g. user mode.
+ * NMI_WIN:
+ *	There is an NMI window request.
+ * SINGLE_STEP:
+ *	KVM_GUESTDBG_SINGLESTEP is set.
+ */
+#define SWITCH_FLAGS_IRQ_WIN				_BITULL(8)
+#define SWITCH_FLAGS_NMI_WIN				_BITULL(9)
+#define SWITCH_FLAGS_SINGLE_STEP			_BITULL(10)
+
 #define SWITCH_FLAGS_INIT	(SWITCH_FLAGS_SMOD)

 #define PVM_SYSCALL_VECTOR		SWITCH_EXIT_REASONS_SYSCALL
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m25de9f37876e75c7b1168173fd21deb1976c578e) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-44-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-44-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-44-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-44-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-44-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-44-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r25de9f37876e75c7b1168173fd21deb1976c578e)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ebd856b434a7deadfc6ce93537dd8df2b513bbc77) **[RFC PATCH 44/73] KVM: x86/PVM: Implement TSC related callbacks**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(42 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r25de9f37876e75c7b1168173fd21deb1976c578e)
  2024-02-26 14:36 ` [[RFC PATCH 43/73] KVM: x86/PVM: Enable direct switching](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m25de9f37876e75c7b1168173fd21deb1976c578e) Lai Jiangshan
**@ 2024-02-26 14:36 ` Lai Jiangshan**
  2024-02-26 14:36 ` [[RFC PATCH 45/73] KVM: x86/PVM: Add dummy PMU](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mc83425f1d7e043377f7c4d27ae91e7d06ec7ec77) " Lai Jiangshan
                   ` [(30 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rc83425f1d7e043377f7c4d27ae91e7d06ec7ec77)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rbd856b434a7deadfc6ce93537dd8df2b513bbc77)
From: Lai Jiangshan @ 2024-02-26 14:36 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-45-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-45-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143702)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143702), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

Without hardware assistance, TSC offset and TSC multiplier are not
supported in PVM. Therefore, the guest uses the host TSC directly, which
means the TSC offset is 0. Although it currently works correctly, a
proper ABI is needed to describe it.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/kvm/pvm/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-45-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) | 26 ++++++++++++++++++++++++++
 1 file [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ebd856b434a7deadfc6ce93537dd8df2b513bbc77), 26 insertions(+)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-45-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) --git a/arch/x86/kvm/pvm/pvm.c b/arch/x86/kvm/pvm/pvm.c
index 138d0c255cb8..f2cd1a1c199d 100644
--- a/arch/x86/kvm/pvm/pvm.c
+++ b/arch/x86/kvm/pvm/pvm.c @@ -725,6 +725,28 @@ static int pvm_check_intercept(struct kvm_vcpu *vcpu,
 	return X86EMUL_CONTINUE;
 }

+static u64 pvm_get_l2_tsc_offset(struct kvm_vcpu *vcpu)
+{
+	return 0;
+}
+
+static u64 pvm_get_l2_tsc_multiplier(struct kvm_vcpu *vcpu)
+{
+	return 0;
+}
+
+static void pvm_write_tsc_offset(struct kvm_vcpu *vcpu)
+{
+	// TODO: add proper ABI and make guest use host TSC
+	vcpu->arch.tsc_offset = 0;
+	vcpu->arch.l1_tsc_offset = 0;
+}
+
+static void pvm_write_tsc_multiplier(struct kvm_vcpu *vcpu)
+{
+	// TODO: add proper ABI and make guest use host TSC
+}
+
 static void pvm_set_msr_linear_address_range(struct vcpu_pvm *pvm,
 					     u64 pml4_i_s, u64 pml4_i_e,
 					     u64 pml5_i_s, u64 pml5_i_e)
@@ -2776,6 +2798,10 @@ static struct kvm_x86_ops pvm_x86_ops __initdata = {
 	.complete_emulated_msr = kvm_complete_insn_gp,
 	.vcpu_deliver_sipi_vector = kvm_vcpu_deliver_sipi_vector,

+	.get_l2_tsc_offset = pvm_get_l2_tsc_offset,
+	.get_l2_tsc_multiplier = pvm_get_l2_tsc_multiplier,
+	.write_tsc_offset = pvm_write_tsc_offset,
+	.write_tsc_multiplier = pvm_write_tsc_multiplier,
 	.check_emulate_instruction = pvm_check_emulate_instruction,
 	.disallowed_va = pvm_disallowed_va,
 	.vcpu_gpc_refresh = pvm_vcpu_gpc_refresh,
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mbd856b434a7deadfc6ce93537dd8df2b513bbc77) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-45-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-45-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-45-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-45-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-45-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-45-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rbd856b434a7deadfc6ce93537dd8df2b513bbc77)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ec83425f1d7e043377f7c4d27ae91e7d06ec7ec77) **[RFC PATCH 45/73] KVM: x86/PVM: Add dummy PMU related callbacks**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(43 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rbd856b434a7deadfc6ce93537dd8df2b513bbc77)
  2024-02-26 14:36 ` [[RFC PATCH 44/73] KVM: x86/PVM: Implement TSC related callbacks](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mbd856b434a7deadfc6ce93537dd8df2b513bbc77) Lai Jiangshan
**@ 2024-02-26 14:36 ` Lai Jiangshan**
  2024-02-26 14:36 ` [[RFC PATCH 46/73] KVM: x86/PVM: Support for CPUID faulting](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m07d7573539e96b204ff87b1da08e99461e387e1b) Lai Jiangshan
                   ` [(29 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r07d7573539e96b204ff87b1da08e99461e387e1b)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rc83425f1d7e043377f7c4d27ae91e7d06ec7ec77)
From: Lai Jiangshan @ 2024-02-26 14:36 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-46-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-46-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143705)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143705), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

Currently, PMU virtualization is not implemented, so dummy PMU related
callbacks are added to make PVM work. In the future, the existing code
in pmu_intel.c and pmu_amd.c will be reused to implement PMU
virtualization for PVM.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/kvm/pvm/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-46-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) | 72 ++++++++++++++++++++++++++++++++++++++++++
 1 file [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ec83425f1d7e043377f7c4d27ae91e7d06ec7ec77), 72 insertions(+)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-46-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) --git a/arch/x86/kvm/pvm/pvm.c b/arch/x86/kvm/pvm/pvm.c
index f2cd1a1c199d..e6464095d40b 100644
--- a/arch/x86/kvm/pvm/pvm.c
+++ b/arch/x86/kvm/pvm/pvm.c @@ -21,6 +21,7 @@
 #include "cpuid.h"
 #include "lapic.h"
 #include "mmu.h"
+#include "pmu.h"
 #include "trace.h"
 #include "x86.h"
 #include "pvm.h"
@@ -2701,6 +2702,76 @@ static void hardware_unsetup(void)
 {
 }

+//====== start of dummy pmu ===========
+//TODO: split kvm-pmu-intel.ko & kvm-pmu-amd.ko from kvm-intel.ko & kvm-amd.ko.
+static bool dummy_pmu_hw_event_available(struct kvm_pmc *pmc)
+{
+	return true;
+}
+
+static struct kvm_pmc *dummy_pmc_idx_to_pmc(struct kvm_pmu *pmu, int pmc_idx)
+{
+	return NULL;
+}
+
+static struct kvm_pmc *dummy_pmu_rdpmc_ecx_to_pmc(struct kvm_vcpu *vcpu,
+						  unsigned int idx, u64 *mask)
+{
+	return NULL;
+}
+
+static bool dummy_pmu_is_valid_rdpmc_ecx(struct kvm_vcpu *vcpu, unsigned int idx)
+{
+	return false;
+}
+
+static struct kvm_pmc *dummy_pmu_msr_idx_to_pmc(struct kvm_vcpu *vcpu, u32 msr)
+{
+	return NULL;
+}
+
+static bool dummy_pmu_is_valid_msr(struct kvm_vcpu *vcpu, u32 msr)
+{
+	return 0;
+}
+
+static int dummy_pmu_get_msr(struct kvm_vcpu *vcpu, struct msr_data *msr_info)
+{
+	return 1;
+}
+
+static int dummy_pmu_set_msr(struct kvm_vcpu *vcpu, struct msr_data *msr_info)
+{
+	return 1;
+}
+
+static void dummy_pmu_refresh(struct kvm_vcpu *vcpu)
+{
+}
+
+static void dummy_pmu_init(struct kvm_vcpu *vcpu)
+{
+}
+
+static void dummy_pmu_reset(struct kvm_vcpu *vcpu)
+{
+}
+
+struct kvm_pmu_ops dummy_pmu_ops = {
+	.hw_event_available = dummy_pmu_hw_event_available,
+	.pmc_idx_to_pmc = dummy_pmc_idx_to_pmc,
+	.rdpmc_ecx_to_pmc = dummy_pmu_rdpmc_ecx_to_pmc,
+	.msr_idx_to_pmc = dummy_pmu_msr_idx_to_pmc,
+	.is_valid_rdpmc_ecx = dummy_pmu_is_valid_rdpmc_ecx,
+	.is_valid_msr = dummy_pmu_is_valid_msr,
+	.get_msr = dummy_pmu_get_msr,
+	.set_msr = dummy_pmu_set_msr,
+	.refresh = dummy_pmu_refresh,
+	.init = dummy_pmu_init,
+	.reset = dummy_pmu_reset,
+};
+//========== end of dummy pmu =============
+
 struct kvm_x86_nested_ops pvm_nested_ops = {};

 static struct kvm_x86_ops pvm_x86_ops __initdata = {
@@ -2811,6 +2882,7 @@ static struct kvm_x86_init_ops pvm_init_ops __initdata = {
 	.hardware_setup = hardware_setup,

 	.runtime_ops = &pvm_x86_ops,
+	.pmu_ops = &dummy_pmu_ops,
 };

 static void pvm_exit(void)
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mc83425f1d7e043377f7c4d27ae91e7d06ec7ec77) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-46-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-46-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-46-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-46-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-46-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-46-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rc83425f1d7e043377f7c4d27ae91e7d06ec7ec77)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e07d7573539e96b204ff87b1da08e99461e387e1b) **[RFC PATCH 46/73] KVM: x86/PVM: Support for CPUID faulting**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(44 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rc83425f1d7e043377f7c4d27ae91e7d06ec7ec77)
  2024-02-26 14:36 ` [[RFC PATCH 45/73] KVM: x86/PVM: Add dummy PMU](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mc83425f1d7e043377f7c4d27ae91e7d06ec7ec77) " Lai Jiangshan
**@ 2024-02-26 14:36 ` Lai Jiangshan**
  2024-02-26 14:36 ` [[RFC PATCH 47/73] KVM: x86/PVM: Handle the left supported MSRs in msrs_to_save_base[]](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m616815a2df532d615348d29e26f196e3302818a2) Lai Jiangshan
                   ` [(28 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r616815a2df532d615348d29e26f196e3302818a2)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r07d7573539e96b204ff87b1da08e99461e387e1b)
From: Lai Jiangshan @ 2024-02-26 14:36 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-47-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-47-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143708)
  Cc: Hou Wenlong, Lai Jiangshan, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143708), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Hou Wenlong <houwenlong.hwl@antgroup.com>

For PVM, CPUID faulting relies on hardware, so the guest could access
the host CPUID information if CPUID faulting is not enabled. To enable
the guest to access its own CPUID information, introduce a module
parameter to force enable CPUID faulting for the guest.

Suggested-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
---
 [arch/x86/kvm/pvm/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-47-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) | 69 ++++++++++++++++++++++++++++++++++++++++++
 1 file [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e07d7573539e96b204ff87b1da08e99461e387e1b), 69 insertions(+)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-47-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) --git a/arch/x86/kvm/pvm/pvm.c b/arch/x86/kvm/pvm/pvm.c
index e6464095d40b..fd3d6f7301af 100644
--- a/arch/x86/kvm/pvm/pvm.c
+++ b/arch/x86/kvm/pvm/pvm.c @@ -29,6 +29,9 @@
 MODULE_AUTHOR("AntGroup");
 MODULE_LICENSE("GPL");

+static bool __read_mostly enable_cpuid_intercept = 0;
+module_param_named(cpuid_intercept, enable_cpuid_intercept, bool, 0444);
+
 static bool __read_mostly is_intel;

 static unsigned long host_idt_base;
@@ -168,6 +171,53 @@ static bool pvm_disallowed_va(struct kvm_vcpu *vcpu, u64 va)
 	return !pvm_guest_allowed_va(vcpu, va);
 }

+static void __set_cpuid_faulting(bool on)
+{
+	u64 msrval;
+
+	rdmsrl_safe(MSR_MISC_FEATURES_ENABLES, &msrval);
+	msrval &= ~MSR_MISC_FEATURES_ENABLES_CPUID_FAULT;
+	msrval |= (on << MSR_MISC_FEATURES_ENABLES_CPUID_FAULT_BIT);
+	wrmsrl(MSR_MISC_FEATURES_ENABLES, msrval);
+}
+
+static void reset_cpuid_intercept(struct kvm_vcpu *vcpu)
+{
+	if (test_thread_flag(TIF_NOCPUID))
+		return;
+
+	if (enable_cpuid_intercept || cpuid_fault_enabled(vcpu))
+		__set_cpuid_faulting(false);
+}
+
+static void set_cpuid_intercept(struct kvm_vcpu *vcpu)
+{
+	if (test_thread_flag(TIF_NOCPUID))
+		return;
+
+	if (enable_cpuid_intercept || cpuid_fault_enabled(vcpu))
+		__set_cpuid_faulting(true);
+}
+
+static void pvm_update_guest_cpuid_faulting(struct kvm_vcpu *vcpu, u64 data)
+{
+	bool guest_enabled = cpuid_fault_enabled(vcpu);
+	bool set_enabled = data & MSR_MISC_FEATURES_ENABLES_CPUID_FAULT;
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+
+	if (!(guest_enabled ^ set_enabled))
+		return;
+	if (enable_cpuid_intercept)
+		return;
+	if (test_thread_flag(TIF_NOCPUID))
+		return;
+
+	preempt_disable();
+	if (pvm->loaded_cpu_state)
+		__set_cpuid_faulting(set_enabled);
+	preempt_enable();
+}
+
 // switch_to_smod() and switch_to_umod() switch the mode (smod/umod) and
 // the CR3.  No vTLB flushing when switching the CR3 per PVM Spec.
 static inline void switch_to_smod(struct kvm_vcpu *vcpu)
@@ -335,6 +385,8 @@ static void pvm_prepare_switch_to_guest(struct kvm_vcpu *vcpu)

 	segments_save_host_and_switch_to_guest(pvm);

+	set_cpuid_intercept(vcpu);
+
 	kvm_set_user_return_msr(0, (u64)entry_SYSCALL_64_switcher, -1ull);
 	kvm_set_user_return_msr(1, pvm->msr_tsc_aux, -1ull);
 	if (ia32_enabled()) {
@@ -352,6 +404,8 @@ static void pvm_prepare_switch_to_host(struct vcpu_pvm *pvm)

 	++pvm->vcpu.stat.host_state_reload;

+	reset_cpuid_intercept(&pvm->vcpu);
+
 #ifdef CONFIG_MODIFY_LDT_SYSCALL
 	if (unlikely(current->mm->context.ldt))
 		kvm_load_ldt(GDT_ENTRY_LDT*8);
@@ -937,6 +991,17 @@ static int pvm_set_msr(struct kvm_vcpu *vcpu, struct msr_data *msr_info)
 	case MSR_IA32_DEBUGCTLMSR:
 		/* It is ignored now. */
 		break;
+	case MSR_MISC_FEATURES_ENABLES:
+		ret = kvm_set_msr_common(vcpu, msr_info);
+		if (!ret)
+			pvm_update_guest_cpuid_faulting(vcpu, data);
+		break;
+	case MSR_PLATFORM_INFO:
+		if ((data & MSR_PLATFORM_INFO_CPUID_FAULT) &&
+		     !boot_cpu_has(X86_FEATURE_CPUID_FAULT))
+			return 1;
+		ret = kvm_set_msr_common(vcpu, msr_info);
+		break;
 	case MSR_PVM_VCPU_STRUCT:
 		if (!PAGE_ALIGNED(data))
 			return 1;
@@ -2925,6 +2990,10 @@ static int __init hardware_cap_check(void)
 		pr_warn("CMPXCHG16B is required for guest.\n");
 		return -EOPNOTSUPP;
 	}
+	if (!boot_cpu_has(X86_FEATURE_CPUID_FAULT) && enable_cpuid_intercept) {
+		pr_warn("Host doesn't support cpuid faulting.\n");
+		return -EOPNOTSUPP;
+	}

 	return 0;
 }
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m07d7573539e96b204ff87b1da08e99461e387e1b) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-47-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-47-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-47-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-47-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-47-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-47-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r07d7573539e96b204ff87b1da08e99461e387e1b)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e616815a2df532d615348d29e26f196e3302818a2) **[RFC PATCH 47/73] KVM: x86/PVM: Handle the left supported MSRs in msrs_to_save_base[]**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(45 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r07d7573539e96b204ff87b1da08e99461e387e1b)
  2024-02-26 14:36 ` [[RFC PATCH 46/73] KVM: x86/PVM: Support for CPUID faulting](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m07d7573539e96b204ff87b1da08e99461e387e1b) Lai Jiangshan
**@ 2024-02-26 14:36 ` Lai Jiangshan**
  2024-02-26 14:36 ` [[RFC PATCH 48/73] KVM: x86/PVM: Implement system registers setting callbacks](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5d9f7a5a2bc967bd2252cc11c9eb76b3778bb902) Lai Jiangshan
                   ` [(27 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r5d9f7a5a2bc967bd2252cc11c9eb76b3778bb902)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r616815a2df532d615348d29e26f196e3302818a2)
From: Lai Jiangshan @ 2024-02-26 14:36 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-48-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-48-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143711)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143711), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

The MSR_TSC_AUX is allowed to be modified by the guest to support
RDTSCP/RDPID in the guest. However, the MSR_IA32_FEAT_CTRL is not fully
supported for the guest at this time; only the FEAT_CTL_LOCKED bit is
valid for the guest.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/kvm/pvm/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-48-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) | 41 +++++++++++++++++++++++++++++++++++++++++
 1 file [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e616815a2df532d615348d29e26f196e3302818a2), 41 insertions(+)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-48-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) --git a/arch/x86/kvm/pvm/pvm.c b/arch/x86/kvm/pvm/pvm.c
index fd3d6f7301af..a32d2728eb02 100644
--- a/arch/x86/kvm/pvm/pvm.c
+++ b/arch/x86/kvm/pvm/pvm.c @@ -854,6 +854,32 @@ static void pvm_msr_filter_changed(struct kvm_vcpu *vcpu)
 	/* Accesses to MSRs are emulated in hypervisor, nothing to do here. */
 }

+static inline bool is_pvm_feature_control_msr_valid(struct vcpu_pvm *pvm,
+						    struct msr_data *msr_info)
+{
+	/*
+	 * currently only FEAT_CTL_LOCKED bit is valid, maybe
+	 * vmx, sgx and mce associated bits can be valid when those features
+	 * are supported for guest.
+	 */
+	u64 valid_bits = pvm->msr_ia32_feature_control_valid_bits;
+
+	if (!msr_info->host_initiated &&
+	    (pvm->msr_ia32_feature_control & FEAT_CTL_LOCKED))
+		return false;
+
+	return !(msr_info->data & ~valid_bits);
+}
+
+static void pvm_update_uret_msr(struct vcpu_pvm *pvm, unsigned int slot,
+				u64 data, u64 mask)
+{
+	preempt_disable();
+	if (pvm->loaded_cpu_state)
+		kvm_set_user_return_msr(slot, data, mask);
+	preempt_enable();
+}
+
 /*
  * Reads an msr value (of 'msr_index') into 'msr_info'.
  * Returns 0 on success, non-0 otherwise.
@@ -899,9 +925,15 @@ static int pvm_get_msr(struct kvm_vcpu *vcpu, struct msr_data *msr_info)
 	case MSR_IA32_SYSENTER_ESP:
 		msr_info->data = pvm->unused_MSR_IA32_SYSENTER_ESP;
 		break;
+	case MSR_TSC_AUX:
+		msr_info->data = pvm->msr_tsc_aux;
+		break;
 	case MSR_IA32_DEBUGCTLMSR:
 		msr_info->data = 0;
 		break;
+	case MSR_IA32_FEAT_CTL:
+		msr_info->data = pvm->msr_ia32_feature_control;
+		break;
 	case MSR_PVM_VCPU_STRUCT:
 		msr_info->data = pvm->msr_vcpu_struct;
 		break;
@@ -988,9 +1020,18 @@ static int pvm_set_msr(struct kvm_vcpu *vcpu, struct msr_data *msr_info)
 	case MSR_IA32_SYSENTER_ESP:
 		pvm->unused_MSR_IA32_SYSENTER_ESP = data;
 		break;
+	case MSR_TSC_AUX:
+		pvm->msr_tsc_aux = data;
+		pvm_update_uret_msr(pvm, 1, data, -1ull);
+		break;
 	case MSR_IA32_DEBUGCTLMSR:
 		/* It is ignored now. */
 		break;
+	case MSR_IA32_FEAT_CTL:
+		if (!is_intel || !is_pvm_feature_control_msr_valid(pvm, msr_info))
+			return 1;
+		pvm->msr_ia32_feature_control = data;
+		break;
 	case MSR_MISC_FEATURES_ENABLES:
 		ret = kvm_set_msr_common(vcpu, msr_info);
 		if (!ret)
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m616815a2df532d615348d29e26f196e3302818a2) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-48-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-48-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-48-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-48-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-48-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-48-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r616815a2df532d615348d29e26f196e3302818a2)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e5d9f7a5a2bc967bd2252cc11c9eb76b3778bb902) **[RFC PATCH 48/73] KVM: x86/PVM: Implement system registers setting callbacks**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(46 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r616815a2df532d615348d29e26f196e3302818a2)
  2024-02-26 14:36 ` [[RFC PATCH 47/73] KVM: x86/PVM: Handle the left supported MSRs in msrs_to_save_base[]](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m616815a2df532d615348d29e26f196e3302818a2) Lai Jiangshan
**@ 2024-02-26 14:36 ` Lai Jiangshan**
  2024-02-26 14:36 ` [[RFC PATCH 49/73] KVM: x86/PVM: Implement emulation for non-PVM mode](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m4d425168fece4bbd313c4b5bac4dc2ad0db5bf15) Lai Jiangshan
                   ` [(26 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r4d425168fece4bbd313c4b5bac4dc2ad0db5bf15)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r5d9f7a5a2bc967bd2252cc11c9eb76b3778bb902)
From: Lai Jiangshan @ 2024-02-26 14:36 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-49-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-49-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143714)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143714), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

In PVM, the hardware CR0, CR3, and EFER are fixed, and the value of the
guest must match the fixed value; otherwise, the guest is not allowed to
run on the CPU.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/kvm/pvm/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-49-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) | 51 ++++++++++++++++++++++++++++++++++++++++++
 1 file [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e5d9f7a5a2bc967bd2252cc11c9eb76b3778bb902), 51 insertions(+)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-49-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) --git a/arch/x86/kvm/pvm/pvm.c b/arch/x86/kvm/pvm/pvm.c
index a32d2728eb02..b261309fc946 100644
--- a/arch/x86/kvm/pvm/pvm.c
+++ b/arch/x86/kvm/pvm/pvm.c @@ -1088,6 +1088,51 @@ static int pvm_set_msr(struct kvm_vcpu *vcpu, struct msr_data *msr_info)
 	return ret;
 }

+static void pvm_cache_reg(struct kvm_vcpu *vcpu, enum kvm_reg reg)
+{
+	/* Nothing to do */
+}
+
+static int pvm_set_efer(struct kvm_vcpu *vcpu, u64 efer)
+{
+	vcpu->arch.efer = efer;
+
+	return 0;
+}
+
+static bool pvm_is_valid_cr0(struct kvm_vcpu *vcpu, unsigned long cr4)
+{
+	return true;
+}
+
+static void pvm_set_cr0(struct kvm_vcpu *vcpu, unsigned long cr0)
+{
+	if (vcpu->arch.efer & EFER_LME) {
+		if (!is_paging(vcpu) && (cr0 & X86_CR0_PG))
+			vcpu->arch.efer |= EFER_LMA;
+
+		if (is_paging(vcpu) && !(cr0 & X86_CR0_PG))
+			vcpu->arch.efer &= ~EFER_LMA;
+	}
+
+	vcpu->arch.cr0 = cr0;
+}
+
+static bool pvm_is_valid_cr4(struct kvm_vcpu *vcpu, unsigned long cr4)
+{
+	return true;
+}
+
+static void pvm_set_cr4(struct kvm_vcpu *vcpu, unsigned long cr4)
+{
+	unsigned long old_cr4 = vcpu->arch.cr4;
+
+	vcpu->arch.cr4 = cr4;
+
+	if ((cr4 ^ old_cr4) & (X86_CR4_OSXSAVE | X86_CR4_PKE))
+		kvm_update_cpuid_runtime(vcpu);
+}
+
 static void pvm_get_segment(struct kvm_vcpu *vcpu,
 			    struct kvm_segment *var, int seg)
 {
@@ -2912,13 +2957,19 @@ static struct kvm_x86_ops pvm_x86_ops __initdata = {
 	.set_segment = pvm_set_segment,
 	.get_cpl = pvm_get_cpl,
 	.get_cs_db_l_bits = pvm_get_cs_db_l_bits,
+	.is_valid_cr0 = pvm_is_valid_cr0,
+	.set_cr0 = pvm_set_cr0,
 	.load_mmu_pgd = pvm_load_mmu_pgd,
+	.is_valid_cr4 = pvm_is_valid_cr4,
+	.set_cr4 = pvm_set_cr4,
+	.set_efer = pvm_set_efer,
 	.get_gdt = pvm_get_gdt,
 	.set_gdt = pvm_set_gdt,
 	.get_idt = pvm_get_idt,
 	.set_idt = pvm_set_idt,
 	.set_dr7 = pvm_set_dr7,
 	.sync_dirty_debug_regs = pvm_sync_dirty_debug_regs,
+	.cache_reg = pvm_cache_reg,
 	.get_rflags = pvm_get_rflags,
 	.set_rflags = pvm_set_rflags,
 	.get_if_flag = pvm_get_if_flag,
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5d9f7a5a2bc967bd2252cc11c9eb76b3778bb902) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-49-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-49-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-49-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-49-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-49-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-49-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r5d9f7a5a2bc967bd2252cc11c9eb76b3778bb902)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e4d425168fece4bbd313c4b5bac4dc2ad0db5bf15) **[RFC PATCH 49/73] KVM: x86/PVM: Implement emulation for non-PVM mode**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(47 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r5d9f7a5a2bc967bd2252cc11c9eb76b3778bb902)
  2024-02-26 14:36 ` [[RFC PATCH 48/73] KVM: x86/PVM: Implement system registers setting callbacks](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5d9f7a5a2bc967bd2252cc11c9eb76b3778bb902) Lai Jiangshan
**@ 2024-02-26 14:36 ` Lai Jiangshan**
  2024-02-26 14:36 ` [[RFC PATCH 50/73] x86/tools/relocs: Cleanup cmdline options](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m0641eb1cf96777477a3260e67b6deb76ef03621e) Lai Jiangshan
                   ` [(25 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r0641eb1cf96777477a3260e67b6deb76ef03621e)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r4d425168fece4bbd313c4b5bac4dc2ad0db5bf15)
From: Lai Jiangshan @ 2024-02-26 14:36 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-50-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-50-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143718)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143718), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

The PVM hypervisor supports a modified long mode with PVM ABI, known as
PVM mode. PVM mode includes a 64-bit supervisor mode, 64-bit user mode,
and a 32-bit compatible user mode. The 32-bit supervisor mode and other
operating modes are considered non-PVM modes. In PVM mode, the states of
system registers are standard, and the guest is allowed to run on the
hardware. So far, non-PVM mode is required for booting the guest and
bringing up vCPUs. Currently, there is only basic support for non-PVM
mode through instruction emulation.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/kvm/pvm/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-50-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) | 145 +++++++++++++++++++++++++++++++++++++++--
 [arch/x86/kvm/pvm/pvm.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-50-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.h) |   1 +
 2 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e4d425168fece4bbd313c4b5bac4dc2ad0db5bf15), 139 insertions(+), 7 deletions(-)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-50-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.c) --git a/arch/x86/kvm/pvm/pvm.c b/arch/x86/kvm/pvm/pvm.c
index b261309fc946..e4b8f0108c31 100644
--- a/arch/x86/kvm/pvm/pvm.c
+++ b/arch/x86/kvm/pvm/pvm.c @@ -12,6 +12,7 @@
 #define pr_fmt(fmt) KBUILD_MODNAME ": " fmt

 #include <linux/module.h>
+#include <linux/entry-kvm.h>

 #include <asm/gsseg.h>
 #include <asm/io_bitmap.h>
@@ -218,6 +219,104 @@ static void pvm_update_guest_cpuid_faulting(struct kvm_vcpu *vcpu, u64 data)
 	preempt_enable();
 }

+/*
+ * Non-PVM mode is not a part of PVM.  Basic support for it via emulation.
+ * Non-PVM mode is required for booting the guest and bringing up vCPUs so far.
+ *
+ * In future, when VMM can directly boot the guest and bring vCPUs up from
+ * 64-bit mode without any help from non-64-bit mode, then the support non-PVM
+ * mode will be removed.
+ */
+#define CONVERT_TO_PVM_CR0_OFF	(X86_CR0_NW | X86_CR0_CD)
+#define CONVERT_TO_PVM_CR0_ON	(X86_CR0_NE | X86_CR0_AM | X86_CR0_WP |\
+				 X86_CR0_PG | X86_CR0_PE)
+
+static bool try_to_convert_to_pvm_mode(struct kvm_vcpu *vcpu)
+{
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+	unsigned long cr0 = vcpu->arch.cr0;
+
+	if (!is_long_mode(vcpu))
+		return false;
+	if (!pvm->segments[VCPU_SREG_CS].l) {
+		if (is_smod(pvm))
+			return false;
+		if (!pvm->segments[VCPU_SREG_CS].db)
+			return false;
+	}
+
+	/* Atomically set EFER_SCE converting to PVM mode. */
+	if ((vcpu->arch.efer | EFER_SCE) != vcpu->arch.efer)
+		vcpu->arch.efer |= EFER_SCE;
+
+	/* Change CR0 on converting to PVM mode. */
+	cr0 &= ~CONVERT_TO_PVM_CR0_OFF;
+	cr0 |= CONVERT_TO_PVM_CR0_ON;
+	if (cr0 != vcpu->arch.cr0)
+		kvm_set_cr0(vcpu, cr0);
+
+	/* Atomically set MSR_STAR on converting to PVM mode. */
+	if (!kernel_cs_by_msr(pvm->msr_star))
+		pvm->msr_star = ((u64)pvm->segments[VCPU_SREG_CS].selector << 32) |
+				((u64)__USER32_CS << 48);
+
+	pvm->non_pvm_mode = false;
+
+	return true;
+}
+
+static int handle_non_pvm_mode(struct kvm_vcpu *vcpu)
+{
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+	int ret = 1;
+	unsigned int count = 130;
+
+	if (try_to_convert_to_pvm_mode(vcpu))
+		return 1;
+
+	while (pvm->non_pvm_mode && count-- != 0) {
+		if (kvm_test_request(KVM_REQ_EVENT, vcpu))
+			return 1;
+
+		if (try_to_convert_to_pvm_mode(vcpu))
+			return 1;
+
+		ret = kvm_emulate_instruction(vcpu, 0);
+
+		if (!ret)
+			goto out;
+
+		/* don't do mode switch in emulation */
+		if (!is_smod(pvm))
+			goto emulation_error;
+
+		if (vcpu->arch.exception.pending)
+			goto emulation_error;
+
+		if (vcpu->arch.halt_request) {
+			vcpu->arch.halt_request = 0;
+			ret = kvm_emulate_halt_noskip(vcpu);
+			goto out;
+		}
+		/*
+		 * Note, return 1 and not 0, vcpu_run() will invoke
+		 * xfer_to_guest_mode() which will create a proper return
+		 * code.
+		 */
+		if (__xfer_to_guest_mode_work_pending())
+			return 1;
+	}
+
+out:
+	return ret;
+
+emulation_error:
+	vcpu->run->exit_reason = KVM_EXIT_INTERNAL_ERROR;
+	vcpu->run->internal.suberror = KVM_INTERNAL_ERROR_EMULATION;
+	vcpu->run->internal.ndata = 0;
+	return 0;
+}
+
 // switch_to_smod() and switch_to_umod() switch the mode (smod/umod) and
 // the CR3.  No vTLB flushing when switching the CR3 per PVM Spec.
 static inline void switch_to_smod(struct kvm_vcpu *vcpu)
@@ -359,6 +458,10 @@ static void pvm_prepare_switch_to_guest(struct kvm_vcpu *vcpu)
 	if (pvm->loaded_cpu_state)
 		return;

+	// we can't load guest state to hardware when guest is not on long mode
+	if (unlikely(pvm->non_pvm_mode))
+		return;
+
 	pvm->loaded_cpu_state = 1;

 #ifdef CONFIG_X86_IOPL_IOPERM
@@ -1138,6 +1241,11 @@ static void pvm_get_segment(struct kvm_vcpu *vcpu,
 {
 	struct vcpu_pvm *pvm = to_pvm(vcpu);

+	if (pvm->non_pvm_mode) {
+		*var = pvm->segments[seg];
+		return;
+	}
+
 	// Update CS or SS to reflect the current mode.
 	if (seg == VCPU_SREG_CS) {
 		if (is_smod(pvm)) {
@@ -1209,7 +1317,7 @@ static void pvm_set_segment(struct kvm_vcpu *vcpu, struct kvm_segment *var, int
 			if (cpl != var->dpl)
 				goto invalid_change;
 			if (cpl == 0 && !var->l)
-				goto invalid_change; +				pvm->non_pvm_mode = true;
 		}
 		break;
 	case VCPU_SREG_LDTR:
@@ -1231,12 +1339,17 @@ static void pvm_get_cs_db_l_bits(struct kvm_vcpu *vcpu, int *db, int *l)
 {
 	struct vcpu_pvm *pvm = to_pvm(vcpu);

-	if (pvm->hw_cs == __USER_CS) {
-		*db = 0;
-		*l = 1; +	if (pvm->non_pvm_mode) {
+		*db = pvm->segments[VCPU_SREG_CS].db;
+		*l = pvm->segments[VCPU_SREG_CS].l;
 	} else {
-		*db = 1;
-		*l = 0; +		if (pvm->hw_cs == __USER_CS) {
+			*db = 0;
+			*l = 1;
+		} else {
+			*db = 1;
+			*l = 0;
+		}
 	}
 }

@@ -1513,7 +1626,7 @@ static void pvm_set_rflags(struct kvm_vcpu *vcpu, unsigned long rflags)
 	 * user mode, so that when the guest switches back to supervisor mode,
 	 * the X86_EFLAGS_IF is already cleared.
 	 */
-	if (!need_update || !is_smod(pvm)) +	if (unlikely(pvm->non_pvm_mode) || !need_update || !is_smod(pvm))
 		return;

 	if (rflags & X86_EFLAGS_IF) {
@@ -1536,7 +1649,11 @@ static u32 pvm_get_interrupt_shadow(struct kvm_vcpu *vcpu)

 static void pvm_set_interrupt_shadow(struct kvm_vcpu *vcpu, int mask)
 {
+	struct vcpu_pvm *pvm = to_pvm(vcpu);
+
 	/* PVM spec: ignore interrupt shadow when in PVM mode. */
+	if (pvm->non_pvm_mode)
+		pvm->int_shadow = mask;
 }

 static void enable_irq_window(struct kvm_vcpu *vcpu)
@@ -2212,6 +2329,9 @@ static int pvm_handle_exit(struct kvm_vcpu *vcpu, fastpath_t exit_fastpath)
 	struct vcpu_pvm *pvm = to_pvm(vcpu);
 	u32 exit_reason = pvm->exit_vector;

+	if (unlikely(pvm->non_pvm_mode))
+		return handle_non_pvm_mode(vcpu);
+
 	if (exit_reason == PVM_SYSCALL_VECTOR)
 		return handle_exit_syscall(vcpu);
 	else if (exit_reason >= 0 && exit_reason < FIRST_EXTERNAL_VECTOR)
@@ -2546,6 +2666,13 @@ static fastpath_t pvm_vcpu_run(struct kvm_vcpu *vcpu)
 	struct vcpu_pvm *pvm = to_pvm(vcpu);
 	bool is_smod_befor_run = is_smod(pvm);

+	/*
+	 * Don't enter guest if guest state is invalid, let the exit handler
+	 * start emulation until we arrive back to a valid state.
+	 */
+	if (pvm->non_pvm_mode)
+		return EXIT_FASTPATH_NONE;
+
 	trace_kvm_entry(vcpu);

 	pvm_load_guest_xsave_state(vcpu);
@@ -2657,6 +2784,10 @@ static void pvm_vcpu_reset(struct kvm_vcpu *vcpu, bool init_event)
 	if (!boot_cpu_has(X86_FEATURE_CPUID_FAULT))
 		vcpu->arch.msr_platform_info &= ~MSR_PLATFORM_INFO_CPUID_FAULT;

+	// Non-PVM mode resets
+	pvm->non_pvm_mode = true;
+	pvm->msr_star = 0;
+
 	// X86 resets
 	for (i = 0; i < ARRAY_SIZE(pvm->segments); i++)
 		reset_segment(&pvm->segments[i], i);
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-50-jiangshanlai::40gmail.com:1arch:x86:kvm:pvm:pvm.h) --git a/arch/x86/kvm/pvm/pvm.h b/arch/x86/kvm/pvm/pvm.h
index e49d9dc70a94..1a4feddb13b3 100644
--- a/arch/x86/kvm/pvm/pvm.h
+++ b/arch/x86/kvm/pvm/pvm.h @@ -106,6 +106,7 @@ struct vcpu_pvm {

 	int loaded_cpu_state;
 	int int_shadow;
+	bool non_pvm_mode;
 	bool nmi_mask;

 	unsigned long guest_dr7;
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m4d425168fece4bbd313c4b5bac4dc2ad0db5bf15) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-50-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-50-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-50-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-50-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-50-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-50-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r4d425168fece4bbd313c4b5bac4dc2ad0db5bf15)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e0641eb1cf96777477a3260e67b6deb76ef03621e) **[RFC PATCH 50/73] x86/tools/relocs: Cleanup cmdline options**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(48 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r4d425168fece4bbd313c4b5bac4dc2ad0db5bf15)
  2024-02-26 14:36 ` [[RFC PATCH 49/73] KVM: x86/PVM: Implement emulation for non-PVM mode](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m4d425168fece4bbd313c4b5bac4dc2ad0db5bf15) Lai Jiangshan
**@ 2024-02-26 14:36 ` Lai Jiangshan**
  2024-02-26 14:36 ` [[RFC PATCH 51/73] x86/tools/relocs: Append relocations into input file](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m439206d2faf20a98af28d6448d297cb8b7c4bb8f) Lai Jiangshan
                   ` [(24 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r439206d2faf20a98af28d6448d297cb8b7c4bb8f)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r0641eb1cf96777477a3260e67b6deb76ef03621e)
From: Lai Jiangshan @ 2024-02-26 14:36 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-51-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-51-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143724)
  Cc: Hou Wenlong, Lai Jiangshan, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143724), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin, Andrew Morton, Alexey Dobriyan,
	Thomas Garnier

From: Hou Wenlong <houwenlong.hwl@antgroup.com>

Collect all cmdline options into a structure to make code
clean and to be easy to add new cmdline option.

Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
---
 [arch/x86/tools/relocs.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-51-jiangshanlai::40gmail.com:1arch:x86:tools:relocs.c)        | 30 ++++++++++++++----------------
 [arch/x86/tools/relocs.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-51-jiangshanlai::40gmail.com:1arch:x86:tools:relocs.h)        | 19 +++++++++++++------
 [arch/x86/tools/relocs_common.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-51-jiangshanlai::40gmail.com:1arch:x86:tools:relocs_common.c) | 27 +++++++++------------------
 3 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e0641eb1cf96777477a3260e67b6deb76ef03621e), 36 insertions(+), 40 deletions(-)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-51-jiangshanlai::40gmail.com:1arch:x86:tools:relocs.c) --git a/arch/x86/tools/relocs.c b/arch/x86/tools/relocs.c
index cf28a8f05375..743e5e44338b 100644
--- a/arch/x86/tools/relocs.c
+++ b/arch/x86/tools/relocs.c @@ -125,13 +125,13 @@ static int is_reloc(enum symtype type, const char *sym_name)
 		!regexec(&sym_regex_c[type], sym_name, 0, NULL, 0);
 }

-static void regex_init(int use_real_mode) +static void regex_init(void)
 {
         char errbuf[128];
         int err;
 	int i;

-	if (use_real_mode) +	if (opts.use_real_mode)
 		sym_regex = sym_regex_realmode;
 	else
 		sym_regex = sym_regex_kernel;
@@ -1164,7 +1164,7 @@ static int write32_as_text(uint32_t v, FILE *f)
 	return fprintf(f, "\t.long 0x%08"PRIx32"\n", v) > 0 ? 0 : -1;
 }

-static void emit_relocs(int as_text, int use_real_mode) +static void emit_relocs(void)
 {
 	int i;
 	int (*write_reloc)(uint32_t, FILE *) = write32;
@@ -1172,12 +1172,12 @@ static void emit_relocs(int as_text, int use_real_mode)
 			const char *symname);

 #if ELF_BITS == 64
-	if (!use_real_mode) +	if (!opts.use_real_mode)
 		do_reloc = do_reloc64;
 	else
 		die("--realmode not valid for a 64-bit ELF file");
 #else
-	if (!use_real_mode) +	if (!opts.use_real_mode)
 		do_reloc = do_reloc32;
 	else
 		do_reloc = do_reloc_real;
@@ -1186,7 +1186,7 @@ static void emit_relocs(int as_text, int use_real_mode)
 	/* Collect up the relocations */
 	walk_relocs(do_reloc);

-	if (relocs16.count && !use_real_mode) +	if (relocs16.count && !opts.use_real_mode)
 		die("Segment relocations found but --realmode not specified\n");

 	/* Order the relocations for more efficient processing */
@@ -1199,7 +1199,7 @@ static void emit_relocs(int as_text, int use_real_mode)
 #endif

 	/* Print the relocations */
-	if (as_text) { +	if (opts.as_text) {
 		/* Print the relocations in a form suitable that
 		 * gas will like.
 		 */
@@ -1208,7 +1208,7 @@ static void emit_relocs(int as_text, int use_real_mode)
 		write_reloc = write32_as_text;
 	}

-	if (use_real_mode) { +	if (opts.use_real_mode) {
 		write_reloc(relocs16.count, stdout);
 		for (i = 0; i < relocs16.count; i++)
 			write_reloc(relocs16.offset[i], stdout);
@@ -1271,11 +1271,9 @@ static void print_reloc_info(void)
 # define process process_32
 #endif

-void process(FILE *fp, int use_real_mode, int as_text,
-	     int show_absolute_syms, int show_absolute_relocs,
-	     int show_reloc_info) +void process(FILE *fp)
 {
-	regex_init(use_real_mode); +	regex_init();
 	read_ehdr(fp);
 	read_shdrs(fp);
 	read_strtabs(fp);
@@ -1284,17 +1282,17 @@ void process(FILE *fp, int use_real_mode, int as_text,
 	read_got(fp);
 	if (ELF_BITS == 64)
 		percpu_init();
-	if (show_absolute_syms) { +	if (opts.show_absolute_syms) {
 		print_absolute_symbols();
 		return;
 	}
-	if (show_absolute_relocs) { +	if (opts.show_absolute_relocs) {
 		print_absolute_relocs();
 		return;
 	}
-	if (show_reloc_info) { +	if (opts.show_reloc_info) {
 		print_reloc_info();
 		return;
 	}
-	emit_relocs(as_text, use_real_mode); +	emit_relocs();
 }
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-51-jiangshanlai::40gmail.com:1arch:x86:tools:relocs.h) --git a/arch/x86/tools/relocs.h b/arch/x86/tools/relocs.h
index 4c49c82446eb..1cb0e235ad73 100644
--- a/arch/x86/tools/relocs.h
+++ b/arch/x86/tools/relocs.h @@ -6,6 +6,7 @@
 #include <stdarg.h>
 #include <stdlib.h>
 #include <stdint.h>
+#include <stdbool.h>
 #include <inttypes.h>
 #include <string.h>
 #include <errno.h>
@@ -30,10 +31,16 @@ enum symtype {
 	S_NSYMTYPES
 };

-void process_32(FILE *fp, int use_real_mode, int as_text,
-		int show_absolute_syms, int show_absolute_relocs,
-		int show_reloc_info);
-void process_64(FILE *fp, int use_real_mode, int as_text,
-		int show_absolute_syms, int show_absolute_relocs,
-		int show_reloc_info); +struct opts {
+	bool use_real_mode;
+	bool as_text;
+	bool show_absolute_syms;
+	bool show_absolute_relocs;
+	bool show_reloc_info;
+};
+
+extern struct opts opts;
+
+void process_32(FILE *fp);
+void process_64(FILE *fp);
 #endif /* RELOCS_H */
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-51-jiangshanlai::40gmail.com:1arch:x86:tools:relocs_common.c) --git a/arch/x86/tools/relocs_common.c b/arch/x86/tools/relocs_common.c
index 6634352a20bc..17d69baee0c3 100644
--- a/arch/x86/tools/relocs_common.c
+++ b/arch/x86/tools/relocs_common.c @@ -1,6 +1,8 @@
 // SPDX-License-Identifier: GPL-2.0
 #include "relocs.h"

+struct opts opts;
+
 void die(char *fmt, ...)
 {
 	va_list ap;
@@ -18,40 +20,33 @@ static void usage(void)

 int main(int argc, char **argv)
 {
-	int show_absolute_syms, show_absolute_relocs, show_reloc_info;
-	int as_text, use_real_mode;
 	const char *fname;
 	FILE *fp;
 	int i;
 	unsigned char e_ident[EI_NIDENT];

-	show_absolute_syms = 0;
-	show_absolute_relocs = 0;
-	show_reloc_info = 0;
-	as_text = 0;
-	use_real_mode = 0;
 	fname = NULL;
 	for (i = 1; i < argc; i++) {
 		char *arg = argv[i];
 		if (*arg == '-') {
 			if (strcmp(arg, "--abs-syms") == 0) {
-				show_absolute_syms = 1; +				opts.show_absolute_syms = true;
 				continue;
 			}
 			if (strcmp(arg, "--abs-relocs") == 0) {
-				show_absolute_relocs = 1; +				opts.show_absolute_relocs = true;
 				continue;
 			}
 			if (strcmp(arg, "--reloc-info") == 0) {
-				show_reloc_info = 1; +				opts.show_reloc_info = true;
 				continue;
 			}
 			if (strcmp(arg, "--text") == 0) {
-				as_text = 1; +				opts.as_text = true;
 				continue;
 			}
 			if (strcmp(arg, "--realmode") == 0) {
-				use_real_mode = 1; +				opts.use_real_mode = true;
 				continue;
 			}
 		}
@@ -73,13 +68,9 @@ int main(int argc, char **argv)
 	}
 	rewind(fp);
 	if (e_ident[EI_CLASS] == ELFCLASS64)
-		process_64(fp, use_real_mode, as_text,
-			   show_absolute_syms, show_absolute_relocs,
-			   show_reloc_info); +		process_64(fp);
 	else
-		process_32(fp, use_real_mode, as_text,
-			   show_absolute_syms, show_absolute_relocs,
-			   show_reloc_info); +		process_32(fp);
 	fclose(fp);
 	return 0;
 }
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m0641eb1cf96777477a3260e67b6deb76ef03621e) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-51-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-51-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-51-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-51-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-51-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-51-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r0641eb1cf96777477a3260e67b6deb76ef03621e)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e439206d2faf20a98af28d6448d297cb8b7c4bb8f) **[RFC PATCH 51/73] x86/tools/relocs: Append relocations into input file**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(49 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r0641eb1cf96777477a3260e67b6deb76ef03621e)
  2024-02-26 14:36 ` [[RFC PATCH 50/73] x86/tools/relocs: Cleanup cmdline options](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m0641eb1cf96777477a3260e67b6deb76ef03621e) Lai Jiangshan
**@ 2024-02-26 14:36 ` Lai Jiangshan**
  2024-02-26 14:36 ` [[RFC PATCH 52/73] x86/boot: Allow to do relocation for uncompressed kernel](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#md2b2dd3e1a7220232ba2b4d850560a9397e9dee0) Lai Jiangshan
                   ` [(23 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rd2b2dd3e1a7220232ba2b4d850560a9397e9dee0)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r439206d2faf20a98af28d6448d297cb8b7c4bb8f)
From: Lai Jiangshan @ 2024-02-26 14:36 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-52-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-52-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143730)
  Cc: Hou Wenlong, Lai Jiangshan, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143730), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin, Andrew Morton, Alexey Dobriyan,
	Thomas Garnier

From: Hou Wenlong <houwenlong.hwl@antgroup.com>

Add a command line option to append relocations into a reserved section
named ".data.reloc" section of the input file. This is the same as the
implementation in MIPS.

Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
---
 [arch/x86/tools/relocs.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-52-jiangshanlai::40gmail.com:1arch:x86:tools:relocs.c)        | 62 +++++++++++++++++++++++++++-------
 [arch/x86/tools/relocs.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-52-jiangshanlai::40gmail.com:1arch:x86:tools:relocs.h)        |  1 +
 [arch/x86/tools/relocs_common.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-52-jiangshanlai::40gmail.com:1arch:x86:tools:relocs_common.c) | 11 ++++--
 3 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e439206d2faf20a98af28d6448d297cb8b7c4bb8f), 60 insertions(+), 14 deletions(-)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-52-jiangshanlai::40gmail.com:1arch:x86:tools:relocs.c) --git a/arch/x86/tools/relocs.c b/arch/x86/tools/relocs.c
index 743e5e44338b..97e0243b9abb 100644
--- a/arch/x86/tools/relocs.c
+++ b/arch/x86/tools/relocs.c @@ -912,6 +912,17 @@ static int is_percpu_sym(ElfW(Sym) *sym, const char *symname)
 		strncmp(symname, "init_per_cpu_", 13);
 }

+static struct section *sec_lookup(const char *name)
+{
+	int i;
+
+	for (i = 0; i < shnum; i++) {
+		if (!strcmp(sec_name(i), name))
+			return &secs[i];
+	}
+
+	return NULL;
+}

 static int do_reloc64(struct section *sec, Elf_Rel *rel, ElfW(Sym) *sym,
 		      const char *symname)
@@ -1164,12 +1175,13 @@ static int write32_as_text(uint32_t v, FILE *f)
 	return fprintf(f, "\t.long 0x%08"PRIx32"\n", v) > 0 ? 0 : -1;
 }

-static void emit_relocs(void) +static void emit_relocs(FILE *f)
 {
 	int i;
 	int (*write_reloc)(uint32_t, FILE *) = write32;
 	int (*do_reloc)(struct section *sec, Elf_Rel *rel, Elf_Sym *sym,
 			const char *symname);
+	FILE *outf = stdout;

 #if ELF_BITS == 64
 	if (!opts.use_real_mode)
@@ -1208,37 +1220,63 @@ static void emit_relocs(void)
 		write_reloc = write32_as_text;
 	}

+#if ELF_BITS == 64
+	if (opts.keep_relocs) {
+		struct section *sec_reloc;
+		uint32_t size_needed;
+		unsigned long offset;
+
+		sec_reloc = sec_lookup(".data.reloc");
+		if (!sec_reloc)
+			die("Could not find relocation data section\n");
+
+		size_needed = (3 + relocs64.count + relocs32neg.count +
+			      relocs32.count) * sizeof(uint32_t);
+		if (size_needed > sec_reloc->shdr.sh_size)
+			die("Relocations overflow available space!\n"\
+			    "Please adjust CONFIG_RELOCATION_TABLE_SIZE"\
+			    "to at least 0x%08x\n", (size_needed + 0x1000) & ~0xFFF);
+
+		offset = sec_reloc->shdr.sh_offset + sec_reloc->shdr.sh_size -
+			 size_needed;
+		if (fseek(f, offset, SEEK_SET) < 0)
+			die("Seek to %ld failed: %s\n", offset, strerror(errno));
+
+		outf = f;
+	}
+#endif
+
 	if (opts.use_real_mode) {
-		write_reloc(relocs16.count, stdout); +		write_reloc(relocs16.count, outf);
 		for (i = 0; i < relocs16.count; i++)
-			write_reloc(relocs16.offset[i], stdout); +			write_reloc(relocs16.offset[i], outf);

-		write_reloc(relocs32.count, stdout); +		write_reloc(relocs32.count, outf);
 		for (i = 0; i < relocs32.count; i++)
-			write_reloc(relocs32.offset[i], stdout); +			write_reloc(relocs32.offset[i], outf);
 	} else {
 #if ELF_BITS == 64
 		/* Print a stop */
-		write_reloc(0, stdout); +		write_reloc(0, outf);

 		/* Now print each relocation */
 		for (i = 0; i < relocs64.count; i++)
-			write_reloc(relocs64.offset[i], stdout); +			write_reloc(relocs64.offset[i], outf);

 		/* Print a stop */
-		write_reloc(0, stdout); +		write_reloc(0, outf);

 		/* Now print each inverse 32-bit relocation */
 		for (i = 0; i < relocs32neg.count; i++)
-			write_reloc(relocs32neg.offset[i], stdout); +			write_reloc(relocs32neg.offset[i], outf);
 #endif

 		/* Print a stop */
-		write_reloc(0, stdout); +		write_reloc(0, outf);

 		/* Now print each relocation */
 		for (i = 0; i < relocs32.count; i++)
-			write_reloc(relocs32.offset[i], stdout); +			write_reloc(relocs32.offset[i], outf);
 	}
 }

@@ -1294,5 +1332,5 @@ void process(FILE *fp)
 		print_reloc_info();
 		return;
 	}
-	emit_relocs(); +	emit_relocs(fp);
 }
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-52-jiangshanlai::40gmail.com:1arch:x86:tools:relocs.h) --git a/arch/x86/tools/relocs.h b/arch/x86/tools/relocs.h
index 1cb0e235ad73..20f729e4579f 100644
--- a/arch/x86/tools/relocs.h
+++ b/arch/x86/tools/relocs.h @@ -37,6 +37,7 @@ struct opts {
 	bool show_absolute_syms;
 	bool show_absolute_relocs;
 	bool show_reloc_info;
+	bool keep_relocs;
 };

 extern struct opts opts;
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-52-jiangshanlai::40gmail.com:1arch:x86:tools:relocs_common.c) --git a/arch/x86/tools/relocs_common.c b/arch/x86/tools/relocs_common.c
index 17d69baee0c3..87d94d9e4b97 100644
--- a/arch/x86/tools/relocs_common.c
+++ b/arch/x86/tools/relocs_common.c @@ -14,7 +14,7 @@ void die(char *fmt, ...)

 static void usage(void)
 {
-	die("relocs [--abs-syms|--abs-relocs|--reloc-info|--text|--realmode]" \ +	die("relocs [--abs-syms|--abs-relocs|--reloc-info|--text|--realmode|--keep]"\
 	    " vmlinux\n");
 }

@@ -49,6 +49,10 @@ int main(int argc, char **argv)
 				opts.use_real_mode = true;
 				continue;
 			}
+			if (strcmp(arg, "--keep") == 0) {
+				opts.keep_relocs = true;
+				continue;
+			}
 		}
 		else if (!fname) {
 			fname = arg;
@@ -59,7 +63,10 @@ int main(int argc, char **argv)
 	if (!fname) {
 		usage();
 	}
-	fp = fopen(fname, "r"); +	if (opts.keep_relocs)
+		fp = fopen(fname, "r+");
+	else
+		fp = fopen(fname, "r");
 	if (!fp) {
 		die("Cannot open %s: %s\n", fname, strerror(errno));
 	}
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m439206d2faf20a98af28d6448d297cb8b7c4bb8f) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-52-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-52-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-52-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-52-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-52-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-52-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r439206d2faf20a98af28d6448d297cb8b7c4bb8f)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ed2b2dd3e1a7220232ba2b4d850560a9397e9dee0) **[RFC PATCH 52/73] x86/boot: Allow to do relocation for uncompressed kernel**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(50 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r439206d2faf20a98af28d6448d297cb8b7c4bb8f)
  2024-02-26 14:36 ` [[RFC PATCH 51/73] x86/tools/relocs: Append relocations into input file](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m439206d2faf20a98af28d6448d297cb8b7c4bb8f) Lai Jiangshan
**@ 2024-02-26 14:36 ` Lai Jiangshan**
  2024-02-26 14:36 ` [[RFC PATCH 53/73] x86/pvm: Add Kconfig option and the CPU feature bit for PVM guest](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m712fb66a83ce7d3ec7a4fbc83a89c97f40f4414a) Lai Jiangshan
                   ` [(22 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r712fb66a83ce7d3ec7a4fbc83a89c97f40f4414a)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rd2b2dd3e1a7220232ba2b4d850560a9397e9dee0)
From: Lai Jiangshan @ 2024-02-26 14:36 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-53-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-53-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143739)
  Cc: Hou Wenlong, Lai Jiangshan, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143739), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin, Petr Pavlu, Josh Poimboeuf,
	Nick Desaulniers

From: Hou Wenlong <houwenlong.hwl@antgroup.com>

Relocation is currently only performed during the uncompression process.
However, in some situations, such as with security containers, the
uncompressed kernel can be booted directly. Therefore, it is useful to
allow for relocation of the uncompressed kernel. Taking inspiration from
the implementation in MIPS, a new section named ".data.relocs" is
reserved for relocations. The relocs tool can then append the
relocations into this section. Additionally, a helper function is
introduced to perform relocations during booting, similar to the
relocations in the bootloader. For PVH entry, relocation for the
pre-constructed page table should not be performed; otherwise, booting
will fail.

Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
---
 [arch/x86/Kconfig](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-53-jiangshanlai::40gmail.com:1arch:x86:Kconfig)                  | 20 +++++++++
 [arch/x86/Makefile.postlink](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-53-jiangshanlai::40gmail.com:1arch:x86:Makefile.postlink)        |  9 +++-
 [arch/x86/kernel/head64_identity.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-53-jiangshanlai::40gmail.com:1arch:x86:kernel:head64_identity.c) | 70 +++++++++++++++++++++++++++++++
 [arch/x86/kernel/vmlinux.lds.S](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-53-jiangshanlai::40gmail.com:1arch:x86:kernel:vmlinux.lds.S)     | 18 ++++++++
 4 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ed2b2dd3e1a7220232ba2b4d850560a9397e9dee0), 116 insertions(+), 1 deletion(-)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-53-jiangshanlai::40gmail.com:1arch:x86:Kconfig) --git a/arch/x86/Kconfig b/arch/x86/Kconfig
index a53b65499951..d02ef3bdb171 100644
--- a/arch/x86/Kconfig
+++ b/arch/x86/Kconfig @@ -2183,6 +2183,26 @@ config RELOCATABLE
 	  it has been loaded at and the compile time physical address
 	  (CONFIG_PHYSICAL_START) is used as the minimum location.

+config RELOCATABLE_UNCOMPRESSED_KERNEL
+	bool
+	depends on RELOCATABLE
+	help
+	  A table of relocation data will be appended to the uncompressed
+	  kernel binary and parsed at boot to fix up the relocated kernel.
+
+config RELOCATION_TABLE_SIZE
+	hex "Relocation table size"
+	depends on RELOCATABLE_UNCOMPRESSED_KERNEL
+	range 0x0 0x01000000
+	default "0x00200000"
+	help
+	  This option allows the amount of space reserved for the table to be
+	  adjusted, although the default of 1Mb should be ok in most cases.
+
+	  The build will fail and a valid size suggested if this is too small.
+
+	  If unsure, leave at the default value.
+
 config X86_PIE
 	bool "Build a PIE kernel"
 	default n
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-53-jiangshanlai::40gmail.com:1arch:x86:Makefile.postlink) --git a/arch/x86/Makefile.postlink b/arch/x86/Makefile.postlink
index fef2e977cc7d..c115692b67b2 100644
--- a/arch/x86/Makefile.postlink
+++ b/arch/x86/Makefile.postlink @@ -4,7 +4,8 @@
 # ===========================================================================
 #
 # 1. Separate relocations from vmlinux into vmlinux.relocs.
-# 2. Strip relocations from vmlinux. +# 2. Insert relocations table into vmlinux
+# 3. Strip relocations from vmlinux.

 PHONY := __archpost
 __archpost:
@@ -20,6 +21,9 @@ quiet_cmd_relocs = RELOCS  $(OUT_RELOCS)/$@.relocs
 	$(CMD_RELOCS) $@ > $(OUT_RELOCS)/$@.relocs;\
 	$(CMD_RELOCS) --abs-relocs $@

+quiet_cmd_insert_relocs = RELOCS  $@
+      cmd_insert_relocs = $(CMD_RELOCS) --keep $@
+
 quiet_cmd_strip_relocs = RSTRIP  $@
       cmd_strip_relocs =\
 	$(OBJCOPY) --remove-section='.rel.*' --remove-section='.rel__*'\
@@ -29,6 +33,9 @@ quiet_cmd_strip_relocs = RSTRIP  $@

 vmlinux: FORCE
 	@true
+ifeq ($(CONFIG_RELOCATABLE_UNCOMPRESSED_KERNEL),y)
+	$(call cmd,insert_relocs)
+endif
 ifeq ($(CONFIG_X86_NEED_RELOCS),y)
 	$(call cmd,relocs)
 	$(call cmd,strip_relocs)
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-53-jiangshanlai::40gmail.com:1arch:x86:kernel:head64_identity.c) --git a/arch/x86/kernel/head64_identity.c b/arch/x86/kernel/head64_identity.c
index ecac6e704868..4548ad615ecf 100644
--- a/arch/x86/kernel/head64_identity.c
+++ b/arch/x86/kernel/head64_identity.c @@ -315,3 +315,73 @@ void __head startup_64_setup_env(void)

 	startup_64_load_idt();
 }
+
+#ifdef CONFIG_RELOCATABLE_UNCOMPRESSED_KERNEL
+extern u8 __relocation_end[];
+
+static bool __head is_in_pvh_pgtable(unsigned long ptr)
+{
+#ifdef CONFIG_PVH
+	if (ptr >= (unsigned long)init_top_pgt &&
+	    ptr < (unsigned long)init_top_pgt + PAGE_SIZE)
+		return true;
+	if (ptr >= (unsigned long)level3_ident_pgt &&
+	    ptr < (unsigned long)level3_ident_pgt + PAGE_SIZE)
+		return true;
+#endif
+	return false;
+}
+
+void __head __relocate_kernel(unsigned long physbase, unsigned long virtbase)
+{
+	int *reloc = (int *)__relocation_end;
+	unsigned long ptr;
+	unsigned long delta = virtbase - __START_KERNEL_map;
+	unsigned long map = physbase - __START_KERNEL;
+	long extended;
+
+	/*
+	 * Relocation had happended in bootloader,
+	 * don't do it again.
+	 */
+	if (SYM_ABS_VA(_text) != __START_KERNEL)
+		return;
+
+	if (!delta)
+		return;
+
+	/*
+	 * Format is:
+	 *
+	 * kernel bits...
+	 * 0 - zero terminator for 64 bit relocations
+	 * 64 bit relocation repeated
+	 * 0 - zero terminator for inverse 32 bit relocations
+	 * 32 bit inverse relocation repeated
+	 * 0 - zero terminator for 32 bit relocations
+	 * 32 bit relocation repeated
+	 *
+	 * So we work backwards from the end of .data.relocs section, see
+	 * handle_relocations() in arch/x86/boot/compressed/misc.c.
+	 */
+	while (*--reloc) {
+		extended = *reloc;
+		ptr = (unsigned long)(extended + map);
+		*(uint32_t *)ptr += delta;
+	}
+
+	while (*--reloc) {
+		extended = *reloc;
+		ptr = (unsigned long)(extended + map);
+		*(int32_t *)ptr -= delta;
+	}
+
+	while (*--reloc) {
+		extended = *reloc;
+		ptr = (unsigned long)(extended + map);
+		if (is_in_pvh_pgtable(ptr))
+			continue;
+		*(uint64_t *)ptr += delta;
+	}
+}
+#endif [diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-53-jiangshanlai::40gmail.com:1arch:x86:kernel:vmlinux.lds.S) --git a/arch/x86/kernel/vmlinux.lds.S b/arch/x86/kernel/vmlinux.lds.S
index 834c68b45f15..3b05807fe1dc 100644
--- a/arch/x86/kernel/vmlinux.lds.S
+++ b/arch/x86/kernel/vmlinux.lds.S @@ -339,6 +339,24 @@ SECTIONS
 	}
 #endif

+#ifdef CONFIG_RELOCATABLE_UNCOMPRESSED_KERNEL
+	. = ALIGN(4);
+	.data.reloc : AT(ADDR(.data.reloc) - LOAD_OFFSET) {
+		__relocation_start = .;
+		/*
+		 * Space for relocation table
+		 * This needs to be filled so that the
+		 * relocs tool can overwrite the content.
+		 * Put a dummy data item at the start to
+		 * avoid to generate NOBITS section.
+		 */
+		LONG(0);
+		FILL(0);
+		. += CONFIG_RELOCATION_TABLE_SIZE - 4;
+		__relocation_end = .;
+	}
+#endif
+
 	/*
 	 * struct alt_inst entries. From the header (alternative.h):
 	 * "Alternative instructions for different CPU types or capabilities"
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#md2b2dd3e1a7220232ba2b4d850560a9397e9dee0) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-53-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-53-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-53-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-53-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-53-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-53-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rd2b2dd3e1a7220232ba2b4d850560a9397e9dee0)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e712fb66a83ce7d3ec7a4fbc83a89c97f40f4414a) **[RFC PATCH 53/73] x86/pvm: Add Kconfig option and the CPU feature bit for PVM guest**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(51 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rd2b2dd3e1a7220232ba2b4d850560a9397e9dee0)
  2024-02-26 14:36 ` [[RFC PATCH 52/73] x86/boot: Allow to do relocation for uncompressed kernel](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#md2b2dd3e1a7220232ba2b4d850560a9397e9dee0) Lai Jiangshan
**@ 2024-02-26 14:36 ` Lai Jiangshan**
  2024-02-26 14:36 ` [[RFC PATCH 54/73] x86/pvm: Detect PVM hypervisor support](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m13af3b16f8dd4dd49ea9779776dc0aad02e2f5cc) Lai Jiangshan
                   ` [(21 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r13af3b16f8dd4dd49ea9779776dc0aad02e2f5cc)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r712fb66a83ce7d3ec7a4fbc83a89c97f40f4414a)
From: Lai Jiangshan @ 2024-02-26 14:36 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-54-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-54-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143744)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143744), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin, Mike Rapoport (IBM), Daniel Sneddon,
	Rick Edgecombe, Alexey Kardashevskiy, Yu-cheng Yu,
	Kirill A. Shutemov

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

Add the configuration option CONFIG_PVM_GUEST to enable the building of
a PVM guest. Introduce a new CPU feature bit to control the behavior of
the PVM guest.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/Kconfig](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-54-jiangshanlai::40gmail.com:1arch:x86:Kconfig)                         | 8 ++++++++
 [arch/x86/include/asm/cpufeatures.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-54-jiangshanlai::40gmail.com:1arch:x86:include:asm:cpufeatures.h)       | 1 +
 [arch/x86/include/asm/disabled-features.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-54-jiangshanlai::40gmail.com:1arch:x86:include:asm:disabled-features.h) | 8 +++++++-
 3 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e712fb66a83ce7d3ec7a4fbc83a89c97f40f4414a), 16 insertions(+), 1 deletion(-)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-54-jiangshanlai::40gmail.com:1arch:x86:Kconfig) --git a/arch/x86/Kconfig b/arch/x86/Kconfig
index d02ef3bdb171..2ccc8a27e081 100644
--- a/arch/x86/Kconfig
+++ b/arch/x86/Kconfig @@ -851,6 +851,14 @@ config KVM_GUEST
 	  underlying device model, the host provides the guest with
 	  timing infrastructure such as time of day, and system time

+config PVM_GUEST
+	bool "PVM Guest support"
+	depends on X86_64 && KVM_GUEST
+	default n
+	help
+	  This option enables the kernel to run as a PVM guest under the PVM
+	  hypervisor.
+
 config ARCH_CPUIDLE_HALTPOLL
 	def_bool n
 	prompt "Disable host haltpoll when loading haltpoll driver"
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-54-jiangshanlai::40gmail.com:1arch:x86:include:asm:cpufeatures.h) --git a/arch/x86/include/asm/cpufeatures.h b/arch/x86/include/asm/cpufeatures.h
index 4af140cf5719..e17e72f13423 100644
--- a/arch/x86/include/asm/cpufeatures.h
+++ b/arch/x86/include/asm/cpufeatures.h @@ -237,6 +237,7 @@
 #define X86_FEATURE_PVUNLOCK		( 8*32+20) /* "" PV unlock function */
 #define X86_FEATURE_VCPUPREEMPT		( 8*32+21) /* "" PV vcpu_is_preempted function */
 #define X86_FEATURE_TDX_GUEST		( 8*32+22) /* Intel Trust Domain Extensions Guest */
+#define X86_FEATURE_KVM_PVM_GUEST	( 8*32+23) /* KVM Pagetable-based Virtual Machine guest */

 /* Intel-defined CPU features, CPUID level 0x00000007:0 (EBX), word 9 */
 #define X86_FEATURE_FSGSBASE		( 9*32+ 0) /* RDFSBASE, WRFSBASE, RDGSBASE, WRGSBASE instructions*/
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-54-jiangshanlai::40gmail.com:1arch:x86:include:asm:disabled-features.h) --git a/arch/x86/include/asm/disabled-features.h b/arch/x86/include/asm/disabled-features.h
index 702d93fdd10e..5d56e804ab18 100644
--- a/arch/x86/include/asm/disabled-features.h
+++ b/arch/x86/include/asm/disabled-features.h @@ -105,6 +105,12 @@
 # define DISABLE_TDX_GUEST	(1 << (X86_FEATURE_TDX_GUEST & 31))
 #endif

+#ifdef CONFIG_PVM_GUEST
+#define DISABLE_KVM_PVM_GUEST	0
+#else
+#define DISABLE_KVM_PVM_GUEST	(1 << (X86_FEATURE_KVM_PVM_GUEST & 31))
+#endif
+
 #ifdef CONFIG_X86_USER_SHADOW_STACK
 #define DISABLE_USER_SHSTK	0
 #else
@@ -128,7 +134,7 @@
 #define DISABLED_MASK5	0
 #define DISABLED_MASK6	0
 #define DISABLED_MASK7	(DISABLE_PTI)
-#define DISABLED_MASK8	(DISABLE_XENPV|DISABLE_TDX_GUEST) +#define DISABLED_MASK8	(DISABLE_XENPV|DISABLE_TDX_GUEST|DISABLE_KVM_PVM_GUEST)
 #define DISABLED_MASK9	(DISABLE_SGX)
 #define DISABLED_MASK10	0
 #define DISABLED_MASK11	(DISABLE_RETPOLINE|DISABLE_RETHUNK|DISABLE_UNRET|\
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m712fb66a83ce7d3ec7a4fbc83a89c97f40f4414a) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-54-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-54-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-54-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-54-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-54-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-54-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r712fb66a83ce7d3ec7a4fbc83a89c97f40f4414a)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e13af3b16f8dd4dd49ea9779776dc0aad02e2f5cc) **[RFC PATCH 54/73] x86/pvm: Detect PVM hypervisor support**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(52 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r712fb66a83ce7d3ec7a4fbc83a89c97f40f4414a)
  2024-02-26 14:36 ` [[RFC PATCH 53/73] x86/pvm: Add Kconfig option and the CPU feature bit for PVM guest](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m712fb66a83ce7d3ec7a4fbc83a89c97f40f4414a) Lai Jiangshan
**@ 2024-02-26 14:36 ` Lai Jiangshan**
  2024-02-26 14:36 ` [[RFC PATCH 55/73] x86/pvm: Relocate kernel image to specific virtual address range](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m859dc440ec3063be3e5a6fcaddeae2901ef1c7fb) Lai Jiangshan
                   ` [(20 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r859dc440ec3063be3e5a6fcaddeae2901ef1c7fb)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r13af3b16f8dd4dd49ea9779776dc0aad02e2f5cc)
From: Lai Jiangshan @ 2024-02-26 14:36 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-55-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-55-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143754)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143754), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin, Mike Rapoport (IBM), Rick Edgecombe,
	Pengfei Xu, Ze Gao, Josh Poimboeuf

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

Detect PVM hypervisor support through the use of the PVM synthetic
instruction 'PVM_SYNTHETIC_CPUID'. This is a necessary step in preparing
to initialize the PVM guest during booting.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/include/asm/pvm_para.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-55-jiangshanlai::40gmail.com:1arch:x86:include:asm:pvm_para.h) | 69 +++++++++++++++++++++++++++++++++
 [arch/x86/kernel/Makefile](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-55-jiangshanlai::40gmail.com:1arch:x86:kernel:Makefile)        |  1 +
 [arch/x86/kernel/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-55-jiangshanlai::40gmail.com:1arch:x86:kernel:pvm.c)           | 22 +++++++++++
 3 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e13af3b16f8dd4dd49ea9779776dc0aad02e2f5cc), 92 insertions(+)
 create mode 100644 arch/x86/include/asm/pvm_para.h
 create mode 100644 arch/x86/kernel/pvm.c

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-55-jiangshanlai::40gmail.com:1arch:x86:include:asm:pvm_para.h) --git a/arch/x86/include/asm/pvm_para.h b/arch/x86/include/asm/pvm_para.h
new file mode 100644
index 000000000000..efd7afdf9be9
--- /dev/null
+++ b/arch/x86/include/asm/pvm_para.h @@ -0,0 +1,69 @@ +/* SPDX-License-Identifier: GPL-2.0 */
+#ifndef _ASM_X86_PVM_PARA_H
+#define _ASM_X86_PVM_PARA_H
+
+#include <linux/init.h>
+#include <uapi/asm/pvm_para.h>
+
+#ifdef CONFIG_PVM_GUEST
+#include <asm/irqflags.h>
+#include <uapi/asm/kvm_para.h>
+
+void __init pvm_early_setup(void);
+
+static inline void pvm_cpuid(unsigned int *eax, unsigned int *ebx,
+			     unsigned int *ecx, unsigned int *edx)
+{
+	asm(__ASM_FORM(.byte PVM_SYNTHETIC_CPUID ;)
+		: "=a" (*eax),
+		  "=b" (*ebx),
+		  "=c" (*ecx),
+		  "=d" (*edx)
+		: "0" (*eax), "2" (*ecx));
+}
+
+/*
+ * pvm_detect() is called before event handling is set up and it might be
+ * possibly called under any hypervisor other than PVM, so it should not
+ * trigger any trap in all possible scenarios. PVM_SYNTHETIC_CPUID is supposed
+ * to not trigger any trap in the real or virtual x86 kernel mode and is also
+ * guaranteed to trigger a trap in the underlying hardware user mode for the
+ * hypervisor emulating it.
+ */
+static inline bool pvm_detect(void)
+{
+	unsigned long cs;
+	uint32_t eax, signature[3];
+
+	/* check underlying interrupt flags */
+	if (arch_irqs_disabled_flags(native_save_fl()))
+		return false;
+
+	/* check underlying CS */
+	asm volatile("mov %%cs,%0\n\t" : "=r" (cs) : );
+	if ((cs & 3) != 3)
+		return false;
+
+	/* check KVM_SIGNATURE and KVM_CPUID_VENDOR_FEATURES */
+	eax = KVM_CPUID_SIGNATURE;
+	pvm_cpuid(&eax, &signature[0], &signature[1], &signature[2]);
+	if (memcmp(KVM_SIGNATURE, signature, 12))
+		return false;
+	if (eax < KVM_CPUID_VENDOR_FEATURES)
+		return false;
+
+	/* check PVM_CPUID_SIGNATURE */
+	eax = KVM_CPUID_VENDOR_FEATURES;
+	pvm_cpuid(&eax, &signature[0], &signature[1], &signature[2]);
+	if (signature[0] != PVM_CPUID_SIGNATURE)
+		return false;
+
+	return true;
+}
+#else
+static inline void pvm_early_setup(void)
+{
+}
+#endif /* CONFIG_PVM_GUEST */
+
+#endif /* _ASM_X86_PVM_PARA_H */ [diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-55-jiangshanlai::40gmail.com:1arch:x86:kernel:Makefile) --git a/arch/x86/kernel/Makefile b/arch/x86/kernel/Makefile
index dc1f5a303e9b..67f11f7d5c88 100644
--- a/arch/x86/kernel/Makefile
+++ b/arch/x86/kernel/Makefile @@ -129,6 +129,7 @@ obj-$(CONFIG_AMD_NB)		+= amd_nb.o
 obj-$(CONFIG_DEBUG_NMI_SELFTEST) += nmi_selftest.o

 obj-$(CONFIG_KVM_GUEST)		+= kvm.o kvmclock.o
+obj-$(CONFIG_PVM_GUEST)		+= pvm.o
 obj-$(CONFIG_PARAVIRT)		+= paravirt.o
 obj-$(CONFIG_PARAVIRT_SPINLOCKS)+= paravirt-spinlocks.o
 obj-$(CONFIG_PARAVIRT_CLOCK)	+= pvclock.o
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-55-jiangshanlai::40gmail.com:1arch:x86:kernel:pvm.c) --git a/arch/x86/kernel/pvm.c b/arch/x86/kernel/pvm.c
new file mode 100644
index 000000000000..2d27044eaf25
--- /dev/null
+++ b/arch/x86/kernel/pvm.c @@ -0,0 +1,22 @@ +// SPDX-License-Identifier: GPL-2.0-or-later
+/*
+ * KVM PVM paravirt_ops implementation
+ *
+ * Copyright (C) 2020 Ant Group
+ *
+ * This work is licensed under the terms of the GNU GPL, version 2.  See
+ * the COPYING file in the top-level directory.
+ *
+ */
+#define pr_fmt(fmt) "pvm-guest: " fmt
+
+#include <asm/cpufeature.h>
+#include <asm/pvm_para.h>
+
+void __init pvm_early_setup(void)
+{
+	if (!pvm_detect())
+		return;
+
+	setup_force_cpu_cap(X86_FEATURE_KVM_PVM_GUEST);
+} --
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m13af3b16f8dd4dd49ea9779776dc0aad02e2f5cc) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-55-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-55-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-55-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-55-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-55-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-55-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r13af3b16f8dd4dd49ea9779776dc0aad02e2f5cc)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e859dc440ec3063be3e5a6fcaddeae2901ef1c7fb) **[RFC PATCH 55/73] x86/pvm: Relocate kernel image to specific virtual address range**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(53 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r13af3b16f8dd4dd49ea9779776dc0aad02e2f5cc)
  2024-02-26 14:36 ` [[RFC PATCH 54/73] x86/pvm: Detect PVM hypervisor support](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m13af3b16f8dd4dd49ea9779776dc0aad02e2f5cc) Lai Jiangshan
**@ 2024-02-26 14:36 ` Lai Jiangshan**
  2024-02-26 14:36 ` [[RFC PATCH 56/73] x86/pvm: Relocate kernel image early in PVH entry](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#me32699725a7f6bb2708fbf48948be563da10ee2d) Lai Jiangshan
                   ` [(19 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#re32699725a7f6bb2708fbf48948be563da10ee2d)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r859dc440ec3063be3e5a6fcaddeae2901ef1c7fb)
From: Lai Jiangshan @ 2024-02-26 14:36 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-56-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-56-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143803)
  Cc: Hou Wenlong, Lai Jiangshan, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143803), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin, David Woodhouse, Brian Gerst,
	Josh Poimboeuf, Thomas Garnier, Ard Biesheuvel, Tom Lendacky

From: Hou Wenlong <houwenlong.hwl@antgroup.com>

For a PVM guest, it is only allowed to run in the specific virtual
address range provided by the hypervisor. Therefore, the PVM guest needs
to be a PIE kernel and perform relocation during the booting process.
Additionally, for a compressed kernel image, kaslr needs to be disabled;
otherwise, it will fail to boot.

Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
---
 [arch/x86/Kconfig](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-56-jiangshanlai::40gmail.com:1arch:x86:Kconfig)                  |  3 ++-
 [arch/x86/kernel/head64_identity.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-56-jiangshanlai::40gmail.com:1arch:x86:kernel:head64_identity.c) | 27 +++++++++++++++++++++++++++
 [arch/x86/kernel/head_64.S](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-56-jiangshanlai::40gmail.com:1arch:x86:kernel:head_64.S)         | 13 +++++++++++++
 [arch/x86/kernel/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-56-jiangshanlai::40gmail.com:1arch:x86:kernel:pvm.c)             |  5 ++++-
 4 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e859dc440ec3063be3e5a6fcaddeae2901ef1c7fb), 46 insertions(+), 2 deletions(-)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-56-jiangshanlai::40gmail.com:1arch:x86:Kconfig) --git a/arch/x86/Kconfig b/arch/x86/Kconfig
index 2ccc8a27e081..1b4bea3db53d 100644
--- a/arch/x86/Kconfig
+++ b/arch/x86/Kconfig @@ -853,7 +853,8 @@ config KVM_GUEST

 config PVM_GUEST
 	bool "PVM Guest support"
-	depends on X86_64 && KVM_GUEST +	depends on X86_64 && KVM_GUEST && X86_PIE
+	select RELOCATABLE_UNCOMPRESSED_KERNEL
 	default n
 	help
 	  This option enables the kernel to run as a PVM guest under the PVM
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-56-jiangshanlai::40gmail.com:1arch:x86:kernel:head64_identity.c) --git a/arch/x86/kernel/head64_identity.c b/arch/x86/kernel/head64_identity.c
index 4548ad615ecf..4e6a073d9e6c 100644
--- a/arch/x86/kernel/head64_identity.c
+++ b/arch/x86/kernel/head64_identity.c @@ -20,6 +20,7 @@
 #include <asm/trapnr.h>
 #include <asm/sev.h>
 #include <asm/init.h>
+#include <asm/pvm_para.h>

 extern pmd_t early_dynamic_pgts[EARLY_DYNAMIC_PAGE_TABLES][PTRS_PER_PMD];
 extern unsigned int next_early_pgt;
@@ -385,3 +386,29 @@ void __head __relocate_kernel(unsigned long physbase, unsigned long virtbase)
 	}
 }
 #endif
+
+#ifdef CONFIG_PVM_GUEST
+extern unsigned long pvm_range_start;
+extern unsigned long pvm_range_end;
+
+static void __head detect_pvm_range(void)
+{
+	unsigned long msr_val;
+	unsigned long pml4_index_start, pml4_index_end;
+
+	msr_val = __rdmsr(MSR_PVM_LINEAR_ADDRESS_RANGE);
+	pml4_index_start = msr_val & 0x1ff;
+	pml4_index_end = (msr_val >> 16) & 0x1ff;
+	pvm_range_start = (0x1fffe00 | pml4_index_start) * P4D_SIZE;
+	pvm_range_end = (0x1fffe00 | pml4_index_end) * P4D_SIZE;
+}
+
+void __head pvm_relocate_kernel(unsigned long physbase)
+{
+	if (!pvm_detect())
+		return;
+
+	detect_pvm_range();
+	__relocate_kernel(physbase, pvm_range_end - (2UL << 30));
+}
+#endif [diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-56-jiangshanlai::40gmail.com:1arch:x86:kernel:head_64.S) --git a/arch/x86/kernel/head_64.S b/arch/x86/kernel/head_64.S
index b8278f05bbd0..1d931bab4393 100644
--- a/arch/x86/kernel/head_64.S
+++ b/arch/x86/kernel/head_64.S @@ -91,6 +91,19 @@ SYM_CODE_START_NOALIGN(startup_64)
 	movq	%rdx, PER_CPU_VAR(this_cpu_off)
 #endif

+#ifdef CONFIG_PVM_GUEST
+	leaq	_text(%rip), %rdi
+	call	pvm_relocate_kernel
+#ifdef CONFIG_SMP
+	/* Fill __per_cpu_offset[0] again, because it got relocated. */
+	movabs	$__per_cpu_load, %rdx
+	movabs	$__per_cpu_start, %rax
+	subq	%rax, %rdx
+	movq	%rdx, __per_cpu_offset(%rip)
+	movq	%rdx, PER_CPU_VAR(this_cpu_off)
+#endif
+#endif
+
 	call	startup_64_setup_env

 	/* Now switch to __KERNEL_CS so IRET works reliably */
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-56-jiangshanlai::40gmail.com:1arch:x86:kernel:pvm.c) --git a/arch/x86/kernel/pvm.c b/arch/x86/kernel/pvm.c
index 2d27044eaf25..fc82c71b305b 100644
--- a/arch/x86/kernel/pvm.c
+++ b/arch/x86/kernel/pvm.c @@ -13,9 +13,12 @@
 #include <asm/cpufeature.h>
 #include <asm/pvm_para.h>

+unsigned long pvm_range_start __initdata;
+unsigned long pvm_range_end __initdata;
+
 void __init pvm_early_setup(void)
 {
-	if (!pvm_detect()) +	if (!pvm_range_end)
 		return;

 	setup_force_cpu_cap(X86_FEATURE_KVM_PVM_GUEST);
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m859dc440ec3063be3e5a6fcaddeae2901ef1c7fb) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-56-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-56-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-56-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-56-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-56-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-56-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r859dc440ec3063be3e5a6fcaddeae2901ef1c7fb)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ee32699725a7f6bb2708fbf48948be563da10ee2d) **[RFC PATCH 56/73] x86/pvm: Relocate kernel image early in PVH entry**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(54 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r859dc440ec3063be3e5a6fcaddeae2901ef1c7fb)
  2024-02-26 14:36 ` [[RFC PATCH 55/73] x86/pvm: Relocate kernel image to specific virtual address range](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m859dc440ec3063be3e5a6fcaddeae2901ef1c7fb) Lai Jiangshan
**@ 2024-02-26 14:36 ` Lai Jiangshan**
  2024-02-26 14:36 ` [[RFC PATCH 57/73] x86/pvm: Make cpu entry area and vmalloc area variable](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m907f649c408bd41d2b5f2f0e2b930380f0d9db5e) Lai Jiangshan
                   ` [(18 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r907f649c408bd41d2b5f2f0e2b930380f0d9db5e)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#re32699725a7f6bb2708fbf48948be563da10ee2d)
From: Lai Jiangshan @ 2024-02-26 14:36 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-57-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-57-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143812)
  Cc: Hou Wenlong, Lai Jiangshan, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143812), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin, Boris Ostrovsky, Darren Hart,
	Andy Shevchenko, xen-devel, [platform-driver-x86](https://lore.kernel.org/platform-driver-x86/?t=20240226143812)

From: Hou Wenlong <houwenlong.hwl@antgroup.com>

For a PIE kernel, it runs in a high virtual address in the PVH entry, so
it needs to relocate the kernel image early in the PVH entry for the PVM
guest.

Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
---
 [arch/x86/include/asm/init.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-57-jiangshanlai::40gmail.com:1arch:x86:include:asm:init.h)       |  5 +++++
 [arch/x86/kernel/head64_identity.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-57-jiangshanlai::40gmail.com:1arch:x86:kernel:head64_identity.c) |  5 -----
 [arch/x86/platform/pvh/enlighten.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-57-jiangshanlai::40gmail.com:1arch:x86:platform:pvh:enlighten.c) | 22 ++++++++++++++++++++++
 [arch/x86/platform/pvh/head.S](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-57-jiangshanlai::40gmail.com:1arch:x86:platform:pvh:head.S)      |  4 ++++
 4 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ee32699725a7f6bb2708fbf48948be563da10ee2d), 31 insertions(+), 5 deletions(-)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-57-jiangshanlai::40gmail.com:1arch:x86:include:asm:init.h) --git a/arch/x86/include/asm/init.h b/arch/x86/include/asm/init.h
index cc9ccf61b6bd..f78edef60253 100644
--- a/arch/x86/include/asm/init.h
+++ b/arch/x86/include/asm/init.h @@ -4,6 +4,11 @@

 #define __head	__section(".head.text")

+#define SYM_ABS_VA(sym) ({\
+	unsigned long __v;\
+	asm("movabsq $" __stringify(sym) ", %0":"=r"(__v));\
+	__v; })
+
 struct x86_mapping_info {
 	void *(*alloc_pgt_page)(void *); /* allocate buf for page table */
 	void *context;			 /* context for alloc_pgt_page */
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-57-jiangshanlai::40gmail.com:1arch:x86:kernel:head64_identity.c) --git a/arch/x86/kernel/head64_identity.c b/arch/x86/kernel/head64_identity.c
index 4e6a073d9e6c..f69f9904003c 100644
--- a/arch/x86/kernel/head64_identity.c
+++ b/arch/x86/kernel/head64_identity.c @@ -82,11 +82,6 @@ static void __head set_kernel_map_base(unsigned long text_base)
 }
 #endif

-#define SYM_ABS_VA(sym) ({\
-	unsigned long __v;\
-	asm("movabsq $" __stringify(sym) ", %0":"=r"(__v));\
-	__v; })
 static unsigned long __head sme_postprocess_startup(struct boot_params *bp, pmdval_t *pmd)
 {
 	unsigned long vaddr, vaddr_end;
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-57-jiangshanlai::40gmail.com:1arch:x86:platform:pvh:enlighten.c) --git a/arch/x86/platform/pvh/enlighten.c b/arch/x86/platform/pvh/enlighten.c
index 00a92cb2c814..8c64c31c971b 100644
--- a/arch/x86/platform/pvh/enlighten.c
+++ b/arch/x86/platform/pvh/enlighten.c @@ -1,8 +1,10 @@
 // SPDX-License-Identifier: GPL-2.0
 #include <linux/acpi.h>
+#include <linux/pgtable.h>

 #include <xen/hvc-console.h>

+#include <asm/init.h>
 #include <asm/io_apic.h>
 #include <asm/hypervisor.h>
 #include <asm/e820/api.h>
@@ -113,6 +115,26 @@ static void __init hypervisor_specific_init(bool xen_guest)
 		xen_pvh_init(&pvh_bootparams);
 }

+#ifdef CONFIG_PVM_GUEST
+void pvm_relocate_kernel(unsigned long physbase);
+
+void __init pvm_update_pgtable(unsigned long physbase)
+{
+	pgdval_t *pgd;
+	pudval_t *pud;
+	unsigned long base;
+
+	pvm_relocate_kernel(physbase);
+
+	pgd = (pgdval_t *)init_top_pgt;
+	base = SYM_ABS_VA(_text);
+	pgd[pgd_index(base)] = pgd[0];
+	pgd[pgd_index(page_offset_base)] = pgd[0];
+	pud = (pudval_t *)level3_ident_pgt;
+	pud[pud_index(base)] = (unsigned long)level2_ident_pgt + _KERNPG_TABLE_NOENC;
+}
+#endif
+
 /*
  * This routine (and those that it might call) should not use
  * anything that lives in .bss since that segment will be cleared later.
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-57-jiangshanlai::40gmail.com:1arch:x86:platform:pvh:head.S) --git a/arch/x86/platform/pvh/head.S b/arch/x86/platform/pvh/head.S
index baaa3fe34a00..127f297f7257 100644
--- a/arch/x86/platform/pvh/head.S
+++ b/arch/x86/platform/pvh/head.S @@ -109,6 +109,10 @@ SYM_CODE_START_LOCAL(pvh_start_xen)
 	wrmsr

 #ifdef CONFIG_X86_PIE
+#ifdef CONFIG_PVM_GUEST
+	leaq	_text(%rip), %rdi
+	call	pvm_update_pgtable
+#endif
 	movabs  $2f, %rax
 	ANNOTATE_RETPOLINE_SAFE
 	jmp *%rax
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#me32699725a7f6bb2708fbf48948be563da10ee2d) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-57-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-57-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-57-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-57-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-57-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-57-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#re32699725a7f6bb2708fbf48948be563da10ee2d)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e907f649c408bd41d2b5f2f0e2b930380f0d9db5e) **[RFC PATCH 57/73] x86/pvm: Make cpu entry area and vmalloc area variable**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(55 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#re32699725a7f6bb2708fbf48948be563da10ee2d)
  2024-02-26 14:36 ` [[RFC PATCH 56/73] x86/pvm: Relocate kernel image early in PVH entry](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#me32699725a7f6bb2708fbf48948be563da10ee2d) Lai Jiangshan
**@ 2024-02-26 14:36 ` Lai Jiangshan**
  2024-02-26 14:36 ` [[RFC PATCH 58/73] x86/pvm: Relocate kernel address space layout](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mf4f5b56a298f562c94b24e02bab22969eb6c7d39) Lai Jiangshan
                   ` [(17 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rf4f5b56a298f562c94b24e02bab22969eb6c7d39)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r907f649c408bd41d2b5f2f0e2b930380f0d9db5e)
From: Lai Jiangshan @ 2024-02-26 14:36 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-58-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-58-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143818)
  Cc: Hou Wenlong, Lai Jiangshan, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143818), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin, Andy Lutomirski, Jonathan Corbet,
	Josh Poimboeuf, Yuntao Wang, Wang Jinchao

From: Hou Wenlong <houwenlong.hwl@antgroup.com>

For the PVM guest, the entire kernel layout should be within the allowed
virtual address range. Therefore, establish CPU_ENTRY_AREA_BASE and
VMEMORY_END as a variable for the PVM guest, allowing it to be
modified as necessary for the PVM guest.

Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
---
 [arch/x86/include/asm/page_64.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-58-jiangshanlai::40gmail.com:1arch:x86:include:asm:page_64.h)          |  3 +++
 [arch/x86/include/asm/pgtable_64_types.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-58-jiangshanlai::40gmail.com:1arch:x86:include:asm:pgtable_64_types.h) | 14 ++++++++++++--
 [arch/x86/kernel/head64.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-58-jiangshanlai::40gmail.com:1arch:x86:kernel:head64.c)                |  7 +++++++
 [arch/x86/mm/dump_pagetables.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-58-jiangshanlai::40gmail.com:1arch:x86:mm:dump_pagetables.c)           |  3 ++-
 [arch/x86/mm/kaslr.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-58-jiangshanlai::40gmail.com:1arch:x86:mm:kaslr.c)                     |  4 ++--
 5 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e907f649c408bd41d2b5f2f0e2b930380f0d9db5e), 26 insertions(+), 5 deletions(-)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-58-jiangshanlai::40gmail.com:1arch:x86:include:asm:page_64.h) --git a/arch/x86/include/asm/page_64.h b/arch/x86/include/asm/page_64.h
index b8692e6cc939..4f64f049f3d0 100644
--- a/arch/x86/include/asm/page_64.h
+++ b/arch/x86/include/asm/page_64.h @@ -18,6 +18,9 @@ extern unsigned long page_offset_base;
 extern unsigned long vmalloc_base;
 extern unsigned long vmemmap_base;

+extern unsigned long cpu_entry_area_base;
+extern unsigned long vmemory_end;
+
 static __always_inline unsigned long __phys_addr_nodebug(unsigned long x)
 {
 	unsigned long y = x - KERNEL_MAP_BASE;
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-58-jiangshanlai::40gmail.com:1arch:x86:include:asm:pgtable_64_types.h) --git a/arch/x86/include/asm/pgtable_64_types.h b/arch/x86/include/asm/pgtable_64_types.h
index 6780f2e63717..66c8e7325d27 100644
--- a/arch/x86/include/asm/pgtable_64_types.h
+++ b/arch/x86/include/asm/pgtable_64_types.h @@ -140,6 +140,7 @@ extern unsigned int ptrs_per_p4d;
 # define VMEMMAP_START		__VMEMMAP_BASE_L4
 #endif /* CONFIG_DYNAMIC_MEMORY_LAYOUT */

+#ifndef CONFIG_PVM_GUEST
 /*
  * End of the region for which vmalloc page tables are pre-allocated.
  * For non-KMSAN builds, this is the same as VMALLOC_END.
@@ -147,6 +148,10 @@ extern unsigned int ptrs_per_p4d;
  * VMALLOC_START..VMALLOC_END (see below).
  */
 #define VMEMORY_END		(VMALLOC_START + (VMALLOC_SIZE_TB << 40) - 1)
+#else
+#define RAW_VMEMORY_END		(__VMALLOC_BASE_L4 + (VMALLOC_SIZE_TB_L4 << 40) - 1)
+#define VMEMORY_END		vmemory_end
+#endif /* CONFIG_PVM_GUEST */

 #ifndef CONFIG_KMSAN
 #define VMALLOC_END		VMEMORY_END
@@ -166,7 +171,7 @@ extern unsigned int ptrs_per_p4d;
  *              KMSAN_MODULES_ORIGIN_START to
  *              KMSAN_MODULES_ORIGIN_START + MODULES_LEN - origins for modules.
  */
-#define VMALLOC_QUARTER_SIZE	((VMALLOC_SIZE_TB << 40) >> 2) +#define VMALLOC_QUARTER_SIZE	((VMEMORY_END + 1 - VMALLOC_START) >> 2)
 #define VMALLOC_END		(VMALLOC_START + VMALLOC_QUARTER_SIZE - 1)

 /*
@@ -202,7 +207,12 @@ extern unsigned int ptrs_per_p4d;
 #define ESPFIX_BASE_ADDR	(ESPFIX_PGD_ENTRY << P4D_SHIFT)

 #define CPU_ENTRY_AREA_PGD	_AC(-4, UL)
-#define CPU_ENTRY_AREA_BASE	(CPU_ENTRY_AREA_PGD << P4D_SHIFT) +#define RAW_CPU_ENTRY_AREA_BASE	(CPU_ENTRY_AREA_PGD << P4D_SHIFT)
+#ifdef CONFIG_PVM_GUEST
+#define CPU_ENTRY_AREA_BASE	cpu_entry_area_base
+#else
+#define CPU_ENTRY_AREA_BASE	RAW_CPU_ENTRY_AREA_BASE
+#endif

 #define EFI_VA_START		( -4 * (_AC(1, UL) << 30))
 #define EFI_VA_END		(-68 * (_AC(1, UL) << 30))
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-58-jiangshanlai::40gmail.com:1arch:x86:kernel:head64.c) --git a/arch/x86/kernel/head64.c b/arch/x86/kernel/head64.c
index 0b0e460609e5..d0e8d648bd38 100644
--- a/arch/x86/kernel/head64.c
+++ b/arch/x86/kernel/head64.c @@ -72,6 +72,13 @@ unsigned long kernel_map_base __ro_after_init = __START_KERNEL_map;
 EXPORT_SYMBOL(kernel_map_base);
 #endif

+#ifdef CONFIG_PVM_GUEST
+unsigned long cpu_entry_area_base __ro_after_init = RAW_CPU_ENTRY_AREA_BASE;
+EXPORT_SYMBOL(cpu_entry_area_base);
+unsigned long vmemory_end __ro_after_init = RAW_VMEMORY_END;
+EXPORT_SYMBOL(vmemory_end);
+#endif
+
 /* Wipe all early page tables except for the kernel symbol map */
 static void __init reset_early_page_tables(void)
 {
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-58-jiangshanlai::40gmail.com:1arch:x86:mm:dump_pagetables.c) --git a/arch/x86/mm/dump_pagetables.c b/arch/x86/mm/dump_pagetables.c
index d5c6f61242aa..166c7d36d8ff 100644
--- a/arch/x86/mm/dump_pagetables.c
+++ b/arch/x86/mm/dump_pagetables.c @@ -95,7 +95,7 @@ static struct addr_marker address_markers[] = {
 #ifdef CONFIG_MODIFY_LDT_SYSCALL
 	[LDT_NR]		= { 0UL,		"LDT remap" },
 #endif
-	[CPU_ENTRY_AREA_NR]	= { CPU_ENTRY_AREA_BASE,"CPU entry Area" }, +	[CPU_ENTRY_AREA_NR]	= { 0UL,		"CPU entry Area" },
 #ifdef CONFIG_X86_ESPFIX64
 	[ESPFIX_START_NR]	= { ESPFIX_BASE_ADDR,	"ESPfix Area", 16 },
 #endif
@@ -479,6 +479,7 @@ static int __init pt_dump_init(void)
 	address_markers[MODULES_VADDR_NR].start_address = MODULES_VADDR;
 	address_markers[MODULES_END_NR].start_address = MODULES_END;
 	address_markers[FIXADDR_START_NR].start_address = FIXADDR_START;
+	address_markers[CPU_ENTRY_AREA_NR].start_address = CPU_ENTRY_AREA_BASE;
 #endif
 #ifdef CONFIG_X86_32
 	address_markers[VMALLOC_START_NR].start_address = VMALLOC_START;
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-58-jiangshanlai::40gmail.com:1arch:x86:mm:kaslr.c) --git a/arch/x86/mm/kaslr.c b/arch/x86/mm/kaslr.c
index 37db264866b6..e3825c7542a3 100644
--- a/arch/x86/mm/kaslr.c
+++ b/arch/x86/mm/kaslr.c @@ -38,7 +38,7 @@
  * highest amount of space for randomization available, but that's too hard
  * to keep straight and caused issues already.
  */
-static const unsigned long vaddr_end = CPU_ENTRY_AREA_BASE; +static const unsigned long vaddr_end = RAW_CPU_ENTRY_AREA_BASE;

 /*
  * Memory regions randomized by KASLR (except modules that use a separate logic
@@ -79,7 +79,7 @@ void __init kernel_randomize_memory(void)
 	 * limited....
 	 */
 	BUILD_BUG_ON(vaddr_start >= vaddr_end);
-	BUILD_BUG_ON(vaddr_end != CPU_ENTRY_AREA_BASE); +	BUILD_BUG_ON(vaddr_end != RAW_CPU_ENTRY_AREA_BASE);
 	BUILD_BUG_ON(vaddr_end > __START_KERNEL_map);

 	if (!kaslr_memory_enabled())
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m907f649c408bd41d2b5f2f0e2b930380f0d9db5e) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-58-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-58-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-58-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-58-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-58-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-58-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r907f649c408bd41d2b5f2f0e2b930380f0d9db5e)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ef4f5b56a298f562c94b24e02bab22969eb6c7d39) **[RFC PATCH 58/73] x86/pvm: Relocate kernel address space layout**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(56 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r907f649c408bd41d2b5f2f0e2b930380f0d9db5e)
  2024-02-26 14:36 ` [[RFC PATCH 57/73] x86/pvm: Make cpu entry area and vmalloc area variable](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m907f649c408bd41d2b5f2f0e2b930380f0d9db5e) Lai Jiangshan
**@ 2024-02-26 14:36 ` Lai Jiangshan**
  2024-02-26 14:36 ` [[RFC PATCH 59/73] x86/pti: Force enabling KPTI for PVM guest](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m8196806734b024e84079f3acb39d041bf3665bd2) Lai Jiangshan
                   ` [(16 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r8196806734b024e84079f3acb39d041bf3665bd2)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rf4f5b56a298f562c94b24e02bab22969eb6c7d39)
From: Lai Jiangshan @ 2024-02-26 14:36 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-59-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-59-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143830)
  Cc: Hou Wenlong, Lai Jiangshan, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143830), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin, Andy Lutomirski

From: Hou Wenlong <houwenlong.hwl@antgroup.com>

Relocate the kernel address space layout to a specific range, which is
similar to KASLR. Since there is not enough room for KASAN, KASAN is not
supported for PVM guest.

Suggested-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
---
 [arch/x86/Kconfig](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-59-jiangshanlai::40gmail.com:1arch:x86:Kconfig)                  |  3 +-
 [arch/x86/include/asm/pvm_para.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-59-jiangshanlai::40gmail.com:1arch:x86:include:asm:pvm_para.h)   |  6 +++
 [arch/x86/kernel/head64_identity.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-59-jiangshanlai::40gmail.com:1arch:x86:kernel:head64_identity.c) |  6 +++
 [arch/x86/kernel/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-59-jiangshanlai::40gmail.com:1arch:x86:kernel:pvm.c)             | 64 +++++++++++++++++++++++++++++++
 [arch/x86/mm/kaslr.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-59-jiangshanlai::40gmail.com:1arch:x86:mm:kaslr.c)               |  4 ++
 5 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ef4f5b56a298f562c94b24e02bab22969eb6c7d39), 82 insertions(+), 1 deletion(-)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-59-jiangshanlai::40gmail.com:1arch:x86:Kconfig) --git a/arch/x86/Kconfig b/arch/x86/Kconfig
index 1b4bea3db53d..ded687cc23ad 100644
--- a/arch/x86/Kconfig
+++ b/arch/x86/Kconfig @@ -853,7 +853,8 @@ config KVM_GUEST

 config PVM_GUEST
 	bool "PVM Guest support"
-	depends on X86_64 && KVM_GUEST && X86_PIE +	depends on X86_64 && KVM_GUEST && X86_PIE && !KASAN
+	select RANDOMIZE_MEMORY
 	select RELOCATABLE_UNCOMPRESSED_KERNEL
 	default n
 	help
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-59-jiangshanlai::40gmail.com:1arch:x86:include:asm:pvm_para.h) --git a/arch/x86/include/asm/pvm_para.h b/arch/x86/include/asm/pvm_para.h
index efd7afdf9be9..ff0bf0fe7dc4 100644
--- a/arch/x86/include/asm/pvm_para.h
+++ b/arch/x86/include/asm/pvm_para.h @@ -10,6 +10,7 @@
 #include <uapi/asm/kvm_para.h>

 void __init pvm_early_setup(void);
+bool __init pvm_kernel_layout_relocate(void);

 static inline void pvm_cpuid(unsigned int *eax, unsigned int *ebx,
 			     unsigned int *ecx, unsigned int *edx)
@@ -64,6 +65,11 @@ static inline bool pvm_detect(void)
 static inline void pvm_early_setup(void)
 {
 }
+
+static inline bool pvm_kernel_layout_relocate(void)
+{
+	return false;
+}
 #endif /* CONFIG_PVM_GUEST */

 #endif /* _ASM_X86_PVM_PARA_H */
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-59-jiangshanlai::40gmail.com:1arch:x86:kernel:head64_identity.c) --git a/arch/x86/kernel/head64_identity.c b/arch/x86/kernel/head64_identity.c
index f69f9904003c..467fe493c9ba 100644
--- a/arch/x86/kernel/head64_identity.c
+++ b/arch/x86/kernel/head64_identity.c @@ -396,6 +396,12 @@ static void __head detect_pvm_range(void)
 	pml4_index_end = (msr_val >> 16) & 0x1ff;
 	pvm_range_start = (0x1fffe00 | pml4_index_start) * P4D_SIZE;
 	pvm_range_end = (0x1fffe00 | pml4_index_end) * P4D_SIZE;
+
+	/*
+	 * early page fault would map page into directing mapping area,
+	 * so we should modify 'page_offset_base' here early.
+	 */
+	page_offset_base = pvm_range_start;
 }

 void __head pvm_relocate_kernel(unsigned long physbase)
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-59-jiangshanlai::40gmail.com:1arch:x86:kernel:pvm.c) --git a/arch/x86/kernel/pvm.c b/arch/x86/kernel/pvm.c
index fc82c71b305b..9cdfbaa15dbb 100644
--- a/arch/x86/kernel/pvm.c
+++ b/arch/x86/kernel/pvm.c @@ -10,7 +10,10 @@
  */
 #define pr_fmt(fmt) "pvm-guest: " fmt

+#include <linux/mm_types.h>
+
 #include <asm/cpufeature.h>
+#include <asm/cpu_entry_area.h>
 #include <asm/pvm_para.h>

 unsigned long pvm_range_start __initdata;
@@ -23,3 +26,64 @@ void __init pvm_early_setup(void)

 	setup_force_cpu_cap(X86_FEATURE_KVM_PVM_GUEST);
 }
+
+#define TB_SHIFT	40
+#define HOLE_SIZE	(1UL << 39)
+
+#define PVM_DIRECT_MAPPING_SIZE		(8UL << TB_SHIFT)
+#define PVM_VMALLOC_SIZE		(5UL << TB_SHIFT)
+#define PVM_VMEM_MAPPING_SIZE		(1UL << TB_SHIFT)
+
+/*
+ * For a PVM guest, the hypervisor would provide one valid virtual address
+ * range for the guest kernel. The guest kernel needs to adjust its layout,
+ * including the direct mapping area, vmalloc area, vmemmap area, and CPU entry
+ * area, to be within this range. If the range start is 0xffffd90000000000, the
+ * PVM guest kernel with 4-level page tables could arrange its layout as
+ * follows:
+ *
+ * ffff800000000000 - ffff87ffffffffff (=43 bits) guard hole, reserved for hypervisor
+ * ... host kernel used ...  guest kernel range start
+ * ffffd90000000000 - ffffe0ffffffffff (=8 TB) directing mapping of all physical memory
+ * ffffe10000000000 - ffffe17fffffffff (=39 bit) hole
+ * ffffe18000000000 - ffffe67fffffffff (=5 TB) vmalloc/ioremap space
+ * ffffe68000000000 - ffffe6ffffffffff (=39 bit) hole
+ * ffffe70000000000 - ffffe7ffffffffff (=40 bit) virtual memory map (1TB)
+ * ffffe80000000000 - ffffe87fffffffff (=39 bit) cpu_entry_area mapping
+ * ffffe88000000000 - ffffe8ff7fffffff (=510 G) hole
+ * ffffe8ff80000000 - ffffe8ffffffffff (=2 G) kernel image
+ * ... host kernel used ... guest kernel range end
+ *
+ */
+bool __init pvm_kernel_layout_relocate(void)
+{
+	unsigned long area_size;
+
+	if (!boot_cpu_has(X86_FEATURE_KVM_PVM_GUEST)) {
+		vmemory_end = VMALLOC_START + (VMALLOC_SIZE_TB << 40) - 1;
+		return false;
+	}
+
+	if (!IS_ALIGNED(pvm_range_start, PGDIR_SIZE))
+		panic("The start of the allowed range is not aligned");
+
+	area_size = max_pfn << PAGE_SHIFT;
+	if (area_size > PVM_DIRECT_MAPPING_SIZE)
+		panic("The memory size is too large for directing mapping area");
+
+	vmalloc_base = page_offset_base + PVM_DIRECT_MAPPING_SIZE + HOLE_SIZE;
+	vmemory_end = vmalloc_base + PVM_VMALLOC_SIZE;
+
+	vmemmap_base = vmemory_end + HOLE_SIZE;
+	area_size = max_pfn * sizeof(struct page);
+	if (area_size > PVM_VMEM_MAPPING_SIZE)
+		panic("The memory size is too large for virtual memory mapping area");
+
+	cpu_entry_area_base = vmemmap_base + PVM_VMEM_MAPPING_SIZE;
+	BUILD_BUG_ON(CPU_ENTRY_AREA_MAP_SIZE > (1UL << 39));
+
+	if (cpu_entry_area_base + (2UL << 39) > pvm_range_end)
+		panic("The size of the allowed range is too small");
+
+	return true;
+} [diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-59-jiangshanlai::40gmail.com:1arch:x86:mm:kaslr.c) --git a/arch/x86/mm/kaslr.c b/arch/x86/mm/kaslr.c
index e3825c7542a3..f6f332abf515 100644
--- a/arch/x86/mm/kaslr.c
+++ b/arch/x86/mm/kaslr.c @@ -28,6 +28,7 @@

 #include <asm/setup.h>
 #include <asm/kaslr.h>
+#include <asm/pvm_para.h>

 #include "mm_internal.h"

@@ -82,6 +83,9 @@ void __init kernel_randomize_memory(void)
 	BUILD_BUG_ON(vaddr_end != RAW_CPU_ENTRY_AREA_BASE);
 	BUILD_BUG_ON(vaddr_end > __START_KERNEL_map);

+	if (pvm_kernel_layout_relocate())
+		return;
+
 	if (!kaslr_memory_enabled())
 		return;

--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mf4f5b56a298f562c94b24e02bab22969eb6c7d39) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-59-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-59-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-59-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-59-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-59-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-59-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rf4f5b56a298f562c94b24e02bab22969eb6c7d39)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e8196806734b024e84079f3acb39d041bf3665bd2) **[RFC PATCH 59/73] x86/pti: Force enabling KPTI for PVM guest**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(57 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rf4f5b56a298f562c94b24e02bab22969eb6c7d39)
  2024-02-26 14:36 ` [[RFC PATCH 58/73] x86/pvm: Relocate kernel address space layout](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mf4f5b56a298f562c94b24e02bab22969eb6c7d39) Lai Jiangshan
**@ 2024-02-26 14:36 ` Lai Jiangshan**
  2024-02-26 14:36 ` [[RFC PATCH 60/73] x86/pvm: Add event entry/exit and dispatch code](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m99d21cd9c0251b0a5ed4be095f4028607e056c60) Lai Jiangshan
                   ` [(15 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r99d21cd9c0251b0a5ed4be095f4028607e056c60)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r8196806734b024e84079f3acb39d041bf3665bd2)
From: Lai Jiangshan @ 2024-02-26 14:36 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-60-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-60-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143833)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143833), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin, Andy Lutomirski

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

For PVM, it needs the guest to provides two different
page tables directly to prevent usermode access to the kernel
address space. So force enabling KPTI for PVM guest.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/Kconfig](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-60-jiangshanlai::40gmail.com:1arch:x86:Kconfig)  | 1 +
 [arch/x86/mm/pti.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-60-jiangshanlai::40gmail.com:1arch:x86:mm:pti.c) | 7 +++++++
 2 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e8196806734b024e84079f3acb39d041bf3665bd2), 8 insertions(+)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-60-jiangshanlai::40gmail.com:1arch:x86:Kconfig) --git a/arch/x86/Kconfig b/arch/x86/Kconfig
index ded687cc23ad..32a2ab49752b 100644
--- a/arch/x86/Kconfig
+++ b/arch/x86/Kconfig @@ -854,6 +854,7 @@ config KVM_GUEST
 config PVM_GUEST
 	bool "PVM Guest support"
 	depends on X86_64 && KVM_GUEST && X86_PIE && !KASAN
+	select PAGE_TABLE_ISOLATION
 	select RANDOMIZE_MEMORY
 	select RELOCATABLE_UNCOMPRESSED_KERNEL
 	default n
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-60-jiangshanlai::40gmail.com:1arch:x86:mm:pti.c) --git a/arch/x86/mm/pti.c b/arch/x86/mm/pti.c
index 5dd733944629..3b06faeca569 100644
--- a/arch/x86/mm/pti.c
+++ b/arch/x86/mm/pti.c @@ -84,6 +84,13 @@ void __init pti_check_boottime_disable(void)
 		return;
 	}

+	if (boot_cpu_has(X86_FEATURE_KVM_PVM_GUEST)) {
+		pti_mode = PTI_FORCE_ON;
+		pti_print_if_insecure("force enabled on kvm pvm guest.");
+		setup_force_cpu_cap(X86_FEATURE_PTI);
+		return;
+	}
+
 	if (cpu_mitigations_off())
 		pti_mode = PTI_FORCE_OFF;
 	if (pti_mode == PTI_FORCE_OFF) {
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m8196806734b024e84079f3acb39d041bf3665bd2) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-60-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-60-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-60-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-60-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-60-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-60-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r8196806734b024e84079f3acb39d041bf3665bd2)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e99d21cd9c0251b0a5ed4be095f4028607e056c60) **[RFC PATCH 60/73] x86/pvm: Add event entry/exit and dispatch code**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(58 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r8196806734b024e84079f3acb39d041bf3665bd2)
  2024-02-26 14:36 ` [[RFC PATCH 59/73] x86/pti: Force enabling KPTI for PVM guest](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m8196806734b024e84079f3acb39d041bf3665bd2) Lai Jiangshan
**@ 2024-02-26 14:36 ` Lai Jiangshan**
  2024-02-26 14:36 ` [[RFC PATCH 61/73] x86/pvm: Allow to install a system interrupt handler](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mfab4880d8049e864d42c2686113871c192625c69) Lai Jiangshan
                   ` [(14 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rfab4880d8049e864d42c2686113871c192625c69)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r99d21cd9c0251b0a5ed4be095f4028607e056c60)
From: Lai Jiangshan @ 2024-02-26 14:36 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-61-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-61-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143842)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143842), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Andy Lutomirski, Dave Hansen, H. Peter Anvin

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

In PVM, it does not use IDT-based event delivery and instead utilizes a
specific event delivery method similar to FRED.

For user mode events, stack switching and GSBASE switching are done
directly by the hypervisor. The default stack in the entry is already
the task stack, and user mode states are saved in the shared PVCS
structure. In order to avoid modifying the "vector" in the PVCS for
direct switching of the syscall event, the syscall event still uses
MSR_LSTAR as the entry.

For supervisor mode events with vector < 32, old states are saved in the
current stack. And for events with vector >=32, old states are saved in
the PVCS, since the entry is irq disabled and old states will be saved
into stack before enabling irq.  Additionally, there is no #DF for PVM
guests, as the hypervisor will treat it as a triple fault directly.
Finally, no IST is needed.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Co-developed-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/entry/Makefile](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-61-jiangshanlai::40gmail.com:1arch:x86:entry:Makefile)         |   1 +
 [arch/x86/entry/entry_64_pvm.S](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-61-jiangshanlai::40gmail.com:1arch:x86:entry:entry_64_pvm.S)   | 152 +++++++++++++++++++++++++++
 [arch/x86/include/asm/pvm_para.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-61-jiangshanlai::40gmail.com:1arch:x86:include:asm:pvm_para.h) |   8 ++
 [arch/x86/kernel/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-61-jiangshanlai::40gmail.com:1arch:x86:kernel:pvm.c)           | 181 ++++++++++++++++++++++++++++++++
 4 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e99d21cd9c0251b0a5ed4be095f4028607e056c60), 342 insertions(+)
 create mode 100644 arch/x86/entry/entry_64_pvm.S

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-61-jiangshanlai::40gmail.com:1arch:x86:entry:Makefile) --git a/arch/x86/entry/Makefile b/arch/x86/entry/Makefile
index 55dd3f193d99..d9cb970dfe06 100644
--- a/arch/x86/entry/Makefile
+++ b/arch/x86/entry/Makefile @@ -20,6 +20,7 @@ obj-y				+= vsyscall/
 obj-$(CONFIG_PREEMPTION)	+= thunk_$(BITS).o
 obj-$(CONFIG_IA32_EMULATION)	+= entry_64_compat.o syscall_32.o
 obj-$(CONFIG_X86_X32_ABI)	+= syscall_x32.o
+obj-$(CONFIG_PVM_GUEST) 	+= entry_64_pvm.o

 ifeq ($(CONFIG_X86_64),y)
 	obj-y += 		entry_64_switcher.o
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-61-jiangshanlai::40gmail.com:1arch:x86:entry:entry_64_pvm.S) --git a/arch/x86/entry/entry_64_pvm.S b/arch/x86/entry/entry_64_pvm.S
new file mode 100644
index 000000000000..256baf86a9f3
--- /dev/null
+++ b/arch/x86/entry/entry_64_pvm.S @@ -0,0 +1,152 @@ +/* SPDX-License-Identifier: GPL-2.0 */
+#include <linux/linkage.h>
+#include <asm/segment.h>
+#include <asm/asm-offsets.h>
+#include <asm/percpu.h>
+#include <asm/pvm_para.h>
+
+#include "calling.h"
+
+/* Construct struct pt_regs on stack */
+.macro PUSH_IRET_FRAME_FROM_PVCS has_cs_ss:req is_kernel:req
+	.if \has_cs_ss == 1
+		movl	PER_CPU_VAR(pvm_vcpu_struct + PVCS_user_ss), %ecx
+		andl	$0xff, %ecx
+		pushq	%rcx				/* pt_regs->ss */
+	.elseif \is_kernel == 1
+		pushq	$__KERNEL_DS
+	.else
+		pushq	$__USER_DS
+	.endif
+
+	pushq	PER_CPU_VAR(pvm_vcpu_struct + PVCS_rsp) /* pt_regs->sp */
+	movl	PER_CPU_VAR(pvm_vcpu_struct + PVCS_eflags), %ecx
+	pushq	%rcx					/* pt_regs->flags */
+
+	.if \has_cs_ss == 1
+		movl	PER_CPU_VAR(pvm_vcpu_struct + PVCS_user_cs), %ecx
+		andl	$0xff, %ecx
+		pushq	%rcx				/* pt_regs->cs */
+	.elseif \is_kernel == 1
+		pushq	$__KERNEL_CS
+	.else
+		pushq	$__USER_CS
+	.endif
+
+	pushq	PER_CPU_VAR(pvm_vcpu_struct + PVCS_rip) /* pt_regs->ip */
+
+	/* set %rcx, %r11 per PVM event handling specification */
+	movq	PER_CPU_VAR(pvm_vcpu_struct + PVCS_rcx), %rcx
+	movq	PER_CPU_VAR(pvm_vcpu_struct + PVCS_r11), %r11
+.endm
+
+.code64
+.section .entry.text, "ax"
+
+SYM_CODE_START(entry_SYSCALL_64_pvm)
+	UNWIND_HINT_ENTRY
+	ENDBR
+
+	PUSH_IRET_FRAME_FROM_PVCS has_cs_ss=0 is_kernel=0
+
+	jmp	entry_SYSCALL_64_after_hwframe
+SYM_CODE_END(entry_SYSCALL_64_pvm)
+
+/*
+ * The new RIP value that PVM event delivery establishes is
+ * MSR_PVM_EVENT_ENTRY for vector events that occur in user mode.
+ */
+	.align 64
+SYM_CODE_START(pvm_user_event_entry)
+	UNWIND_HINT_ENTRY
+	ENDBR
+
+	PUSH_IRET_FRAME_FROM_PVCS has_cs_ss=1 is_kernel=0
+	/* pt_regs->orig_ax: errcode and vector */
+	pushq	PER_CPU_VAR(pvm_vcpu_struct + PVCS_event_errcode)
+
+	PUSH_AND_CLEAR_REGS
+	movq	%rsp, %rdi	/* %rdi -> pt_regs */
+	call	pvm_event
+
+SYM_INNER_LABEL(pvm_restore_regs_and_return_to_usermode, SYM_L_GLOBAL)
+	POP_REGS
+
+	/* Copy %rcx, %r11 to the PVM CPU structure. */
+	movq	%rcx, PER_CPU_VAR(pvm_vcpu_struct + PVCS_rcx)
+	movq	%r11, PER_CPU_VAR(pvm_vcpu_struct + PVCS_r11)
+
+	/* Copy the IRET frame to the PVM CPU structure. */
+	movq	1*8(%rsp), %rcx		/* RIP */
+	movq	%rcx, PER_CPU_VAR(pvm_vcpu_struct + PVCS_rip)
+	movq	2*8(%rsp), %rcx		/* CS */
+	movw	%cx, PER_CPU_VAR(pvm_vcpu_struct + PVCS_user_cs)
+	movq	3*8(%rsp), %rcx		/* RFLAGS */
+	movl	%ecx, PER_CPU_VAR(pvm_vcpu_struct + PVCS_eflags)
+	movq	4*8(%rsp), %rcx		/* RSP */
+	movq	%rcx, PER_CPU_VAR(pvm_vcpu_struct + PVCS_rsp)
+	movq	5*8(%rsp), %rcx		/* SS */
+	movw	%cx, PER_CPU_VAR(pvm_vcpu_struct + PVCS_user_ss)
+	/*
+	 * We are on the trampoline stack.  All regs are live.
+	 * We can do future final exit work right here.
+	 */
+	STACKLEAK_ERASE_NOCLOBBER
+
+	addq	$6*8, %rsp
+SYM_INNER_LABEL(pvm_retu_rip, SYM_L_GLOBAL)
+	ANNOTATE_NOENDBR
+	syscall
+SYM_CODE_END(pvm_user_event_entry)
+
+/*
+ * The new RIP value that PVM event delivery establishes is
+ * MSR_PVM_EVENT_ENTRY + 256 for events with vector < 32
+ * that occur in supervisor mode.
+ */
+	.org pvm_user_event_entry+256, 0xcc
+SYM_CODE_START(pvm_kernel_exception_entry)
+	UNWIND_HINT_ENTRY
+	ENDBR
+
+	/* set %rcx, %r11 per PVM event handling specification */
+	movq	6*8(%rsp), %rcx
+	movq	7*8(%rsp), %r11
+
+	PUSH_AND_CLEAR_REGS
+	movq	%rsp, %rdi	/* %rdi -> pt_regs */
+	call	pvm_event
+
+	jmp pvm_restore_regs_and_return_to_kernel
+SYM_CODE_END(pvm_kernel_exception_entry)
+
+/*
+ * The new RIP value that PVM event delivery establishes is
+ * MSR_PVM_EVENT_ENTRY + 512 for events with vector >= 32
+ * that occur in supervisor mode.
+ */
+	.org pvm_user_event_entry+512, 0xcc
+SYM_CODE_START(pvm_kernel_interrupt_entry)
+	UNWIND_HINT_ENTRY
+	ENDBR
+
+	/* Reserve space for rcx/r11 */
+	subq	$16, %rsp
+
+	PUSH_IRET_FRAME_FROM_PVCS has_cs_ss=0 is_kernel=1
+	/* pt_regs->orig_ax: errcode and vector */
+	pushq	PER_CPU_VAR(pvm_vcpu_struct + PVCS_event_errcode)
+
+	PUSH_AND_CLEAR_REGS
+	movq	%rsp, %rdi	/* %rdi -> pt_regs */
+	call	pvm_event
+
+SYM_INNER_LABEL(pvm_restore_regs_and_return_to_kernel, SYM_L_GLOBAL)
+	POP_REGS
+
+	movq	%rcx, 6*8(%rsp)
+	movq	%r11, 7*8(%rsp)
+SYM_INNER_LABEL(pvm_rets_rip, SYM_L_GLOBAL)
+	ANNOTATE_NOENDBR
+	syscall
+SYM_CODE_END(pvm_kernel_interrupt_entry) [diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-61-jiangshanlai::40gmail.com:1arch:x86:include:asm:pvm_para.h) --git a/arch/x86/include/asm/pvm_para.h b/arch/x86/include/asm/pvm_para.h
index ff0bf0fe7dc4..c344185a192c 100644
--- a/arch/x86/include/asm/pvm_para.h
+++ b/arch/x86/include/asm/pvm_para.h @@ -5,6 +5,8 @@
 #include <linux/init.h>
 #include <uapi/asm/pvm_para.h>

+#ifndef __ASSEMBLY__
+
 #ifdef CONFIG_PVM_GUEST
 #include <asm/irqflags.h>
 #include <uapi/asm/kvm_para.h>
@@ -72,4 +74,10 @@ static inline bool pvm_kernel_layout_relocate(void)
 }
 #endif /* CONFIG_PVM_GUEST */

+void entry_SYSCALL_64_pvm(void);
+void pvm_user_event_entry(void);
+void pvm_retu_rip(void);
+void pvm_rets_rip(void);
+#endif /* !__ASSEMBLY__ */
+
 #endif /* _ASM_X86_PVM_PARA_H */
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-61-jiangshanlai::40gmail.com:1arch:x86:kernel:pvm.c) --git a/arch/x86/kernel/pvm.c b/arch/x86/kernel/pvm.c
index 9cdfbaa15dbb..9399e45b3c13 100644
--- a/arch/x86/kernel/pvm.c
+++ b/arch/x86/kernel/pvm.c @@ -11,14 +11,195 @@
 #define pr_fmt(fmt) "pvm-guest: " fmt

 #include <linux/mm_types.h>
+#include <linux/nospec.h>

 #include <asm/cpufeature.h>
 #include <asm/cpu_entry_area.h>
+#include <asm/desc.h>
 #include <asm/pvm_para.h>
+#include <asm/traps.h>
+
+DEFINE_PER_CPU_PAGE_ALIGNED(struct pvm_vcpu_struct, pvm_vcpu_struct);

 unsigned long pvm_range_start __initdata;
 unsigned long pvm_range_end __initdata;

+static noinstr void pvm_bad_event(struct pt_regs *regs, unsigned long vector,
+				  unsigned long error_code)
+{
+	irqentry_state_t irq_state = irqentry_nmi_enter(regs);
+
+	instrumentation_begin();
+
+	/* Panic on events from a high stack level */
+	if (!user_mode(regs)) {
+		pr_emerg("PANIC: invalid or fatal PVM event;"
+			 "vector %lu error 0x%lx at %04lx:%016lx\n",
+			 vector, error_code, regs->cs, regs->ip);
+		die("invalid or fatal PVM event", regs, error_code);
+		panic("invalid or fatal PVM event");
+	} else {
+		unsigned long flags = oops_begin();
+		int sig = SIGKILL;
+
+		pr_alert("BUG: invalid or fatal FRED event;"
+			 "vector %lu error 0x%lx at %04lx:%016lx\n",
+			 vector, error_code, regs->cs, regs->ip);
+
+		if (__die("Invalid or fatal FRED event", regs, error_code))
+			sig = 0;
+
+		oops_end(flags, regs, sig);
+	}
+	instrumentation_end();
+	irqentry_nmi_exit(regs, irq_state);
+}
+
+DEFINE_IDTENTRY_RAW(pvm_exc_debug)
+{
+	/*
+	 * There's no IST on PVM. but we still need to sipatch
+	 * to the correct handler.
+	 */
+	if (user_mode(regs))
+		noist_exc_debug(regs);
+	else
+		exc_debug(regs);
+}
+
+#ifdef CONFIG_X86_MCE
+DEFINE_IDTENTRY_RAW(pvm_exc_machine_check)
+{
+	/*
+	 * There's no IST on PVM, but we still need to dispatch
+	 * to the correct handler.
+	 */
+	if (user_mode(regs))
+		noist_exc_machine_check(regs);
+	else
+		exc_machine_check(regs);
+}
+#endif
+
+static noinstr void pvm_exception(struct pt_regs *regs, unsigned long vector,
+				  unsigned long error_code)
+{
+	/* Optimize for #PF. That's the only exception which matters performance wise */
+	if (likely(vector == X86_TRAP_PF)) {
+		exc_page_fault(regs, error_code);
+		return;
+	}
+
+	switch (vector) {
+	case X86_TRAP_DE: return exc_divide_error(regs);
+	case X86_TRAP_DB: return pvm_exc_debug(regs);
+	case X86_TRAP_NMI: return exc_nmi(regs);
+	case X86_TRAP_BP: return exc_int3(regs);
+	case X86_TRAP_OF: return exc_overflow(regs);
+	case X86_TRAP_BR: return exc_bounds(regs);
+	case X86_TRAP_UD: return exc_invalid_op(regs);
+	case X86_TRAP_NM: return exc_device_not_available(regs);
+	case X86_TRAP_DF: return exc_double_fault(regs, error_code);
+	case X86_TRAP_TS: return exc_invalid_tss(regs, error_code);
+	case X86_TRAP_NP: return exc_segment_not_present(regs, error_code);
+	case X86_TRAP_SS: return exc_stack_segment(regs, error_code);
+	case X86_TRAP_GP: return exc_general_protection(regs, error_code);
+	case X86_TRAP_MF: return exc_coprocessor_error(regs);
+	case X86_TRAP_AC: return exc_alignment_check(regs, error_code);
+	case X86_TRAP_XF: return exc_simd_coprocessor_error(regs);
+#ifdef CONFIG_X86_MCE
+	case X86_TRAP_MC: return pvm_exc_machine_check(regs);
+#endif
+#ifdef CONFIG_X86_CET
+	case X86_TRAP_CP: return exc_control_protection(regs, error_code);
+#endif
+	default: return pvm_bad_event(regs, vector, error_code);
+	}
+}
+
+static noinstr void pvm_handle_INT80_compat(struct pt_regs *regs)
+{
+#ifdef CONFIG_IA32_EMULATION
+	if (ia32_enabled()) {
+		int80_emulation(regs);
+		return;
+	}
+#endif
+	exc_general_protection(regs, 0);
+}
+
+typedef void (*idtentry_t)(struct pt_regs *regs);
+
+#define SYSVEC(_vector, _function) [_vector - FIRST_SYSTEM_VECTOR] = sysvec_##_function
+
+#define pvm_handle_spurious_interrupt ((idtentry_t)(void *)spurious_interrupt)
+
+static idtentry_t pvm_sysvec_table[NR_SYSTEM_VECTORS] __ro_after_init = {
+	[0 ... NR_SYSTEM_VECTORS-1] = pvm_handle_spurious_interrupt,
+
+	SYSVEC(ERROR_APIC_VECTOR,		error_interrupt),
+	SYSVEC(SPURIOUS_APIC_VECTOR,		spurious_apic_interrupt),
+	SYSVEC(LOCAL_TIMER_VECTOR,		apic_timer_interrupt),
+	SYSVEC(X86_PLATFORM_IPI_VECTOR,		x86_platform_ipi),
+
+#ifdef CONFIG_SMP
+	SYSVEC(RESCHEDULE_VECTOR,		reschedule_ipi),
+	SYSVEC(CALL_FUNCTION_SINGLE_VECTOR,	call_function_single),
+	SYSVEC(CALL_FUNCTION_VECTOR,		call_function),
+	SYSVEC(REBOOT_VECTOR,			reboot),
+#endif
+#ifdef CONFIG_X86_MCE_THRESHOLD
+	SYSVEC(THRESHOLD_APIC_VECTOR,		threshold),
+#endif
+#ifdef CONFIG_X86_MCE_AMD
+	SYSVEC(DEFERRED_ERROR_VECTOR,		deferred_error),
+#endif
+#ifdef CONFIG_X86_THERMAL_VECTOR
+	SYSVEC(THERMAL_APIC_VECTOR,		thermal),
+#endif
+#ifdef CONFIG_IRQ_WORK
+	SYSVEC(IRQ_WORK_VECTOR,			irq_work),
+#endif
+#ifdef CONFIG_HAVE_KVM
+	SYSVEC(POSTED_INTR_VECTOR,		kvm_posted_intr_ipi),
+	SYSVEC(POSTED_INTR_WAKEUP_VECTOR,	kvm_posted_intr_wakeup_ipi),
+	SYSVEC(POSTED_INTR_NESTED_VECTOR,	kvm_posted_intr_nested_ipi),
+#endif
+};
+
+/*
+ * some pointers in pvm_sysvec_table are actual spurious_interrupt() who
+ * expects the second argument to be the vector.
+ */
+typedef void (*idtentry_x_t)(struct pt_regs *regs, int vector);
+
+static __always_inline void pvm_handle_sysvec(struct pt_regs *regs, unsigned long vector)
+{
+	unsigned int index = array_index_nospec(vector - FIRST_SYSTEM_VECTOR,
+						NR_SYSTEM_VECTORS);
+	idtentry_x_t func = (void *)pvm_sysvec_table[index];
+
+	func(regs, vector);
+}
+
+__visible noinstr void pvm_event(struct pt_regs *regs)
+{
+	u32 error_code = regs->orig_ax;
+	u64 vector = regs->orig_ax >> 32;
+
+	/* Invalidate orig_ax so that syscall_get_nr() works correctly */
+	regs->orig_ax = -1;
+
+	if (vector < NUM_EXCEPTION_VECTORS)
+		pvm_exception(regs, vector, error_code);
+	else if (vector >= FIRST_SYSTEM_VECTOR)
+		pvm_handle_sysvec(regs, vector);
+	else if (unlikely(vector == IA32_SYSCALL_VECTOR))
+		pvm_handle_INT80_compat(regs);
+	else
+		common_interrupt(regs, vector);
+}
+
 void __init pvm_early_setup(void)
 {
 	if (!pvm_range_end)
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m99d21cd9c0251b0a5ed4be095f4028607e056c60) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-61-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-61-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-61-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-61-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-61-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-61-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r99d21cd9c0251b0a5ed4be095f4028607e056c60)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#efab4880d8049e864d42c2686113871c192625c69) **[RFC PATCH 61/73] x86/pvm: Allow to install a system interrupt handler**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(59 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r99d21cd9c0251b0a5ed4be095f4028607e056c60)
  2024-02-26 14:36 ` [[RFC PATCH 60/73] x86/pvm: Add event entry/exit and dispatch code](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m99d21cd9c0251b0a5ed4be095f4028607e056c60) Lai Jiangshan
**@ 2024-02-26 14:36 ` Lai Jiangshan**
  2024-02-26 14:36 ` [[RFC PATCH 62/73] x86/pvm: Add early kernel event entry and dispatch code](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m635896d24115d0f5add479c6e5ed8f8911dd4c23) Lai Jiangshan
                   ` [(13 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r635896d24115d0f5add479c6e5ed8f8911dd4c23)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rfab4880d8049e864d42c2686113871c192625c69)
From: Lai Jiangshan @ 2024-02-26 14:36 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-62-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-62-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143849)
  Cc: Hou Wenlong, Lai Jiangshan, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143849), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin, Wanpeng Li, Vitaly Kuznetsov

From: Hou Wenlong <houwenlong.hwl@antgroup.com>

Add pvm_sysvec_install() to install a system interrupt handler into PVM
system interrupt handler table.

Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
---
 [arch/x86/include/asm/pvm_para.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-62-jiangshanlai::40gmail.com:1arch:x86:include:asm:pvm_para.h) |  6 ++++++
 [arch/x86/kernel/kvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-62-jiangshanlai::40gmail.com:1arch:x86:kernel:kvm.c)           |  2 ++
 [arch/x86/kernel/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-62-jiangshanlai::40gmail.com:1arch:x86:kernel:pvm.c)           | 11 +++++++++--
 3 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#efab4880d8049e864d42c2686113871c192625c69), 17 insertions(+), 2 deletions(-)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-62-jiangshanlai::40gmail.com:1arch:x86:include:asm:pvm_para.h) --git a/arch/x86/include/asm/pvm_para.h b/arch/x86/include/asm/pvm_para.h
index c344185a192c..9216e539fea8 100644
--- a/arch/x86/include/asm/pvm_para.h
+++ b/arch/x86/include/asm/pvm_para.h @@ -6,12 +6,14 @@
 #include <uapi/asm/pvm_para.h>

 #ifndef __ASSEMBLY__
+typedef void (*idtentry_t)(struct pt_regs *regs);

 #ifdef CONFIG_PVM_GUEST
 #include <asm/irqflags.h>
 #include <uapi/asm/kvm_para.h>

 void __init pvm_early_setup(void);
+void __init pvm_install_sysvec(unsigned int sysvec, idtentry_t handler);
 bool __init pvm_kernel_layout_relocate(void);

 static inline void pvm_cpuid(unsigned int *eax, unsigned int *ebx,
@@ -68,6 +70,10 @@ static inline void pvm_early_setup(void)
 {
 }

+static inline void pvm_install_sysvec(unsigned int sysvec, idtentry_t handler)
+{
+}
+
 static inline bool pvm_kernel_layout_relocate(void)
 {
 	return false;
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-62-jiangshanlai::40gmail.com:1arch:x86:kernel:kvm.c) --git a/arch/x86/kernel/kvm.c b/arch/x86/kernel/kvm.c
index de72a5a1f7ad..87b00c279aaf 100644
--- a/arch/x86/kernel/kvm.c
+++ b/arch/x86/kernel/kvm.c @@ -43,6 +43,7 @@
 #include <asm/reboot.h>
 #include <asm/svm.h>
 #include <asm/e820/api.h>
+#include <asm/pvm_para.h>

 DEFINE_STATIC_KEY_FALSE(kvm_async_pf_enabled);

@@ -843,6 +844,7 @@ static void __init kvm_guest_init(void)
 	if (kvm_para_has_feature(KVM_FEATURE_ASYNC_PF_INT) && kvmapf) {
 		static_branch_enable(&kvm_async_pf_enabled);
 		alloc_intr_gate(HYPERVISOR_CALLBACK_VECTOR, asm_sysvec_kvm_asyncpf_interrupt);
+		pvm_install_sysvec(HYPERVISOR_CALLBACK_VECTOR, sysvec_kvm_asyncpf_interrupt);
 	}

 #ifdef CONFIG_SMP
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-62-jiangshanlai::40gmail.com:1arch:x86:kernel:pvm.c) --git a/arch/x86/kernel/pvm.c b/arch/x86/kernel/pvm.c
index 9399e45b3c13..88b013185ecd 100644
--- a/arch/x86/kernel/pvm.c
+++ b/arch/x86/kernel/pvm.c @@ -128,8 +128,6 @@ static noinstr void pvm_handle_INT80_compat(struct pt_regs *regs)
 	exc_general_protection(regs, 0);
 }

-typedef void (*idtentry_t)(struct pt_regs *regs);
 #define SYSVEC(_vector, _function) [_vector - FIRST_SYSTEM_VECTOR] = sysvec_##_function

 #define pvm_handle_spurious_interrupt ((idtentry_t)(void *)spurious_interrupt)
@@ -167,6 +165,15 @@ static idtentry_t pvm_sysvec_table[NR_SYSTEM_VECTORS] __ro_after_init = {
 #endif
 };

+void __init pvm_install_sysvec(unsigned int sysvec, idtentry_t handler)
+{
+	if (WARN_ON_ONCE(sysvec < FIRST_SYSTEM_VECTOR))
+		return;
+	if (!WARN_ON_ONCE(pvm_sysvec_table[sysvec - FIRST_SYSTEM_VECTOR] !=
+			  pvm_handle_spurious_interrupt))
+		pvm_sysvec_table[sysvec - FIRST_SYSTEM_VECTOR] = handler;
+}
+
 /*
  * some pointers in pvm_sysvec_table are actual spurious_interrupt() who
  * expects the second argument to be the vector.
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mfab4880d8049e864d42c2686113871c192625c69) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-62-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-62-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-62-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-62-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-62-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-62-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rfab4880d8049e864d42c2686113871c192625c69)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e635896d24115d0f5add479c6e5ed8f8911dd4c23) **[RFC PATCH 62/73] x86/pvm: Add early kernel event entry and dispatch code**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(60 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rfab4880d8049e864d42c2686113871c192625c69)
  2024-02-26 14:36 ` [[RFC PATCH 61/73] x86/pvm: Allow to install a system interrupt handler](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mfab4880d8049e864d42c2686113871c192625c69) Lai Jiangshan
**@ 2024-02-26 14:36 ` Lai Jiangshan**
  2024-02-26 14:36 ` [[RFC PATCH 63/73] x86/pvm: Add hypercall support](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m89cc21528bcaef9776de23d2e19df5a99f2dc50f) Lai Jiangshan
                   ` [(12 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r89cc21528bcaef9776de23d2e19df5a99f2dc50f)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r635896d24115d0f5add479c6e5ed8f8911dd4c23)
From: Lai Jiangshan @ 2024-02-26 14:36 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-63-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-63-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143858)
  Cc: Hou Wenlong, Lai Jiangshan, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143858), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin, David Woodhouse, Brian Gerst,
	Josh Poimboeuf, Thomas Garnier, Ard Biesheuvel, Tom Lendacky

From: Hou Wenlong <houwenlong.hwl@antgroup.com>

Since PVM doesn't support IDT-based event delivery, it needs to handle
early kernel events during the booting. Currently, there are two stages
before the final IDT setup. Firstly, all exception handlers are set as
do_early_exception() in idt_setup_early_handlers(). Later, #DB, #BP, and
dispatch code.

Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
---
 [arch/x86/include/asm/pvm_para.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-63-jiangshanlai::40gmail.com:1arch:x86:include:asm:pvm_para.h) |  5 +++++
 [arch/x86/kernel/head_64.S](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-63-jiangshanlai::40gmail.com:1arch:x86:kernel:head_64.S)       | 21 +++++++++++++++++++++
 [arch/x86/kernel/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-63-jiangshanlai::40gmail.com:1arch:x86:kernel:pvm.c)           | 33 +++++++++++++++++++++++++++++++++
 3 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e635896d24115d0f5add479c6e5ed8f8911dd4c23), 59 insertions(+)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-63-jiangshanlai::40gmail.com:1arch:x86:include:asm:pvm_para.h) --git a/arch/x86/include/asm/pvm_para.h b/arch/x86/include/asm/pvm_para.h
index 9216e539fea8..bfb08f0ea293 100644
--- a/arch/x86/include/asm/pvm_para.h
+++ b/arch/x86/include/asm/pvm_para.h @@ -13,6 +13,7 @@ typedef void (*idtentry_t)(struct pt_regs *regs);
 #include <uapi/asm/kvm_para.h>

 void __init pvm_early_setup(void);
+void __init pvm_setup_early_traps(void);
 void __init pvm_install_sysvec(unsigned int sysvec, idtentry_t handler);
 bool __init pvm_kernel_layout_relocate(void);

@@ -70,6 +71,10 @@ static inline void pvm_early_setup(void)
 {
 }

+static inline void pvm_setup_early_traps(void)
+{
+}
+
 static inline void pvm_install_sysvec(unsigned int sysvec, idtentry_t handler)
 {
 }
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-63-jiangshanlai::40gmail.com:1arch:x86:kernel:head_64.S) --git a/arch/x86/kernel/head_64.S b/arch/x86/kernel/head_64.S
index 1d931bab4393..6ad3aedca7da 100644
--- a/arch/x86/kernel/head_64.S
+++ b/arch/x86/kernel/head_64.S @@ -633,6 +633,27 @@ SYM_CODE_START_NOALIGN(vc_no_ghcb)
 SYM_CODE_END(vc_no_ghcb)
 #endif

+#ifdef CONFIG_PVM_GUEST
+	.align 256
+SYM_CODE_START_NOALIGN(pvm_early_kernel_event_entry)
+	UNWIND_HINT_ENTRY
+	ENDBR
+
+	incl	early_recursion_flag(%rip)
+
+	/* set %rcx, %r11 per PVM event handling specification */
+	movq	6*8(%rsp), %rcx
+	movq	7*8(%rsp), %r11
+
+	PUSH_AND_CLEAR_REGS
+	movq	%rsp, %rdi	/* %rdi -> pt_regs */
+	call	pvm_early_event
+
+	decl	early_recursion_flag(%rip)
+	jmp	pvm_restore_regs_and_return_to_kernel
+SYM_CODE_END(pvm_early_kernel_event_entry)
+#endif
+
 #define SYM_DATA_START_PAGE_ALIGNED(name)\
 	SYM_START(name, SYM_L_GLOBAL, .balign PAGE_SIZE)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-63-jiangshanlai::40gmail.com:1arch:x86:kernel:pvm.c) --git a/arch/x86/kernel/pvm.c b/arch/x86/kernel/pvm.c
index 88b013185ecd..b3b4ff0bbc91 100644
--- a/arch/x86/kernel/pvm.c
+++ b/arch/x86/kernel/pvm.c @@ -17,6 +17,7 @@
 #include <asm/cpu_entry_area.h>
 #include <asm/desc.h>
 #include <asm/pvm_para.h>
+#include <asm/setup.h>
 #include <asm/traps.h>

 DEFINE_PER_CPU_PAGE_ALIGNED(struct pvm_vcpu_struct, pvm_vcpu_struct);
@@ -24,6 +25,38 @@ DEFINE_PER_CPU_PAGE_ALIGNED(struct pvm_vcpu_struct, pvm_vcpu_struct);
 unsigned long pvm_range_start __initdata;
 unsigned long pvm_range_end __initdata;

+static bool early_traps_setup __initdata;
+
+void __init pvm_early_event(struct pt_regs *regs)
+{
+	int vector = regs->orig_ax >> 32;
+
+	if (!early_traps_setup) {
+		do_early_exception(regs, vector);
+		return;
+	}
+
+	switch (vector) {
+	case X86_TRAP_DB:
+		exc_debug(regs);
+		return;
+	case X86_TRAP_BP:
+		exc_int3(regs);
+		return;
+	case X86_TRAP_PF:
+		exc_page_fault(regs, regs->orig_ax);
+		return;
+	default:
+		do_early_exception(regs, vector);
+		return;
+	}
+}
+
+void __init pvm_setup_early_traps(void)
+{
+	early_traps_setup = true;
+}
+
 static noinstr void pvm_bad_event(struct pt_regs *regs, unsigned long vector,
 				  unsigned long error_code)
 {
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m635896d24115d0f5add479c6e5ed8f8911dd4c23) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-63-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-63-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-63-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-63-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-63-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-63-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r635896d24115d0f5add479c6e5ed8f8911dd4c23)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e89cc21528bcaef9776de23d2e19df5a99f2dc50f) **[RFC PATCH 63/73] x86/pvm: Add hypercall support**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(61 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r635896d24115d0f5add479c6e5ed8f8911dd4c23)
  2024-02-26 14:36 ` [[RFC PATCH 62/73] x86/pvm: Add early kernel event entry and dispatch code](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m635896d24115d0f5add479c6e5ed8f8911dd4c23) Lai Jiangshan
**@ 2024-02-26 14:36 ` Lai Jiangshan**
  2024-02-26 14:36 ` [[RFC PATCH 64/73] x86/pvm: Enable PVM event delivery](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m2c3dd6d46e7c121b8a8aa343c7a963dd616bb40f) Lai Jiangshan
                   ` [(11 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r2c3dd6d46e7c121b8a8aa343c7a963dd616bb40f)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r89cc21528bcaef9776de23d2e19df5a99f2dc50f)
From: Lai Jiangshan @ 2024-02-26 14:36 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-64-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-64-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143906)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143906), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Andy Lutomirski, Dave Hansen, H. Peter Anvin

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

For the PVM guest, it will use the syscall instruction as the hypercall
instruction and follow the KVM hypercall call convention.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/entry/entry_64_pvm.S](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-64-jiangshanlai::40gmail.com:1arch:x86:entry:entry_64_pvm.S)   | 15 +++++++++++
 [arch/x86/include/asm/pvm_para.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-64-jiangshanlai::40gmail.com:1arch:x86:include:asm:pvm_para.h) |  1 +
 [arch/x86/kernel/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-64-jiangshanlai::40gmail.com:1arch:x86:kernel:pvm.c)           | 46 +++++++++++++++++++++++++++++++++
 3 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e89cc21528bcaef9776de23d2e19df5a99f2dc50f), 62 insertions(+)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-64-jiangshanlai::40gmail.com:1arch:x86:entry:entry_64_pvm.S) --git a/arch/x86/entry/entry_64_pvm.S b/arch/x86/entry/entry_64_pvm.S
index 256baf86a9f3..abb57e251e73 100644
--- a/arch/x86/entry/entry_64_pvm.S
+++ b/arch/x86/entry/entry_64_pvm.S @@ -52,6 +52,21 @@ SYM_CODE_START(entry_SYSCALL_64_pvm)
 	jmp	entry_SYSCALL_64_after_hwframe
 SYM_CODE_END(entry_SYSCALL_64_pvm)

+.pushsection .noinstr.text, "ax"
+SYM_FUNC_START(pvm_hypercall)
+	push	%r11
+	push	%r10
+	movq	%rcx, %r10
+	UNWIND_HINT_SAVE
+	syscall
+	UNWIND_HINT_RESTORE
+	movq	%r10, %rcx
+	popq	%r10
+	popq	%r11
+	RET
+SYM_FUNC_END(pvm_hypercall)
+.popsection
+
 /*
  * The new RIP value that PVM event delivery establishes is
  * MSR_PVM_EVENT_ENTRY for vector events that occur in user mode.
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-64-jiangshanlai::40gmail.com:1arch:x86:include:asm:pvm_para.h) --git a/arch/x86/include/asm/pvm_para.h b/arch/x86/include/asm/pvm_para.h
index bfb08f0ea293..72c74545dba6 100644
--- a/arch/x86/include/asm/pvm_para.h
+++ b/arch/x86/include/asm/pvm_para.h @@ -87,6 +87,7 @@ static inline bool pvm_kernel_layout_relocate(void)

 void entry_SYSCALL_64_pvm(void);
 void pvm_user_event_entry(void);
+void pvm_hypercall(void);
 void pvm_retu_rip(void);
 void pvm_rets_rip(void);
 #endif /* !__ASSEMBLY__ */
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-64-jiangshanlai::40gmail.com:1arch:x86:kernel:pvm.c) --git a/arch/x86/kernel/pvm.c b/arch/x86/kernel/pvm.c
index b3b4ff0bbc91..352d74394c4a 100644
--- a/arch/x86/kernel/pvm.c
+++ b/arch/x86/kernel/pvm.c @@ -27,6 +27,52 @@ unsigned long pvm_range_end __initdata;

 static bool early_traps_setup __initdata;

+static __always_inline long pvm_hypercall0(unsigned int nr)
+{
+	long ret;
+
+	asm volatile("call pvm_hypercall"
+		     : "=a"(ret)
+		     : "a"(nr)
+		     : "memory");
+	return ret;
+}
+
+static __always_inline long pvm_hypercall1(unsigned int nr, unsigned long p1)
+{
+	long ret;
+
+	asm volatile("call pvm_hypercall"
+		     : "=a"(ret)
+		     : "a"(nr), "b"(p1)
+		     : "memory");
+	return ret;
+}
+
+static __always_inline long pvm_hypercall2(unsigned int nr, unsigned long p1,
+					   unsigned long p2)
+{
+	long ret;
+
+	asm volatile("call pvm_hypercall"
+		     : "=a"(ret)
+		     : "a"(nr), "b"(p1), "c"(p2)
+		     : "memory");
+	return ret;
+}
+
+static __always_inline long pvm_hypercall3(unsigned int nr, unsigned long p1,
+					   unsigned long p2, unsigned long p3)
+{
+	long ret;
+
+	asm volatile("call pvm_hypercall"
+		     : "=a"(ret)
+		     : "a"(nr), "b"(p1), "c"(p2), "d"(p3)
+		     : "memory");
+	return ret;
+}
+
 void __init pvm_early_event(struct pt_regs *regs)
 {
 	int vector = regs->orig_ax >> 32;
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m89cc21528bcaef9776de23d2e19df5a99f2dc50f) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-64-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-64-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-64-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-64-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-64-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-64-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r89cc21528bcaef9776de23d2e19df5a99f2dc50f)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e2c3dd6d46e7c121b8a8aa343c7a963dd616bb40f) **[RFC PATCH 64/73] x86/pvm: Enable PVM event delivery**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(62 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r89cc21528bcaef9776de23d2e19df5a99f2dc50f)
  2024-02-26 14:36 ` [[RFC PATCH 63/73] x86/pvm: Add hypercall support](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m89cc21528bcaef9776de23d2e19df5a99f2dc50f) Lai Jiangshan
**@ 2024-02-26 14:36 ` Lai Jiangshan**
  2024-02-26 14:36 ` [[RFC PATCH 65/73] x86/kvm: Patch KVM hypercall as PVM hypercall](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m92ec0e5a6b593ee132eaefd57fad44391dfe83ec) Lai Jiangshan
                   ` [(10 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r92ec0e5a6b593ee132eaefd57fad44391dfe83ec)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r2c3dd6d46e7c121b8a8aa343c7a963dd616bb40f)
From: Lai Jiangshan @ 2024-02-26 14:36 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-65-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-65-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143916)
  Cc: Hou Wenlong, Lai Jiangshan, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143916), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Andy Lutomirski, Dave Hansen, H. Peter Anvin, Nikolay Borisov,
	Rick Edgecombe, Daniel Sneddon, Adam Dunlap, Yuntao Wang,
	Wang Jinchao, Josh Poimboeuf, Mike Rapoport (IBM), Yu-cheng Yu

From: Hou Wenlong <houwenlong.hwl@antgroup.com>

Invoke pvm_early_setup() after idt_setup_early_handler() to enable early
kernel event delivery. Also, modify cpu_init_exception_handling() to
call pvm_setup_event_handling() in order to enable event delivery for
the current CPU. Additionally, for the syscall event, change MSR_LSTAR
to PVM specific entry.

Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
---
 [arch/x86/entry/entry_64.S](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-65-jiangshanlai::40gmail.com:1arch:x86:entry:entry_64.S)       |  9 ++++++--
 [arch/x86/include/asm/pvm_para.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-65-jiangshanlai::40gmail.com:1arch:x86:include:asm:pvm_para.h) |  5 +++++
 [arch/x86/kernel/cpu/common.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-65-jiangshanlai::40gmail.com:1arch:x86:kernel:cpu:common.c)    | 11 ++++++++++
 [arch/x86/kernel/head64.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-65-jiangshanlai::40gmail.com:1arch:x86:kernel:head64.c)        |  3 +++
 [arch/x86/kernel/idt.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-65-jiangshanlai::40gmail.com:1arch:x86:kernel:idt.c)           |  2 ++
 [arch/x86/kernel/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-65-jiangshanlai::40gmail.com:1arch:x86:kernel:pvm.c)           | 37 +++++++++++++++++++++++++++++++++
 6 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e2c3dd6d46e7c121b8a8aa343c7a963dd616bb40f), 65 insertions(+), 2 deletions(-)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-65-jiangshanlai::40gmail.com:1arch:x86:entry:entry_64.S) --git a/arch/x86/entry/entry_64.S b/arch/x86/entry/entry_64.S
index 5b25ea4a16ae..fe12605b3c05 100644
--- a/arch/x86/entry/entry_64.S
+++ b/arch/x86/entry/entry_64.S @@ -124,10 +124,12 @@ SYM_INNER_LABEL(entry_SYSCALL_64_after_hwframe, SYM_L_GLOBAL)
 	 * a completely clean 64-bit userspace context.  If we're not,
 	 * go to the slow exit path.
 	 * In the Xen PV case we must use iret anyway.
+	 * In the PVM guest case we must use eretu synthetic instruction.
 	 */

-	ALTERNATIVE "testb %al, %al; jz swapgs_restore_regs_and_return_to_usermode",\
-		"jmp swapgs_restore_regs_and_return_to_usermode", X86_FEATURE_XENPV +	ALTERNATIVE_2 "testb %al, %al; jz swapgs_restore_regs_and_return_to_usermode",\
+		"jmp swapgs_restore_regs_and_return_to_usermode", X86_FEATURE_XENPV,\
+		"jmp swapgs_restore_regs_and_return_to_usermode", X86_FEATURE_KVM_PVM_GUEST

 	/*
 	 * We win! This label is here just for ease of understanding
@@ -597,6 +599,9 @@ SYM_INNER_LABEL(swapgs_restore_regs_and_return_to_usermode, SYM_L_GLOBAL)
 #ifdef CONFIG_XEN_PV
 	ALTERNATIVE "", "jmp xenpv_restore_regs_and_return_to_usermode", X86_FEATURE_XENPV
 #endif
+#ifdef CONFIG_PVM_GUEST
+	ALTERNATIVE "", "jmp pvm_restore_regs_and_return_to_usermode", X86_FEATURE_KVM_PVM_GUEST
+#endif

 	POP_REGS pop_rdi=0

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-65-jiangshanlai::40gmail.com:1arch:x86:include:asm:pvm_para.h) --git a/arch/x86/include/asm/pvm_para.h b/arch/x86/include/asm/pvm_para.h
index 72c74545dba6..f5d40a57c423 100644
--- a/arch/x86/include/asm/pvm_para.h
+++ b/arch/x86/include/asm/pvm_para.h @@ -15,6 +15,7 @@ typedef void (*idtentry_t)(struct pt_regs *regs);
 void __init pvm_early_setup(void);
 void __init pvm_setup_early_traps(void);
 void __init pvm_install_sysvec(unsigned int sysvec, idtentry_t handler);
+void pvm_setup_event_handling(void);
 bool __init pvm_kernel_layout_relocate(void);

 static inline void pvm_cpuid(unsigned int *eax, unsigned int *ebx,
@@ -79,6 +80,10 @@ static inline void pvm_install_sysvec(unsigned int sysvec, idtentry_t handler)
 {
 }

+static inline void pvm_setup_event_handling(void)
+{
+}
+
 static inline bool pvm_kernel_layout_relocate(void)
 {
 	return false;
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-65-jiangshanlai::40gmail.com:1arch:x86:kernel:cpu:common.c) --git a/arch/x86/kernel/cpu/common.c b/arch/x86/kernel/cpu/common.c
index 45f214e41a9a..89874559dbc2 100644
--- a/arch/x86/kernel/cpu/common.c
+++ b/arch/x86/kernel/cpu/common.c @@ -66,6 +66,7 @@
 #include <asm/set_memory.h>
 #include <asm/traps.h>
 #include <asm/sev.h>
+#include <asm/pvm_para.h>

 #include "cpu.h"

@@ -2066,7 +2067,15 @@ static void wrmsrl_cstar(unsigned long val)
 void syscall_init(void)
 {
 	wrmsr(MSR_STAR, 0, (__USER32_CS << 16) | __KERNEL_CS);
+
+#ifdef CONFIG_PVM_GUEST
+	if (boot_cpu_has(X86_FEATURE_KVM_PVM_GUEST))
+		wrmsrl(MSR_LSTAR, (unsigned long)entry_SYSCALL_64_pvm);
+	else
+		wrmsrl(MSR_LSTAR, (unsigned long)entry_SYSCALL_64);
+#else
 	wrmsrl(MSR_LSTAR, (unsigned long)entry_SYSCALL_64);
+#endif

 	if (ia32_enabled()) {
 		wrmsrl_cstar((unsigned long)entry_SYSCALL_compat);
@@ -2217,6 +2226,8 @@ void cpu_init_exception_handling(void)

 	/* Finally load the IDT */
 	load_current_idt();
+
+	pvm_setup_event_handling();
 }

 /*
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-65-jiangshanlai::40gmail.com:1arch:x86:kernel:head64.c) --git a/arch/x86/kernel/head64.c b/arch/x86/kernel/head64.c
index d0e8d648bd38..17cd11dd1f03 100644
--- a/arch/x86/kernel/head64.c
+++ b/arch/x86/kernel/head64.c @@ -42,6 +42,7 @@
 #include <asm/sev.h>
 #include <asm/tdx.h>
 #include <asm/init.h>
+#include <asm/pvm_para.h>

 /*
  * Manage page tables very early on.
@@ -286,6 +287,8 @@ asmlinkage __visible void __init __noreturn x86_64_start_kernel(char * real_mode

 	idt_setup_early_handler();

+	pvm_early_setup();
+
 	/* Needed before cc_platform_has() can be used for TDX */
 	tdx_early_init();

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-65-jiangshanlai::40gmail.com:1arch:x86:kernel:idt.c) --git a/arch/x86/kernel/idt.c b/arch/x86/kernel/idt.c
index 660b601f1d6c..0dc3ded6da01 100644
--- a/arch/x86/kernel/idt.c
+++ b/arch/x86/kernel/idt.c @@ -12,6 +12,7 @@
 #include <asm/hw_irq.h>
 #include <asm/ia32.h>
 #include <asm/idtentry.h>
+#include <asm/pvm_para.h>

 #define DPL0		0x0
 #define DPL3		0x3
@@ -259,6 +260,7 @@ void __init idt_setup_early_pf(void)
 {
 	idt_setup_from_table(idt_table, early_pf_idts,
 			     ARRAY_SIZE(early_pf_idts), true);
+	pvm_setup_early_traps();
 }
 #endif

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-65-jiangshanlai::40gmail.com:1arch:x86:kernel:pvm.c) --git a/arch/x86/kernel/pvm.c b/arch/x86/kernel/pvm.c
index 352d74394c4a..c38e46a96ad3 100644
--- a/arch/x86/kernel/pvm.c
+++ b/arch/x86/kernel/pvm.c @@ -286,12 +286,49 @@ __visible noinstr void pvm_event(struct pt_regs *regs)
 		common_interrupt(regs, vector);
 }

+extern void pvm_early_kernel_event_entry(void);
+
+/*
+ * Reserve a fixed-size area in the current stack during an event from
+ * supervisor mode. This is for the int3 handler to emulate a call instruction.
+ */
+#define PVM_SUPERVISOR_REDZONE_SIZE	(2*8UL)
+
 void __init pvm_early_setup(void)
 {
 	if (!pvm_range_end)
 		return;

 	setup_force_cpu_cap(X86_FEATURE_KVM_PVM_GUEST);
+
+	wrmsrl(MSR_PVM_VCPU_STRUCT, __pa(this_cpu_ptr(&pvm_vcpu_struct)));
+	wrmsrl(MSR_PVM_EVENT_ENTRY, (unsigned long)(void *)pvm_early_kernel_event_entry - 256);
+	wrmsrl(MSR_PVM_SUPERVISOR_REDZONE, PVM_SUPERVISOR_REDZONE_SIZE);
+	wrmsrl(MSR_PVM_RETS_RIP, (unsigned long)(void *)pvm_rets_rip);
+}
+
+void pvm_setup_event_handling(void)
+{
+	if (boot_cpu_has(X86_FEATURE_KVM_PVM_GUEST)) {
+		u64 xpa = slow_virt_to_phys(this_cpu_ptr(&pvm_vcpu_struct));
+
+		wrmsrl(MSR_PVM_VCPU_STRUCT, xpa);
+		wrmsrl(MSR_PVM_EVENT_ENTRY, (unsigned long)(void *)pvm_user_event_entry);
+		wrmsrl(MSR_PVM_SUPERVISOR_REDZONE, PVM_SUPERVISOR_REDZONE_SIZE);
+		wrmsrl(MSR_PVM_RETU_RIP, (unsigned long)(void *)pvm_retu_rip);
+		wrmsrl(MSR_PVM_RETS_RIP, (unsigned long)(void *)pvm_rets_rip);
+
+		/*
+		 * PVM spec requires the hypervisor-maintained
+		 * MSR_KERNEL_GS_BASE to be the same as the kernel GSBASE for
+		 * event delivery for user mode. wrmsrl(MSR_KERNEL_GS_BASE)
+		 * accesses only the user GSBASE in the PVCS via
+		 * pvm_write_msr() without hypervisor involved, so use
+		 * PVM_HC_WRMSR instead.
+		 */
+		pvm_hypercall2(PVM_HC_WRMSR, MSR_KERNEL_GS_BASE,
+			       cpu_kernelmode_gs_base(smp_processor_id()));
+	}
 }

 #define TB_SHIFT	40
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m2c3dd6d46e7c121b8a8aa343c7a963dd616bb40f) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-65-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-65-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-65-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-65-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-65-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-65-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r2c3dd6d46e7c121b8a8aa343c7a963dd616bb40f)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e92ec0e5a6b593ee132eaefd57fad44391dfe83ec) **[RFC PATCH 65/73] x86/kvm: Patch KVM hypercall as PVM hypercall**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(63 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r2c3dd6d46e7c121b8a8aa343c7a963dd616bb40f)
  2024-02-26 14:36 ` [[RFC PATCH 64/73] x86/pvm: Enable PVM event delivery](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m2c3dd6d46e7c121b8a8aa343c7a963dd616bb40f) Lai Jiangshan
**@ 2024-02-26 14:36 ` Lai Jiangshan**
  2024-02-26 14:36 ` [[RFC PATCH 66/73] x86/pvm: Use new cpu feature to describe XENPV and PVM](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m9d2e3415e38f877972ab77863f494706ffb770a0) Lai Jiangshan
                   ` [(9 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r9d2e3415e38f877972ab77863f494706ffb770a0)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r92ec0e5a6b593ee132eaefd57fad44391dfe83ec)
From: Lai Jiangshan @ 2024-02-26 14:36 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-66-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-66-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143920)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143920), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Wanpeng Li, Vitaly Kuznetsov, Dave Hansen, H. Peter Anvin

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

Modify the KVM_HYPERCALL macro to enable patching the KVM hypercall as a
PVM hypercall. Note that this modification will increase the size by two
bytes for each KVM hypercall instruction.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/include/asm/kvm_para.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-66-jiangshanlai::40gmail.com:1arch:x86:include:asm:kvm_para.h) | 7 +++++++
 1 file [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e92ec0e5a6b593ee132eaefd57fad44391dfe83ec), 7 insertions(+)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-66-jiangshanlai::40gmail.com:1arch:x86:include:asm:kvm_para.h) --git a/arch/x86/include/asm/kvm_para.h b/arch/x86/include/asm/kvm_para.h
index 57bc74e112f2..1a322b684146 100644
--- a/arch/x86/include/asm/kvm_para.h
+++ b/arch/x86/include/asm/kvm_para.h @@ -2,6 +2,7 @@
 #ifndef _ASM_X86_KVM_PARA_H
 #define _ASM_X86_KVM_PARA_H

+#include <asm/pvm_para.h>
 #include <asm/processor.h>
 #include <asm/alternative.h>
 #include <linux/interrupt.h>
@@ -18,8 +19,14 @@ static inline bool kvm_check_and_clear_guest_paused(void)
 }
 #endif /* CONFIG_KVM_GUEST */

+#ifdef CONFIG_PVM_GUEST
+#define KVM_HYPERCALL\
+	ALTERNATIVE_2("vmcall", "vmmcall", X86_FEATURE_VMMCALL,\
+		      "call pvm_hypercall", X86_FEATURE_KVM_PVM_GUEST)
+#else
 #define KVM_HYPERCALL\
         ALTERNATIVE("vmcall", "vmmcall", X86_FEATURE_VMMCALL)
+#endif /* CONFIG_PVM_GUEST */

 /* For KVM hypercalls, a three-byte sequence of either the vmcall or the vmmcall
  * instruction.  The hypervisor may replace it with something else but only the
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m92ec0e5a6b593ee132eaefd57fad44391dfe83ec) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-66-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-66-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-66-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-66-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-66-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-66-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r92ec0e5a6b593ee132eaefd57fad44391dfe83ec)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e9d2e3415e38f877972ab77863f494706ffb770a0) **[RFC PATCH 66/73] x86/pvm: Use new cpu feature to describe XENPV and PVM**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(64 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r92ec0e5a6b593ee132eaefd57fad44391dfe83ec)
  2024-02-26 14:36 ` [[RFC PATCH 65/73] x86/kvm: Patch KVM hypercall as PVM hypercall](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m92ec0e5a6b593ee132eaefd57fad44391dfe83ec) Lai Jiangshan
**@ 2024-02-26 14:36 ` Lai Jiangshan**
  2024-02-26 14:36 ` [[RFC PATCH 67/73] x86/pvm: Implement cpu related PVOPS](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mf306e82734c379c14ea3a038c5663661d4f4d56e) Lai Jiangshan
                   ` [(8 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rf306e82734c379c14ea3a038c5663661d4f4d56e)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r9d2e3415e38f877972ab77863f494706ffb770a0)
From: Lai Jiangshan @ 2024-02-26 14:36 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-67-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-67-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143926)
  Cc: Hou Wenlong, Lai Jiangshan, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143926), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Andy Lutomirski, Dave Hansen, H. Peter Anvin, Ajay Kaher,
	Alexey Makhalov, VMware PV-Drivers Reviewers, Boris Ostrovsky,
	Mike Rapoport (IBM), Daniel Sneddon, Rick Edgecombe,
	Alexey Kardashevskiy, virtualization, xen-devel

From: Hou Wenlong <houwenlong.hwl@antgroup.com>

Some PVOPS are patched as the native version directly if the guest is
not a XENPV guest. However, this approach will not work after
introducing a PVM guest. To address this, use a new CPU feature to
describe XENPV and PVM, and ensure that those PVOPS are patched only
when it is not a paravirtual guest.

Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
---
 [arch/x86/entry/entry_64.S](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-67-jiangshanlai::40gmail.com:1arch:x86:entry:entry_64.S)          |  5 ++---
 [arch/x86/include/asm/cpufeatures.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-67-jiangshanlai::40gmail.com:1arch:x86:include:asm:cpufeatures.h) |  1 +
 [arch/x86/include/asm/paravirt.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-67-jiangshanlai::40gmail.com:1arch:x86:include:asm:paravirt.h)    | 14 +++++++-------
 [arch/x86/kernel/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-67-jiangshanlai::40gmail.com:1arch:x86:kernel:pvm.c)              |  1 +
 [arch/x86/xen/enlighten_pv.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-67-jiangshanlai::40gmail.com:1arch:x86:xen:enlighten_pv.c)        |  1 +
 5 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e9d2e3415e38f877972ab77863f494706ffb770a0), 12 insertions(+), 10 deletions(-)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-67-jiangshanlai::40gmail.com:1arch:x86:entry:entry_64.S) --git a/arch/x86/entry/entry_64.S b/arch/x86/entry/entry_64.S
index fe12605b3c05..6b41a1837698 100644
--- a/arch/x86/entry/entry_64.S
+++ b/arch/x86/entry/entry_64.S @@ -127,9 +127,8 @@ SYM_INNER_LABEL(entry_SYSCALL_64_after_hwframe, SYM_L_GLOBAL)
 	 * In the PVM guest case we must use eretu synthetic instruction.
 	 */

-	ALTERNATIVE_2 "testb %al, %al; jz swapgs_restore_regs_and_return_to_usermode",\
-		"jmp swapgs_restore_regs_and_return_to_usermode", X86_FEATURE_XENPV,\
-		"jmp swapgs_restore_regs_and_return_to_usermode", X86_FEATURE_KVM_PVM_GUEST +	ALTERNATIVE "testb %al, %al; jz swapgs_restore_regs_and_return_to_usermode",\
+		"jmp swapgs_restore_regs_and_return_to_usermode", X86_FEATURE_PV_GUEST

 	/*
 	 * We win! This label is here just for ease of understanding
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-67-jiangshanlai::40gmail.com:1arch:x86:include:asm:cpufeatures.h) --git a/arch/x86/include/asm/cpufeatures.h b/arch/x86/include/asm/cpufeatures.h
index e17e72f13423..72ef58a2db19 100644
--- a/arch/x86/include/asm/cpufeatures.h
+++ b/arch/x86/include/asm/cpufeatures.h @@ -238,6 +238,7 @@
 #define X86_FEATURE_VCPUPREEMPT		( 8*32+21) /* "" PV vcpu_is_preempted function */
 #define X86_FEATURE_TDX_GUEST		( 8*32+22) /* Intel Trust Domain Extensions Guest */
 #define X86_FEATURE_KVM_PVM_GUEST	( 8*32+23) /* KVM Pagetable-based Virtual Machine guest */
+#define X86_FEATURE_PV_GUEST		( 8*32+24) /* "" Paravirtual guest */

 /* Intel-defined CPU features, CPUID level 0x00000007:0 (EBX), word 9 */
 #define X86_FEATURE_FSGSBASE		( 9*32+ 0) /* RDFSBASE, WRFSBASE, RDGSBASE, WRGSBASE instructions*/
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-67-jiangshanlai::40gmail.com:1arch:x86:include:asm:paravirt.h) --git a/arch/x86/include/asm/paravirt.h b/arch/x86/include/asm/paravirt.h
index deaee9ec575e..a864ee481ca2 100644
--- a/arch/x86/include/asm/paravirt.h
+++ b/arch/x86/include/asm/paravirt.h @@ -143,7 +143,7 @@ static __always_inline unsigned long read_cr2(void)
 {
 	return PVOP_ALT_CALLEE0(unsigned long, mmu.read_cr2,
 				"mov %%cr2, %%rax;",
-				ALT_NOT(X86_FEATURE_XENPV)); +				ALT_NOT(X86_FEATURE_PV_GUEST));
 }

 static __always_inline void write_cr2(unsigned long x)
@@ -154,13 +154,13 @@ static __always_inline void write_cr2(unsigned long x)
 static inline unsigned long __read_cr3(void)
 {
 	return PVOP_ALT_CALL0(unsigned long, mmu.read_cr3,
-			      "mov %%cr3, %%rax;", ALT_NOT(X86_FEATURE_XENPV)); +			      "mov %%cr3, %%rax;", ALT_NOT(X86_FEATURE_PV_GUEST));
 }

 static inline void write_cr3(unsigned long x)
 {
 	PVOP_ALT_VCALL1(mmu.write_cr3, x,
-			"mov %%rdi, %%cr3", ALT_NOT(X86_FEATURE_XENPV)); +			"mov %%rdi, %%cr3", ALT_NOT(X86_FEATURE_PV_GUEST));
 }

 static inline void __write_cr4(unsigned long x)
@@ -694,17 +694,17 @@ bool __raw_callee_save___native_vcpu_is_preempted(long cpu);
 static __always_inline unsigned long arch_local_save_flags(void)
 {
 	return PVOP_ALT_CALLEE0(unsigned long, irq.save_fl, "pushf; pop %%rax;",
-				ALT_NOT(X86_FEATURE_XENPV)); +				ALT_NOT(X86_FEATURE_PV_GUEST));
 }

 static __always_inline void arch_local_irq_disable(void)
 {
-	PVOP_ALT_VCALLEE0(irq.irq_disable, "cli;", ALT_NOT(X86_FEATURE_XENPV)); +	PVOP_ALT_VCALLEE0(irq.irq_disable, "cli;", ALT_NOT(X86_FEATURE_PV_GUEST));
 }

 static __always_inline void arch_local_irq_enable(void)
 {
-	PVOP_ALT_VCALLEE0(irq.irq_enable, "sti;", ALT_NOT(X86_FEATURE_XENPV)); +	PVOP_ALT_VCALLEE0(irq.irq_enable, "sti;", ALT_NOT(X86_FEATURE_PV_GUEST));
 }

 static __always_inline unsigned long arch_local_irq_save(void)
@@ -776,7 +776,7 @@ void native_pv_lock_init(void) __init;
 .endm

 #define SAVE_FLAGS	ALTERNATIVE "PARA_IRQ_save_fl;", "pushf; pop %rax;",\
-				    ALT_NOT(X86_FEATURE_XENPV) +				    ALT_NOT(X86_FEATURE_PV_GUEST)
 #endif
 #endif /* CONFIG_PARAVIRT_XXL */
 #endif	/* CONFIG_X86_64 */
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-67-jiangshanlai::40gmail.com:1arch:x86:kernel:pvm.c) --git a/arch/x86/kernel/pvm.c b/arch/x86/kernel/pvm.c
index c38e46a96ad3..d39550a8159f 100644
--- a/arch/x86/kernel/pvm.c
+++ b/arch/x86/kernel/pvm.c @@ -300,6 +300,7 @@ void __init pvm_early_setup(void)
 		return;

 	setup_force_cpu_cap(X86_FEATURE_KVM_PVM_GUEST);
+	setup_force_cpu_cap(X86_FEATURE_PV_GUEST);

 	wrmsrl(MSR_PVM_VCPU_STRUCT, __pa(this_cpu_ptr(&pvm_vcpu_struct)));
 	wrmsrl(MSR_PVM_EVENT_ENTRY, (unsigned long)(void *)pvm_early_kernel_event_entry - 256);
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-67-jiangshanlai::40gmail.com:1arch:x86:xen:enlighten_pv.c) --git a/arch/x86/xen/enlighten_pv.c b/arch/x86/xen/enlighten_pv.c
index aeb33e0a3f76..c56483051528 100644
--- a/arch/x86/xen/enlighten_pv.c
+++ b/arch/x86/xen/enlighten_pv.c @@ -335,6 +335,7 @@ static bool __init xen_check_xsave(void)
 static void __init xen_init_capabilities(void)
 {
 	setup_force_cpu_cap(X86_FEATURE_XENPV);
+	setup_force_cpu_cap(X86_FEATURE_PV_GUEST);
 	setup_clear_cpu_cap(X86_FEATURE_DCA);
 	setup_clear_cpu_cap(X86_FEATURE_APERFMPERF);
 	setup_clear_cpu_cap(X86_FEATURE_MTRR);
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m9d2e3415e38f877972ab77863f494706ffb770a0) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-67-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-67-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-67-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-67-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-67-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-67-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r9d2e3415e38f877972ab77863f494706ffb770a0)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ef306e82734c379c14ea3a038c5663661d4f4d56e) **[RFC PATCH 67/73] x86/pvm: Implement cpu related PVOPS**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(65 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r9d2e3415e38f877972ab77863f494706ffb770a0)
  2024-02-26 14:36 ` [[RFC PATCH 66/73] x86/pvm: Use new cpu feature to describe XENPV and PVM](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m9d2e3415e38f877972ab77863f494706ffb770a0) Lai Jiangshan
**@ 2024-02-26 14:36 ` Lai Jiangshan**
  2024-02-26 14:36 ` [[RFC PATCH 68/73] x86/pvm: Implement irq](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mb4741beb7127ff80a72ab793d185c8631dff405e) " Lai Jiangshan
                   ` [(7 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rb4741beb7127ff80a72ab793d185c8631dff405e)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rf306e82734c379c14ea3a038c5663661d4f4d56e)
From: Lai Jiangshan @ 2024-02-26 14:36 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-68-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-68-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143931)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143931), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

The MSR read/write operations are in the hot path, so use hypercalls in
their PVOPS to enhance performance. Additionally, it is important to
ensure that load_gs_index() and load_tls() notify the hypervisor in
their PVOPS.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/Kconfig](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-68-jiangshanlai::40gmail.com:1arch:x86:Kconfig)      |  1 +
 [arch/x86/kernel/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-68-jiangshanlai::40gmail.com:1arch:x86:kernel:pvm.c) | 85 +++++++++++++++++++++++++++++++++++++++++++
 2 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ef306e82734c379c14ea3a038c5663661d4f4d56e), 86 insertions(+)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-68-jiangshanlai::40gmail.com:1arch:x86:Kconfig) --git a/arch/x86/Kconfig b/arch/x86/Kconfig
index 32a2ab49752b..60e28727580a 100644
--- a/arch/x86/Kconfig
+++ b/arch/x86/Kconfig @@ -855,6 +855,7 @@ config PVM_GUEST
 	bool "PVM Guest support"
 	depends on X86_64 && KVM_GUEST && X86_PIE && !KASAN
 	select PAGE_TABLE_ISOLATION
+	select PARAVIRT_XXL
 	select RANDOMIZE_MEMORY
 	select RELOCATABLE_UNCOMPRESSED_KERNEL
 	default n
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-68-jiangshanlai::40gmail.com:1arch:x86:kernel:pvm.c) --git a/arch/x86/kernel/pvm.c b/arch/x86/kernel/pvm.c
index d39550a8159f..12a35bef9bb8 100644
--- a/arch/x86/kernel/pvm.c
+++ b/arch/x86/kernel/pvm.c @@ -73,6 +73,81 @@ static __always_inline long pvm_hypercall3(unsigned int nr, unsigned long p1,
 	return ret;
 }

+static void pvm_load_gs_index(unsigned int sel)
+{
+	if (sel & 4) {
+		pr_warn_once("pvm guest doesn't support LDT");
+		this_cpu_write(pvm_vcpu_struct.user_gsbase, 0);
+	} else {
+		unsigned long base;
+
+		preempt_disable();
+		base = pvm_hypercall1(PVM_HC_LOAD_GS, sel);
+		__this_cpu_write(pvm_vcpu_struct.user_gsbase, base);
+		preempt_enable();
+	}
+}
+
+static unsigned long long pvm_read_msr_safe(unsigned int msr, int *err)
+{
+	switch (msr) {
+	case MSR_FS_BASE:
+		*err = 0;
+		return rdfsbase();
+	case MSR_KERNEL_GS_BASE:
+		*err = 0;
+		return this_cpu_read(pvm_vcpu_struct.user_gsbase);
+	default:
+		return native_read_msr_safe(msr, err);
+	}
+}
+
+static unsigned long long pvm_read_msr(unsigned int msr)
+{
+	switch (msr) {
+	case MSR_FS_BASE:
+		return rdfsbase();
+	case MSR_KERNEL_GS_BASE:
+		return this_cpu_read(pvm_vcpu_struct.user_gsbase);
+	default:
+		return pvm_hypercall1(PVM_HC_RDMSR, msr);
+	}
+}
+
+static int notrace pvm_write_msr_safe(unsigned int msr, u32 low, u32 high)
+{
+	unsigned long base = ((u64)high << 32) | low;
+
+	switch (msr) {
+	case MSR_FS_BASE:
+		wrfsbase(base);
+		return 0;
+	case MSR_KERNEL_GS_BASE:
+		this_cpu_write(pvm_vcpu_struct.user_gsbase, base);
+		return 0;
+	default:
+		return pvm_hypercall2(PVM_HC_WRMSR, msr, base);
+	}
+}
+
+static void notrace pvm_write_msr(unsigned int msr, u32 low, u32 high)
+{
+	pvm_write_msr_safe(msr, low, high);
+}
+
+static void pvm_load_tls(struct thread_struct *t, unsigned int cpu)
+{
+	struct desc_struct *gdt = get_cpu_gdt_rw(cpu);
+	unsigned long *tls_array = (unsigned long *)gdt;
+
+	if (memcmp(&gdt[GDT_ENTRY_TLS_MIN], &t->tls_array[0], sizeof(t->tls_array))) {
+		native_load_tls(t, cpu);
+		pvm_hypercall3(PVM_HC_LOAD_TLS, tls_array[GDT_ENTRY_TLS_MIN],
+			       tls_array[GDT_ENTRY_TLS_MIN + 1],
+			       tls_array[GDT_ENTRY_TLS_MIN + 2]);
+	}
+}
+
 void __init pvm_early_event(struct pt_regs *regs)
 {
 	int vector = regs->orig_ax >> 32;
@@ -302,6 +377,16 @@ void __init pvm_early_setup(void)
 	setup_force_cpu_cap(X86_FEATURE_KVM_PVM_GUEST);
 	setup_force_cpu_cap(X86_FEATURE_PV_GUEST);

+	/* PVM takes care of %gs when switching to usermode for us */
+	pv_ops.cpu.load_gs_index = pvm_load_gs_index;
+	pv_ops.cpu.cpuid = pvm_cpuid;
+
+	pv_ops.cpu.read_msr = pvm_read_msr;
+	pv_ops.cpu.write_msr = pvm_write_msr;
+	pv_ops.cpu.read_msr_safe = pvm_read_msr_safe;
+	pv_ops.cpu.write_msr_safe = pvm_write_msr_safe;
+	pv_ops.cpu.load_tls = pvm_load_tls;
+
 	wrmsrl(MSR_PVM_VCPU_STRUCT, __pa(this_cpu_ptr(&pvm_vcpu_struct)));
 	wrmsrl(MSR_PVM_EVENT_ENTRY, (unsigned long)(void *)pvm_early_kernel_event_entry - 256);
 	wrmsrl(MSR_PVM_SUPERVISOR_REDZONE, PVM_SUPERVISOR_REDZONE_SIZE);
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mf306e82734c379c14ea3a038c5663661d4f4d56e) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-68-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-68-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-68-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-68-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-68-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-68-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rf306e82734c379c14ea3a038c5663661d4f4d56e)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#eb4741beb7127ff80a72ab793d185c8631dff405e) **[RFC PATCH 68/73] x86/pvm: Implement irq related PVOPS**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(66 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rf306e82734c379c14ea3a038c5663661d4f4d56e)
  2024-02-26 14:36 ` [[RFC PATCH 67/73] x86/pvm: Implement cpu related PVOPS](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mf306e82734c379c14ea3a038c5663661d4f4d56e) Lai Jiangshan
**@ 2024-02-26 14:36 ` Lai Jiangshan**
  2024-02-26 14:36 ` [[RFC PATCH 69/73] x86/pvm: Implement mmu](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m287a2f06f262269e2610cadf9db062ef5e5b89c1) " Lai Jiangshan
                   ` [(6 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r287a2f06f262269e2610cadf9db062ef5e5b89c1)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rb4741beb7127ff80a72ab793d185c8631dff405e)
From: Lai Jiangshan @ 2024-02-26 14:36 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-69-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-69-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143938)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143938), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Andy Lutomirski, Dave Hansen, H. Peter Anvin

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

The save_fl(), irq_enable(), and irq_disable() functions are in the hot
path, so the hypervisor shares the X86_EFLAG_IF status in the PVCS
structure for the guest kernel. This allows it to be read and modified
directly without a VM exit if there is no IRQ window request.
Additionally, the irq_halt() function remains the same, and a hypercall
is used in its PVOPS to enhance performance.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/entry/entry_64_pvm.S](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-69-jiangshanlai::40gmail.com:1arch:x86:entry:entry_64_pvm.S)   | 22 ++++++++++++++++++++++
 [arch/x86/include/asm/pvm_para.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-69-jiangshanlai::40gmail.com:1arch:x86:include:asm:pvm_para.h) |  3 +++
 [arch/x86/kernel/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-69-jiangshanlai::40gmail.com:1arch:x86:kernel:pvm.c)           | 10 ++++++++++
 3 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#eb4741beb7127ff80a72ab793d185c8631dff405e), 35 insertions(+)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-69-jiangshanlai::40gmail.com:1arch:x86:entry:entry_64_pvm.S) --git a/arch/x86/entry/entry_64_pvm.S b/arch/x86/entry/entry_64_pvm.S
index abb57e251e73..1d17bac2909a 100644
--- a/arch/x86/entry/entry_64_pvm.S
+++ b/arch/x86/entry/entry_64_pvm.S @@ -65,6 +65,28 @@ SYM_FUNC_START(pvm_hypercall)
 	popq	%r11
 	RET
 SYM_FUNC_END(pvm_hypercall)
+
+SYM_FUNC_START(pvm_save_fl)
+	movq	PER_CPU_VAR(pvm_vcpu_struct + PVCS_event_flags), %rax
+	RET
+SYM_FUNC_END(pvm_save_fl)
+
+SYM_FUNC_START(pvm_irq_disable)
+	btrq	$X86_EFLAGS_IF_BIT, PER_CPU_VAR(pvm_vcpu_struct + PVCS_event_flags)
+	RET
+SYM_FUNC_END(pvm_irq_disable)
+
+SYM_FUNC_START(pvm_irq_enable)
+	/* set X86_EFLAGS_IF */
+	orq	$X86_EFLAGS_IF, PER_CPU_VAR(pvm_vcpu_struct + PVCS_event_flags)
+	btq	$PVM_EVENT_FLAGS_IP_BIT, PER_CPU_VAR(pvm_vcpu_struct + PVCS_event_flags)
+	jc	.L_maybe_interrupt_pending
+	RET
+.L_maybe_interrupt_pending:
+	/* handle pending IRQ */
+	movq	$PVM_HC_IRQ_WIN, %rax
+	jmp	pvm_hypercall
+SYM_FUNC_END(pvm_irq_enable)
 .popsection

 /*
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-69-jiangshanlai::40gmail.com:1arch:x86:include:asm:pvm_para.h) --git a/arch/x86/include/asm/pvm_para.h b/arch/x86/include/asm/pvm_para.h
index f5d40a57c423..9484a1a23568 100644
--- a/arch/x86/include/asm/pvm_para.h
+++ b/arch/x86/include/asm/pvm_para.h @@ -95,6 +95,9 @@ void pvm_user_event_entry(void);
 void pvm_hypercall(void);
 void pvm_retu_rip(void);
 void pvm_rets_rip(void);
+void pvm_save_fl(void);
+void pvm_irq_disable(void);
+void pvm_irq_enable(void);
 #endif /* !__ASSEMBLY__ */

 #endif /* _ASM_X86_PVM_PARA_H */
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-69-jiangshanlai::40gmail.com:1arch:x86:kernel:pvm.c) --git a/arch/x86/kernel/pvm.c b/arch/x86/kernel/pvm.c
index 12a35bef9bb8..b4522947374d 100644
--- a/arch/x86/kernel/pvm.c
+++ b/arch/x86/kernel/pvm.c @@ -148,6 +148,11 @@ static void pvm_load_tls(struct thread_struct *t, unsigned int cpu)
 	}
 }

+static noinstr void pvm_safe_halt(void)
+{
+	pvm_hypercall0(PVM_HC_IRQ_HALT);
+}
+
 void __init pvm_early_event(struct pt_regs *regs)
 {
 	int vector = regs->orig_ax >> 32;
@@ -387,6 +392,11 @@ void __init pvm_early_setup(void)
 	pv_ops.cpu.write_msr_safe = pvm_write_msr_safe;
 	pv_ops.cpu.load_tls = pvm_load_tls;

+	pv_ops.irq.save_fl = __PV_IS_CALLEE_SAVE(pvm_save_fl);
+	pv_ops.irq.irq_disable = __PV_IS_CALLEE_SAVE(pvm_irq_disable);
+	pv_ops.irq.irq_enable = __PV_IS_CALLEE_SAVE(pvm_irq_enable);
+	pv_ops.irq.safe_halt = pvm_safe_halt;
+
 	wrmsrl(MSR_PVM_VCPU_STRUCT, __pa(this_cpu_ptr(&pvm_vcpu_struct)));
 	wrmsrl(MSR_PVM_EVENT_ENTRY, (unsigned long)(void *)pvm_early_kernel_event_entry - 256);
 	wrmsrl(MSR_PVM_SUPERVISOR_REDZONE, PVM_SUPERVISOR_REDZONE_SIZE);
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mb4741beb7127ff80a72ab793d185c8631dff405e) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-69-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-69-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-69-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-69-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-69-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-69-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rb4741beb7127ff80a72ab793d185c8631dff405e)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e287a2f06f262269e2610cadf9db062ef5e5b89c1) **[RFC PATCH 69/73] x86/pvm: Implement mmu related PVOPS**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(67 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rb4741beb7127ff80a72ab793d185c8631dff405e)
  2024-02-26 14:36 ` [[RFC PATCH 68/73] x86/pvm: Implement irq](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mb4741beb7127ff80a72ab793d185c8631dff405e) " Lai Jiangshan
**@ 2024-02-26 14:36 ` Lai Jiangshan**
  2024-02-26 14:36 ` [[RFC PATCH 70/73] x86/pvm: Don't use SWAPGS for gsbase read/write](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mdf41d4fe4b50256bc48f76328dc595756b850b19) Lai Jiangshan
                   ` [(5 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rdf41d4fe4b50256bc48f76328dc595756b850b19)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r287a2f06f262269e2610cadf9db062ef5e5b89c1)
From: Lai Jiangshan @ 2024-02-26 14:36 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-70-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-70-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143943)
  Cc: Lai Jiangshan, Hou Wenlong, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143943), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Lai Jiangshan <jiangshan.ljs@antgroup.com>

CR2 is passed directly in the event entry, allowing it to be read
directly in PVOPS. Additionally, write_cr3() for context switch needs to
notify the hypervisor in its PVOPS. For performance reasons, TLB-related
PVOPS utilize hypercalls.

Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
---
 [arch/x86/kernel/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-70-jiangshanlai::40gmail.com:1arch:x86:kernel:pvm.c) | 56 +++++++++++++++++++++++++++++++++++++++++++
 1 file [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e287a2f06f262269e2610cadf9db062ef5e5b89c1), 56 insertions(+)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-70-jiangshanlai::40gmail.com:1arch:x86:kernel:pvm.c) --git a/arch/x86/kernel/pvm.c b/arch/x86/kernel/pvm.c
index b4522947374d..1dc2c0fb7daa 100644
--- a/arch/x86/kernel/pvm.c
+++ b/arch/x86/kernel/pvm.c @@ -21,6 +21,7 @@
 #include <asm/traps.h>

 DEFINE_PER_CPU_PAGE_ALIGNED(struct pvm_vcpu_struct, pvm_vcpu_struct);
+static DEFINE_PER_CPU(unsigned long, pvm_guest_cr3);

 unsigned long pvm_range_start __initdata;
 unsigned long pvm_range_end __initdata;
@@ -153,6 +154,52 @@ static noinstr void pvm_safe_halt(void)
 	pvm_hypercall0(PVM_HC_IRQ_HALT);
 }

+static noinstr unsigned long pvm_read_cr2(void)
+{
+	return this_cpu_read(pvm_vcpu_struct.cr2);
+}
+
+static noinstr void pvm_write_cr2(unsigned long cr2)
+{
+	native_write_cr2(cr2);
+	this_cpu_write(pvm_vcpu_struct.cr2, cr2);
+}
+
+static unsigned long pvm_read_cr3(void)
+{
+	return this_cpu_read(pvm_guest_cr3);
+}
+
+static unsigned long pvm_user_pgd(unsigned long pgd)
+{
+	return pgd | BIT(PTI_PGTABLE_SWITCH_BIT) | BIT(X86_CR3_PTI_PCID_USER_BIT);
+}
+
+static void pvm_write_cr3(unsigned long val)
+{
+	/* Convert CR3_NO_FLUSH bit to hypercall flags. */
+	unsigned long flags = ~val >> 63;
+	unsigned long pgd = val & ~X86_CR3_PCID_NOFLUSH;
+
+	this_cpu_write(pvm_guest_cr3, pgd);
+	pvm_hypercall3(PVM_HC_LOAD_PGTBL, flags, pgd, pvm_user_pgd(pgd));
+}
+
+static void pvm_flush_tlb_user(void)
+{
+	pvm_hypercall0(PVM_HC_TLB_FLUSH_CURRENT);
+}
+
+static void pvm_flush_tlb_kernel(void)
+{
+	pvm_hypercall0(PVM_HC_TLB_FLUSH);
+}
+
+static void pvm_flush_tlb_one_user(unsigned long addr)
+{
+	pvm_hypercall1(PVM_HC_TLB_INVLPG, addr);
+}
+
 void __init pvm_early_event(struct pt_regs *regs)
 {
 	int vector = regs->orig_ax >> 32;
@@ -397,6 +444,15 @@ void __init pvm_early_setup(void)
 	pv_ops.irq.irq_enable = __PV_IS_CALLEE_SAVE(pvm_irq_enable);
 	pv_ops.irq.safe_halt = pvm_safe_halt;

+	this_cpu_write(pvm_guest_cr3, __native_read_cr3());
+	pv_ops.mmu.read_cr2 = __PV_IS_CALLEE_SAVE(pvm_read_cr2);
+	pv_ops.mmu.write_cr2 = pvm_write_cr2;
+	pv_ops.mmu.read_cr3 = pvm_read_cr3;
+	pv_ops.mmu.write_cr3 = pvm_write_cr3;
+	pv_ops.mmu.flush_tlb_user = pvm_flush_tlb_user;
+	pv_ops.mmu.flush_tlb_kernel = pvm_flush_tlb_kernel;
+	pv_ops.mmu.flush_tlb_one_user = pvm_flush_tlb_one_user;
+
 	wrmsrl(MSR_PVM_VCPU_STRUCT, __pa(this_cpu_ptr(&pvm_vcpu_struct)));
 	wrmsrl(MSR_PVM_EVENT_ENTRY, (unsigned long)(void *)pvm_early_kernel_event_entry - 256);
 	wrmsrl(MSR_PVM_SUPERVISOR_REDZONE, PVM_SUPERVISOR_REDZONE_SIZE);
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m287a2f06f262269e2610cadf9db062ef5e5b89c1) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-70-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-70-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-70-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-70-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-70-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-70-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r287a2f06f262269e2610cadf9db062ef5e5b89c1)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#edf41d4fe4b50256bc48f76328dc595756b850b19) **[RFC PATCH 70/73] x86/pvm: Don't use SWAPGS for gsbase read/write**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(68 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r287a2f06f262269e2610cadf9db062ef5e5b89c1)
  2024-02-26 14:36 ` [[RFC PATCH 69/73] x86/pvm: Implement mmu](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m287a2f06f262269e2610cadf9db062ef5e5b89c1) " Lai Jiangshan
**@ 2024-02-26 14:36 ` Lai Jiangshan**
  2024-02-26 14:36 ` [[RFC PATCH 71/73] x86/pvm: Adapt pushf/popf in this_cpu_cmpxchg16b_emu()](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mfa18bdf43e5a1f7b7b4bb426168497c30c3b01bd) Lai Jiangshan
                   ` [(4 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rfa18bdf43e5a1f7b7b4bb426168497c30c3b01bd)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rdf41d4fe4b50256bc48f76328dc595756b850b19)
From: Lai Jiangshan @ 2024-02-26 14:36 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-71-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-71-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143947)
  Cc: Hou Wenlong, Lai Jiangshan, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143947), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin, Kirill A. Shutemov, Mike Rapoport,
	Rick Edgecombe

From: Hou Wenlong <houwenlong.hwl@antgroup.com>

On PVM guest, SWAPGS doesn't work. So let __rdgsbase_inactive() and
__wrgsbase_inactive() to use rdmsrl()/wrmsrl() on PVM guest.

Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
---
 [arch/x86/kernel/process_64.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-71-jiangshanlai::40gmail.com:1arch:x86:kernel:process_64.c) | 10 ++++++----
 1 file [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#edf41d4fe4b50256bc48f76328dc595756b850b19), 6 insertions(+), 4 deletions(-)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-71-jiangshanlai::40gmail.com:1arch:x86:kernel:process_64.c) --git a/arch/x86/kernel/process_64.c b/arch/x86/kernel/process_64.c
index 33b268747bb7..9a56bcef515e 100644
--- a/arch/x86/kernel/process_64.c
+++ b/arch/x86/kernel/process_64.c @@ -157,7 +157,7 @@ enum which_selector {
  * traced or probed than any access to a per CPU variable happens with
  * the wrong GS.
  *
- * It is not used on Xen paravirt. When paravirt support is needed, it + * It is not used on Xen/PVM paravirt. When paravirt support is needed, it
  * needs to be renamed with native_ prefix.
  */
 static noinstr unsigned long __rdgsbase_inactive(void)
@@ -166,7 +166,8 @@ static noinstr unsigned long __rdgsbase_inactive(void)

 	lockdep_assert_irqs_disabled();

-	if (!cpu_feature_enabled(X86_FEATURE_XENPV)) { +	if (!cpu_feature_enabled(X86_FEATURE_XENPV) &&
+	    !cpu_feature_enabled(X86_FEATURE_KVM_PVM_GUEST)) {
 		native_swapgs();
 		gsbase = rdgsbase();
 		native_swapgs();
@@ -184,14 +185,15 @@ static noinstr unsigned long __rdgsbase_inactive(void)
  * traced or probed than any access to a per CPU variable happens with
  * the wrong GS.
  *
- * It is not used on Xen paravirt. When paravirt support is needed, it + * It is not used on Xen/PVM paravirt. When paravirt support is needed, it
  * needs to be renamed with native_ prefix.
  */
 static noinstr void __wrgsbase_inactive(unsigned long gsbase)
 {
 	lockdep_assert_irqs_disabled();

-	if (!cpu_feature_enabled(X86_FEATURE_XENPV)) { +	if (!cpu_feature_enabled(X86_FEATURE_XENPV) &&
+	    !cpu_feature_enabled(X86_FEATURE_KVM_PVM_GUEST)) {
 		native_swapgs();
 		wrgsbase(gsbase);
 		native_swapgs();
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mdf41d4fe4b50256bc48f76328dc595756b850b19) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-71-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-71-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-71-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-71-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-71-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-71-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rdf41d4fe4b50256bc48f76328dc595756b850b19)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#efa18bdf43e5a1f7b7b4bb426168497c30c3b01bd) **[RFC PATCH 71/73] x86/pvm: Adapt pushf/popf in this_cpu_cmpxchg16b_emu()**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(69 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rdf41d4fe4b50256bc48f76328dc595756b850b19)
  2024-02-26 14:36 ` [[RFC PATCH 70/73] x86/pvm: Don't use SWAPGS for gsbase read/write](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mdf41d4fe4b50256bc48f76328dc595756b850b19) Lai Jiangshan
**@ 2024-02-26 14:36 ` Lai Jiangshan**
  2024-02-26 14:36 ` [[RFC PATCH 72/73] x86/pvm: Use RDTSCP as default in vdso_read_cpunode()](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mef68ca04da4883790fbd04377f306afc713ac6f6) Lai Jiangshan
                   ` [(3 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ref68ca04da4883790fbd04377f306afc713ac6f6)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rfa18bdf43e5a1f7b7b4bb426168497c30c3b01bd)
From: Lai Jiangshan @ 2024-02-26 14:36 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-72-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-72-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143952)
  Cc: Hou Wenlong, Lai Jiangshan, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143952), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin

From: Hou Wenlong <houwenlong.hwl@antgroup.com>

The pushf/popf instructions in this_cpu_cmpxchg16b_emu() are
non-privilege instructions, so they cannot be trapped and emulated,
which could cause a boot failure. However, since the cmpxchg16b
instruction is supported for PVM guest. we can patch
this_cpu_cmpxchg16b_emu() and use cmpxchg16b directly.

Suggested-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
---
 [arch/x86/kernel/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-72-jiangshanlai::40gmail.com:1arch:x86:kernel:pvm.c) | 30 ++++++++++++++++++++++++++++++
 1 file [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#efa18bdf43e5a1f7b7b4bb426168497c30c3b01bd), 30 insertions(+)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-72-jiangshanlai::40gmail.com:1arch:x86:kernel:pvm.c) --git a/arch/x86/kernel/pvm.c b/arch/x86/kernel/pvm.c
index 1dc2c0fb7daa..567ea19d569c 100644
--- a/arch/x86/kernel/pvm.c
+++ b/arch/x86/kernel/pvm.c @@ -413,6 +413,34 @@ __visible noinstr void pvm_event(struct pt_regs *regs)
 		common_interrupt(regs, vector);
 }

+asm (
+	".pushsection .rodata				\n"
+	".global pvm_cmpxchg16b_emu_template		\n"
+	"pvm_cmpxchg16b_emu_template:			\n"
+	"	cmpxchg16b %gs:(%rsi)			\n"
+	"	ret					\n"
+	".global pvm_cmpxchg16b_emu_tail		\n"
+	"pvm_cmpxchg16b_emu_tail:			\n"
+	".popsection					\n"
+);
+
+extern u8 this_cpu_cmpxchg16b_emu[];
+extern u8 pvm_cmpxchg16b_emu_template[];
+extern u8 pvm_cmpxchg16b_emu_tail[];
+
+static void __init pvm_early_patch(void)
+{
+	/*
+	 * The pushf/popf instructions in this_cpu_cmpxchg16b_emu() are
+	 * non-privilege instructions, so they cannot be trapped and emulated,
+	 * which could cause a boot failure. However, since the cmpxchg16b
+	 * instruction is supported for PVM guest. we can patch
+	 * this_cpu_cmpxchg16b_emu() and use cmpxchg16b directly.
+	 */
+	memcpy(this_cpu_cmpxchg16b_emu, pvm_cmpxchg16b_emu_template,
+	       (unsigned int)(pvm_cmpxchg16b_emu_tail - pvm_cmpxchg16b_emu_template));
+}
+
 extern void pvm_early_kernel_event_entry(void);

 /*
@@ -457,6 +485,8 @@ void __init pvm_early_setup(void)
 	wrmsrl(MSR_PVM_EVENT_ENTRY, (unsigned long)(void *)pvm_early_kernel_event_entry - 256);
 	wrmsrl(MSR_PVM_SUPERVISOR_REDZONE, PVM_SUPERVISOR_REDZONE_SIZE);
 	wrmsrl(MSR_PVM_RETS_RIP, (unsigned long)(void *)pvm_rets_rip);
+
+	pvm_early_patch();
 }

 void pvm_setup_event_handling(void)
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mfa18bdf43e5a1f7b7b4bb426168497c30c3b01bd) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-72-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-72-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-72-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-72-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-72-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-72-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rfa18bdf43e5a1f7b7b4bb426168497c30c3b01bd)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#eef68ca04da4883790fbd04377f306afc713ac6f6) **[RFC PATCH 72/73] x86/pvm: Use RDTSCP as default in vdso_read_cpunode()**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(70 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#rfa18bdf43e5a1f7b7b4bb426168497c30c3b01bd)
  2024-02-26 14:36 ` [[RFC PATCH 71/73] x86/pvm: Adapt pushf/popf in this_cpu_cmpxchg16b_emu()](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mfa18bdf43e5a1f7b7b4bb426168497c30c3b01bd) Lai Jiangshan
**@ 2024-02-26 14:36 ` Lai Jiangshan**
  2024-02-26 14:36 ` [[RFC PATCH 73/73] x86/pvm: Disable some unsupported syscalls and features](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m4a887cf49752ba5740fdb806b5c648135c88c0ca) Lai Jiangshan
                   ` [(2 subsequent siblings)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r4a887cf49752ba5740fdb806b5c648135c88c0ca)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ref68ca04da4883790fbd04377f306afc713ac6f6)
From: Lai Jiangshan @ 2024-02-26 14:36 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-73-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-73-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226143957)
  Cc: Hou Wenlong, Lai Jiangshan, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226143957), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Dave Hansen, H. Peter Anvin, Sami Tolvanen, Fangrui Song,
	Willy Tarreau, Thomas Garnier, Josh Poimboeuf, Xin Li

From: Hou Wenlong <houwenlong.hwl@antgroup.com>

The CPUNODE description of the guest cannot be installed into the host's
GDT, as this index is also used for the host to retrieve the current CPU
in paranoid entry. As a result, LSL in vdso_read_cpunode() does not work
correctly for the PVM guest. To address this issue, use RDTSCP as the
default in vdso_read_cpunode(), as it is supported by the hypervisor.

Suggested-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
---
 [arch/x86/include/asm/alternative.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-73-jiangshanlai::40gmail.com:1arch:x86:include:asm:alternative.h) | 14 ++++++++++++++
 [arch/x86/include/asm/segment.h](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-73-jiangshanlai::40gmail.com:1arch:x86:include:asm:segment.h)     | 14 ++++++++++----
 2 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#eef68ca04da4883790fbd04377f306afc713ac6f6), 24 insertions(+), 4 deletions(-)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-73-jiangshanlai::40gmail.com:1arch:x86:include:asm:alternative.h) --git a/arch/x86/include/asm/alternative.h b/arch/x86/include/asm/alternative.h
index cf4b236b47a3..caebb49c5d61 100644
--- a/arch/x86/include/asm/alternative.h
+++ b/arch/x86/include/asm/alternative.h @@ -299,6 +299,20 @@ static inline int alternatives_text_reserved(void *start, void *end)
 	asm_inline volatile (ALTERNATIVE(oldinstr, newinstr, ft_flags)\
 		: output : "i" (0), ## input)

+/*
+ * This is similar to alternative_io. But it has two features and
+ * respective instructions.
+ *
+ * If CPU has feature2, newinstr2 is used.
+ * Otherwise, if CPU has feature1, newinstr1 is used.
+ * Otherwise, oldinstr is used.
+ */
+#define alternative_io_2(oldinstr, newinstr1, ft_flags1, newinstr2,\
+			 ft_flags2, output, input...)\
+	asm_inline volatile (ALTERNATIVE_2(oldinstr, newinstr1, ft_flags1,\
+		newinstr2, ft_flags2)\
+		: output : "i" (0), ## input)
+
 /* Like alternative_io, but for replacing a direct call with another one. */
 #define alternative_call(oldfunc, newfunc, ft_flags, output, input...)\
 	asm_inline volatile (ALTERNATIVE("call %P[old]", "call %P[new]", ft_flags)\
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-73-jiangshanlai::40gmail.com:1arch:x86:include:asm:segment.h) --git a/arch/x86/include/asm/segment.h b/arch/x86/include/asm/segment.h
index 9d6411c65920..555966922e8f 100644
--- a/arch/x86/include/asm/segment.h
+++ b/arch/x86/include/asm/segment.h @@ -253,11 +253,17 @@ static inline void vdso_read_cpunode(unsigned *cpu, unsigned *node)
 	 * hoisting it out of the calling function.
 	 *
 	 * If RDPID is available, use it.
+	 *
+	 * If it is PVM guest and RDPID is not available, use RDTSCP.
 	 */
-	alternative_io ("lsl %[seg],%[p]",
-			".byte 0xf3,0x0f,0xc7,0xf8", /* RDPID %eax/rax */
-			X86_FEATURE_RDPID,
-			[p] "=a" (p), [seg] "r" (__CPUNODE_SEG)); +	alternative_io_2("lsl %[seg],%[p]",
+			 ".byte 0x0f,0x01,0xf9\n\t" /* RDTSCP %eax:%edx, %ecx */
+			 "mov %%ecx,%%eax\n\t",
+			 X86_FEATURE_KVM_PVM_GUEST,
+			 ".byte 0xf3,0x0f,0xc7,0xf8", /* RDPID %eax/rax */
+			 X86_FEATURE_RDPID,
+			 [p] "=a" (p), [seg] "r" (__CPUNODE_SEG)
+			 : "cx", "dx");

 	if (cpu)
 		*cpu = (p & VDSO_CPUNODE_MASK);
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mef68ca04da4883790fbd04377f306afc713ac6f6) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-73-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-73-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-73-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-73-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-73-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-73-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ref68ca04da4883790fbd04377f306afc713ac6f6)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e4a887cf49752ba5740fdb806b5c648135c88c0ca) **[RFC PATCH 73/73] x86/pvm: Disable some unsupported syscalls and features**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(71 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ref68ca04da4883790fbd04377f306afc713ac6f6)
  2024-02-26 14:36 ` [[RFC PATCH 72/73] x86/pvm: Use RDTSCP as default in vdso_read_cpunode()](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mef68ca04da4883790fbd04377f306afc713ac6f6) Lai Jiangshan
**@ 2024-02-26 14:36 ` Lai Jiangshan**
  2024-02-26 14:49 ` [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m472a265ead19356ad7c61fa702ce5ad7dfb56312) Paolo Bonzini
  2024-03-06 11:05 ` [Like Xu](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m556764c73fec9ccbc2e8f38afce91448d4254fb9)
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r4a887cf49752ba5740fdb806b5c648135c88c0ca)
From: Lai Jiangshan @ 2024-02-26 14:36 UTC ([permalink](https://lore.kernel.org/lkml/20240226143630.33643-74-jiangshanlai@gmail.com/) / [raw](https://lore.kernel.org/lkml/20240226143630.33643-74-jiangshanlai@gmail.com/raw))
  To: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226144005)
  Cc: Hou Wenlong, Lai Jiangshan, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226144005), Paolo Bonzini, x86, Kees Cook, Juergen Gross,
	Andy Lutomirski, Dave Hansen, H. Peter Anvin, Kirill A. Shutemov,
	Andrew Morton, Hugh Dickins

From: Hou Wenlong <houwenlong.hwl@antgroup.com>

n the PVM guest, the LDT won't be loaded into hardware, rendering it
ineffective. Consequently, the modify_ldt() syscall should be disabled.
Additionally, the VSYSCALL address is not within the allowed address
range, making full emulation of the vsyscall page unsupported in the PVM
guest. It is recommended to use XONLY mode instead. Furthermore,
SYSENTER (Intel) and SYSCALL32 (AMD) are not supported by the
hypervisor, so they should not be used in VDSO.

Suggested-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
Signed-off-by: Hou Wenlong <houwenlong.hwl@antgroup.com>
Signed-off-by: Lai Jiangshan <jiangshan.ljs@antgroup.com>
---
 [arch/x86/entry/vsyscall/vsyscall_64.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-74-jiangshanlai::40gmail.com:1arch:x86:entry:vsyscall:vsyscall_64.c) | 4 ++++
 [arch/x86/kernel/ldt.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-74-jiangshanlai::40gmail.com:1arch:x86:kernel:ldt.c)                 | 3 +++
 [arch/x86/kernel/pvm.c](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#Z2e.:..:20240226143630.33643-74-jiangshanlai::40gmail.com:1arch:x86:kernel:pvm.c)                 | 4 ++++
 3 files [changed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e4a887cf49752ba5740fdb806b5c648135c88c0ca), 11 insertions(+)

[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-74-jiangshanlai::40gmail.com:1arch:x86:entry:vsyscall:vsyscall_64.c) --git a/arch/x86/entry/vsyscall/vsyscall_64.c b/arch/x86/entry/vsyscall/vsyscall_64.c
index f469f8dc36d4..dc6bc7fb490e 100644
--- a/arch/x86/entry/vsyscall/vsyscall_64.c
+++ b/arch/x86/entry/vsyscall/vsyscall_64.c @@ -378,6 +378,10 @@ void __init map_vsyscall(void)
 	extern char __vsyscall_page;
 	unsigned long physaddr_vsyscall = __pa_symbol(&__vsyscall_page);

+	/* Full emulation is not supported in PVM guest, use XONLY instead. */
+	if (vsyscall_mode == EMULATE && boot_cpu_has(X86_FEATURE_KVM_PVM_GUEST))
+		vsyscall_mode = XONLY;
+
 	/*
 	 * For full emulation, the page needs to exist for real.  In
 	 * execute-only mode, there is no PTE at all backing the vsyscall
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-74-jiangshanlai::40gmail.com:1arch:x86:kernel:ldt.c) --git a/arch/x86/kernel/ldt.c b/arch/x86/kernel/ldt.c
index adc67f98819a..d75815491d7e 100644
--- a/arch/x86/kernel/ldt.c
+++ b/arch/x86/kernel/ldt.c @@ -669,6 +669,9 @@ SYSCALL_DEFINE3(modify_ldt, int , func , void __user * , ptr ,
 {
 	int ret = -ENOSYS;

+	if (cpu_feature_enabled(X86_FEATURE_KVM_PVM_GUEST))
+		return (unsigned int)ret;
+
 	switch (func) {
 	case 0:
 		ret = read_ldt(ptr, bytecount);
[diff](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#iZ2e.:..:20240226143630.33643-74-jiangshanlai::40gmail.com:1arch:x86:kernel:pvm.c) --git a/arch/x86/kernel/pvm.c b/arch/x86/kernel/pvm.c
index 567ea19d569c..b172bd026594 100644
--- a/arch/x86/kernel/pvm.c
+++ b/arch/x86/kernel/pvm.c @@ -457,6 +457,10 @@ void __init pvm_early_setup(void)
 	setup_force_cpu_cap(X86_FEATURE_KVM_PVM_GUEST);
 	setup_force_cpu_cap(X86_FEATURE_PV_GUEST);

+	/* Don't use SYSENTER (Intel) and SYSCALL32 (AMD) in vdso. */
+	setup_clear_cpu_cap(X86_FEATURE_SYSENTER32);
+	setup_clear_cpu_cap(X86_FEATURE_SYSCALL32);
+
 	/* PVM takes care of %gs when switching to usermode for us */
 	pv_ops.cpu.load_gs_index = pvm_load_gs_index;
 	pv_ops.cpu.cpuid = pvm_cpuid;
--
2.19.1.6.gb485710b

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m4a887cf49752ba5740fdb806b5c648135c88c0ca) [permalink](https://lore.kernel.org/lkml/20240226143630.33643-74-jiangshanlai@gmail.com/) [raw](https://lore.kernel.org/lkml/20240226143630.33643-74-jiangshanlai@gmail.com/raw) [reply](https://lore.kernel.org/lkml/20240226143630.33643-74-jiangshanlai@gmail.com/#R) [related](https://lore.kernel.org/lkml/20240226143630.33643-74-jiangshanlai@gmail.com/#related)	[[**flat**](https://lore.kernel.org/lkml/20240226143630.33643-74-jiangshanlai@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/20240226143630.33643-74-jiangshanlai@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r4a887cf49752ba5740fdb806b5c648135c88c0ca)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e472a265ead19356ad7c61fa702ce5ad7dfb56312) **Re: [RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(72 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r4a887cf49752ba5740fdb806b5c648135c88c0ca)
  2024-02-26 14:36 ` [[RFC PATCH 73/73] x86/pvm: Disable some unsupported syscalls and features](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m4a887cf49752ba5740fdb806b5c648135c88c0ca) Lai Jiangshan
**@ 2024-02-26 14:49 ` Paolo Bonzini**
  2024-02-27 17:27   ` [Sean Christopherson](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m1eed65dc0f579f488a7f37bee71500eb13abfbca)
  2024-02-29 14:55   ` [Lai Jiangshan](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m86efe1ee03a1b357432e094e98efaf53215b2a68)
  2024-03-06 11:05 ` [Like Xu](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m556764c73fec9ccbc2e8f38afce91448d4254fb9)
  [74 siblings, 2 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r472a265ead19356ad7c61fa702ce5ad7dfb56312)
From: Paolo Bonzini @ 2024-02-26 14:49 UTC ([permalink](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/) / [raw](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/raw))
  To: Lai Jiangshan
  Cc: [linux-kernel](https://lore.kernel.org/lkml/?t=20240226144951), Lai Jiangshan, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240226144951), x86, Kees Cook, Juergen Gross, Hou Wenlong

On Mon, Feb 26, 2024 at 3:34 PM Lai Jiangshan <jiangshanlai@gmail.com> wrote:
> - Full control: In XENPV/Lguest, the host Linux (dom0) entry code is
>   subordinate to the hypervisor/switcher, and the host Linux kernel
>   loses control over the entry code. This can cause inconvenience if
>   there is a need to update something when there is a bug in the
>   switcher or hardware.  Integral entry gives the control back to the
>   host kernel.
>
> - Zero overhead incurred: The integrated entry code doesn't cause any
>   overhead in host Linux entry path, thanks to the discreet design with
>   PVM code in the switcher, where the PVM path is bypassed on host events.
>   While in XENPV/Lguest, host events must be handled by the
>   hypervisor/switcher before being processed.
Lguest... Now that's a name I haven't heard in a long time. :)  To be
honest, it's a bit weird to see yet another PV hypervisor. I think
what really killed Xen PV was the impossibility to protect from
various speculation side channel attacks, and I would like to
understand how PVM fares here.

You obviously did a great job in implementing this within the KVM
framework; the changes in arch/x86/ are impressively small. On the
other hand this means it's also not really my call to decide whether
this is suitable for merging upstream. The bulk of the changes are
really in arch/x86/kernel/ and arch/x86/entry/, and those are well
outside my maintenance area.

Paolo

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m472a265ead19356ad7c61fa702ce5ad7dfb56312) [permalink](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/) [raw](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/raw) [reply](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/#R)	[[**flat**](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r472a265ead19356ad7c61fa702ce5ad7dfb56312)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e934efcc0c018f5e064353a9160954ce7103b69e3) **Re: [RFC PATCH 15/73] mm/vmalloc: Add a helper to reserve a contiguous and aligned kernel virtual area**
  2024-02-26 14:35 ` [[RFC PATCH 15/73] mm/vmalloc: Add a helper to reserve a contiguous and aligned kernel virtual area](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ma2decc0d9dda7301af1d323411463772c3ee3e15) Lai Jiangshan
**@ 2024-02-27 14:56   ` Christoph Hellwig**
  2024-02-27 17:07     ` [Lai Jiangshan](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#me17e5aa8923b210192c673fa1b09192ac6acf7d6)
  [0 siblings, 1 reply; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r934efcc0c018f5e064353a9160954ce7103b69e3)
From: Christoph Hellwig @ 2024-02-27 14:56 UTC ([permalink](https://lore.kernel.org/lkml/Zd34GHtHlnpPtg5v@infradead.org/) / [raw](https://lore.kernel.org/lkml/Zd34GHtHlnpPtg5v@infradead.org/raw))
  To: Lai Jiangshan
  Cc: [linux-kernel](https://lore.kernel.org/lkml/?t=20240227145627), Hou Wenlong, Lai Jiangshan, Linus Torvalds,
	Peter Zijlstra, Sean Christopherson, Thomas Gleixner,
	Borislav Petkov, Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240227145627), Paolo Bonzini, x86, Kees Cook,
	Juergen Gross, Andrew Morton, Uladzislau Rezki, Christoph Hellwig,
	Lorenzo Stoakes, [linux-mm](https://lore.kernel.org/linux-mm/?t=20240227145627)

On Mon, Feb 26, 2024 at 10:35:32PM +0800, Lai Jiangshan wrote:
> From: Hou Wenlong <houwenlong.hwl@antgroup.com>
>
> PVM needs to reserve a contiguous and aligned kernel virtual area for
Who is "PVM", and why does it need aligned virtual memory space?

> +extern struct vm_struct *get_vm_area_align(unsigned long size, unsigned long align,
No need for the extern here.

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m934efcc0c018f5e064353a9160954ce7103b69e3) [permalink](https://lore.kernel.org/lkml/Zd34GHtHlnpPtg5v@infradead.org/) [raw](https://lore.kernel.org/lkml/Zd34GHtHlnpPtg5v@infradead.org/raw) [reply](https://lore.kernel.org/lkml/Zd34GHtHlnpPtg5v@infradead.org/#R)	[[**flat**](https://lore.kernel.org/lkml/Zd34GHtHlnpPtg5v@infradead.org/T/#u)|[nested](https://lore.kernel.org/lkml/Zd34GHtHlnpPtg5v@infradead.org/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r934efcc0c018f5e064353a9160954ce7103b69e3)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ee17e5aa8923b210192c673fa1b09192ac6acf7d6) **Re: [RFC PATCH 15/73] mm/vmalloc: Add a helper to reserve a contiguous and aligned kernel virtual area**
  2024-02-27 14:56   ` [Christoph Hellwig](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m934efcc0c018f5e064353a9160954ce7103b69e3)
**@ 2024-02-27 17:07     ` Lai Jiangshan**
  [0 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#re17e5aa8923b210192c673fa1b09192ac6acf7d6)
From: Lai Jiangshan @ 2024-02-27 17:07 UTC ([permalink](https://lore.kernel.org/lkml/CAJhGHyDdsm3BT4fL3Z_H5-_m4VpDi9FnG6GCcrup6YfMr_MBCw@mail.gmail.com/) / [raw](https://lore.kernel.org/lkml/CAJhGHyDdsm3BT4fL3Z_H5-_m4VpDi9FnG6GCcrup6YfMr_MBCw@mail.gmail.com/raw))
  To: Christoph Hellwig
  Cc: [linux-kernel](https://lore.kernel.org/lkml/?t=20240227170752), Hou Wenlong, Lai Jiangshan, Linus Torvalds,
	Peter Zijlstra, Sean Christopherson, Thomas Gleixner,
	Borislav Petkov, Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240227170752), Paolo Bonzini, x86, Kees Cook,
	Juergen Gross, Andrew Morton, Uladzislau Rezki, Lorenzo Stoakes,
	[linux-mm](https://lore.kernel.org/linux-mm/?t=20240227170752)

Hello

On Tue, Feb 27, 2024 at 10:56 PM Christoph Hellwig <hch@infradead.org> wrote:
>
> On Mon, Feb 26, 2024 at 10:35:32PM +0800, Lai Jiangshan wrote:
> > From: Hou Wenlong <houwenlong.hwl@antgroup.com>
> >
> > PVM needs to reserve a contiguous and aligned kernel virtual area for
>
> Who is "PVM", and why does it need aligned virtual memory space?
PVM stands for Pagetable-based Virtual Machine. It is a new pure
software-implemented virtualization solution. The details are in the
cover letter:
<https://lore.kernel.org/lkml/20240226143630.33643-1-jiangshanlai@gmail.com/>

I'm sorry for not CC'ing you on the cover letter (I haven't made/found a proper
script to generate all cc-recipients for the cover letter.) nor elaborating
the reason in the changelog.

One of the core designs in PVM is the "Exclusive address space separation",
with which in the higher half of the address spaces (where the most significant
bits in the addresses are 1s), the address ranges that a PVM guest is
allowed are exclusive from the host kernel.  So PVM hypervisor has to use
get_vm_area_align() to reserve a huge range (normally 16T) with the
alignment 512G (PGDIR_SIZE) for all the guests to accommodate the
whole guest kernel space. The reserved range cannot be used by the
host.

The rationale of this core design is also in the cover letter.

Thanks
Lai

>
> > +extern struct vm_struct *get_vm_area_align(unsigned long size, unsigned long align,
>
> No need for the extern here.
>
[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#me17e5aa8923b210192c673fa1b09192ac6acf7d6) [permalink](https://lore.kernel.org/lkml/CAJhGHyDdsm3BT4fL3Z_H5-_m4VpDi9FnG6GCcrup6YfMr_MBCw@mail.gmail.com/) [raw](https://lore.kernel.org/lkml/CAJhGHyDdsm3BT4fL3Z_H5-_m4VpDi9FnG6GCcrup6YfMr_MBCw@mail.gmail.com/raw) [reply](https://lore.kernel.org/lkml/CAJhGHyDdsm3BT4fL3Z_H5-_m4VpDi9FnG6GCcrup6YfMr_MBCw@mail.gmail.com/#R)	[[**flat**](https://lore.kernel.org/lkml/CAJhGHyDdsm3BT4fL3Z_H5-_m4VpDi9FnG6GCcrup6YfMr_MBCw@mail.gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/CAJhGHyDdsm3BT4fL3Z_H5-_m4VpDi9FnG6GCcrup6YfMr_MBCw@mail.gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#re17e5aa8923b210192c673fa1b09192ac6acf7d6)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e1eed65dc0f579f488a7f37bee71500eb13abfbca) **Re: [RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor**
  2024-02-26 14:49 ` [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m472a265ead19356ad7c61fa702ce5ad7dfb56312) Paolo Bonzini
**@ 2024-02-27 17:27   ` Sean Christopherson**
  2024-02-29  9:33     ` [David Woodhouse](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ma7b0af9963f7560606e08b8a75060ba74889ad62)
  2024-03-01 14:00     ` [Lai Jiangshan](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m3d94fd0f83e423356927d8d6b67384949980f462)
  2024-02-29 14:55   ` [Lai Jiangshan](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m86efe1ee03a1b357432e094e98efaf53215b2a68)
  [1 sibling, 2 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r1eed65dc0f579f488a7f37bee71500eb13abfbca)
From: Sean Christopherson @ 2024-02-27 17:27 UTC ([permalink](https://lore.kernel.org/lkml/Zd4bhQPwZDvyrF44@google.com/) / [raw](https://lore.kernel.org/lkml/Zd4bhQPwZDvyrF44@google.com/raw))
  To: Paolo Bonzini
  Cc: Lai Jiangshan, [linux-kernel](https://lore.kernel.org/lkml/?t=20240227172735), Lai Jiangshan, Linus Torvalds,
	Peter Zijlstra, Thomas Gleixner, Borislav Petkov, Ingo Molnar,
	[kvm](https://lore.kernel.org/kvm/?t=20240227172735), x86, Kees Cook, Juergen Gross, Hou Wenlong

On Mon, Feb 26, 2024, Paolo Bonzini wrote:
> On Mon, Feb 26, 2024 at 3:34 PM Lai Jiangshan <jiangshanlai@gmail.com> wrote:
> > - Full control: In XENPV/Lguest, the host Linux (dom0) entry code is
> >   subordinate to the hypervisor/switcher, and the host Linux kernel
> >   loses control over the entry code. This can cause inconvenience if
> >   there is a need to update something when there is a bug in the
> >   switcher or hardware.  Integral entry gives the control back to the
> >   host kernel.
> >
> > - Zero overhead incurred: The integrated entry code doesn't cause any
> >   overhead in host Linux entry path, thanks to the discreet design with
> >   PVM code in the switcher, where the PVM path is bypassed on host events.
> >   While in XENPV/Lguest, host events must be handled by the
> >   hypervisor/switcher before being processed.
>
> Lguest... Now that's a name I haven't heard in a long time. :)  To be
> honest, it's a bit weird to see yet another PV hypervisor. I think
> what really killed Xen PV was the impossibility to protect from
> various speculation side channel attacks, and I would like to
> understand how PVM fares here.
>
> You obviously did a great job in implementing this within the KVM
> framework; the changes in arch/x86/ are impressively small. On the
> other hand this means it's also not really my call to decide whether
> this is suitable for merging upstream. The bulk of the changes are
> really in arch/x86/kernel/ and arch/x86/entry/, and those are well
> outside my maintenance area.
The bulk of changes in _this_ patchset are outside of arch/x86/kvm, but there are
more changes on the horizon:

 : To mitigate the performance problem, we designed several optimizations
 : for the shadow MMU (not included in the patchset) and also planning to
 : build a shadow EPT in L0 for L2 PVM guests.

 : - Parallel Page fault for SPT and Paravirtualized MMU Optimization.

And even absent _new_ shadow paging functionality, merging PVM would effectively
shatter any hopes of ever removing KVM's existing, complex shadow paging code.

Specifically, unsync 4KiB PTE support in KVM provides almost no benefit for nested
TDP.  So if we can ever drop support for legacy shadow paging, which is a big if,
but not completely impossible, then we could greatly simplify KVM's shadow MMU.

Which is a good segue into my main question: was there any one thing that was
_the_ motivating factor for taking on the cost+complexity of shadow paging?  And
as alluded to be Paolo, taking on the downsides of reduced isolation?

It doesn't seem like avoiding L0 changes was the driving decision, since IIUC
you have plans to make changes there as well.

 : To mitigate the performance problem, we designed several optimizations
 : for the shadow MMU (not included in the patchset) and also planning to
 : build a shadow EPT in L0 for L2 PVM guests.

Performance I can kinda sorta understand, but my gut feeling is that the problems
with nested virtualization are solvable by adding nested paravirtualization between
L0<=>L1, with likely lower overall cost+complexity than paravirtualizing L1<=>L2.

The bulk of the pain with nested hardware virtualization lies in having to emulate
VMX/SVM, and shadow L1's TDP page tables.  Hyper-V's eVMCS takes some of the sting
off nVMX in particular, but eVMCS is still hobbled by its desire to be almost
drop-in compatible with VMX.

If we're willing to define a fully PV interface between L0 and L1 hypervisors, I
suspect we provide performance far, far better than nVMX/nSVM.  E.g. if L0 provides
a hypercall to map an L2=>L1 GPA, then L0 doesn't need to shadow L1 TDP, and L1
doesn't even need to maintain hardware-defined page tables, it can use whatever
software-defined data structure best fits it needs.

And if we limit support to 64-bit L2 kernels and drop support for unnecessary cruft,
the L1<=>L2 entry/exit paths could be drastically simplified and streamlined.  And
it should be very doable to concoct an ABI between L0 and L2 that allows L0 to
directly emulate "hot" instructions from L2, e.g. CPUID, common MSRs, etc.  I/O
would likely be solvable too, e.g. maybe with a mediated device type solution that
allows L0 to handle the data path for L2?

The one thing that I don't see line of sight to supporting is taking L0 out of the
TCB, i.e. running L2 VMs inside TDX/SNP guests.  But for me at least, that alone
isn't sufficient justification for adding a PV flavor of KVM.

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m1eed65dc0f579f488a7f37bee71500eb13abfbca) [permalink](https://lore.kernel.org/lkml/Zd4bhQPwZDvyrF44@google.com/) [raw](https://lore.kernel.org/lkml/Zd4bhQPwZDvyrF44@google.com/raw) [reply](https://lore.kernel.org/lkml/Zd4bhQPwZDvyrF44@google.com/#R)	[[**flat**](https://lore.kernel.org/lkml/Zd4bhQPwZDvyrF44@google.com/T/#u)|[nested](https://lore.kernel.org/lkml/Zd4bhQPwZDvyrF44@google.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r1eed65dc0f579f488a7f37bee71500eb13abfbca)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ea7b0af9963f7560606e08b8a75060ba74889ad62) **Re: [RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor**
  2024-02-27 17:27   ` [Sean Christopherson](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m1eed65dc0f579f488a7f37bee71500eb13abfbca)
**@ 2024-02-29  9:33     ` David Woodhouse**
  2024-03-01 14:00     ` [Lai Jiangshan](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m3d94fd0f83e423356927d8d6b67384949980f462)
  [1 sibling, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ra7b0af9963f7560606e08b8a75060ba74889ad62)
From: David Woodhouse @ 2024-02-29  9:33 UTC ([permalink](https://lore.kernel.org/lkml/16781c35a30cc5d8548da66303b323436187bbd9.camel@infradead.org/) / [raw](https://lore.kernel.org/lkml/16781c35a30cc5d8548da66303b323436187bbd9.camel@infradead.org/raw))
  To: Sean Christopherson, Paolo Bonzini
  Cc: Lai Jiangshan, [linux-kernel](https://lore.kernel.org/lkml/?t=20240229093334), Lai Jiangshan, Linus Torvalds,
	Peter Zijlstra, Thomas Gleixner, Borislav Petkov, Ingo Molnar,
	[kvm](https://lore.kernel.org/kvm/?t=20240229093334), x86, Kees Cook, Juergen Gross, Hou Wenlong

[[-- Attachment #1: Type: text/plain, Size: 1070 bytes --]](https://lore.kernel.org/lkml/16781c35a30cc5d8548da66303b323436187bbd9.camel@infradead.org/1-a.txt)

On Tue, 2024-02-27 at 09:27 -0800, Sean Christopherson wrote:
>
> The bulk of the pain with nested hardware virtualization lies in having to emulate
> VMX/SVM, and shadow L1's TDP page tables.  Hyper-V's eVMCS takes some of the sting
> off nVMX in particular, but eVMCS is still hobbled by its desire to be almost
> drop-in compatible with VMX.
>
> If we're willing to define a fully PV interface between L0 and L1 hypervisors, I
> suspect we provide performance far, far better than nVMX/nSVM.  E.g. if L0 provides
> a hypercall to map an L2=>L1 GPA, then L0 doesn't need to shadow L1 TDP, and L1
> doesn't even need to maintain hardware-defined page tables, it can use whatever
> software-defined data structure best fits it needs.
I'd like to understand how, if at all, this intersects with the
requirements we have for pKVM on x86. For example, would pKVM run the
untrusted part of the kernel as a PVM guest using this model? Would the
PV interface of which you speak also map to the calls from the kernel
into the secure pKVM hypervisor... ?

[[-- Attachment #2: smime.p7s --]
[-- Type: application/pkcs7-signature, Size: 5965 bytes --]](https://lore.kernel.org/lkml/16781c35a30cc5d8548da66303b323436187bbd9.camel@infradead.org/2-smime.p7s)

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ma7b0af9963f7560606e08b8a75060ba74889ad62) [permalink](https://lore.kernel.org/lkml/16781c35a30cc5d8548da66303b323436187bbd9.camel@infradead.org/) [raw](https://lore.kernel.org/lkml/16781c35a30cc5d8548da66303b323436187bbd9.camel@infradead.org/raw) [reply](https://lore.kernel.org/lkml/16781c35a30cc5d8548da66303b323436187bbd9.camel@infradead.org/#R)	[[**flat**](https://lore.kernel.org/lkml/16781c35a30cc5d8548da66303b323436187bbd9.camel@infradead.org/T/#u)|[nested](https://lore.kernel.org/lkml/16781c35a30cc5d8548da66303b323436187bbd9.camel@infradead.org/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ra7b0af9963f7560606e08b8a75060ba74889ad62)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e86efe1ee03a1b357432e094e98efaf53215b2a68) **Re: [RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor**
  2024-02-26 14:49 ` [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m472a265ead19356ad7c61fa702ce5ad7dfb56312) Paolo Bonzini
  2024-02-27 17:27   ` [Sean Christopherson](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m1eed65dc0f579f488a7f37bee71500eb13abfbca)
**@ 2024-02-29 14:55   ` Lai Jiangshan**
  [1 sibling, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r86efe1ee03a1b357432e094e98efaf53215b2a68)
From: Lai Jiangshan @ 2024-02-29 14:55 UTC ([permalink](https://lore.kernel.org/lkml/CAJhGHyDSHzPPhwaipSbcZXDJ+P3d6-K=ngjk1Ru3DbwzPGuz4Q@mail.gmail.com/) / [raw](https://lore.kernel.org/lkml/CAJhGHyDSHzPPhwaipSbcZXDJ+P3d6-K=ngjk1Ru3DbwzPGuz4Q@mail.gmail.com/raw))
  To: Paolo Bonzini
  Cc: [linux-kernel](https://lore.kernel.org/lkml/?t=20240229145539), Lai Jiangshan, Linus Torvalds, Peter Zijlstra,
	Sean Christopherson, Thomas Gleixner, Borislav Petkov,
	Ingo Molnar, [kvm](https://lore.kernel.org/kvm/?t=20240229145539), x86, Kees Cook, Juergen Gross, Hou Wenlong

Hello, Paolo

On Mon, Feb 26, 2024 at 10:49 PM Paolo Bonzini <pbonzini@redhat.com> wrote:
>
> On Mon, Feb 26, 2024 at 3:34 PM Lai Jiangshan <jiangshanlai@gmail.com> wrote:
> > - Full control: In XENPV/Lguest, the host Linux (dom0) entry code is
> >   subordinate to the hypervisor/switcher, and the host Linux kernel
> >   loses control over the entry code. This can cause inconvenience if
> >   there is a need to update something when there is a bug in the
> >   switcher or hardware.  Integral entry gives the control back to the
> >   host kernel.
> >
> > - Zero overhead incurred: The integrated entry code doesn't cause any
> >   overhead in host Linux entry path, thanks to the discreet design with
> >   PVM code in the switcher, where the PVM path is bypassed on host events.
> >   While in XENPV/Lguest, host events must be handled by the
> >   hypervisor/switcher before being processed.
>
> Lguest... Now that's a name I haven't heard in a long time. :)  To be
> honest, it's a bit weird to see yet another PV hypervisor. I think
> what really killed Xen PV was the impossibility to protect from
> various speculation side channel attacks, and I would like to
> understand how PVM fares here.
How does the host kernel protect itself from guest's speculation side
channel attacks?

PVM is primarily designed for secure containers like Kata containers,
where safety and security are of utmost importance.

Guests run in the hardware ring3 and they are treated as the same as the
normal user applications in the views of the host kernel's protections
and mitigations. The code employs all of the current protections and
mitigations for kernel/user interactions to host/guest and with extra
protections from pagetable isolation and with protections/mitigations
usually used for host/VTX_or_AMDV_guest (with some similar VM enter/exit
code as in vmx/ svm/). All of these are sorta easily achieved by the
"integral entry" design and "the distinct separation of the address
spaces" design can also help for protections.

How does the guest kernel protect itself from guest users' speculation
side channel attacks?

The code also tries its best to provide all of the current protections
and mitigations between the native kernel/user for virtualized kernel/user.
It is obvious that the PVM virtualized kernel operates in hardware ring3
and you can't expect all methods can be effective. Since the linux kernel
can provide protections for threads switching between different user
processes, PVM can potentially offer similar protections between guest
kernel/user through the PVM hypervisor's support.

I'm not familiar with XENPV's handling and its solutions (including its
impossibility) for the speculation side channel attacks, thus I cannot
provide additional insights or assurances in this context.

PVM is not designed as a general-purpose virtualization. The primary
objective is for secure container and Linux kernel testing. PVM intends
to allow for the universal deployment of Kata Containers inside cloud
VMs leased from any provider over the world.

For Kata containers, the protection between host/guest is much more
important and every container is only for a single tenement in which
the guest kernel is not a TCB of the external container services.
It means the protection requirements between guest kernel/user are
more flexible and customized.

>
> You obviously did a great job in implementing this within the KVM
> framework; the changes in arch/x86/ are impressively small.
Thanks for your appreciation.

> On the
> other hand this means it's also not really my call to decide whether
> this is suitable for merging upstream. The bulk of the changes are
> really in arch/x86/kernel/ and arch/x86/entry/, and those are well
> outside my maintenance area.
>
Thanks
Lai

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m86efe1ee03a1b357432e094e98efaf53215b2a68) [permalink](https://lore.kernel.org/lkml/CAJhGHyDSHzPPhwaipSbcZXDJ+P3d6-K=ngjk1Ru3DbwzPGuz4Q@mail.gmail.com/) [raw](https://lore.kernel.org/lkml/CAJhGHyDSHzPPhwaipSbcZXDJ+P3d6-K=ngjk1Ru3DbwzPGuz4Q@mail.gmail.com/raw) [reply](https://lore.kernel.org/lkml/CAJhGHyDSHzPPhwaipSbcZXDJ+P3d6-K=ngjk1Ru3DbwzPGuz4Q@mail.gmail.com/#R)	[[**flat**](https://lore.kernel.org/lkml/CAJhGHyDSHzPPhwaipSbcZXDJ+P3d6-K=ngjk1Ru3DbwzPGuz4Q@mail.gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/CAJhGHyDSHzPPhwaipSbcZXDJ+P3d6-K=ngjk1Ru3DbwzPGuz4Q@mail.gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r86efe1ee03a1b357432e094e98efaf53215b2a68)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e3d94fd0f83e423356927d8d6b67384949980f462) **Re: [RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor**
  2024-02-27 17:27   ` [Sean Christopherson](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m1eed65dc0f579f488a7f37bee71500eb13abfbca)
  2024-02-29  9:33     ` [David Woodhouse](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ma7b0af9963f7560606e08b8a75060ba74889ad62)
**@ 2024-03-01 14:00     ` Lai Jiangshan**
  [1 sibling, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r3d94fd0f83e423356927d8d6b67384949980f462)
From: Lai Jiangshan @ 2024-03-01 14:00 UTC ([permalink](https://lore.kernel.org/lkml/CAJhGHyChprt9LvLXXDeu1KwS4_V5mqhUTwJyDvqca-S_PSy6zg@mail.gmail.com/) / [raw](https://lore.kernel.org/lkml/CAJhGHyChprt9LvLXXDeu1KwS4_V5mqhUTwJyDvqca-S_PSy6zg@mail.gmail.com/raw))
  To: Sean Christopherson
  Cc: Paolo Bonzini, [linux-kernel](https://lore.kernel.org/lkml/?t=20240301140030), Lai Jiangshan, Linus Torvalds,
	Peter Zijlstra, Thomas Gleixner, Borislav Petkov, Ingo Molnar,
	[kvm](https://lore.kernel.org/kvm/?t=20240301140030), x86, Kees Cook, Juergen Gross, Hou Wenlong

Hello, Sean

On Wed, Feb 28, 2024 at 1:27 AM Sean Christopherson <seanjc@google.com> wrote:
>
> On Mon, Feb 26, 2024, Paolo Bonzini wrote:
> > On Mon, Feb 26, 2024 at 3:34 PM Lai Jiangshan <jiangshanlai@gmail.com> wrote:
> > > - Full control: In XENPV/Lguest, the host Linux (dom0) entry code is
> > >   subordinate to the hypervisor/switcher, and the host Linux kernel
> > >   loses control over the entry code. This can cause inconvenience if
> > >   there is a need to update something when there is a bug in the
> > >   switcher or hardware.  Integral entry gives the control back to the
> > >   host kernel.
> > >
> > > - Zero overhead incurred: The integrated entry code doesn't cause any
> > >   overhead in host Linux entry path, thanks to the discreet design with
> > >   PVM code in the switcher, where the PVM path is bypassed on host events.
> > >   While in XENPV/Lguest, host events must be handled by the
> > >   hypervisor/switcher before being processed.
> >
> > Lguest... Now that's a name I haven't heard in a long time. :)  To be
> > honest, it's a bit weird to see yet another PV hypervisor. I think
> > what really killed Xen PV was the impossibility to protect from
> > various speculation side channel attacks, and I would like to
> > understand how PVM fares here.
> >
> > You obviously did a great job in implementing this within the KVM
> > framework; the changes in arch/x86/ are impressively small. On the
> > other hand this means it's also not really my call to decide whether
> > this is suitable for merging upstream. The bulk of the changes are
> > really in arch/x86/kernel/ and arch/x86/entry/, and those are well
> > outside my maintenance area.
>
> The bulk of changes in _this_ patchset are outside of arch/x86/kvm, but there are
> more changes on the horizon:
>
>  : To mitigate the performance problem, we designed several optimizations
>  : for the shadow MMU (not included in the patchset) and also planning to
>  : build a shadow EPT in L0 for L2 PVM guests.
>
>  : - Parallel Page fault for SPT and Paravirtualized MMU Optimization.
>
> And even absent _new_ shadow paging functionality, merging PVM would effectively
> shatter any hopes of ever removing KVM's existing, complex shadow paging code.
>
> Specifically, unsync 4KiB PTE support in KVM provides almost no benefit for nested
> TDP.  So if we can ever drop support for legacy shadow paging, which is a big if,
> but not completely impossible, then we could greatly simplify KVM's shadow MMU.
>
One of the important goals of open-sourcing PVM is to allow for the
optimization of shadow paging, especially through paravirtualization
methods, and potentially even to eliminate the need for shadow paging.

1) Technology: Shadow paging is a technique for page table compaction in
   the category of "one-dimensional paging", which includes the direct
   paging technology in XenPV. When the page tables are stable,
   one-dimensional paging can outperform TDP because it saves on TLB
   resources. Another one-dimensional paging technology would be better
   to be introduced before shadow paging is removed for performance.

2) Naming: The reason we use the name shadowpage in our paper and the
   cover letter is that this term is more widely recognized and makes it
   easier for people to understand how PVM implements its page tables.
   It also demonstrates that PVM is able to implement a paging mechanism
   with very little code on top of KVM. However, this does not mean we
   adhere to shadow paging. Any one-dimensional paging technology can
   work here too.

3) Paravirt: As you mentioned, the best way to eliminate shadow paging
   is by using a paravirtualization (PV) approach. PVM is inherently
   suitable for having PV since it is a paravirt solution and has a
   corresponding framework. However, PV pagetables leads to a complex
   patchset, which we prefer not to include in the initial PVM patchset
   introduction.

4) Pave the path: One of the purposes of open-sourcing PVM is to bring
   in a new scenario for possibly introducing PV pagetable interfaces
   and optimizing shadow paging. Moreover, investing development effort
   in shadow paging is the only way to ultimately remove it.

5) Optimizations: We have experimented with numerous optimizations
   including at least two categories: parallel-pagetable and
   enlightened-pagetable. The parallel pagetable overhauls the locking
   mechanism within the shadow paging. The enlightened-pagetable
   introduces PVOPS in the guest to modify the page tables. One set of
   PVOPS, used on 4KiB PTEs, queues the pointers of the modified GPTEs
   in a hypervisor-guest shared ring buffer. Although the overall
   mechanism, including TLB handling, is not simple, the hypervisor
   portion is simpler than the unsync-sp method, and it bypasses many
   unsync-sp related code paths. The other set of PVOPS targets larger
   page table entries and directly issues hypercalls. Should both sets
   of PVOPS be utilized, write-protect for SPs is unneeded and shadow
   paging could be considered as being removed.

> Which is a good segue into my main question: was there any one thing that was
> _the_ motivating factor for taking on the cost+complexity of shadow paging?  And
> as alluded to be Paolo, taking on the downsides of reduced isolation?
>
> It doesn't seem like avoiding L0 changes was the driving decision, since IIUC
> you have plans to make changes there as well.
>
>  : To mitigate the performance problem, we designed several optimizations
>  : for the shadow MMU (not included in the patchset) and also planning to
>  : build a shadow EPT in L0 for L2 PVM guests.
>

Getting every cloud provider to adopt a technology is more challenging
than developing the technology itself. It is easy to compile a list that
includes many technologies for L0 that have been merged into upstream
KVM for quite some time, yet not all major cloud providers use or
support them.

The purpose of PVM includes enabling the use of KVM within various cloud
VMs, allowing for easy operation of businesses with secure containers.
Therefore, it cannot rely on whether cloud providers make such changes
to L0.

The reason we are experimenting with modifications to L0 is because we
have many physical machines. Developing this technology getting help from
L0 for L2 paging could provide us and others who have their own physical
machines with an additional option.

> Performance I can kinda sorta understand, but my gut feeling is that the problems
> with nested virtualization are solvable by adding nested paravirtualization between
> L0<=>L1, with likely lower overall cost+complexity than paravirtualizing L1<=>L2.
>
> The bulk of the pain with nested hardware virtualization lies in having to emulate
> VMX/SVM, and shadow L1's TDP page tables.  Hyper-V's eVMCS takes some of the sting
> off nVMX in particular, but eVMCS is still hobbled by its desire to be almost
> drop-in compatible with VMX.
>
> If we're willing to define a fully PV interface between L0 and L1 hypervisors, I
> suspect we provide performance far, far better than nVMX/nSVM.  E.g. if L0 provides
> a hypercall to map an L2=>L1 GPA, then L0 doesn't need to shadow L1 TDP, and L1
> doesn't even need to maintain hardware-defined page tables, it can use whatever
> software-defined data structure best fits it needs.
>
> And if we limit support to 64-bit L2 kernels and drop support for unnecessary cruft,
> the L1<=>L2 entry/exit paths could be drastically simplified and streamlined.  And
> it should be very doable to concoct an ABI between L0 and L2 that allows L0 to
> directly emulate "hot" instructions from L2, e.g. CPUID, common MSRs, etc.  I/O
> would likely be solvable too, e.g. maybe with a mediated device type solution that
> allows L0 to handle the data path for L2?
>
> The one thing that I don't see line of sight to supporting is taking L0 out of the
> TCB, i.e. running L2 VMs inside TDX/SNP guests.  But for me at least, that alone
> isn't sufficient justification for adding a PV flavor of KVM.

I didn't want to suggest that running PVM inside TDX is an important use
case, but I just used it to emphasize PVM's universally accessibility in
all environments, including inside the notoriously otherwise impossible
environment as TDX as Paolo said in a LWN comment:
<https://lwn.net/Articles/865807/>

 : TDX cannot be used in a nested VM, and you cannot use nested
 : virtualization inside a TDX virtual machine.

(and actually the support for PVM in TDX/SNP is not completed yet)

Thanks
Lai

[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m3d94fd0f83e423356927d8d6b67384949980f462) [permalink](https://lore.kernel.org/lkml/CAJhGHyChprt9LvLXXDeu1KwS4_V5mqhUTwJyDvqca-S_PSy6zg@mail.gmail.com/) [raw](https://lore.kernel.org/lkml/CAJhGHyChprt9LvLXXDeu1KwS4_V5mqhUTwJyDvqca-S_PSy6zg@mail.gmail.com/raw) [reply](https://lore.kernel.org/lkml/CAJhGHyChprt9LvLXXDeu1KwS4_V5mqhUTwJyDvqca-S_PSy6zg@mail.gmail.com/#R)	[[**flat**](https://lore.kernel.org/lkml/CAJhGHyChprt9LvLXXDeu1KwS4_V5mqhUTwJyDvqca-S_PSy6zg@mail.gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/CAJhGHyChprt9LvLXXDeu1KwS4_V5mqhUTwJyDvqca-S_PSy6zg@mail.gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r3d94fd0f83e423356927d8d6b67384949980f462)

* * * * *

[*](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#e556764c73fec9ccbc2e8f38afce91448d4254fb9) **Re: [RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor**
  2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
                   ` [(73 preceding siblings ...)](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r472a265ead19356ad7c61fa702ce5ad7dfb56312)
  2024-02-26 14:49 ` [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m472a265ead19356ad7c61fa702ce5ad7dfb56312) Paolo Bonzini
**@ 2024-03-06 11:05 ` Like Xu**
  [74 siblings, 0 replies; 82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r556764c73fec9ccbc2e8f38afce91448d4254fb9)
From: Like Xu @ 2024-03-06 11:05 UTC ([permalink](https://lore.kernel.org/lkml/302ef225-7a45-4153-acd1-a0066b652da2@gmail.com/) / [raw](https://lore.kernel.org/lkml/302ef225-7a45-4153-acd1-a0066b652da2@gmail.com/raw))
  To: Lai Jiangshan
  Cc: Lai Jiangshan, Sean Christopherson, Borislav Petkov, [kvm](https://lore.kernel.org/kvm/?t=20240306110558),
	Paolo Bonzini, x86, Hou Wenlong, [linux-kernel@vger.kernel.org](https://lore.kernel.org/lkml/?t=20240306110558)

Hi Jiangshan,

On 26/2/2024 10:35 pm, Lai Jiangshan wrote:
> Performance drawback
> ====================
> The most significant drawback of PVM is shadowpaging. Shadowpaging
> results in very bad performance when guest applications frequently
> modify pagetable, including excessive processes forking.
Some numbers are needed here to show how bad this RFC virt-pvm version
without SPT optimization is in terms of performance. Compared to L2-VM
based on nested EPT-on-EPT, the following benchmarks show a significant
performance loss in PVM-based L2-VM (per pvm-get-started-with-kata.md):

- byte/UnixBench-shell1: -67%
- pts/sysbench-1.1.0 [Test: RAM / Memory]: -55%
- Mmap Latency [lmbench]: -92%
- Context switching [lmbench]: -83%
- syscall_get_pid_latency: -77%

Not sure if these performance conclusions are reproducible on your VM,
but it reveals the concern of potential users that there is not a strong
enough incentive to offload the burden of maintaining kvm-pvm.ko to the
upstream community until there is a public available SPT optimization
based on your or any state-of-art MMU-PV-ops impl. brought to the ring.

There are other kernel technologies used by PVM that have user scenarios
outside of PVM (e.g. unikernel/kernel-level sandbox), and it seems to me
that there's opportunities for all of them to be absorbed by upstream
individually and sequentially, but getting the KVM community to take
kvm-pvm.ko seriously may be more dependent on how much room there can
be for performance optimization based on your "Parallel Page fault for SPT
and Paravirtualized MMU Optimization" implementation, and the optimizing
space developers can squeeze out of legacy EPT-on-EPT solution.

>
> However, many long-running cloud services, such as Java, modify
> pagetables less frequently and can perform very well with shadowpaging.
> In some cases, they can even outperform EPT since they can avoid EPT TLB
> entries. Furthermore, PVM can utilize host PCIDs for guest processes,
> providing a finer-grained approach compared to VPID/ASID.
>
> To mitigate the performance problem, we designed several optimizations
> for the shadow MMU (not included in the patchset) and also planning to
> build a shadow EPT in L0 for L2 PVM guests.
>
> See the paper for more optimizations and the performance details.
>
> Future plans
> ============
> Some optimizations are not covered in this series now.
>
> - Parallel Page fault for SPT and Paravirtualized MMU Optimization.
[^](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m556764c73fec9ccbc2e8f38afce91448d4254fb9) [permalink](https://lore.kernel.org/lkml/302ef225-7a45-4153-acd1-a0066b652da2@gmail.com/) [raw](https://lore.kernel.org/lkml/302ef225-7a45-4153-acd1-a0066b652da2@gmail.com/raw) [reply](https://lore.kernel.org/lkml/302ef225-7a45-4153-acd1-a0066b652da2@gmail.com/#R)	[[**flat**](https://lore.kernel.org/lkml/302ef225-7a45-4153-acd1-a0066b652da2@gmail.com/T/#u)|[nested](https://lore.kernel.org/lkml/302ef225-7a45-4153-acd1-a0066b652da2@gmail.com/t/#u)] [82+ messages in thread](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#r556764c73fec9ccbc2e8f38afce91448d4254fb9)

* * * * *

end of thread, other threads:[[~2024-03-06 11:05 UTC](https://lore.kernel.org/lkml/?t=20240306110558) | [newest](https://lore.kernel.org/lkml/)]

**Thread overview:** 82+ messages (download: [mbox.gz](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/t.mbox.gz) follow: [Atom feed](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/t.atom)
-- links below jump to the message on this page --
2024-02-26 14:35 [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5380166ee3c0ce945348e361d39bf5ca577a1fbe) Lai Jiangshan
2024-02-26 14:35 ` [[RFC PATCH 01/73] KVM: Documentation: Add the specification for PVM](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mdb114605fe7991cfe69b8719c9ca244d6e37e4b1) Lai Jiangshan
2024-02-26 14:35 ` [[RFC PATCH 02/73] x86/ABI/PVM: Add PVM-specific ABI header file](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m298a49e9ed7494c1be0fd7a4563fb73d2c26cb2f) Lai Jiangshan
2024-02-26 14:35 ` [[RFC PATCH 03/73] x86/entry: Implement switcher for PVM VM enter/exit](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m9f2524581c30e4d18403640ceddcb5f0aaccd684) Lai Jiangshan
2024-02-26 14:35 ` [[RFC PATCH 04/73] x86/entry: Implement direct switching for the switcher](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m3b243b78a7b5a0502322405fa52e63002c19c978) Lai Jiangshan
2024-02-26 14:35 ` [[RFC PATCH 05/73] KVM: x86: Set 'vcpu->arch.exception.injected' as true before vendor callback](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ma4640c2ddc5dbd24e9892d48e636c76b70be6ee7) Lai Jiangshan
2024-02-26 14:35 ` [[RFC PATCH 06/73] KVM: x86: Move VMX interrupt/nmi handling into kvm.ko](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m05a0809c1a3c5bc57be7bf03a668f0d8f1e00eaa) Lai Jiangshan
2024-02-26 14:35 ` [[RFC PATCH 07/73] KVM: x86/mmu: Adapt shadow MMU for PVM](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#me39c9679e5b0232d0df0d7ab320253a7791c3210) Lai Jiangshan
2024-02-26 14:35 ` [[RFC PATCH 08/73] KVM: x86: Allow hypercall handling to not skip the instruction](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ma85007e874e0c3c0f689d477d793302d04c044e6) Lai Jiangshan
2024-02-26 14:35 ` [[RFC PATCH 09/73] KVM: x86: Add PVM virtual MSRs into emulated_msrs_all[]](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m012c1a1c873dc334ab545cbff69fc79bcc23f3da) Lai Jiangshan
2024-02-26 14:35 ` [[RFC PATCH 10/73] KVM: x86: Introduce vendor feature to expose vendor-specific CPUID](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m0c0e5f5df8feaba74f67cf6883aa5a1c22c3a5a1) Lai Jiangshan
2024-02-26 14:35 ` [[RFC PATCH 11/73] KVM: x86: Implement gpc refresh for guest usage](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m72b6ae0685b99573cb31030b77b7b790135df13d) Lai Jiangshan
2024-02-26 14:35 ` [[RFC PATCH 12/73] KVM: x86: Add NR_VCPU_SREG in SREG enum](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#me826405a148de8c11b9dad7f7ca0417305d7bd6d) Lai Jiangshan
2024-02-26 14:35 ` [[RFC PATCH 13/73] KVM: x86/emulator: Reinject #GP if instruction emulation failed for PVM](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m1ec193e69ae78265dd8d5a2ea61788103ea240e1) Lai Jiangshan
2024-02-26 14:35 ` [[RFC PATCH 14/73] KVM: x86: Create stubs for PVM module as a new vendor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m3bec210bc1ebe2b4a3a15d38d96cb43675c08340) Lai Jiangshan
2024-02-26 14:35 ` [[RFC PATCH 15/73] mm/vmalloc: Add a helper to reserve a contiguous and aligned kernel virtual area](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ma2decc0d9dda7301af1d323411463772c3ee3e15) Lai Jiangshan
2024-02-27 14:56   ` [Christoph Hellwig](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m934efcc0c018f5e064353a9160954ce7103b69e3)
2024-02-27 17:07     ` [Lai Jiangshan](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#me17e5aa8923b210192c673fa1b09192ac6acf7d6)
2024-02-26 14:35 ` [[RFC PATCH 16/73] KVM: x86/PVM: Implement host mmu initialization](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m252184d3f37bddd3067ba3ba05f9140dd3e5aebf) Lai Jiangshan
2024-02-26 14:35 ` [[RFC PATCH 17/73] KVM: x86/PVM: Implement module initialization related callbacks](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mcffc6d9b931c065a515cc5ebaa312b7cf6ed76ea) Lai Jiangshan
2024-02-26 14:35 ` [[RFC PATCH 18/73] KVM: x86/PVM: Implement VM/VCPU](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m999e9100dc7bf390c8b2892c071ccfe18cfc9c7c) " Lai Jiangshan
2024-02-26 14:35 ` [[RFC PATCH 19/73] x86/entry: Export 32-bit ignore syscall entry and __ia32_enabled variable](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mb34ef91f220b438260491c4cf826cb0c72c41609) Lai Jiangshan
2024-02-26 14:35 ` [[RFC PATCH 20/73] KVM: x86/PVM: Implement vcpu_load()/vcpu_put() related callbacks](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m977febae904fda3cf07f049403d3fd9a28dd7017) Lai Jiangshan
2024-02-26 14:35 ` [[RFC PATCH 21/73] KVM: x86/PVM: Implement vcpu_run() callbacks](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m6a1a901f25c380d80ddb67622cbc167b148a8f94) Lai Jiangshan
2024-02-26 14:35 ` [[RFC PATCH 22/73] KVM: x86/PVM: Handle some VM exits before enable interrupts](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m7008898d987993cb9ff2bb727d36d62a5bd13fd7) Lai Jiangshan
2024-02-26 14:35 ` [[RFC PATCH 23/73] KVM: x86/PVM: Handle event handling related MSR read/write operation](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5cb5213fee46dd2beab66f058b8ddbe3bb7cd3d3) Lai Jiangshan
2024-02-26 14:35 ` [[RFC PATCH 24/73] KVM: x86/PVM: Introduce PVM mode switching](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m11884219130ef982a74482ba96cdb82e086660b5) Lai Jiangshan
2024-02-26 14:35 ` [[RFC PATCH 25/73] KVM: x86/PVM: Implement APIC emulation related callbacks](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#md1c28920b1bd98bd862098afd244cffaa8cb4ff5) Lai Jiangshan
2024-02-26 14:35 ` [[RFC PATCH 26/73] KVM: x86/PVM: Implement event delivery flags](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m832f2e4e35b2d9ed4bad146f00bd3812873304ef) " Lai Jiangshan
2024-02-26 14:35 ` [[RFC PATCH 27/73] KVM: x86/PVM: Implement event injection](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m33dc6d1661d230e4e082e894184c3b24dc45d32f) " Lai Jiangshan
2024-02-26 14:35 ` [[RFC PATCH 28/73] KVM: x86/PVM: Handle syscall from user mode](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m047f1b27ead6cff47e8d3a5c1d3cc4ffe9ca24ad) Lai Jiangshan
2024-02-26 14:35 ` [[RFC PATCH 29/73] KVM: x86/PVM: Implement allowed range checking for #PF](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m1e0e233f733da10c3280e7afaf54c4c86f476d0f) Lai Jiangshan
2024-02-26 14:35 ` [[RFC PATCH 30/73] KVM: x86/PVM: Implement segment related callbacks](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#md376ee490f7f7f78f1acbe712885be489e294ece) Lai Jiangshan
2024-02-26 14:35 ` [[RFC PATCH 31/73] KVM: x86/PVM: Implement instruction emulation for #UD and #GP](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5396337203ffa3dcf5a0740edb14f424af5c53dd) Lai Jiangshan
2024-02-26 14:35 ` [[RFC PATCH 32/73] KVM: x86/PVM: Enable guest debugging functions](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m7ef1d017b302aff61b6ddea0d53bd06db238e693) Lai Jiangshan
2024-02-26 14:35 ` [[RFC PATCH 33/73] KVM: x86/PVM: Handle VM-exit due to hardware exceptions](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#me5509b00e1fd0d054e9cd1221f60150678e7902a) Lai Jiangshan
2024-02-26 14:35 ` [[RFC PATCH 34/73] KVM: x86/PVM: Handle ERETU/ERETS synthetic instruction](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mc42f8da11f3737be4b493b5bcada0e082f08f631) Lai Jiangshan
2024-02-26 14:35 ` [[RFC PATCH 35/73] KVM: x86/PVM: Handle PVM_SYNTHETIC_CPUID](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5dc2eca0376fb278548d57d6a9e719e2461aadd5) " Lai Jiangshan
2024-02-26 14:35 ` [[RFC PATCH 36/73] KVM: x86/PVM: Handle KVM hypercall](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m3bb448f8fe42d888eb3384b3a04b0e38e77af341) Lai Jiangshan
2024-02-26 14:35 ` [[RFC PATCH 37/73] KVM: x86/PVM: Use host PCID to reduce guest TLB flushing](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m2d1522a59d0e90ba1385d0417217234a95b13e8d) Lai Jiangshan
2024-02-26 14:35 ` [[RFC PATCH 38/73] KVM: x86/PVM: Handle hypercalls for privilege instruction emulation](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mbda22272cd4bb3a7a881e6ca2e65572f999dc4c7) Lai Jiangshan
2024-02-26 14:35 ` [[RFC PATCH 39/73] KVM: x86/PVM: Handle hypercall for CR3 switching](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m97850ee556b8a2ea2c4a71f23922a5669f15d854) Lai Jiangshan
2024-02-26 14:35 ` [[RFC PATCH 40/73] KVM: x86/PVM: Handle hypercall for loading GS selector](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mabc67608b691853150bde97ac631b7a1d6d00eb4) Lai Jiangshan
2024-02-26 14:35 ` [[RFC PATCH 41/73] KVM: x86/PVM: Allow to load guest TLS in host GDT](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mc76a78e8e1e7b7d584ff1ef56de2a847e07a6cd4) Lai Jiangshan
2024-02-26 14:35 ` [[RFC PATCH 42/73] KVM: x86/PVM: Support for kvm_exit() tracepoint](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ma6bec3331b150ec2a3f44a0d098516cddd1978ca) Lai Jiangshan
2024-02-26 14:36 ` [[RFC PATCH 43/73] KVM: x86/PVM: Enable direct switching](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m25de9f37876e75c7b1168173fd21deb1976c578e) Lai Jiangshan
2024-02-26 14:36 ` [[RFC PATCH 44/73] KVM: x86/PVM: Implement TSC related callbacks](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mbd856b434a7deadfc6ce93537dd8df2b513bbc77) Lai Jiangshan
2024-02-26 14:36 ` [[RFC PATCH 45/73] KVM: x86/PVM: Add dummy PMU](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mc83425f1d7e043377f7c4d27ae91e7d06ec7ec77) " Lai Jiangshan
2024-02-26 14:36 ` [[RFC PATCH 46/73] KVM: x86/PVM: Support for CPUID faulting](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m07d7573539e96b204ff87b1da08e99461e387e1b) Lai Jiangshan
2024-02-26 14:36 ` [[RFC PATCH 47/73] KVM: x86/PVM: Handle the left supported MSRs in msrs_to_save_base[]](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m616815a2df532d615348d29e26f196e3302818a2) Lai Jiangshan
2024-02-26 14:36 ` [[RFC PATCH 48/73] KVM: x86/PVM: Implement system registers setting callbacks](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m5d9f7a5a2bc967bd2252cc11c9eb76b3778bb902) Lai Jiangshan
2024-02-26 14:36 ` [[RFC PATCH 49/73] KVM: x86/PVM: Implement emulation for non-PVM mode](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m4d425168fece4bbd313c4b5bac4dc2ad0db5bf15) Lai Jiangshan
2024-02-26 14:36 ` [[RFC PATCH 50/73] x86/tools/relocs: Cleanup cmdline options](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m0641eb1cf96777477a3260e67b6deb76ef03621e) Lai Jiangshan
2024-02-26 14:36 ` [[RFC PATCH 51/73] x86/tools/relocs: Append relocations into input file](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m439206d2faf20a98af28d6448d297cb8b7c4bb8f) Lai Jiangshan
2024-02-26 14:36 ` [[RFC PATCH 52/73] x86/boot: Allow to do relocation for uncompressed kernel](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#md2b2dd3e1a7220232ba2b4d850560a9397e9dee0) Lai Jiangshan
2024-02-26 14:36 ` [[RFC PATCH 53/73] x86/pvm: Add Kconfig option and the CPU feature bit for PVM guest](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m712fb66a83ce7d3ec7a4fbc83a89c97f40f4414a) Lai Jiangshan
2024-02-26 14:36 ` [[RFC PATCH 54/73] x86/pvm: Detect PVM hypervisor support](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m13af3b16f8dd4dd49ea9779776dc0aad02e2f5cc) Lai Jiangshan
2024-02-26 14:36 ` [[RFC PATCH 55/73] x86/pvm: Relocate kernel image to specific virtual address range](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m859dc440ec3063be3e5a6fcaddeae2901ef1c7fb) Lai Jiangshan
2024-02-26 14:36 ` [[RFC PATCH 56/73] x86/pvm: Relocate kernel image early in PVH entry](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#me32699725a7f6bb2708fbf48948be563da10ee2d) Lai Jiangshan
2024-02-26 14:36 ` [[RFC PATCH 57/73] x86/pvm: Make cpu entry area and vmalloc area variable](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m907f649c408bd41d2b5f2f0e2b930380f0d9db5e) Lai Jiangshan
2024-02-26 14:36 ` [[RFC PATCH 58/73] x86/pvm: Relocate kernel address space layout](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mf4f5b56a298f562c94b24e02bab22969eb6c7d39) Lai Jiangshan
2024-02-26 14:36 ` [[RFC PATCH 59/73] x86/pti: Force enabling KPTI for PVM guest](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m8196806734b024e84079f3acb39d041bf3665bd2) Lai Jiangshan
2024-02-26 14:36 ` [[RFC PATCH 60/73] x86/pvm: Add event entry/exit and dispatch code](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m99d21cd9c0251b0a5ed4be095f4028607e056c60) Lai Jiangshan
2024-02-26 14:36 ` [[RFC PATCH 61/73] x86/pvm: Allow to install a system interrupt handler](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mfab4880d8049e864d42c2686113871c192625c69) Lai Jiangshan
2024-02-26 14:36 ` [[RFC PATCH 62/73] x86/pvm: Add early kernel event entry and dispatch code](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m635896d24115d0f5add479c6e5ed8f8911dd4c23) Lai Jiangshan
2024-02-26 14:36 ` [[RFC PATCH 63/73] x86/pvm: Add hypercall support](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m89cc21528bcaef9776de23d2e19df5a99f2dc50f) Lai Jiangshan
2024-02-26 14:36 ` [[RFC PATCH 64/73] x86/pvm: Enable PVM event delivery](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m2c3dd6d46e7c121b8a8aa343c7a963dd616bb40f) Lai Jiangshan
2024-02-26 14:36 ` [[RFC PATCH 65/73] x86/kvm: Patch KVM hypercall as PVM hypercall](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m92ec0e5a6b593ee132eaefd57fad44391dfe83ec) Lai Jiangshan
2024-02-26 14:36 ` [[RFC PATCH 66/73] x86/pvm: Use new cpu feature to describe XENPV and PVM](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m9d2e3415e38f877972ab77863f494706ffb770a0) Lai Jiangshan
2024-02-26 14:36 ` [[RFC PATCH 67/73] x86/pvm: Implement cpu related PVOPS](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mf306e82734c379c14ea3a038c5663661d4f4d56e) Lai Jiangshan
2024-02-26 14:36 ` [[RFC PATCH 68/73] x86/pvm: Implement irq](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mb4741beb7127ff80a72ab793d185c8631dff405e) " Lai Jiangshan
2024-02-26 14:36 ` [[RFC PATCH 69/73] x86/pvm: Implement mmu](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m287a2f06f262269e2610cadf9db062ef5e5b89c1) " Lai Jiangshan
2024-02-26 14:36 ` [[RFC PATCH 70/73] x86/pvm: Don't use SWAPGS for gsbase read/write](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mdf41d4fe4b50256bc48f76328dc595756b850b19) Lai Jiangshan
2024-02-26 14:36 ` [[RFC PATCH 71/73] x86/pvm: Adapt pushf/popf in this_cpu_cmpxchg16b_emu()](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mfa18bdf43e5a1f7b7b4bb426168497c30c3b01bd) Lai Jiangshan
2024-02-26 14:36 ` [[RFC PATCH 72/73] x86/pvm: Use RDTSCP as default in vdso_read_cpunode()](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#mef68ca04da4883790fbd04377f306afc713ac6f6) Lai Jiangshan
2024-02-26 14:36 ` [[RFC PATCH 73/73] x86/pvm: Disable some unsupported syscalls and features](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m4a887cf49752ba5740fdb806b5c648135c88c0ca) Lai Jiangshan
2024-02-26 14:49 ` [[RFC PATCH 00/73] KVM: x86/PVM: Introduce a new hypervisor](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m472a265ead19356ad7c61fa702ce5ad7dfb56312) Paolo Bonzini
2024-02-27 17:27   ` [Sean Christopherson](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m1eed65dc0f579f488a7f37bee71500eb13abfbca)
2024-02-29  9:33     ` [David Woodhouse](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#ma7b0af9963f7560606e08b8a75060ba74889ad62)
2024-03-01 14:00     ` [Lai Jiangshan](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m3d94fd0f83e423356927d8d6b67384949980f462)
2024-02-29 14:55   ` [Lai Jiangshan](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m86efe1ee03a1b357432e094e98efaf53215b2a68)
2024-03-06 11:05 ` [Like Xu](https://lore.kernel.org/lkml/CABgObfaSGOt4AKRF5WEJt2fGMj_hLXd7J2x2etce2ymvT4HkpA@mail.gmail.com/T/#m556764c73fec9ccbc2e8f38afce91448d4254fb9)

* * * * *

This is a public inbox, see [mirroring instructions](https://lore.kernel.org/lkml/_/text/mirror/)
for how to clone and mirror all data and code used for this inbox;
as well as URLs for NNTP newsgroup(s).
