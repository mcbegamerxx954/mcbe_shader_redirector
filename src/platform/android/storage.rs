use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::Path,
};
macro_rules! from_error {
    ($dis:ident, $errorType:ty, $targetError:ty) => {
        impl From<$errorType> for $targetError {
            fn from(value: $errorType) -> $targetError {
                <$targetError>::$dis(value)
            }
        }
    };
}

#[derive(Debug)]
pub enum OptionsError {
    //    #[error("Options file reading error")]
    Io(std::io::Error),
    //    #[error("Storage locations int parse error")]
    IntParse(std::num::ParseIntError),
    //    #[error("Options file parsing error")]
    NotFound,
}
impl std::fmt::Display for OptionsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "Options file reading error: {e}"),
            Self::IntParse(e) => write!(f, "Storage location parse error: {e}"),
            Self::NotFound => write!(f, "Parse error"),
        }
    }
}
from_error!(Io, std::io::Error, OptionsError);
from_error!(IntParse, std::num::ParseIntError, OptionsError);
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
