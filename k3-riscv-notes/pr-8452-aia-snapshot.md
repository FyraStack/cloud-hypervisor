# PR #8452 — "hypervisor: riscv64: implement AIA snapshot state save and restore"

- PR: https://github.com/cloud-hypervisor/cloud-hypervisor/pull/8452 — **closed 2026-07-15, unmerged** (author unresponsive to review feedback; pings by rbradford 07-13/07-15)
- Author branch: https://github.com/mengzhuo/cloud-hypervisor/tree/aia (full PR state; tarball diffed against local main on 2026-10-28)
- Patches saved in `./patches/`:
  - `8452-3987aab-aia-snapshot-v1.patch` (Jun 15; config-level state only)
  - `8452-59ab544-aia-snapshot-v2.patch` (identical to v1, repush)
  - `8452-68c8acf-aia-snapshot.patch` (**final, 334 lines** — full register save/restore)

## Review feedback status in the final version (68c8acf)

| Reviewer comment | Addressed? |
|---|---|
| sboeuf: don't expose state struct fields publicly | ✅ fields private in final |
| sboeuf: why save vcpu_count/aplic_addr/imsic_addr if unused in set_state | ✅ final saves only `imsic_num_ids` + register vectors |
| rbradford: "These registers are u64, no?" (imsic get/set) | ❌ **still u32 via `as u64` pointer cast — real bug** |
| rbradford: APLIC "skip on -ENOENT, it's sparse — cross-reference the kernel" | ❌ **still `Err(_) => break` — aborts whole scan on first error** |
| rbradford: APLIC aliases "could clear bits — save a specific set" | ❌ full-range scan 0x0..=0x3FFC still |
| rbradford: "Why are you touching ARM code?" (get_reg_list E2BIG retry at `kvm/mod.rs:3150`) | ❌ still there |
| (enabler, not in review): riscv reg-list 200→300 w/ E2BIG retry; `is_non_core_register` skip non-U64 (u32 timer regs) instead of assert | ✅ legitimate riscv64 enabling fix, keep |

## Real bugs in the final version (with kernel evidence)

1. **IMSIC values are u64, code uses u32.** Kernel `kvm_riscv_aia_imsic_rw_attr()`
   (`linux/arch/riscv/kvm/aia_imsic.c:951`) takes `unsigned long *val`. The PR's
   `get_imsic_reg`/`set_imsic_reg` pass `&raw mut val as u64` for a `u32 val` →
   kernel reads/writes 8 bytes into a 4-byte stack variable = **stack corruption on
   get, OOB read on set**. (APLIC is correctly u32: `kvm_riscv_aia_aplic_get_attr`
   takes `u32 *v`.) Fix: u64 values for IMSIC (e.g. `Vec<(u32, u32, u64)>`), u32 for
   APLIC.
2. **APLIC `Err(_) => break` kills the snapshot.** Kernel `aplic_mmio_read_offset()`
   (`linux/arch/riscv/kvm/aia_aplic.c`) returns `-ENODEV` for any offset outside the
   valid blocks (domaincfg, sourcecfg[1..N], setip/clrip/setie/clrie words, genmsi,
   target[1..N]) and `-EOPNOTSUPP` for unaligned. The PR scans every 4-byte offset up
   to 0x3FFC; the first gap (e.g. offset 0x04, right after domaincfg at 0x0) errors →
   `break` → **only domaincfg ever saved, rest silently dropped**. Fix: `continue` on
   error (or, better, see #3).
3. **APLIC alias hazard / full-range scan.** Per rbradford: save a **specific offset
   set** instead of scanning: `domaincfg`, `genmsi`, `sourcecfg[1..=nr_irqs-1]`,
   `setip words`, `target[1..=nr_irqs-1]`, (setie words if needed). SET*/CLR* read as
   0 so they drop out of a "non-zero only" save naturally; a fixed set also keeps the
   snapshot deterministic and avoids writing back aliases. `nr_irqs`/`nr_words` are
   known at save time (from the KVM config attributes), so the set is computable.
4. **Scope creep: ARM `get_reg_list` E2BIG retry** (`kvm/mod.rs` hunk @3150). Drop it;
   keep only the riscv64-specific reg-list growth (200→300) + `is_non_core_register`
   tolerance.

## Things that are fine (verified against the tree)

- **Restore ordering is OK.** Restore path: `Vm::new_from_memory_manager` →
  `create_boot_vcpus(snapshot)` (vCPUs created + state restored, `vm.rs:898`) →
  `init_kvm → create_devices(snapshot)` (`vm.rs:1048`) → `add_interrupt_controller`
  restores AIA. Kernel IMSIC get/set require the vCPU to exist
  (`kvm_get_vcpu_by_id` → `-ENODEV`), so this ordering is actually a requirement, and
  CH satisfies it. (Boot path: no snapshot → no `set_state` call.)
- `restore_vaia()` on the `Aia` device **does exist in the author branch**
  (`devices/src/aia.rs:66`) — the single-commit patch alone wouldn't compile, the
  branch does. Mirrors the aarch64 `restore_gic` pattern (saved vCPU states arg unused).
- `Snapshottable for Aia` impl (id = "aia", snapshot via `vaia.state()`) matches the
  aarch64 GIC precedent.
- `snapshot()` uses `state().unwrap()` — panics on error; consider mapping to
  `MigratableError` like the GIC path does.
- Unit test `test_state_roundtrip` is weak (only compares counts/ids, no register
  values); a stronger test would set known IMSIC/APLIC regs, save, reset, restore,
  re-read. Integration test: riscv64 snapshot/restore case in `tests/` (aarch64 has
  the pattern; needs AIA hardware or QEMU w/ H+AIA emulation).

## Plan to finish it (on top of current main)

1. Rebase the branch onto current main (branch base is stale: predates x86 tdx/smbios
   removal, qcow refactor, etc. — ~64 files diverge).
2. Fix the 4 bugs above (u64 IMSIC; APLIC continue-on-error or fixed offset set; drop
   ARM hunk).
3. Keep: private struct fields, riscv reg-list growth, `is_non_core_register` skip,
   `restore_vaia`, device-manager wiring, `_AIA_SNAPSHOT_ID = "aia"`.
4. Error handling: replace `unwrap()`s in `Aia::snapshot()`/`restore_vaia()` with
   mapped errors.
5. Strengthen unit test; add riscv64 snapshot/restore integration test (gated, like
   aarch64 migration tests).
6. Note for the PR: mention it revives #8452 with reviewer feedback incorporated
   (CONTRIBUTING.md disclosure applies; no Co-authored-by trailer).

## Open questions

- CH never sets the AIA mode; `init_device_attributes` only reads back the mode KVM
  selected (EMUL/HWACCEL). Kernel `rw_attr` works in both modes (swfile vs vsfile),
  so the register-level save/restore covers both.
- `imsic_num_ids` is re-read from KVM at device creation on restore (init happens in
  `Aia::new` before `set_state`), so saving it is belt-and-braces; fine.
- Whether to also persist APLIC `nr_sources`/config: not needed — config is identical
  at restore (same `VmConfig`), device re-inited with same params.
