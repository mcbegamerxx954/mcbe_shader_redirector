#[cfg(target_pointer_width = "32")]
pub mod small;
#[cfg(target_pointer_width = "32")]
pub use self::small::*;
#[cfg(target_pointer_width = "64")]
pub mod long;
#[cfg(target_pointer_width = "64")]
pub use self::long::*;
// Switch based on arch..
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub const R_GLOB_DAT: u32 = 6;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub const R_JUMP_SLOT: u32 = 7;

#[cfg(target_arch = "arm")]
pub const R_GLOB_DAT: u32 = 21;
#[cfg(target_arch = "arm")]
pub const R_JUMP_SLOT: u32 = 22;
#[cfg(target_arch = "aarch64")]
pub const R_GLOB_DAT: u32 = 1025;
#[cfg(target_arch = "aarch64")]
pub const R_JUMP_SLOT: u32 = 1026;
#[derive(Debug, Clone, Copy)]
#[allow(non_camel_case_types)]
pub enum SectionType {
    DT_PLTRELSZ,
    DT_PLTGOT,

    DT_STRTAB,
    DT_SYMTAB,

    DT_RELA,
    DT_RELASZ,
    DT_RELAENT,

    DT_REL,
    DT_RELSZ,
    DT_RELENT,

    DT_STRSZ,
    DT_JMPREL,
}

#[repr(C)]
#[derive(Debug)]
pub struct LinkMap {
    pub l_addr: ElfAddr,
    pub l_name: *const libc::c_char,
    pub l_ld: *const ElfDyn,
    pub l_next: *const LinkMap,
    pub l_prev: *const LinkMap,
}
