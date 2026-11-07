# Cloud Hypervisor RISC-V Stage 2 — working notes

Fyra Stack's working copy for the riscv64 Stage 2 work (upstream
https://github.com/cloud-hypervisor/cloud-hypervisor/issues/6978).

## Layout

| Path | What |
|------|------|
| `/workspace/cloud-hypervisor` | repo checkout (origin/main @ `cb5b81583`, 2026-10-28; local branch `main`) |
| `/workspace/linux` | **shallow (depth 1) clone of torvalds/linux @ `65ba8f34e`** (2026-10-27) — kernel reference |
| `/workspace/notes/riscv-stage2-status.md` | status of the 9 Stage 2 TODOs, with code evidence |
| `/workspace/notes/pr-8452-aia-snapshot.md` | evaluation of the closed AIA-snapshot PR + kernel cross-reference |
| `/workspace/notes/multicore-riscv.md` | what actually blocks (and doesn't block) multicore riscv64 VMs |
| `/workspace/notes/patches/` | saved PR patches (v1/v2/final) |
| `/workspace/notes/k3-platform.md` | SpacemiT K3 facts relevant to KVM/CH (from the vendor kernel in `/workspace/k3-repo-backup`) |
| `/workspace/notes/pr-draft-riscv-multicore.md` | PR draft for the `riscv-multicore` branch (descriptions, checklist, testing) |
| `/workspace/notes/HANDOFF.md` | current state of the multicore branch + next steps |
| `/workspace/rvtest/` | emulated K3-like riscv64 test bed (scripts + README) |

## Upstream references

- Stage 2 tracking issue: https://github.com/cloud-hypervisor/cloud-hypervisor/issues/6978
- Closed AIA snapshot PR: https://github.com/cloud-hypervisor/cloud-hypervisor/pull/8452
  (closed 2026-07-15, author unresponsive to review feedback; final commit `68c8acf`)
- Author branch (full state of the PR): https://github.com/mengzhuo/cloud-hypervisor/tree/aia
- Fyra Stack fork (riscv docs/experiments): https://github.com/FyraStack/cloud-hypervisor/tree/riscv
  — treat fork docs as source of truth via link, not by copying here (they go stale).

## Kernel reference (fast paths)

- KVM riscv AIA device (APLIC config/get/set): `linux/arch/riscv/kvm/aia_device.c`
- KVM riscv AIA APLIC emul (sparse offset handling): `linux/arch/riscv/kvm/aia_aplic.c`
  - `aplic_mmio_read_offset()`: `-ENODEV` on invalid offsets, `-EOPNOTSUPP` unaligned;
    SET*/CLR* registers read as 0 (write-only)
- KVM riscv AIA IMSIC: `linux/arch/riscv/kvm/aia_imsic.c`
  - `kvm_riscv_aia_imsic_rw_attr()`: takes `unsigned long *val` (**u64**), requires the
    vCPU to exist (`kvm_get_vcpu_by_id` → `-ENODEV` otherwise)
  - `imsic_mrif_isel_check()`: valid iselects = EIDELIVERY(0x80), EITHRESHOLD(0x81),
    EIP0-63(0x82..0x8F), EIE0-63(0xC2..0xCF); unknown → `-ENOENT`, odd (64-bit) → `-EINVAL`
- uapi: `linux/arch/riscv/include/uapi/asm/kvm.h`
  - `KVM_DEV_RISCV_AIA_GRP_APLIC = 3` (u32 values), `KVM_DEV_RISCV_AIA_GRP_IMSIC = 4` (u64 values)
  - `KVM_DEV_RISCV_AIA_IMSIC_ISEL_BITS = 12`, attr = (vcpu << 12) | iselect
- KVM riscv guest PMU: exposed via the **SBI PMU extension** (`vcpu_pmu.c`,
  perf-backed; registered in `vcpu_sbi.c`) — no dedicated uapi like
  `KVM_ARM_VCPU_PMU_V3`, but the guest's SBI PMU driver works without extra
  CH/KVM work. See `multicore-riscv.md`.
- KVM riscv SBI IPI + HSM extensions (SMP bringup): `arch/riscv/kvm/vcpu_sbi.c`

## Gotchas

- `git fetch` from this VM fails (no remote credentials); the checkout's `origin/main`
  is a snapshot — re-verify upstream state via web/patches when it matters.
- The repo folder was renamed `clouid-hypervisor` → `cloud-hypervisor` on 2026-10-28.
