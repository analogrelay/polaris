# Polaris

A hobby operating system written in Rust.

## Overview

Polaris is a personal OS project intended for learning and experimentation. It targets x86_64 architecture and uses the Limine bootloader.

## Features

### Memory Management
- Physical memory manager (PMM) with frame allocation
- Virtual address space management
- Block allocator for kernel heap

### Hardware Support
- Serial console output (UART 16550)
- VGA framebuffer for graphical console
- Interrupt handling (x86_64 IDT/GDT/TSS)
- ACPI support (optional)

### Debugging
- Stack unwinding with frame pointer tracking
- Symbol table support via custom symbolicator tool
- DWARF debug info conversion to simplified symbol format
- Panic handler with stack trace

## Building

Requirements:
- Rust nightly toolchain
- QEMU for x86_64
- just (command runner)
- OVMF firmware (for UEFI boot)

Build the OS image:
```sh
just build
```

Run in QEMU:
```sh
just run
```

Run tests:
```sh
just test
```

## Project Structure

- `crates/kernel` - Main kernel implementation
- `crates/pmm` - Physical memory manager library
- `crates/symbolicator` - Debug symbol processing tool

## License

See individual source files for license information.
