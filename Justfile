# cSpell:ignore mcopy readelf virtio bootx sgdisk mformat

# Default architecture for the kernel. Intended to be overridden at command line if needed.
arch := "x86_64"

# Build profile: "dev" or "release". Intended to be overridden at command line if needed.
profile := "dev"

# Values derived from the architecture
kernel-target := arch + "-polaris-kernel"
limine-image-name := if arch == "x86_64" { "BOOTX64.EFI" } else { error("Unsupported architecture: {{arch}}") }

# Other constants
build-std := "core,alloc,compiler_builtins"
build-std-features := "compiler-builtins-mem"
profile-dir := if profile == "release" { "release" } else { "debug" }
release-flag := if profile == "release" { "--release" } else { "" }
kernel-cargo-args := "-Z build-std=" + build-std + " -Z build-std-features=" + build-std-features + " --target " + kernel-target + " " + release-flag

# Default task: build the full OS image
build: build-image

# Checks all packages in the workspace for errors
check:
    cargo check \
        {{kernel-cargo-args}} \
        --workspace
    cargo check --test --workspace

# Run all tests in the workspace
test:
    cargo test --lib -p pmm

# Launch the kernel in QEMU with the debugger stub enabled
monitor *FLAGS: build-image
    qemu-system-{{arch}} \
        -m 512M \
        -drive file=artifacts/{{arch}}/polaris.img,format=raw,if=virtio \
        -bios $OVMF_DIR/ovmf-code-{{arch}}.fd \
        -net none \
        -monitor stdio

# Run the OS image in QEMU, passing any additional flags given to QEMU
run *FLAGS: build-image
    qemu-system-{{arch}} \
        -m 512M \
        -drive file=artifacts/{{arch}}/polaris.img,format=raw,if=virtio \
        -bios $OVMF_DIR/ovmf-code-{{arch}}.fd \
        -net none \
        -serial stdio \
        {{FLAGS}}

# Build the full development OS image, including the kernel and bootloader
build-image: build-kernel _ensure-image
    mcopy -i artifacts/{{arch}}/polaris.img@@1M -D o artifacts/{{arch}}/polaris.kernel ::/polaris/polaris.kernel
    mcopy -i artifacts/{{arch}}/polaris.img@@1M -D o artifacts/{{arch}}/polaris.symtab ::/polaris/polaris.symtab
    mcopy -i artifacts/{{arch}}/polaris.img@@1M -D o crates/kernel/limine.conf ::/EFI/BOOT/limine.conf

# Build the kernel for the specified target
build-kernel: _mk-artifacts-dir
    cargo build \
        {{kernel-cargo-args}} \
        --package polaris_kernel
    nm --numeric-sort "target/{{kernel-target}}/{{profile-dir}}/polaris.kernel" | c++filt > artifacts/{{arch}}/polaris.kernel.nm
    objdump -S "target/{{kernel-target}}/{{profile-dir}}/polaris.kernel" > artifacts/{{arch}}/polaris.kernel.asm
    cp "target/{{kernel-target}}/{{profile-dir}}/polaris.kernel" artifacts/{{arch}}/polaris.kernel
    cargo run --package symbolicator -- generate \
        --input artifacts/{{arch}}/polaris.kernel \
        --output artifacts/{{arch}}/polaris.symtab
    strip --strip-debug artifacts/{{arch}}/polaris.kernel

dump-kernel-elf: build-kernel
    readelf -a "artifacts/{{arch}}/polaris.kernel"

# Clean build artifacts
clean:
    rm -rf artifacts/{{arch}} target/{{kernel-target}}
    cargo clean

# Create the artifacts directory if it doesn't exist
_mk-artifacts-dir:
    mkdir -p artifacts/{{arch}}

_reset-image: _mk-artifacts-dir
    [ -f artifacts/{{arch}}/polaris.img ] && rm artifacts/{{arch}}/polaris.img || true
    dd if=/dev/zero of=artifacts/{{arch}}/polaris.img bs=1M count=64
    sgdisk -N 1 -t 1:EF00 -c 1:"EFI System Partition" artifacts/{{arch}}/polaris.img
    mformat -i artifacts/{{arch}}/polaris.img@@1M -F -v BOOT ::
    mmd -i artifacts/{{arch}}/polaris.img@@1M ::/EFI
    mmd -i artifacts/{{arch}}/polaris.img@@1M ::/EFI/BOOT
    mmd -i artifacts/{{arch}}/polaris.img@@1M ::/polaris
    mcopy -i artifacts/{{arch}}/polaris.img@@1M vendor/limine/{{limine-image-name}} ::/EFI/BOOT/

_ensure-image:
    [ -f artifacts/{{arch}}/polaris.img ] || just _reset-image