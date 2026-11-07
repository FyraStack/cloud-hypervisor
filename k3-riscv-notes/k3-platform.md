# SpacemiT K3 — what matters for KVM / cloud-hypervisor (2026-10-28)

Source: `/workspace/k3-repo-backup/linux-6.18` (SpacemiT vendor kernel, branch `k3-br-v1.0.y`,
6.18.3, HEAD `4158237f35b8`). DTS: `arch/riscv/boot/dts/spacemit/k3.dtsi`, `k3-cpus.dtsi`.

**Do not run broad `find`/`grep -r` over `/workspace/k3-repo-backup` (46 GB).** It exhausts the
host virtiofsd fd limit ("Too many open files in system" for everything under /workspace).
Fix: `sync; echo 2 > /proc/sys/vm/drop_caches` (fallback: `umount -l /workspace && mount -t virtiofs workspace /workspace`).

## CPUs (heterogeneous, 16 harts)

| harts | core | H ext | notes |
|---|---|---|---|
| 0-7 | X100 (cluster0/1, 64K L1) | **yes** | regular CPUs, all host tasks incl. vCPU threads |
| 8-15 | A100 (cluster2/3, `cpu-ai = "true"`) | **no** | AI cores, only threads marked HMP_AI may run there |

- ISA (X100): RVA23-ish, `h`, `smaia/ssaia`, `sstc`, `sscofpmf`, `svpbmt`, `svnapot`, `zicbo*`, V + crypto, `sdtrig`.
- **MMU: Sv39 only** (`mmu-type = "riscv,sv39"`) → G-stage Sv39x4 → **41-bit guest-physical space**.
  CH hardcodes guest `mmu-type = "sv48"` (harmless: Linux treats it as an upper limit and probes satp)
  and `get_host_cpu_phys_bits() = 40` (fits in 41). Better: query `KVM_CAP_VM_GPA_BITS`.
- `cpu-map`: 4 clusters × 4 cores, no threads. timebase 24 MHz.

## Vendor KVM patches (needed on the host; mainline KVM refuses to load on K3)

- `01f07bdc9410` "hmp: hypervisor: support hypervisor for k3" (`CONFIG_SPACEMIT_HMP=y`):
  - mainline computes the ISA as the AND over all harts → no `h` → `riscv_kvm_init` returns -ENODEV.
    Vendor patch forces `h` into the common ISA and makes `riscv_isa_extension_available(h)` true only on regular CPUs;
    `kvm_arch_enable_virtualization_cpu` is a no-op on A100.
  - `drivers/soc/spacemit/hmp.c`: non-AI threads default to `regular_cpu_mask` (0-7);
    `sched_setaffinity` to a mask outside it → **-EINVAL** (CH `--cpus affinity=` with CPUs 8-15 will fail).
- `dbc223b1bd24` `CONFIG_KVM_RISCV_VTVM` (default y on K3): sets `hstatus.VTVM`, so every guest `satp` write /
  `sfence.vma` traps to HS and is emulated (`vcpu_insn.c`). Also hfence helpers preserve hgatp mode bits.
  Smells like a HW erratum workaround; expect a **TLB-heavy guest workload performance cost**.
- Host kernel to use: the vendor kernel (or mainline + these patches). Upstream mainline won't do KVM on K3.

## AIA

- IMSIC @0xe0400000: `num-ids = 511`, `guest-index-bits = 6`, `hart-index-bits = 4`.
  DTS comment: **X100: only 7 of 63 VS-level interrupt files are valid; A100: none.**
  → at most 8 harts × 7 = 56 vCPUs with HW-accelerated IMSIC at any moment; beyond that, AUTO mode
    falls back to SW files (KVM default on hosts with hgei is AUTO, `aia_device.c:622`).
    A vCPU set > 7 per physical hart (e.g. overcommit, or several VMs) is where this bites.
- APLIC @0xe0804000: 512 wired sources, MSI mode.

## Implications for CH multicore on K3

- Realistic max without overcommit: **8 vCPUs** (8 X100 harts). Test 1/2/4/8, then 16 (2× overcommit).
- Guest topology to mirror host: `--cpus boot=8,topology=1:4:2:1`-ish (2 clusters × 4) → needs riscv `cpu-map` support in CH.
- No NUMA on the host (single node), so guest NUMA is only for feature parity, not K3 perf.
- QEMU model that approximates K3 for the testbed:
  `-machine virt,aia=aplic-imsic,aia-guests=7 -cpu rva23s64 -smp 8` (Sv39 limit: `-cpu ...,sv48=off,sv57=off`).
