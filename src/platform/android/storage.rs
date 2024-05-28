use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::Path,
};
use thiserror::Error;
#[derive(Error, Debug)]
pub enum OptionsError {
    #[error("Options file reading error")]
    Io(#[from] std::io::Error),
    #[error("Storage locations int parse error")]
    IntParse(#[from] std::num::ParseIntError),
    #[error("Options file parsing error")]
    NotFound,
}
#[derive(PartialEq, Copy, Clone, Debug)]
pub enum StorageLocation {
    Internal,
    External,
}
impl StorageLocation {
    pub fn from_i8(int: i8) -> Option<Self> {
        match int {
            1 => Some(Self::External),
            2 => Some(Self::Internal),
            _ => None,
        }
    }
}
pub fn parse_storage_location(opt_path: &Path) -> Result<i8, OptionsError> {
    let file = File::open(opt_path)?;
    let reader = BufReader::new(file);
    for line in reader.lines() {
        let line = line?;
        let (key, value) = line.split_once(':').unwrap();
        if key == "dvce_filestoragelocation" {
            return Ok(value.parse::<i8>()?);
        }
    }
    Err(OptionsError::NotFound)
}
