#!/bin/bash


#sudo apt update
#sudo apt-get install bash gcc musl-dev rustup zig perl-FindBin

sudo dnf install bash gcc musl-devel rustup zig perl-core perl

rustup-init --profile minimal --target riscv64a23-unknown-linux-gnu -y
