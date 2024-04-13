#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub mod none;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub use none::*;

pub mod android;
pub use android::*;
mod errors;
