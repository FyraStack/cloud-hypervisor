# RISC-V Stage 2 TODO status (as of local origin/main @ cb5b81583, 2026-10-28)

Issue: https://github.com/cloud-hypervisor/cloud-hypervisor/issues/6978

| # | TODO | Status | Evidence |
|---|------|--------|----------|
| 1 | fdt: CPU freq from cpu timer regs | ✅ done | `vm.rs:1931` queries `vcpu.get_timebase_frequency()` → KVM `KVM_REG_RISCV_TIMER.frequency` (`hypervisor/src/kvm/mod.rs:2722`); 10 MHz fallback. Merged as `14aa30cd2` |
| 2 | fdt: num-ids | ✅ done | `fdt.rs:238` uses `imsic_num_ids()` from KVM `KVM_DEV_RISCV_AIA_CONFIG_IDS` (`kvm/riscv64/aia.rs:60-67`) |
| 3 | fdt: num-srcs | ✅ done on branch `riscv-multicore` (commit 8); was ❌ open: | `fdt.rs` still has `// TODO complete num-srcs`, hardcodes `riscv,num-sources = 96`. Partial: `085642dd4` clamps *device-side* SRCS to KVM capacity, but FDT property is still hardcoded. Fix: expose the clamped nr_irqs via the `Vaia` trait and use it in the FDT |
| 4 | AIA snapshotting | ❌ open | `impl Snapshottable/Migratable for Aia {}` empty stubs; `KvmAiaImsics::state()/set_state()` = `unimplemented!()`; `_AIA_SNAPSHOT_ID = ""`; restore path panics at `device_manager.rs:1905`. PR #8452 implements it but was closed unmerged — see `pr-8452-aia-snapshot.md` |
| 5 | TPM device | ❌ open (easy) | `devices/src/lib.rs:36-38`: `// TODO: TPM is not yet supported` + `#[cfg(not(riscv64))] pub mod tpm;`; `device_manager.rs:1553` gates the add; `c257cdd69` gates the vmm import. The original gate (`0042447fb`) was scheduling only ("not scheduled in this stage"), not a technical blocker. TPM is a PCI MMIO (CRB) device, arch-neutral; needs `TPM_SIZE`/`TPM_START` in `arch/riscv64/layout.rs` + un-gating the 3 cfgs |
| 6 | PMU | ❌ open (re-scoped) | `vm.rs:1929`, `device_manager.rs:1901` TODOs; the `init_pmu` path is aarch64-only (`KVM_ARM_VCPU_PMU_V3`). **Correction:** KVM riscv *does* have a guest PMU — via the **SBI PMU extension** (`vcpu_pmu.c`, perf-backed), auto-detected by the guest kernel. So #6 is mostly "verify SBI PMU works + optionally advertise", not kernel-blocked. See `multicore-riscv.md` |
| 7 | UEFI | ✅ done | `0df4b1ac4`/`2e0ec8095`/`1a63b4b2f`/`17195e1a4`: `load_firmware` is `cfg(any(aarch64, riscv64))`, `arch::riscv64::uefi` module + `UefiLoad` error variant |
| 8 | IOMMU (virtio-iommu) | ✅ done | No riscv64 gate anywhere: `devices/src/iommu` ungated; `add_pci_devices` creates `virtio_devices::Iommu` when `config.virtio_iommu_attached()` (`config.rs:3480`, arch-neutral per-device `iommu=on` check). Smmuv3/iommufd is the aarch64-only part, separate |
| 9 | ACPI | ❌ **not done** (corrected 2026-10-28) | `Vm::create_acpi_tables` is `cfg(not(riscv64))` (`vm.rs:2368`) and riscv64 boots via FDT-only `configure_system()`; the riscv MADT bits in `cpu.rs` are only reachable through the optional `fw_cfg` path; lots of dead-code warnings in `vmm/src/acpi.rs` on riscv64. Original (wrong) note: | `create_acpi_tables()` called unconditionally for riscv64 (`vm.rs:2492`); MADT gets IMSIC/APLIC entries (`cpu.rs:1985/2003`, `ACPI_RISC_V_IMSIC`/`ACPI_RISC_V_APLIC` in `acpi.rs:76-79`); GED/shutdown/PM-timer/ACPI-TAD (hotplug) added ungated. Only VmGenId excluded on riscv64 (`device_manager.rs:2110`) |

**Score (corrected): 4 done (1,2,7,8 — 7 UEFI unverified), 5 open (3,4,5,6,9).** #6 PMU: SBI PMU verified working in a guest (emulator) — mostly a non-issue.

Multicore work (not on the issue list) is on branch `riscv-multicore` — see `HANDOFF.md`.

## Suggested order for the open items

1. **#5 TPM** — easiest: ~3 cfg-gate removals + layout constants + a compile/test pass. No
   riscv hardware needed to validate the code path (device is arch-neutral PCI; functional
   test needs a riscv64 guest, but x86/aarch64 CI already covers the device itself).
2. **#3 num-srcs** — small: add e.g. `aplic_nr_sources()` to the `Vaia` trait
   (return `safe_nr_irqs` stored at `init_device_attributes`), use it in `create_aia_node`
   instead of the literal 96; keep the FDT `riscv,num-sources` consistent with the KVM
   device SRCS. Validate via `print_fdt()` / guest boot.
3. **#4 AIA snapshot** — the valuable one for snapshot/restore on riscv64; a near-complete
   closed PR exists (see `pr-8452-aia-snapshot.md`); needs 3 real bug fixes + rebase.
4. **#6 PMU** — re-scoped: KVM exposes it via SBI (guest auto-detects); verify on
   hardware, advertise if needed. No longer "kernel-blocked" (see `multicore-riscv.md`).

