#![no_std]

use core::mem::size_of;

const MAGIC: &[u8; 6] = b"SYMTAB";

/// Header at the start of the symbol table file.
///
/// Contains magic number and offsets/sizes for all tables and the string pool.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Header {
    magic: [u8; 6],
    _padding: [u8; 2],
    lines_offset: u64,
    lines_size: u64,
    functions_offset: u64,
    functions_size: u64,
    source_files_offset: u64,
    source_files_size: u64,
    string_pool_offset: u64,
    string_pool_size: u64,
}

impl Header {
    pub fn new(
        lines_offset: u64,
        lines_size: u64,
        functions_offset: u64,
        functions_size: u64,
        source_files_offset: u64,
        source_files_size: u64,
        string_pool_offset: u64,
        string_pool_size: u64,
    ) -> Self {
        Self {
            magic: *MAGIC,
            _padding: [0; 2],
            lines_offset,
            lines_size,
            functions_offset,
            functions_size,
            source_files_offset,
            source_files_size,
            string_pool_offset,
            string_pool_size,
        }
    }
}

/// Entry in the Lines table.
///
/// Maps a code address to a function, source file, and line number.
/// Table is sorted by address for binary search.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LineEntry {
    pub address: u64,
    pub function_id: u32,
    pub source_file_id: u32,
    pub line_number: u32,
    _padding: u32,
}

impl LineEntry {
    pub fn new(address: u64, function_id: u32, source_file_id: u32, line_number: u32) -> Self {
        Self {
            address,
            function_id,
            source_file_id,
            line_number,
            _padding: 0,
        }
    }
}

/// Entry in the Functions table.
///
/// Contains the function name as a StringRef into the string pool.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FunctionEntry {
    pub name: StringRef,
}

impl FunctionEntry {
    pub fn new(name: StringRef) -> Self {
        Self { name }
    }
}

/// Entry in the SourceFiles table.
///
/// Contains the file path as a StringRef into the string pool.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SourceFileEntry {
    pub path: StringRef,
}

impl SourceFileEntry {
    pub fn new(path: StringRef) -> Self {
        Self { path }
    }
}

/// Reference to a string in the string pool.
///
/// Offset and length specify a slice of the string pool.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct StringRef {
    pub offset: u64,
    pub length: u64,
}

impl StringRef {
    pub fn new(offset: u64, length: u64) -> Self {
        Self { offset, length }
    }
}

/// Symbol table for zero-copy access to symbol information.
///
/// Constructed from a byte slice containing the entire symbol table file.
/// Uses transmutation to access tables directly without parsing or copying.
pub struct SymbolTable<'a> {
    data: &'a [u8],
    header: &'a Header,
    string_pool: &'a str,
}

impl<'a> SymbolTable<'a> {
    /// Creates a symbol table from a byte slice.
    ///
    /// Validates the magic number and structure, then provides zero-copy access.
    pub fn from_bytes(data: &'a [u8]) -> Result<Self, &'static str> {
        if data.len() < size_of::<Header>() {
            return Err("Data too small for header");
        }

        let header = unsafe { &*(data.as_ptr() as *const Header) };

        if &header.magic != MAGIC {
            return Err("Invalid magic number");
        }

        let pool_start = header.string_pool_offset as usize;
        let pool_end = pool_start + header.string_pool_size as usize;

        if pool_end > data.len() {
            return Err("String pool extends beyond data");
        }

        let string_pool = core::str::from_utf8(&data[pool_start..pool_end])
            .map_err(|_| "Invalid UTF-8 in string pool")?;

        Ok(Self {
            data,
            header,
            string_pool,
        })
    }

    /// Looks up symbol information for a given address.
    ///
    /// Uses floor search to find the line entry with the highest address <= query.
    /// Returns function name, source file, and line number if found.
    pub fn lookup(&self, address: u64) -> Option<SymbolInfo<'_>> {
        let lines = self.lines_table();

        if lines.is_empty() {
            return None;
        }

        // Floor search: find the entry with highest address <= query address
        let idx = match lines.binary_search_by_key(&address, |entry| entry.address) {
            Ok(i) => i,
            Err(i) => {
                if i == 0 {
                    return None;
                }
                i - 1
            }
        };
        let entry = &lines[idx];

        let function = self.get_function(entry.function_id)?;
        let source_file = self.get_source_file(entry.source_file_id)?;

        Some(SymbolInfo {
            function_name: self.resolve_string(&function.name)?,
            source_file: self.resolve_string(&source_file.path)?,
            line: entry.line_number as usize,
        })
    }

    fn lines_table(&self) -> &[LineEntry] {
        let start = self.header.lines_offset as usize;
        let size = self.header.lines_size as usize;
        let count = size / size_of::<LineEntry>();

        unsafe {
            core::slice::from_raw_parts(self.data.as_ptr().add(start) as *const LineEntry, count)
        }
    }

    fn get_function(&self, id: u32) -> Option<&FunctionEntry> {
        let start = self.header.functions_offset as usize;
        let size = self.header.functions_size as usize;
        let count = size / size_of::<FunctionEntry>();

        let functions = unsafe {
            core::slice::from_raw_parts(
                self.data.as_ptr().add(start) as *const FunctionEntry,
                count,
            )
        };

        functions.get(id as usize)
    }

    fn get_source_file(&self, id: u32) -> Option<&SourceFileEntry> {
        let start = self.header.source_files_offset as usize;
        let size = self.header.source_files_size as usize;
        let count = size / size_of::<SourceFileEntry>();

        let files = unsafe {
            core::slice::from_raw_parts(
                self.data.as_ptr().add(start) as *const SourceFileEntry,
                count,
            )
        };

        files.get(id as usize)
    }

    fn resolve_string(&self, string_ref: &StringRef) -> Option<&str> {
        let start = string_ref.offset as usize;
        let end = start + string_ref.length as usize;

        if end > self.string_pool.len() {
            return None;
        }

        Some(&self.string_pool[start..end])
    }
}

/// Symbol information returned by lookup.
pub struct SymbolInfo<'a> {
    pub function_name: &'a str,
    pub source_file: &'a str,
    pub line: usize,
}
