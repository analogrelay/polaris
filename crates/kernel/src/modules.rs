use core::ffi::CStr;

use limine::request::ModulesRequest;

#[used]
#[unsafe(link_section = ".requests")]
static MODULES: ModulesRequest = ModulesRequest::new();

#[derive(Debug, Clone, Copy)]
pub struct ModuleName(&'static str);

impl PartialEq for ModuleName {
    fn eq(&self, other: &Self) -> bool {
        // Manual byte-by-byte comparison: the derived PartialEq routes through the
        // compiler-builtins x86_64 memcmp, which reads u128 chunks and triggers a
        // Rust debug precondition check when the source bytes aren't 16-byte aligned
        // (as is typical for strings in Limine module responses).
        let a = self.0.as_bytes();
        let b = other.0.as_bytes();
        if a.len() != b.len() {
            return false;
        }
        for i in 0..a.len() {
            if a[i] != b[i] {
                return false;
            }
        }
        true
    }
}

impl Eq for ModuleName {}

impl ModuleName {
    pub const DEBUG_SYMBOLS: Self = Self("/polaris/polaris.symtab");

    pub fn new(name: &'static str) -> Self {
        Self(name)
    }

    pub fn as_cstr(&self) -> &'static str {
        self.0
    }
}

pub struct Module {
    pub data: &'static [u8],
    pub name: ModuleName,
}

impl Module {
    pub fn get(module_name: ModuleName) -> Option<Module> {
        let modules = MODULES.response()?.modules();
        for module in modules {
            let name = ModuleName::new(module.path());
            log::debug!("matching {:?} against {:?}", name, module_name);
            if name == module_name {
                return Some(Module {
                    data: module.data(),
                    name,
                });
            }
        }
        None
    }
}
