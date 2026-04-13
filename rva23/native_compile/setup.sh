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
