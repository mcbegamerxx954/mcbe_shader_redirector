use mc_rpfs::DataManager;
use notify::event::{AccessKind, AccessMode, EventKind};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use once_cell::sync::Lazy;
use std::ffi::{CStr, CString, OsStr};
use std::fs;
use std::ops::Deref;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
// This is fucking disgusting
static SHADER_PATHS: Lazy<Arc<Mutex<Vec<PathBuf>>>> =
    Lazy::new(|| Arc::new(Mutex::new(Vec::new())));

#[inline(never)]
pub(crate) unsafe extern "C" fn aasset_hook(
    manager: *mut ndk_sys::AAssetManager,
    filename: *const libc::c_char,
    mode: libc::c_int,
) -> *mut ndk_sys::AAsset {
    // SAFETY: The hook blocks the thread calling it
    // So the data should not change
    let file_name = CStr::from_ptr(filename);
    log::info!("Hook ran with filename: {:#?}", file_name);
    let os_filename = OsStr::from_bytes(file_name.to_bytes());
    let file_path: &Path = os_filename.as_ref();
    if !os_filename.as_bytes().ends_with(b".material.bin") {
        return unsafe { ndk_sys::AAssetManager_open(manager, filename, mode) };
    }
    log::info!("Interesting filepath: {:#?}", file_path);
    // Now we lock global shader_paths since we verified that we want to use it
    let global_sp = SHADER_PATHS.clone();
    let shader_paths = global_sp.lock().unwrap();
    let shader_paths = shader_paths.deref();
    for path in shader_paths {
        if file_path.file_name().unwrap() == path.file_name().unwrap() {
            let c_str = CString::new(path.as_os_str().as_bytes()).unwrap();
            log::info!("Successful intercept: {:#?}", c_str);
            return unsafe { ndk_sys::AAssetManager_open(manager, c_str.as_ptr(), mode) };
        }
    }
    log::info!("didnt find filepath in replace list {:#?}", file_path);
    unsafe { ndk_sys::AAssetManager_open(manager, filename, mode) }
}

pub(crate) unsafe extern "C" fn fopen_hook(
    filename: *const libc::c_char,
    mode: *const libc::c_char,
) -> *mut libc::FILE {
    // SAFETY: The hook blocks the thread calling it
    // So the data should not change
    let file_name = CStr::from_ptr(filename);
    log::info!("Hook ran with filename: {:#?}", file_name);
    let os_filename = OsStr::from_bytes(file_name.to_bytes());
    let file_path: &Path = os_filename.as_ref();
    if !os_filename.as_bytes().ends_with(b".material.bin") {
        return unsafe { libc::fopen(filename, mode) };
    }
    log::info!("Interesting filepath: {:#?}", file_path);
    // Now we lock global shader_paths since we verified that we want to use it
    let global_sp = SHADER_PATHS.clone();
    let shader_paths = global_sp.lock().unwrap();
    let shader_paths = shader_paths.deref();
    for path in shader_paths {
        if file_path.file_name().unwrap() == path.file_name().unwrap() {
            let c_str = CString::new(path.as_os_str().as_bytes()).unwrap();
            log::info!("Successful intercept: {:#?}", c_str);
            return unsafe { libc::fopen(c_str.as_ptr(), mode) };
        }
    }
    log::info!("didnt find filepath in replace list {:#?}", file_path);
    unsafe { libc::fopen(filename, mode) }
}

pub(crate) fn watch_jsons(app_dir: PathBuf) {
    let mut dataman = DataManager::init_data(&app_dir);
    let (sender, reciever) = crossbeam_channel::unbounded();
    let mut watcher = RecommendedWatcher::new(sender, Config::default()).unwrap();

    if let Err(e) = watcher.watch(&app_dir, RecursiveMode::NonRecursive) {
        panic!("Watch failed: {}", e);
    };

    let shader_paths = SHADER_PATHS.clone();
    for event in reciever {
        if event.is_err() {
            log::info!("event is an error, skipping..");
        }
        let mut event = match event {
            Ok(event) => event,
            Err(_) => continue,
        };
        if event.kind != EventKind::Access(AccessKind::Close(AccessMode::Write)) {
            log::info!("Skipping event..");
            continue;
        }
        log::info!("Recieved interesting event: {:#?}", event);
        let file_name = event.paths[0].file_name().unwrap();
        /*        if file_name == "option.txt" {
            let options_file = fs::read_to_string(event.paths[0]).unwrap();
            let options = serde_yaml::from_str(&options_file).unwrap();
            let storage_type = options.get("dvce_filestoragelocation").unwrap();
            let storage_type = storage_type.as_u32().unwrap();
        }
            */
        if file_name == "global_resource_packs.json" {
            log::info!("Grp changed, updating..");
            let mut sp_mut = shader_paths.lock().unwrap();
            let updated_sp = dataman.shader_paths().unwrap();
            *sp_mut = updated_sp;
            log::info!("sp_shared is :{:#?}", sp_mut);
        }
        if file_name == "valid_known_packs.json" {
            log::info!("Vpk changed, updating it with globalpaths...");
            dataman.update_validpacks().unwrap();
            let shader_packs = dataman.shader_paths().unwrap();
            let mut sp_global = shader_paths.lock().unwrap();
            *sp_global = shader_packs;
            log::info!("sp_shared is :{:#?}", sp_global);
        }
    }
}