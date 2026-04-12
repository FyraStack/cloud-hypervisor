#sudo apt update
#sudo apt upgrade


# stuff for compiling cloud-hypervisor
# perl-core is for compiling openssl

sudo apt install -y bash gcc musl-dev rustup perl-core


# install rust
rustup-init --profile minimal --target riscv64a23-unknown-linux-gnu -y


# stuff for running cloud hypervisorvb
# this doesnt work :(
sudo apt install -y qemu-kvm libvirt-daemon-system libvirt-clients bridge-utils virt-manager


# qemu-kvm definitely doesnt.

# works
sudo apt install -y bridge-utils virt-manager


# unknown
sudo apt install -y libvirt-daemon-system libvirt-clients 
