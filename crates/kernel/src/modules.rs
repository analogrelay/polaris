use core::ffi::CStr;

use limine::request::ModuleRequest;

#[used]
#[unsafe(link_section = ".requests")]
static MODULE: ModuleRequest = ModuleRequest::new();

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct ModuleName(&'static CStr);

impl ModuleName {
    pub const DEBUG_SYMBOLS: Self = Self(c"debug_symbols");

    pub fn new(name: &'static CStr) -> Self {
        Self(name)
    }

    pub fn as_cstr(&self) -> &'static CStr {
        self.0
    }
}

pub struct Module {
    pub base: usize,
    pub size: usize,
    pub name: ModuleName,
}

impl Module {
    pub fn get(module_name: ModuleName) -> Option<Module> {
        let modules = MODULE.get_response()?.modules();
        for module in modules {
            let name = ModuleName::new(module.string());
            if name == module_name {
                return Some(Module {
                    base: module.addr() as usize,
                    size: module.size() as usize,
                    name,
                });
            }
        }
        None
    }
}
