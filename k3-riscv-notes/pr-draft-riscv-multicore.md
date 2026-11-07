# PR draft: riscv64 multi-vCPU support (branch `riscv-multicore`)

Working notes for opening upstream PR(s) against cloud-hypervisor/cloud-hypervisor.
Written 2026-10-28; the branch is based on `cb5b81583` (origin/main snapshot of that day).

## Where the code is

- Branch `riscv-multicore` in `/workspace/cloud-hypervisor` (8 commits, not pushed).
- Same commits as patch files: `notes/patches/riscv-multicore/000{1..8}-*.patch`
  (re-apply with `git am notes/patches/riscv-multicore/*.patch` on a fresh checkout).

| # | Commit | Kind | Applies alone on main? |
|---|---|---|---|
| 1 | `arch, hypervisor, vmm: riscv64: Describe guest ISA from KVM in the FDT` | bug fix | yes |
| 2 | `arch, vmm: riscv64: Describe the vCPU topology in the FDT` | feature | **needs 1** (same signatures) |
| 3 | `arch, vmm: riscv64: Describe guest NUMA nodes in the FDT` | feature | **needs 2** (same signatures) |
| 4 | `vmm: config: Reject CPU hotplug on riscv64` | guard | yes |
| 5 | `devices, hypervisor, vmm: riscv64: Pulse AIA wired interrupts` | bug fix | yes |
| 6 | `docs: riscv: Document multiple vCPUs, topology and NUMA` | docs | builds alone, but documents 2–4 |
| 7 | `hypervisor: riscv64: Report which AIA device attribute failed` | diagnostics | yes |
| 8 | `arch, hypervisor: riscv64: Describe the APLIC sources configured in KVM` | bug fix (Stage 2 #3) | yes |

"Applies alone" = cherry-picked by itself onto `cb5b81583` and built for riscv64 (checked 2026-10-28). The
dependencies of 2 and 3 are only textual: they add parameters next to the ones commit 1 adds. If 2/3 must go
first, rebase them onto main and fix up the `create_cpu_nodes()` / `configure_system()` signatures.

Commit 7 is based on Caleb's `06d1a962f` ("riscv: report failing AIA device attribute", FyraStack `origin/riscv`),
reworked for current main: attribute *names* + group/attr in the error (values are already logged by upstream's
`debug!` lines), get errors too, and `create_vaia()` keeps the error as a source instead of a `{e:?}` string.
Caleb's other commit there, `a7704fd37` (`IRQ_NUM = 7`), is **superseded** by upstream `085642dd4` (clamps SRCS to
KVM's limit, which is 63 on the K3, not 7) and is **not** carried over: with 7 sources the serial port (APLIC
source 9) loses its interrupts. Commit 8 completes that upstream fix by making the FDT match.

Each commit is DCO signed-off (Caleb Jones <caleb@calebgj.io>), builds on its own for riscv64, passes nightly
`rustfmt --check` and the repo's gitlint rules. Re-check the sign-off identity/email you want before pushing.

## Before opening the PR (checklist)

- [ ] Rebase onto current upstream `main` (the base is a snapshot; re-run the checks below after rebasing).
- [ ] Test on K3 hardware (see "Testing" — so far only emulated). Add the results to the description.
- [ ] Re-read every commit yourself; CONTRIBUTING.md requires the submitter to understand all of it.
- [ ] Fill in the LLM disclosure line (CONTRIBUTING.md "AI/LLM Assistance").
- [ ] Decide on one PR vs. split (recommendation below).
- [ ] Optional: send the kernel-side fix for the irqfd issue (see commit 5 background) and link it.

Checks to re-run (scripts in `/workspace/rvtest/`, see its README):

```console
rvtest/build-ch.sh                                   # riscv64 build, same as CI's build-riscv64 job
rvtest/build-ch.sh cargo +nightly fmt --all -- --check
rvtest/clippy-ch.sh                                  # riscv64 clippy on touched crates
rvtest/test-ch.sh -p arch --features kvm             # arch unit tests (qemu-user)
rvtest/test-ch.sh -p vmm --features kvm --lib config::
gitlint --commits origin/main..HEAD
# KVM unit tests (hypervisor crate) must run on a riscv64 host with /dev/kvm (K3 or the emulated L1)
```

## Recommendation: how to split

Suggested PRs, each self-contained (descriptions below; PR A's text covers commits 1 and 5, so split it if
they go separately):

- **PR A1, ISA fix:** commit 1. (Fixes guests that can't boot at all.)
- **PR A2, serial IRQ storm:** commit 5.
- **PR C, AIA sources:** commits 7 + 8 (diagnostics + `riscv,num-sources`; both touch the AIA setup). Could
  also be two tiny PRs.
- **PR B, SMP topology/NUMA:** commits 2, 3, 4, 6, after PR A1 merges (or stacked on it).

Commit 4 (reject hotplug) also stands alone if a reviewer wants it separate from B.

---

## PR A description (ready to paste)

**Title:** `riscv64: Fix guest ISA description and serial interrupt storm`

While testing multi-vCPU riscv64 guests I found two bugs that affect every riscv64 guest.

**1. Guest ISA in the FDT** (`arch, hypervisor, vmm: riscv64: Describe guest ISA from KVM in the FDT`)

The CPU nodes only had the deprecated `riscv,isa` string, built from the host's `/proc/cpuinfo`.

- Kernels built with `CONFIG_RISCV_ISA_FALLBACK=n` (e.g. Ubuntu 26.04's) don't parse `riscv,isa` any more and
  reject every hart (`CPU with hartid=0 is invalid: this kernel does not parse "riscv,isa"`, then a BUG in
  `of_parse_and_init_cpus()`), so they can't boot at all.
- The host ISA isn't the guest ISA: KVM doesn't necessarily expose every host extension to the guest.

The extensions are now read from KVM's `KVM_REG_RISCV_ISA_EXT` one-regs (new `Vcpu::get_isa_extensions()`), and
the FDT gets `riscv,isa-base` and `riscv,isa-extensions`, plus a matching `riscv,isa` for older kernels. If the
hypervisor can't report the ISA, it falls back to the previous host-based behaviour. The names come from a
table that mirrors `enum KVM_RISCV_ISA_EXT_ID` (uAPI, append-only), because kvm-bindings 0.14 lacks the newest IDs;
IDs unknown to the running kernel are skipped.

**2. Serial interrupt storm** (`devices, hypervisor, vmm: riscv64: Pulse AIA wired interrupts`)

Every riscv64 guest takes about 100,000 spurious serial interrupts within seconds of booting, then Linux disables
the IRQ (`irq 11: nobody cared`) and the console falls back to polling.

Root cause: on riscv64, KVM handles irqfds routed to the in-kernel APLIC in atomic context
(`kvm_arch_set_irq_inatomic()` in `arch/riscv/kvm/vm.c` handles `KVM_IRQ_ROUTING_IRQCHIP`), which only raises the
line. The `irqfd_inject()` work that raises and then lowers it, used on other architectures, never runs. So
after the first interrupt the APLIC source stays asserted. Linux's APLIC MSI driver re-pends level-triggered
interrupts at EOI by writing `setipnum_le` (AIA spec §4.9.2), and KVM honours that while the input is high, so
the interrupt fires forever. An edge-triggered source would instead never fire again.

Traced with kprobes on KVM's APLIC emulation during a guest boot:

| | before | after |
|---|---|---|
| `kvm_riscv_aia_aplic_inject(src=9)` | 179 × level 1, 0 × level 0 | 304 × level 1, 304 × level 0 |
| guest ttyS0 interrupts | 100001, then disabled | 22 |

Legacy interrupts are now pulsed with `KVM_IRQ_LINE` (new riscv64 `Vm::set_irq_line()`), as QEMU and kvmtool do,
and seccomp allows that ioctl for the threads that emulate legacy devices (VMM, vCPU, serial manager). Only
wired interrupts are affected; PCI MSIs use MSI routes and were fine.

(A kernel fix, e.g. raising and lowering the line for IRQCHIP routes in `kvm_arch_set_irq_inatomic()`, would also
be good, but the VMM needs the workaround for existing kernels, including vendor ones.)

**Testing:** see "Testing" below.

---

## PR B description (ready to paste)

**Title:** `riscv64: Describe vCPU topology and NUMA nodes`

Multi-vCPU riscv64 guests already boot, but the guest only sees a flat list of harts:
`--cpus topology=` and `--numa` were accepted and silently ignored.

- **Topology** (`arch, vmm: riscv64: Describe the vCPU topology in the FDT`): emit a `cpu-map`
  (socket/cluster/core/thread) like aarch64. Unlike aarch64 (which always emits a single `cluster0`), each **die**
  becomes a cluster, so a guest can mirror hosts that have several clusters per socket. For example,
  `topology=1:4:2:1` matches the SpacemiT K3's two 4-core X100 clusters, and the guest scheduler sees the
  `cluster_id`s. The map is only emitted when it covers exactly the booted harts.
- **NUMA** (`arch, vmm: riscv64: Describe guest NUMA nodes in the FDT`): keep the NUMA nodes on riscv64 (they were
  computed and then dropped) and describe them per the devicetree NUMA binding: `numa-node-id` on CPUs, one
  memory node per NUMA node, PCI host bridges of assigned segments, and a `numa-distance-map-v1`. Differences
  from aarch64:
  - memory zones that aren't in any NUMA node are still described, without a node id, instead of being dropped
    from the guest's memory map;
  - a distance configured in only one direction is used for both directions.
- **CPU hotplug** (`vmm: config: Reject CPU hotplug on riscv64`): the AIA is created for the boot vCPUs only, and
  KVM only sets up IMSIC state for vCPUs that exist when the AIA is initialized. So `max > boot` is now a
  validation error instead of a VM that would break on hotplug. (Related kernel patch, unmerged as of 2026-10:
  "RISC-V: KVM: Reject vCPU creation after AIA initialization", Andrew Jones, LKML 2026-09-25.)
- **Docs** (`docs: riscv: ...`): topology/NUMA examples, the guest-interrupt-file limit, and no CPU hotplug.

New unit tests: `cpu-map` layout, `cpu-map` skipped on mismatch, ISA properties, NUMA nodes/distance map/PCI
affinity (arch), config rejection (vmm). All FDT tests parse the generated blob with `fdt_parser`.

---

## PR C description (ready to paste)

**Title:** `riscv64: Describe the configured APLIC sources and report failing AIA attributes`

- **`riscv,num-sources`** (`arch, hypervisor: riscv64: Describe the APLIC sources configured in KVM`): the FDT
  always said 96 sources (the `// TODO complete num-srcs` from the Stage 2 list, #6978). Since `085642dd4`, KVM
  gets the requested 1023 clamped to the host's guest MSI identities: 255 on QEMU virt, **63 on the SpacemiT K3**.
  So the guest was told about sources that don't exist (K3), or couldn't use most of them (QEMU). The configured
  number is now exposed through `Vaia::aplic_num_sources()` and used in the FDT. KVM accepts sources
  `1..=nr_sources` (`aplic->nr_irqs = nr_sources + 1`), which is what `riscv,num-sources` means.
  Checked: the guest's APLIC driver now reports "255 interrupts forwarded" in QEMU (was 96).
- **Diagnostics** (`hypervisor: riscv64: Report which AIA device attribute failed`): the error for a rejected AIA
  attribute didn't say which one (`Vaia error SetDeviceAttribute(SetDeviceAttribute(Invalid argument (os error
  22)))`). On the K3, before `085642dd4`, that's how the source-count limit showed up, and it wasn't obvious.
  It now reads `Failed setting AIA device attribute config srcs (group 0, attr 2)` → `Invalid argument`.

New KVM unit test `test_aplic_num_sources` (passes on riscv64 KVM).

---

## Testing (fill in hardware results)

**Emulated** (done, 2026-10-28):
- Nested setup: QEMU 10.2 `virt,aia=aplic-imsic,aia-guests=7`, `rva23s64,sv48=off,sv57=off` (set up like the K3:
  7 guest interrupt files per hart, Sv39). Ubuntu 26.04 host kernel 7.0 with KVM, Ubuntu 26.04 guest kernel.
- Results:
  - `--cpus boot=1/2/4/8/16`: all harts online, IPIs on every CPU, all-CPU load, clean poweroff.
  - Topology `1:4:2:1` and `2:2:1:2`: guest `cluster_id`/`core_id`/`thread_siblings`/`physical_package_id` correct.
  - 2 NUMA nodes: per-node CPUs and memory, distance matrix `10 20 / 20 10`, PCI devices on the right node.
  - virtio-blk with 4 queues: each queue's MSI-X arrives on its managed-affinity CPU (MSIs reach every hart).
  - Reboot with 4 vCPUs; pause/resume through the API under load (no RCU stalls).
  - Serial: no spurious interrupts; host-to-guest input works (socket mode).
  - hypervisor crate KVM unit tests (`test_create_aia`, `test_get_and_set_regs`, new `test_get_isa_extensions`,
    new `test_aplic_num_sources`) pass on riscv64 KVM.
  - APLIC: guest sees 255 sources (= KVM config); a scratch build without the clamp shows the new
    `config srcs` error chain.
- x86_64: build, clippy and the `arch`/`vmm` config tests are unchanged and clean.

**Hardware** (TODO: SpacemiT K3, Spacemit vendor host kernel 6.18):
- [ ] same matrix natively (boot=1/4/8, `topology=1:4:2:1`, NUMA, disk MSI, reboot, pause/resume)
- [ ] no `nobody cared` on ttyS0
- [ ] boot=16 (2× over-commit of the 8 X100 harts) and several VMs to exceed 7 guest files per hart
      (AUTO mode → software-file fallback)
- [ ] guest ISA list vs. host `/proc/cpuinfo` (which host-only extensions are now hidden)
- [ ] CH log says "Configuring AIA interrupt sources: 63"; guest APLIC reports 63; ttyS0 interrupts work
      (this is what failed with `IRQ_NUM = 1023` before upstream's clamp, and lost serial IRQs with `IRQ_NUM = 7`)

## LLM disclosure (CONTRIBUTING.md) — edit to taste

> This work was done with the help of an LLM coding agent (Claude): it investigated the issues, wrote the code,
> tests and commit messages, and ran the tests in an emulated riscv64 KVM host. I reviewed all changes and
> validated them [on SpacemiT K3 hardware].

## Background / context for reviewers

- Tracking issue: https://github.com/cloud-hypervisor/cloud-hypervisor/issues/6978 (riscv64 Stage 2).
  These PRs aren't on its TODO list; they come from getting SMP guests working well. Correction for the
  issue's status: ACPI isn't actually wired up on riscv64 (`Vm::create_acpi_tables` is `cfg(not(riscv64))`).
- Things deliberately **not** in these PRs: AIA snapshot/restore (closed PR #8452), TPM, guest `mmu-type` (hardcoded `sv48`, harmless since Linux probes satp), and
  `get_host_cpu_phys_bits()` (hardcoded 40; could use `KVM_CAP_VM_GPA_BITS`).
- Possible reviewer questions:
  - *Why a name table instead of kvm-bindings constants?* kvm-bindings 0.14.2 stops at ID 71; the table is
    indexed by the uAPI enum, and the code probes each ID, skipping errors.
  - *Why not fix the irqfd issue in KVM only?* Existing and vendor kernels (e.g. the SpacemiT K3 6.18 kernel)
    have the same code, and QEMU/kvmtool already use `KVM_IRQ_LINE` on riscv64.
  - *Seccomp surface:* `KVM_IRQ_LINE` is only added on riscv64 with KVM.
  - *Newer stable clippy (1.99)* flags pre-existing riscv-only code (`hypervisor/src/kvm/riscv64/aia.rs` etc.).
    That's not from these commits; CI doesn't run clippy for riscv64.
