use core::fmt;
#[derive(Debug)]
pub enum HookError {
    MissingLib(String),
    OsError(String),
}
impl fmt::Display for HookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingLib(details) => write!(f, "Missing minecraft library: {details}"),
            Self::OsError(detail) => write!(f, "Os error: {detail}"),
        }
    }
}
