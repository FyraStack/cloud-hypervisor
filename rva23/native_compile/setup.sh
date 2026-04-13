#sudo apt update
#sudo apt upgrade


# stuff for compiling cloud-hypervisor
# perl-core is for compiling openssl, the other stuff was from cross-compile so it might not be necessary?

sudo apt install -y bash gcc musl-dev rustup perl-core


# install rust
rustup-init --profile minimal --target riscv64a23-unknown-linux-gnu -y


# stuff for running cloud hypervisorvb
# these just work
sudo apt install -y libvirt-daemon-system libvirt-clients bridge-utils virt-manager

 
# new problem. something in the Kvm::new() function fails.
# this isnt part of cloud-hypervisor, its part of a dependency and has unsafe code.
# I need to setup remote debug unfortunately.


