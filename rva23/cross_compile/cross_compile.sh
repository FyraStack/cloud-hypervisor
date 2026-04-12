#!/bin/bash

# todo: export a proper like root or something for the files.

export CARGO_BUILD_TARGET="riscv64a23-unknown-linux-gnu"
export CC_riscv64a23_unknown_linux_gnu="/home/caleb/Code/fyra/fork/cloud-hypervisor/rva23_cross_compile/zig-cc-rva23.sh"
export CARGO_TARGET_RISCV64A23_UNKNOWN_LINUX_GNU_RUSTFLAGS="-C linker=/home/caleb/Code/fyra/fork/cloud-hypervisor/rva23_cross_compile/zig-cc-rva23.sh"

cargo build --release --verbose


# cross compile or just in 
# wait