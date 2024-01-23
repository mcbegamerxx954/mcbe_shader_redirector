use mc_rpfs::DataManager;
use notify::event::{AccessKind, AccessMode, EventKind};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::ffi::{CStr, CString, OsStr, OsString};
use std::ops::Deref;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
// This is fucking disgusting
static SHADER_PATHS: Lazy<Arc<Mutex<HashMap<OsString, PathBuf>>>> =
    Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));

#[inline(never)]
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
fn find_replacement(raw_path: &CStr) -> Option<CString> {
    // I want to check this later for correctness
    let raw_bytes = raw_path.to_bytes();
    if !raw_bytes.ends_with(b".material.bin") {
        return None;
    }
    let os_str = OsStr::from_bytes(raw_bytes);
    let path = Path::new(os_str);
    let filename = path.file_name()?;
    let sp_handle = SHADER_PATHS.clone();
    let sp_owned = sp_handle.lock().unwrap();
    if sp_owned.contains_key(filename) {
        let new_path = sp_owned.get(filename)?;
        let result = CString::new(new_path.to_str()?).expect("Non utf in sp (this is a bug)");

        return Some(result);
    }
    None
}

pub(crate) fn watch_jsons(app_dir: PathBuf) {
    let mut dataman = DataManager::init_data(&app_dir);
    let (sender, reciever) = crossbeam_channel::unbounded();
    let mut watcher = RecommendedWatcher::new(sender, Config::default()).unwrap();

    if let Err(e) = watcher.watch(&app_dir, RecursiveMode::NonRecursive) {
        panic!("Watch failed: {}", e);
    };

    for event in reciever {
        let event = match event {
            Ok(event) => event,
            Err(_) => {
                log::info!("Event is err, skipping");
                continue;
            }
        };
        if event.kind != EventKind::Access(AccessKind::Close(AccessMode::Write)) {
            log::info!("Skipping event..");
            continue;
        }
        log::info!("Recieved interesting event: {:#?}", event);
        let file_name = event.paths[0].file_name().unwrap();
        if file_name == "global_resource_packs.json" {
            log::info!("Grp changed, updating..");
            update_global_sp(&mut dataman, false);
        }
        if file_name == "valid_known_packs.json" {
            log::info!("Vpk changed, full updating..");
            update_global_sp(&mut dataman, true);
        }
    }
}
pub(crate) fn update_global_sp(dataman: &mut DataManager, full: bool) {
    if full {
        dataman
            .update_validpacks()
            .expect("Cant update valid packs");
    }
    let data = dataman.shader_paths().expect("Cant update shader_paths");
    let owned_sp = SHADER_PATHS.clone();
    let mut locked_sp = owned_sp.lock().expect("Mutex got poisoned!");
    *locked_sp = data;
    log::info!("Updated global shader paths: {:#?}", &locked_sp);
}
