// Copyright © 2024 Institute of Software, CAS. All rights reserved.
// Copyright 2020 Arm Limited (or its affiliates). All rights reserved.
// Copyright 2019 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

/// Module for the flattened device tree.
pub mod fdt;
/// Layout for this riscv64 system.
pub mod layout;
/// Module for loading UEFI binary.
pub mod uefi;

use std::collections::HashMap;
use std::fmt::Debug;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::sync::{Arc, Mutex};

use hypervisor::arch::riscv64::aia::Vaia;
use log::{Level, log_enabled};
use thiserror::Error;
use vm_memory::{Address, GuestAddress, GuestMemoryAtomic, GuestMemoryBackend};

pub use self::fdt::DeviceInfoForFdt;
use crate::{DeviceType, GuestMemoryMmap, PciSpaceInfo, RegionType};

pub const CLOUDHV_IRQCHIP_NUM_MSIS: u16 = 255;
pub const CLOUDHV_IRQCHIP_NUM_SOURCES: u8 = 96;
pub const CLOUDHV_IRQCHIP_NUM_PRIO_BITS: u8 = 3;
pub const CLOUDHV_IRQCHIP_MAX_GUESTS_BITS: u8 = 3;
pub const CLOUDHV_IRQCHIP_MAX_GUESTS: u8 = (1 << CLOUDHV_IRQCHIP_MAX_GUESTS_BITS) - 1;
pub const _NSIG: i32 = 65;

/// Errors thrown while configuring riscv64 system.
#[derive(Debug, Error)]
pub enum Error {
    /// Failed to create a FDT.
    #[error("Failed to create a FDT")]
    SetupFdt,

    /// Failed to write FDT to memory.
    #[error("Failed to write FDT to memory")]
    WriteFdtToMemory(#[source] fdt::Error),

    /// Failed to create a AIA.
    #[error("Failed to create a AIA")]
    SetupAia,

    /// Failed to compute the initramfs address.
    #[error("Failed to compute the initramfs address")]
    InitramfsAddress,

    /// Error configuring the general purpose registers
    #[error("Error configuring the general purpose registers")]
    RegsConfiguration(#[source] hypervisor::HypervisorCpuError),

    /// Error opening /proc/cpuinfo
    #[error("Error opening /proc/cpuinfo")]
    OpenCpuInfo(#[source] std::io::Error),

    /// Error reading /proc/cpuinfo
    #[error("Error reading /proc/cpuinfo")]
    ReadCpuInfo(#[source] std::io::Error),

    /// Invalid ISA string
    #[error("Invalid ISA string: {0}")]
    InvalidIsaString(String),

    /// Error parsing /proc/cpuinfo
    #[error("Error parsing /proc/cpuinfo")]
    CpuInfoParsing,
}

#[derive(Debug, Copy, Clone)]
/// Specifies the entry point address where the guest must start
/// executing code.
pub struct EntryPoint {
    /// Address in guest memory where the guest must start execution
    pub entry_addr: GuestAddress,
}

/// Configure the specified VCPU, and return its MPIDR.
pub fn configure_vcpu(
    vcpu: &dyn hypervisor::Vcpu,
    id: u32,
    boot_setup: Option<(EntryPoint, &GuestMemoryAtomic<GuestMemoryMmap>)>,
) -> super::Result<()> {
    if let Some((kernel_entry_point, _guest_memory)) = boot_setup {
        vcpu.setup_regs(
            id,
            kernel_entry_point.entry_addr.raw_value(),
            layout::FDT_START.raw_value(),
        )
        .map_err(Error::RegsConfiguration)?;
    }

    Ok(())
}

pub fn arch_memory_regions() -> Vec<(GuestAddress, usize, RegionType)> {
    vec![
        // 0 MiB ~ 256 MiB: AIA and legacy devices
        (
            GuestAddress(0),
            layout::MEM_32BIT_DEVICES_START.0 as usize,
            RegionType::Reserved,
        ),
        // 256 MiB ~ 768 MiB: MMIO space
        (
            layout::MEM_32BIT_DEVICES_START,
            layout::MEM_32BIT_DEVICES_SIZE as usize,
            RegionType::SubRegion,
        ),
        // 768 MiB ~ 1 GiB: reserved. The leading 256M for PCIe MMCONFIG space
        (
            layout::PCI_MMCONFIG_START,
            layout::PCI_MMCONFIG_SIZE as usize,
            RegionType::Reserved,
        ),
        // 1GiB ~ inf: RAM
        (layout::RAM_START, usize::MAX, RegionType::Ram),
    ]
}

// Read the first "isa" string from /proc/cpuinfo and split it into extension names,
// filtering out the H extension. Used when the hypervisor cannot report the guest ISA.
fn isa_extensions_from_host() -> Result<Vec<String>, Error> {
    let file = File::open("/proc/cpuinfo").map_err(Error::OpenCpuInfo)?;
    let reader = BufReader::new(file);

    for line in reader.lines() {
        let line = line.map_err(Error::ReadCpuInfo)?;
        let trimmed_line = line.trim();

        if trimmed_line.starts_with("isa") {
            let parts: Vec<&str> = trimmed_line.split(':').collect();
            if parts.len() == 2 {
                return isa_extensions_from_isa_string(parts[1].trim());
            }
        }
    }

    Err(Error::CpuInfoParsing)
}

// Split an ISA string such as "rv64imafdch_zicbom_zicboz" into extension names,
// dropping the H extension.
fn isa_extensions_from_isa_string(isa_string: &str) -> Result<Vec<String>, Error> {
    let mut components = isa_string.split('_');
    let single_letter = components
        .next()
        .and_then(|base| base.strip_prefix("rv64"))
        .ok_or_else(|| Error::InvalidIsaString(isa_string.to_string()))?;

    Ok(single_letter
        .chars()
        .filter(|&c| c != 'h')
        .map(|c| c.to_string())
        .chain(components.filter(|s| !s.is_empty()).map(str::to_string))
        .collect())
}

// Build the legacy "riscv,isa" string from extension names: the single-letter
// extensions in canonical order, then the multi-letter ones separated by '_'.
fn isa_string_from_extensions(extensions: &[String]) -> String {
    let mut isa_string = String::from("rv64");
    for c in "imafdqlcbjtpvh".chars() {
        if extensions.iter().any(|e| e.len() == 1 && e.starts_with(c)) {
            isa_string.push(c);
        }
    }
    for extension in extensions.iter().filter(|e| e.len() > 1) {
        isa_string.push('_');
        isa_string.push_str(extension);
    }
    isa_string
}

/// Configures the system and should be called once per vm before starting vcpu threads.
#[expect(clippy::too_many_arguments)]
pub fn configure_system<T: DeviceInfoForFdt + Clone + Debug, S: ::std::hash::BuildHasher>(
    guest_mem: &GuestMemoryMmap,
    cmdline: &str,
    num_vcpu: u32,
    device_info: &HashMap<(DeviceType, String), T, S>,
    initrd: &Option<super::InitramfsConfig>,
    pci_space_info: &[PciSpaceInfo],
    aia_device: &Arc<Mutex<dyn Vaia>>,
    timebase_frequency: u32,
    isa_extensions: &[String],
) -> super::Result<()> {
    let isa_extensions = if isa_extensions.is_empty() {
        isa_extensions_from_host()?
    } else {
        isa_extensions.to_vec()
    };
    let isa_string = isa_string_from_extensions(&isa_extensions);
    let fdt_final = fdt::create_fdt(
        guest_mem,
        cmdline,
        num_vcpu,
        &isa_string,
        &isa_extensions,
        device_info,
        aia_device,
        initrd,
        pci_space_info,
        timebase_frequency,
    )
    .map_err(|_| Error::SetupFdt)?;

    if log_enabled!(Level::Debug) {
        fdt::print_fdt(&fdt_final);
    }

    fdt::write_fdt_to_memory(&fdt_final, guest_mem).map_err(Error::WriteFdtToMemory)?;

    Ok(())
}

/// Returns the memory address where the initramfs could be loaded.
pub fn initramfs_load_addr(
    guest_mem: &GuestMemoryMmap,
    initramfs_size: usize,
) -> super::Result<u64> {
    let round_to_pagesize = |size| (size + (super::PAGE_SIZE - 1)) & !(super::PAGE_SIZE - 1);
    match guest_mem
        .last_addr()
        .checked_sub(round_to_pagesize(initramfs_size) as u64 - 1)
    {
        Some(offset) => {
            if guest_mem.address_in_range(offset) {
                Ok(offset.raw_value())
            } else {
                Err(super::Error::PlatformSpecific(Error::InitramfsAddress))
            }
        }
        None => Err(super::Error::PlatformSpecific(Error::InitramfsAddress)),
    }
}

pub fn get_host_cpu_phys_bits(_hypervisor: &dyn hypervisor::Hypervisor) -> u8 {
    40
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arch_memory_regions_dram() {
        let regions = arch_memory_regions();
        assert_eq!(4, regions.len());
        assert_eq!(layout::RAM_START, regions[3].0);
        assert_eq!(RegionType::Ram, regions[3].2);
    }

    #[test]
    fn test_isa_extensions_from_isa_string() {
        assert_eq!(
            isa_extensions_from_isa_string("rv64imafdcvh_zicbom_zicboz").unwrap(),
            ["i", "m", "a", "f", "d", "c", "v", "zicbom", "zicboz"]
        );
        isa_extensions_from_isa_string("rv32imac").unwrap_err();
    }

    #[test]
    fn test_isa_string_from_extensions() {
        // KVM reports extensions in enum order, not canonical order.
        let extensions: Vec<String> = ["a", "c", "d", "f", "i", "m", "sstc", "v", "zicbom"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            isa_string_from_extensions(&extensions),
            "rv64imafdcv_sstc_zicbom"
        );
    }
}
