# RISC-V multicore: what actually blocks it (investigated 2026-10-28)

Question: "I remember someone mentioning that there was something blocking
multicore in RISC-V" — assessment against CH main (`cb5b81583`) and
mainline kernel (`65ba8f34e`, 2026-10-27).

## Bottom line

- **Static multicore boot (N vCPUs from the start): nothing in CH or KVM
  blocks it. It's simply never been verified** — docs/riscv.md and all Stage 1
  validation use `--cpus boot=1`.
- **Two real, known restrictions** exist: HWACCEL vs-file exhaustion (kernel,
  documented in the `KVM_EXIT_FAIL_ENTRY_NO_VSFILE` patch, 2025) and CPU
  **hotplug after AIA init** (kernel fix in flight, LKML 2026-09, not yet
  merged).

## Why static multicore works (all pieces verified in code)

1. **CH ordering satisfies KVM's requirements.** `Vm::new_from_memory_manager`:
   `create_boot_vcpus()` (`vm.rs:898`) → `init_kvm → create_devices()`
   (`vm.rs:1048`) creates the AIA. Kernel `aia_init()`
   (`linux/arch/riscv/kvm/aia_device.c:226`) iterates *existing* vCPUs to set up
   per-vCPU IMSIC contexts, and `aia_create()` (`aia_device.c`) rejects
   creation once a vCPU has run (`-EBUSY`). CH is inside the window.
2. **Per-hart boot protocol is correct.** `setup_regs`
   (`hypervisor/src/kvm/mod.rs:2784`): a0=hart id, pc=entry, a1=FDT for *every*
   vCPU — the standard RISC-V KVM multicore entry protocol.
3. **FDT enumerates all harts.** `create_cpu_nodes` iterates `0..num_vcpu`;
   `interrupts-extended` lists a phandle per vCPU; `imsic_num_ids` comes from
   KVM. ACPI MADT: `hart_index_bits` computed from `boot_vcpus` (`cpu.rs:1977`).
4. **SMP bringup mechanism exists in KVM.** SBI v0.1 **IPI** and **HSM**
   (hart start) extensions registered in `linux/arch/riscv/kvm/vcpu_sbi.c`
   (lines 50, 62) — the guest kernel uses these to start secondary harts.
5. **AIA mode is safe by default.** Kernel default
   (`aia_device.c:637`): `AUTO` if the host has guest files (hgei), else `EMUL`.
   In `AUTO`, a failed vs-file allocation **falls back to the software file**
   (`aia_imsic.c:820-840`), so exceeding the hgei count degrades performance
   rather than breaking.

## Real restriction 1: HWACCEL vs-file (hgei) exhaustion

- In `KVM_DEV_RISCV_AIA_MODE_HWACCEL`, a vCPU whose host hart has no free
  guest file cannot run the entry at all:
  `KVM_EXIT_FAIL_ENTRY` + `hardware_entry_failure_reason =
  KVM_EXIT_FAIL_ENTRY_NO_VSFILE` (introduced by the 2025 LKML patch
  "RISC-V: KVM: Introduce KVM_EXIT_FAIL_ENTRY_NO_VSFILE").
- The Linux scheduler is not GIF-aware: e.g. 8 harts × 5 GIFs means >5 vCPUs
  scheduled on one hart breaks a HWACCEL-only guest. QEMU's docs discuss this
  (`-machine riscv-aia=hwaccel`).
- CH doesn't set the mode explicitly (`init_device_attributes` only reads back
  the kernel-selected mode), so CH guests get AUTO → sw-file fallback, i.e.
  **multicore works, at reduced AIA performance past the hgei count**.
- Mitigations if it ever matters: force EMUL, or let the VMM react to the
  fail-entry exit (reschedule); CH currently has no handler for it.

## Real restriction 2: CPU hotplug after AIA init (kernel fix in flight)

- `aia_init()` only sets up IMSIC context for vCPUs existing *at that moment*.
  A vCPU created later has none: `kvm_riscv_vcpu_aia_init()` at vcpu-create
  (`vcpu.c:184`) is the light path (hart_index only); the full
  `kvm_riscv_vcpu_aia_imsic_init()` is only called from `aia_init()`
  (`aia_device.c:285`).
- LKML 2026-09-25, Andrew Jones: "[PATCH] RISC-V: KVM: Reject vCPU creation
  after AIA initialization" (`kvm_arch_vcpu_precreate()` → `-EBUSY` if
  `kvm_riscv_aia_initialized()`) — **not merged as of our clone (2026-10-27)**.
- CH-side implications for riscv64 AIA hotplug (beyond the kernel fix):
  - AIA is created with `config.boot_vcpus` (`device_manager.rs`
    `add_interrupt_controller`) → hot-added vCPUs exceed the device's vcpu
    count (FDT `interrupts-extended` is static too).
  - FDT direct-boot guests have no hotplug story anyway; ACPI-UEFI guests
    would need the AIA sized to `max_vcpus` and guest-side IMSIC sizing to
    match.
  - Practical stance: treat CPU hotplug on riscv64 AIA VMs as **blocked until
    the kernel patch lands + a CH design decision**; consider gating
    `--cpu-hottable`/dynamic for riscv64 or documenting it.

## Things that are NOT blockers (checked)

- SBI PMU extension: registered in KVM (`vcpu_sbi.c:66`, `vcpu_pmu.c` is a
  perf-backed SBI PMU). Guest gets a PMU via SBI without any CH/KVM uapi work
  — this corrects the earlier "kernel-blocked" note for Stage 2 TODO #6: it's
  mostly "verify + possibly advertise" (SBI auto-detects), not a missing uapi.
- VmGenId exclusion, vmgenid regeneration skipped on riscv64 (`vm.rs:2587`) —
  unrelated to multicore.
- The `aia_create` "vCPU already ran" check: CH ordering satisfies it.

## Suggested verification (when on riscv64 AIA hardware)

1. `--cpus boot=4` (or higher) direct-kernel boot, guest `nproc` +
   `stress-ng`/smpboot check; confirm SBI IPI bringup in the log.
2. Boot past the hgei count in AUTO mode; confirm sw-file fallback works
   (KVM trace / `KVM_EXIT_FAIL_ENTRY` absence) and measure AIA perf drop.
3. Attempt CPU hotplug; document the -EBUSY/broken-IMSIC behaviour until the
   kernel patch lands.
