use core::fmt;

pub enum HookError {
    MissingLib(String),
    MissingSym(String),
    OsError(String),
    Unknown(String),
}
impl fmt::Display for HookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingLib(details) => write!(f, "Missing minecraft library: {details}"),
            Self::MissingSym(symbol) => write!(f, "Cant find symbol in minecraft lib: {symbol}"),
            Self::OsError(detail) => write!(f, "Os error: {detail}"),
            Self::Unknown(detail_what) => write!(f, "Unexpected error {detail_what}"),
        }
    }
}
