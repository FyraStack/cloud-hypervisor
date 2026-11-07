# Handoff — Cloud Hypervisor riscv64 multicore (updated 2026-10-28, evening)

Goal (Caleb, Fyra Labs): **multi-vCPU riscv64 VMs on SpacemiT K3 boards (RVA23)** with cloud-hypervisor,
plus other cheap Stage 2 (issue #6978) wins. CPU hotplug: not needed. No K3 hardware yet (expected in 1–2 days);
everything is validated on an emulated K3-like host (see `/workspace/rvtest/README.md`).

## State

Branch `riscv-multicore` in `/workspace/cloud-hypervisor` (base `cb5b81583` = origin/main snapshot),
**8 local commits, not pushed** (no creds in VM — Caleb pushes from the host). DCO signed-off with Caleb's
git identity; each commit builds alone, nightly rustfmt clean, gitlint clean, x86 clippy/tests clean.

1. `arch, hypervisor, vmm: riscv64: Describe guest ISA from KVM in the FDT` — ISA from KVM `ISA_EXT` one-regs
   (`Vcpu::get_isa_extensions`), emits `riscv,isa-base` + `riscv,isa-extensions` (+ legacy `riscv,isa`).
   Fixes: Ubuntu 26.04 kernel (RISCV_ISA_FALLBACK=n) couldn't boot at all; host ISA leaked to guest.
2. `... Describe the vCPU topology in the FDT` — `cpu-map`; **dies → clusters** (so `topology=1:4:2:1` = K3 X100 layout).
3. `... Describe guest NUMA nodes in the FDT` — numa-node-id on cpus/memory/PCI host bridges + distance-map.
4. `vmm: config: Reject CPU hotplug on riscv64` — max_vcpus must equal boot_vcpus.
5. `devices, hypervisor, vmm: riscv64: Pulse AIA wired interrupts` — **serial IRQ storm fix**: riscv KVM's
   `kvm_arch_set_irq_inatomic()` raises IRQCHIP-routed irqfds but never lowers them → level source stuck high →
   guest APLIC-MSI EOI `setipnum_le` re-pends forever (~100k spurious IRQs, "irq 11: nobody cared").
   Now `KVM_IRQ_LINE` 1→0 pulse (new `Vm::set_irq_line`), seccomp allows it for vmm/vcpu/serial-manager threads.
   Same kernel code in the K3 vendor 6.18 kernel. A kernel-side fix (pulse in inatomic for IRQCHIP) would be
   upstreamable too.
6. `docs: riscv: Document multiple vCPUs, topology and NUMA`.
7. `hypervisor: riscv64: Report which AIA device attribute failed` — based on Caleb's FyraStack `06d1a962f`.
8. `arch, hypervisor: riscv64: Describe the APLIC sources configured in KVM` — Stage 2 #3 (num-srcs) done.
   Caleb's old K3 workaround `IRQ_NUM = 7` (FyraStack `a7704fd37`) is superseded by upstream `085642dd4` (clamp to
   KVM limit = 63 on K3) and must NOT be reapplied: serial is APLIC source 9 → its IRQs get dropped.

PR draft (descriptions, checklist, testing): `notes/pr-draft-riscv-multicore.md`; patch copies in
`notes/patches/riscv-multicore/`.

Validated in the emulator (L1 = QEMU virt AIA, 7 guest files/hart, Sv39, Ubuntu 7.0 KVM; L2 = Ubuntu 7.0 kernel):
boot=1/2/4/8/16, topology (clusters/SMT/sockets), 2-node NUMA (+PCI affinity), virtio-blk MSI-X delivered to
each hart per managed affinity, reboot with 4 vCPUs, pause/resume under load, serial input (socket mode),
SBI PMU visible in guest (22 fw + 18 hw counters), hypervisor KVM unit tests (3) pass in L1.

## K3 facts that matter (details: `notes/k3-platform.md`)

- 8× X100 (harts 0-7, have H) + 8× A100 AI cores (no H). Vendor kernel patches make KVM work only on X100;
  **mainline KVM refuses to load on K3**. vCPU threads are confined to CPUs 0-7 (setting affinity to 8-15 → EINVAL).
- 7 VS interrupt files per X100 hart; Sv39 only; vendor `KVM_RISCV_VTVM` traps guest satp/sfence.vma (perf cost).

## Next steps

1. **On K3 hardware** (first thing): build branch natively, boot with the vendor kernel host; run the matrix in
   `rvtest` (boot=1/4/8, `topology=1:4:2:1`, NUMA, rvdisk, reboot, pause/resume); check `nobody cared` absent;
   try boot=16 (2× overcommit) and several VMs to exceed 7 guest files per hart (AUTO → SW-file fallback);
   measure VTVM cost. Check `--cpus affinity=` with A100 CPUs gives a clear error.
2. Open items not done: AIA snapshot/restore (revive PR #8452, see `pr-8452-aia-snapshot.md`), TPM (#5), ACPI (**not actually done on riscv64** — see status file), UEFI unverified.
3. Possible extra: guest `mmu-type` hardcoded "sv48" (harmless, Linux probes), `get_host_cpu_phys_bits()` hardcoded
   40 (K3 Sv39x4 = 41, OK) — could use `KVM_CAP_VM_GPA_BITS`.

## Traps

- Never run broad find/grep over `/workspace/k3-repo-backup` (46 GB): exhausts host virtiofsd fds
  ("Too many open files in system" everywhere). Fix: `sync; echo 2 > /proc/sys/vm/drop_caches`.
- Fedora's riscv64 cross gcc has no glibc sysroot → build in the container (`rvtest/build-ch.sh`).
- Clippy 1.99 flags pre-existing riscv-only code (aia.rs etc.) — not ours; `rvtest/clippy-ch.sh` allows those lints.
- CH `--serial tty` ignores stdin unless it is a tty; use `SERIAL=socket=...` + `/share/serial-input-test.py`.
