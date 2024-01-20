use crate::unix::SectionType;
#[cfg(target_os = "android")]
pub type ElfPhdr = libc::Elf64_Phdr;
pub type ElfAddr = libc::Elf64_Addr;
pub type ElfWord = libc::Elf64_Word;
pub type ElfXword = libc::Elf64_Xword;
pub type ElfSxword = i64;
pub type ElfHalf = libc::Elf64_Half;

#[repr(C)]
#[derive(Debug)]
pub struct ElfDyn {
    pub d_tag: ElfSxword,
    pub d_val: ElfXword,
}

#[repr(C)]
#[derive(Debug)]
pub struct ElfRela {
    pub r_offset: ElfAddr,
    pub r_info: ElfXword,
    pub r_addend: ElfSxword,
}

#[repr(C)]
#[derive(Debug)]
pub struct ElfRel {
    pub r_offset: ElfAddr,
    pub r_info: ElfXword,
}
#[allow(dead_code)]
impl ElfRela {
    pub const fn symbol_index(&self) -> ElfWord {
        (self.r_info >> 32) as ElfWord
    }

    pub const fn symbol_type(&self) -> ElfWord {
        (self.r_info & 0xffffffff) as ElfWord
    }
}
#[allow(dead_code)]
impl ElfRel {
    pub const fn symbol_index(&self) -> ElfWord {
        (self.r_info >> 32) as ElfWord
    }

    pub const fn symbol_type(&self) -> ElfWord {
        (self.r_info & 0xffffffff) as ElfWord
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct ElfSym {
    pub st_name: ElfWord,
    pub st_info: u8,
    pub st_other: u8,
    pub st_shndx: ElfHalf,
    pub st_value: ElfAddr,
    pub st_size: ElfXword,
}

impl From<SectionType> for i64 {
    fn from(val: SectionType) -> Self {
        match val {
            SectionType::DT_PLTRELSZ => 2,
            SectionType::DT_PLTGOT => 3,
            SectionType::DT_STRTAB => 5,
            SectionType::DT_SYMTAB => 6,

            SectionType::DT_REL => 17,
            SectionType::DT_RELSZ => 18,
            SectionType::DT_RELENT => 19,

            SectionType::DT_RELA => 7,
            SectionType::DT_RELASZ => 8,
            SectionType::DT_RELAENT => 9,

            SectionType::DT_STRSZ => 10,
            SectionType::DT_JMPREL => 23,
        }
    }
}
