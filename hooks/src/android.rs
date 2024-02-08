use crate::SHADER_PATHS;
use std::ffi::{CStr, CString, OsStr};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

// Nvm RwLock is slower
pub(crate) unsafe extern "C" fn aasset_hook(
    man: *mut ndk_sys::AAssetManager,
    filename: *const libc::c_char,
    mode: libc::c_int,
) -> *mut ndk_sys::AAsset {
    let c_str = CStr::from_ptr(filename);
    match find_replacement(c_str) {
        Some(rep_path) => {
            log::info!("aasset intercepted with path: {:#?}", &rep_path);
            ndk_sys::AAssetManager_open(man, rep_path.as_ptr().cast(), mode)
        }
        None => {
            log::info!("didnt intercept aasset path: {:#?}", c_str);
            ndk_sys::AAssetManager_open(man, filename, mode)
        }
    }
}

pub(crate) unsafe extern "C" fn fopen_hook(
    filename: *const libc::c_char,
    mode: *const libc::c_char,
) -> *mut libc::FILE {
    let c_str = CStr::from_ptr(filename);
    match find_replacement(c_str) {
        Some(rep_path) => {
            log::info!("fopen intercepted with path: {:#?}", &rep_path);
            libc::fopen(rep_path.as_ptr().cast(), mode)
        }
        None => {
            log::info!("didnt intercept fopen path: {:#?}", c_str);
            libc::fopen(filename, mode)
        }
    }
}
// We log all Nones and errs because i cant BACKTRACE  :(
fn find_replacement(raw_path: &CStr) -> Option<CString> {
    // I want to check this later for correctness
    let raw_bytes = raw_path.to_bytes();
    if !raw_bytes.ends_with(b".material.bin") {
        log::info!("path does not end with shader suffix: {:#?}", raw_path);
        return None;
    }
    let os_str = OsStr::from_bytes(raw_bytes);
    let path = Path::new(os_str);
    let filename = path.file_name()?;
    log::trace!("filename is: {:#?}", filename);
    // If this is err something went very wrong
    let sp_owned = match SHADER_PATHS.lock() {
        Ok(sp_owned) => sp_owned,
        Err(e) => {
            //Prevent Crash if other thread silently fails
            log::error!("Ignoring lock poison: {e}");
            return None;
        }
    };
    let Some(new_path) = sp_owned.get(filename) else {
        log::warn!("Cant find replacement for filename: {:#?}", filename);
        return None;
    };
    let npath_bytes = new_path.as_os_str().as_bytes();
    let Ok(replacement) = CString::new(npath_bytes) else {
        log::warn!("PathBuf: [{:#?}] has a trailing 0!", new_path);
        return None;
    };
    log::warn!("Found replacement: {:#?}", new_path);
    Some(replacement)
}
