use crate::unix::SectionType;

#[cfg(all(target_os = "android"))]
pub type ElfPhdr = libc::Elf32_Phdr;
pub type ElfAddr = libc::Elf32_Addr;
pub type ElfWord = libc::Elf32_Word;
pub type ElfSword = libc::c_int;
pub type ElfHalf = libc::Elf32_Half;

#[repr(C)]
#[derive(Debug)]
pub struct ElfDyn {
    pub d_tag: ElfSword,
    pub d_val: ElfWord,
}

#[repr(C)]
#[derive(Debug)]
pub struct ElfRela {
    pub r_offset: ElfAddr,
    pub r_info: ElfWord,
    pub r_addend: ElfSword,
}

impl ElfRela {
    pub fn symbol_index(&self) -> ElfWord {
        (self.r_info >> 8) as ElfWord
    }

    pub fn symbol_type(&self) -> ElfWord {
        (self.r_info & 0x0ff) as ElfWord
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct ElfRel {
    pub r_offset: ElfAddr,
    pub r_info: ElfWord,
}

impl ElfRel {
    pub fn symbol_index(&self) -> ElfWord {
        (self.r_info >> 8) as ElfWord
    }

    pub fn symbol_type(&self) -> ElfWord {
        (self.r_info & 0x0ff) as ElfWord
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct ElfSym {
    pub st_name: ElfWord,
    pub st_value: ElfAddr,
    pub st_size: ElfWord,
    pub st_info: u8,
    pub st_other: u8,
    pub st_shndx: ElfHalf,
}

impl From<SectionType> for i32 {
    fn from(val: SectionType) -> Self {
        match val {
            SectionType::DT_PLTRELSZ => 2,
            SectionType::DT_PLTGOT => 3,
            SectionType::DT_STRTAB => 5,
            SectionType::DT_SYMTAB => 6,
            SectionType::DT_RELA => 7,
            SectionType::DT_RELASZ => 8,
            SectionType::DT_RELAENT => 9,

            SectionType::DT_STRSZ => 10,
            SectionType::DT_JMPREL => 23,

            SectionType::DT_REL => 17,
            SectionType::DT_RELSZ => 18,
            SectionType::DT_RELENT => 19,
        }
    }
}
