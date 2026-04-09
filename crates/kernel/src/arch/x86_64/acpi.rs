use limine::request::RsdpRequest;
use pmm::AddressTranslator;

#[used]
#[unsafe(link_section = ".requests")]
static RSDP_REQUEST: RsdpRequest = RsdpRequest::new();

/// Converts a physical address to a virtual address using the current address translator.
///
/// # Panics
/// Panics if the address translator has not been initialized (i.e., `mem::init_allocator()` has
/// not been called yet).
fn phys_to_virt(phys: usize) -> usize {
    AddressTranslator::current().phys_to_virt(phys)
}

#[repr(packed)]
struct Rsdp {
    signature: [u8; 8],
    checksum: u8,
    oem_id: [u8; 6],
    revision: u8,
    rsdt_addr: u32,
    length: u32,
    xsdt_addr: u64,
    extended_checksum: u8,
    reserved: [u8; 3],
}

fn sum_bytes(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0u8, |acc, &b| acc.wrapping_add(b))
}

/// Walks the ACPI XSDT to find the HPET table and returns the virtual address of the
/// HPET MMIO region.
///
/// # Panics
/// Panics if the bootloader did not provide an RSDP, or if no HPET table is found.
pub fn hpet_base_address() -> usize {
    let rsdp_virt = RSDP_REQUEST
        .response()
        .expect("bootloader did not provide RSDP")
        .address as *const u8;
    log::debug!("RSDP virtual address: {rsdp_virt:p}");
    let rsdp_bytes = unsafe { core::slice::from_raw_parts(rsdp_virt, 36) };
    let rsdp = unsafe { &*(rsdp_virt as *const Rsdp) };

    // Validate the signature
    if &rsdp.signature != b"RSD PTR " {
        panic!("Invalid RSDP signature: {:?}", rsdp.signature);
    }

    // Report the OEM ID
    if let Ok(oem_id) = core::str::from_utf8(&rsdp.oem_id) {
        log::debug!("OEM ID: {oem_id}");
    } else {
        panic!("OEM ID is not valid UTF-8");
    }

    if rsdp.revision < 2 {
        panic!("ACPI revision is too old: {}", rsdp.revision);
    }

    let bytes = unsafe { core::slice::from_raw_parts(rsdp_virt, rsdp.length as usize) };
    let sum: u8 = bytes.iter().fold(0u8, |acc, b| acc.wrapping_add(*b));
    if sum != 0 {
        panic!("RSDP checksum failed: expected 0, got {}", sum);
    }

    // XSDT physical address is at RSDP offset 24 (8 bytes, ACPI 2.0+).
    // SAFETY: RSDP is valid and large enough (v2 RSDP is 36 bytes).
    let xsdt_phys = rsdp.xsdt_addr as usize;
    let xsdt_virt = phys_to_virt(xsdt_phys);
    log::debug!("XSDT physical address: {xsdt_phys:#x}");

    // XSDT table length is at offset 4 (4 bytes).
    // SAFETY: XSDT is a valid ACPI table.
    let xsdt_length = unsafe { core::ptr::read_unaligned((xsdt_virt + 4) as *const u32) };
    log::debug!("XSDT length: {xsdt_length}");

    // XSDT header is 36 bytes; remaining bytes are 8-byte physical address entries.
    let entry_count = (xsdt_length as usize).saturating_sub(36) / 8;

    for i in 0..entry_count {
        // SAFETY: We bounds-checked via entry_count derived from the table length.
        let entry_phys =
            unsafe { core::ptr::read_unaligned((xsdt_virt + 36 + i * 8) as *const u64) } as usize;
        let entry_virt = phys_to_virt(entry_phys);

        // Read 4-byte signature at offset 0.
        // SAFETY: Every ACPI table starts with a 4-byte ASCII signature.
        let sig = unsafe { core::ptr::read_unaligned(entry_virt as *const [u8; 4]) };
        log::debug!(
            "found ACPI table with signature {:?}",
            str::from_utf8(&sig).unwrap_or("<unknown>")
        );
        if &sig == b"HPET" {
            // Generic Address Structure starts at offset 40 within the HPET table.
            // The physical MMIO address is at GAS offset 4 (total table offset 44).
            // SAFETY: HPET table is at least 56 bytes by spec.
            let hpet_phys =
                unsafe { core::ptr::read_unaligned((entry_virt + 44) as *const u64) } as usize;

            // Map the HPET MMIO page. Limine's HHDM does not cover device MMIO regions.
            // SAFETY: called from timer::init() which runs after mem::use_pmm().
            unsafe { super::paging::map_mmio(hpet_phys, 4096) };

            return phys_to_virt(hpet_phys);
        }
    }

    panic!("HPET ACPI table not found");
}
