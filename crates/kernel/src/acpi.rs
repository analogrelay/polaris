use core::ptr::NonNull;

use acpi::{AcpiHandler, AcpiTables, HpetInfo, PciConfigRegions};
use limine::request::RsdpRequest;
use pmm::{AddressTranslator, HumanAddress, HumanSize};

#[used]
#[unsafe(link_section = ".requests")]
static RSDP_REQUEST: RsdpRequest = RsdpRequest::new();

#[derive(Clone)]
struct AddressTranslatorHandler(&'static AddressTranslator);

impl core::fmt::Debug for AddressTranslatorHandler {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AddressTranslatorHandler").finish()
    }
}

impl AcpiHandler for AddressTranslatorHandler {
    unsafe fn map_physical_region<T>(
        &self,
        physical_address: usize,
        size: usize,
    ) -> acpi::PhysicalMapping<Self, T> {
        let virt_addr = NonNull::new(self.0.phys_to_ptr(physical_address) as *mut T)
            .expect("failed to map physical region for ACPI");
        unsafe { acpi::PhysicalMapping::new(physical_address, virt_addr, size, size, self.clone()) }
    }

    fn unmap_physical_region<T>(_region: &acpi::PhysicalMapping<Self, T>) {
        // No action needed for un-mapping in this implementation
    }
}

pub fn init() {
    let Some(rsdp) = RSDP_REQUEST.get_response() else {
        return;
    };

    // Limine Base Revision 4 returns the RSDP address as virtual
    // However, the ACPI crate works in physical addresses, so we need to translate it
    let rsdp_addr = AddressTranslator::current().virt_to_phys(rsdp.address());

    let tables = unsafe {
        AcpiTables::from_rsdp(
            AddressTranslatorHandler(AddressTranslator::current()),
            rsdp_addr,
        )
        .expect("failed to read ACPI tables")
    };
}
