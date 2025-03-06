//! Format of an ELF executable file

pub const ELF_MAGIC: u32 = 0x46_4c_45_7f; // "\x7FELF" in dittle endian

/// File Header
#[repr(C)]
#[derive(Debug)]
pub struct ElfHeader {
    pub magic: u32,
    pub elf: [u8; 12],
    pub ty: u16,
    pub machine: u16,
    pub version: u32,
    pub entry: u64,
    pub phoff: u64,
    pub shoff: u64,
    pub flags: u32,
    pub ehsize: u16,
    pub phentsize: u16,
    pub phnum: u16,
    pub shentsize: u16,
    pub shnum: u16,
    pub shstrndx: u16,
}

impl ElfHeader {
    pub const fn zero() -> Self {
        Self {
            magic: 0,
            elf: [0; 12],
            ty: 0,
            machine: 0,
            version: 0,
            entry: 0,
            phoff: 0,
            shoff: 0,
            flags: 0,
            ehsize: 0,
            phentsize: 0,
            phnum: 0,
            shentsize: 0,
            shnum: 0,
            shstrndx: 0,
        }
    }
}

/// Program section header
#[repr(C)]
#[derive(Debug)]
pub struct ProgramHeader {
    pub ty: u32,
    pub flags: u32,
    pub off: u64,
    pub vaddr: u64,
    pub paddr: u64,
    pub filesz: u64,
    pub memsz: u64,
    pub align: u64,
}

impl ProgramHeader {
    pub const fn zero() -> Self {
        Self {
            ty: 0,
            flags: 0,
            off: 0,
            vaddr: 0,
            paddr: 0,
            filesz: 0,
            memsz: 0,
            align: 0,
        }
    }
}

pub const ELF_PROG_LOAD: u32 = 1;
