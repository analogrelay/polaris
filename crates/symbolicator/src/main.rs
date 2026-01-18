use clap::{Parser, Subcommand};
use elf::ElfBytes;
use gimli::{read::Dwarf, AttributeValue, EndianSlice, LittleEndian, LocationLists, RangeLists};
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::mem::size_of;
use std::path::PathBuf;

use symbolicator::{FunctionEntry, Header, LineEntry, SourceFileEntry, StringRef, SymbolTable};

#[derive(Parser)]
#[command(name = "symbolicator")]
#[command(about = "Symbol table generation and lookup tool")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate a symbol table from an ELF binary with DWARF debug info
    Generate {
        /// Input ELF file with debug symbols
        #[arg(short, long)]
        input: PathBuf,

        /// Output symbol table file
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Look up an address in a symbol table
    Lookup {
        /// Symbol table file
        #[arg(short, long)]
        symtab: PathBuf,

        /// Address to look up (in hexadecimal, e.g., 0xffffffff80001234)
        #[arg(short, long)]
        address: String,
    },
}

/// Builder for constructing a symbol table.
///
/// Accumulates entries and strings, then builds a binary symbol table file.
pub struct SymbolTableBuilder {
    lines: Vec<LineEntry>,
    functions: Vec<FunctionEntry>,
    source_files: Vec<SourceFileEntry>,
    strings: Vec<u8>,
    string_cache: HashMap<String, StringRef>,
    function_cache: HashMap<String, u32>,
    file_cache: HashMap<String, u32>,
}

impl SymbolTableBuilder {
    pub fn new() -> Self {
        Self {
            lines: Vec::new(),
            functions: Vec::new(),
            source_files: Vec::new(),
            strings: Vec::new(),
            string_cache: HashMap::new(),
            function_cache: HashMap::new(),
            file_cache: HashMap::new(),
        }
    }

    /// Adds a line entry mapping an address to source location.
    pub fn add_line(&mut self, address: u64, function: &str, source_file: &str, line: u32) {
        let function_id = self.intern_function(function);
        let source_file_id = self.intern_source_file(source_file);

        self.lines
            .push(LineEntry::new(address, function_id, source_file_id, line));
    }

    fn intern_function(&mut self, name: &str) -> u32 {
        if let Some(&id) = self.function_cache.get(name) {
            return id;
        }

        let string_ref = self.intern_string(name);
        let id = self.functions.len() as u32;
        self.functions.push(FunctionEntry::new(string_ref));
        self.function_cache.insert(name.to_string(), id);
        id
    }

    fn intern_source_file(&mut self, path: &str) -> u32 {
        if let Some(&id) = self.file_cache.get(path) {
            return id;
        }

        let string_ref = self.intern_string(path);
        let id = self.source_files.len() as u32;
        self.source_files.push(SourceFileEntry::new(string_ref));
        self.file_cache.insert(path.to_string(), id);
        id
    }

    fn intern_string(&mut self, s: &str) -> StringRef {
        if let Some(existing) = self.string_cache.get(s) {
            return *existing;
        }

        let offset = self.strings.len() as u64;
        let length = s.len() as u64;

        self.strings.extend_from_slice(s.as_bytes());

        let string_ref = StringRef::new(offset, length);
        self.string_cache.insert(s.to_string(), string_ref);
        string_ref
    }

    /// Builds the final binary symbol table.
    ///
    /// Sorts line entries by address and serializes all tables with header.
    pub fn build(mut self) -> Vec<u8> {
        self.lines.sort_by_key(|entry| entry.address);

        let header_size = size_of::<Header>();

        let lines_offset = header_size;
        let lines_size = self.lines.len() * size_of::<LineEntry>();

        let functions_offset = lines_offset + lines_size;
        let functions_size = self.functions.len() * size_of::<FunctionEntry>();

        let source_files_offset = functions_offset + functions_size;
        let source_files_size = self.source_files.len() * size_of::<SourceFileEntry>();

        let string_pool_offset = source_files_offset + source_files_size;
        let string_pool_size = self.strings.len();

        let header = Header::new(
            lines_offset as u64,
            lines_size as u64,
            functions_offset as u64,
            functions_size as u64,
            source_files_offset as u64,
            source_files_size as u64,
            string_pool_offset as u64,
            string_pool_size as u64,
        );

        let mut output = Vec::new();

        output.extend_from_slice(unsafe {
            core::slice::from_raw_parts(&header as *const Header as *const u8, size_of::<Header>())
        });

        output.extend_from_slice(unsafe {
            core::slice::from_raw_parts(self.lines.as_ptr() as *const u8, lines_size)
        });

        output.extend_from_slice(unsafe {
            core::slice::from_raw_parts(self.functions.as_ptr() as *const u8, functions_size)
        });

        output.extend_from_slice(unsafe {
            core::slice::from_raw_parts(self.source_files.as_ptr() as *const u8, source_files_size)
        });

        output.extend_from_slice(&self.strings);

        output
    }
}

fn load_debug_sections<'a>(
    elf: &'a ElfBytes<'a, elf::endian::LittleEndian>,
) -> Option<Dwarf<EndianSlice<'a, LittleEndian>>> {
    let load_section = |name: &str| -> EndianSlice<LittleEndian> {
        match elf.section_header_by_name(name) {
            Ok(Some(header)) => match elf.section_data(&header) {
                Ok((data, _)) => EndianSlice::new(data, LittleEndian),
                Err(_) => EndianSlice::new(&[], LittleEndian),
            },
            _ => EndianSlice::new(&[], LittleEndian),
        }
    };

    let dwarf = Dwarf {
        debug_abbrev: load_section(".debug_abbrev").into(),
        debug_addr: load_section(".debug_addr").into(),
        debug_aranges: load_section(".debug_aranges").into(),
        debug_info: load_section(".debug_info").into(),
        debug_line: load_section(".debug_line").into(),
        debug_line_str: load_section(".debug_line_str").into(),
        debug_str: load_section(".debug_str").into(),
        debug_str_offsets: load_section(".debug_str_offsets").into(),
        debug_types: load_section(".debug_types").into(),
        locations: LocationLists::new(
            load_section(".debug_loc").into(),
            load_section(".debug_loclists").into(),
        ),
        ranges: RangeLists::new(
            load_section(".debug_ranges").into(),
            load_section(".debug_rnglists").into(),
        ),
        ..Default::default()
    };

    Some(dwarf)
}

fn generate(input: PathBuf, output: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let file_data = fs::read(&input)?;
    let elf = ElfBytes::<elf::endian::LittleEndian>::minimal_parse(&file_data)?;

    let dwarf = load_debug_sections(&elf).ok_or("Failed to load debug sections")?;

    let function_map = build_function_map(&dwarf)?;

    let mut unit_headers = Vec::new();
    let mut units = dwarf.units();
    while let Ok(Some(header)) = units.next() {
        unit_headers.push(header);
    }

    let line_entries: Vec<_> = unit_headers
        .par_iter()
        .flat_map(|header| {
            let unit = match dwarf.unit(*header) {
                Ok(unit) => unit,
                Err(_) => return Vec::new(),
            };

            let line_program = match unit.line_program.clone() {
                Some(program) => program,
                None => return Vec::new(),
            };

            let header = line_program.header().clone();
            let mut rows = line_program.rows();
            let mut entries = Vec::new();

            while let Ok(Some((_, row))) = rows.next_row() {
                let address = row.address();
                let line = row.line().map(|l| l.get()).unwrap_or(0);

                if line == 0 {
                    continue;
                }

                let file = match row.file(&header) {
                    Some(file_entry) => {
                        let dir = file_entry
                            .directory(&header)
                            .and_then(|dir| dwarf.attr_string(&unit, dir).ok())
                            .map(|s| s.to_string_lossy().into_owned());

                        let file_name = dwarf
                            .attr_string(&unit, file_entry.path_name())
                            .ok()
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "<unknown>".into());

                        if let Some(d) = dir {
                            if !d.is_empty() {
                                format!("{}/{}", d, file_name)
                            } else {
                                file_name
                            }
                        } else {
                            file_name
                        }
                    }
                    None => "<unknown>".into(),
                };

                let function_name =
                    find_function_for_address(&function_map, address).unwrap_or("<unknown>");

                entries.push((address, function_name.to_string(), file, line as u32));
            }

            entries
        })
        .collect();

    let mut builder = SymbolTableBuilder::new();
    for (address, function_name, file, line) in line_entries {
        builder.add_line(address, &function_name, &file, line);
    }

    let output_data = builder.build();

    let mut file = fs::File::create(&output)?;
    file.write_all(&output_data)?;

    Ok(())
}

struct FunctionRange {
    low: u64,
    high: u64,
    name: String,
}

fn build_function_map(
    dwarf: &Dwarf<EndianSlice<LittleEndian>>,
) -> Result<Vec<FunctionRange>, Box<dyn std::error::Error>> {
    let mut functions = Vec::new();
    let mut units = dwarf.units();
    while let Ok(Some(header)) = units.next() {
        let unit = match dwarf.unit(header) {
            Ok(unit) => unit,
            Err(_) => continue,
        };

        let mut entries = unit.entries();
        while let Ok(Some((_, entry))) = entries.next_dfs() {
            if entry.tag() != gimli::DW_TAG_subprogram {
                continue;
            }

            let mut low_pc = None;
            let mut high_pc = None;
            let mut ranges_offset = None;
            let mut name = None;
            let mut linkage_name = None;

            let mut attrs = entry.attrs();
            while let Ok(Some(attr)) = attrs.next() {
                match attr.name() {
                    gimli::DW_AT_low_pc => {
                        if let AttributeValue::Addr(addr) = attr.value() {
                            low_pc = Some(addr);
                        }
                    }
                    gimli::DW_AT_high_pc => match attr.value() {
                        AttributeValue::Addr(addr) => high_pc = Some(addr),
                        AttributeValue::Udata(offset) => {
                            if let Some(low) = low_pc {
                                high_pc = Some(low + offset);
                            }
                        }
                        _ => {}
                    },
                    gimli::DW_AT_ranges => {
                        ranges_offset = match attr.value() {
                            AttributeValue::RangeListsRef(offset) => {
                                Some(dwarf.ranges_offset_from_raw(&unit, offset))
                            }
                            _ => None,
                        };
                    }
                    gimli::DW_AT_name => {
                        if let AttributeValue::DebugStrRef(offset) = attr.value() {
                            if let Ok(s) = dwarf.debug_str.get_str(offset) {
                                name = Some(s.to_string_lossy().into_owned());
                            }
                        }
                    }
                    gimli::DW_AT_linkage_name => {
                        if let AttributeValue::DebugStrRef(offset) = attr.value() {
                            if let Ok(s) = dwarf.debug_str.get_str(offset) {
                                linkage_name = Some(s.to_string_lossy().into_owned());
                            }
                        }
                    }
                    _ => {}
                }
            }

            // Prefer linkage_name (mangled) for demangling, fall back to name
            let func_name = match linkage_name.or(name) {
                Some(n) => n,
                None => continue,
            };

            let demangled = rustc_demangle::try_demangle(&func_name)
                .ok()
                .map(|d| format!("{:#}", d))
                .unwrap_or(func_name);

            // Try low_pc/high_pc first
            if let (Some(low), Some(high)) = (low_pc, high_pc) {
                functions.push(FunctionRange {
                    low,
                    high,
                    name: demangled,
                });
            } else if let Some(ranges_offset) = ranges_offset {
                // Try ranges if low_pc/high_pc not available
                if let Ok(mut ranges) = dwarf.ranges(&unit, ranges_offset) {
                    while let Ok(Some(range)) = ranges.next() {
                        functions.push(FunctionRange {
                            low: range.begin,
                            high: range.end,
                            name: demangled.clone(),
                        });
                    }
                }
            }
        }
    }

    // Sort by low address for binary search
    functions.sort_by_key(|f| f.low);
    Ok(functions)
}

fn find_function_for_address<'a>(functions: &'a [FunctionRange], address: u64) -> Option<&'a str> {
    // Binary search to find the function containing this address
    let idx = functions.binary_search_by(|f| {
        if address < f.low {
            std::cmp::Ordering::Greater
        } else if address >= f.high {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Equal
        }
    });

    match idx {
        Ok(i) => Some(&functions[i].name),
        Err(_) => None,
    }
}

fn lookup(symtab_path: PathBuf, address_str: String) -> Result<(), Box<dyn std::error::Error>> {
    let address = if let Some(hex) = address_str.strip_prefix("0x") {
        u64::from_str_radix(hex, 16)?
    } else {
        address_str.parse::<u64>()?
    };

    let data = fs::read(&symtab_path)?;
    let symtab = SymbolTable::from_bytes(&data)
        .map_err(|e| format!("Failed to load symbol table: {}", e))?;

    if let Some(info) = symtab.lookup(address) {
        println!(
            "{:#x}: {} at {}:{}",
            address, info.function_name, info.source_file, info.line
        );
    } else {
        println!("{:#x}: <not found>", address);
    }

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    match args.command {
        Command::Generate { input, output } => generate(input, output),
        Command::Lookup { symtab, address } => lookup(symtab, address),
    }
}
