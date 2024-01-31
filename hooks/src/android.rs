use app_dirs2::{app_dir, app_root, AppDataType};
use notify::event::{AccessKind, AccessMode, EventKind};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use once_cell::sync::Lazy;
use serde::Deserialize;
use std::collections::HashMap;
use std::ffi::{CStr, CString, OsStr, OsString};
use std::fs;
use std::ops::{Deref, DerefMut};
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;

use crate::common::{self, StorageType};
use crate::mc_utils::ActivePack;
use crate::mc_utils::Error;
use crate::mc_utils::{self, DataManager, ValidPack};
// Yeh
#[derive(Default)]
struct WorldStuff {
    last_world: OsString,
    world_path: PathBuf,
}

// Nvm RwLock is slower
static SHADER_PATHS: Lazy<Mutex<HashMap<OsString, PathBuf>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

static WORLD_CACHE: Lazy<Mutex<WorldStuff>> = Lazy::new(|| Mutex::new(WorldStuff::default()));

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
            log::debug!("didnt intercept aasset path: {:#?}", c_str);
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
            return libc::fopen(rep_path.as_ptr().cast(), mode);
        }
        None => {
            log::debug!("didnt intercept fopen path: {:#?}", c_str);
        }
    }
    log::info!("perfoming world check..");
    let file = libc::fopen(filename, mode);
    let raw_path = c_str.to_bytes();
    // This normally means that minecraft is loading files form a resource pack
    if !raw_path.ends_with(b"contents.json") {
        return file;
    }
    let mut wc_lock = WORLD_CACHE.lock().expect("Lock is poisoned!");
    let mut wc_ref = wc_lock.deref_mut();
    let os_path = OsStr::from_bytes(raw_path);
    let path = Path::new(os_path);
    if !path.starts_with(&wc_ref.world_path) {
        log::warn!("Path does not end with standard world prefix: {:#?}", path);
        return file;
    }
    // Drop them as they are not used anymore and they lock
    let mut components = path.components();
    // This should be the world name
    let Some(world_name) = components.nth_back(3) else {
        return file;
    };
    let Component::Normal(world_name) = world_name else {
        log::warn!(
            "Supposed World name is not a normal component!: {:#?}",
            world_name
        );
        return file;
    };
    if wc_ref.last_world == world_name {
        log::info!("World is already cached: {:#?}", world_name);
        return file;
    }
    let mut world_path: PathBuf = components.collect();
    world_path.push(world_name);
    if !world_path.exists() {
        log::warn!("Supposed world path does not exist!: {:#?}", world_path);
        return file;
    }
    log::trace!("Updated last world with : {:#?}", &world_name);
    wc_ref.last_world = world_name.to_owned();
    // We drop ref because its not gonna be there
    drop(wc_ref);
    //Now we drop the lock to avoid bugs
    drop(wc_lock);
    let cringe = match prep_world_cache(&world_path) {
        Ok(cringe) => cringe,
        Err(e) => log::error!("Ok so prepping world cache failed with : {}", e),
    };

    log::info!("Prepared world cache for world :{:#?}", &world_name);
    file
}
pub(crate) unsafe extern "C" fn cxx_fopen_hook(
    filename: *const libc::c_char,
    mode: *const libc::c_char,
) -> *mut libc::FILE {
    let file = libc::fopen(filename, mode);
    let c_str = CStr::from_ptr(filename);
    let raw_path = c_str.to_bytes();
    // This normally means that minecraft is loading files form a resource pack
    if !raw_path.ends_with(b"contents.json") {
        return file;
    }
    let mut locked_wc = WORLD_CACHE.lock().expect("Lock is poisoned!");
    let mut wc_owned = locked_wc.deref_mut();
    let os_path = OsStr::from_bytes(raw_path);
    let path = Path::new(os_path);
    if !path.starts_with(&wc_owned.world_path) {
        log::warn!("Path does not end with standard world prefix: {:#?}", path);
        return file;
    }
    // Drop them as they are not used anymore and they lock
    // World name should be before contents.json, rp name and rp folder path
    let Some(world_name) = path.components().nth_back(2) else {
        return file;
    };
    let Component::Normal(world_name) = world_name else {
        log::warn!(
            "Supposed World name is not a standard os string!: {:#?}",
            world_name
        );
        return file;
    };
    let world_path = path.join(world_name);
    if !world_path.exists() {
        log::warn!("Supposed world path does not exist!: {:#?}", world_path);
        return file;
    }
    log::trace!("Updating last world with:{:#?}", &world_name);
    wc_owned.last_world = world_name.to_owned();
    // drop wc to prevent deadlock
    drop(locked_wc);
    let cringe = match prep_world_cache(&world_path) {
        Ok(cringe) => cringe,
        Err(e) => log::error!("Ok so prepping world cache failed with : {}", e),
    };

    log::info!("cxx loaded path, filename:{:#?}", c_str);
    file
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
    let sp_owned = SHADER_PATHS.lock().expect("Mutex got poisoned!");
    if sp_owned.contains_key(filename) {
        let new_path = sp_owned.get(filename)?;
        // sp_owned is locked so once we use it we drop to make way for watch_jsons
        let result = CString::new(new_path.to_str()?).expect("Non utf in sp (this is a bug)");

        return Some(result);
    }
    None
}

pub(crate) fn watch_jsons(app_dir: PathBuf) {
    let mut dataman = DataManager::new(&app_dir);
    let (sender, reciever) = crossbeam_channel::unbounded();
    let mut watcher = RecommendedWatcher::new(sender, Config::default()).unwrap();

    if let Err(e) = watcher.watch(&app_dir, RecursiveMode::NonRecursive) {
        panic!("Watch failed: {}", e);
    };

    for event in reciever {
        let mut event = match event {
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
        let mut path = event.paths.swap_remove(0);
        let mut external_path: Option<PathBuf> = Option::None;
        let file_name = path.file_name().unwrap();
        if file_name == "options.txt" {
            let storage_type = common::parse_options(&path);
            match storage_type {
                StorageType::Internal => {
                    let mut locked = WORLD_CACHE.lock().expect("Lock got poisoned!");
                    let mut wc_owned = locked.deref_mut();
                    let mut cringe = path.clone();
                    cringe.pop();
                    cringe.push("minecraftWorlds");
                    wc_owned.world_path = cringe;
                }
                StorageType::External => {
                    let mut locked = WORLD_CACHE.lock().expect("Lock got poisoned!");
                    let mut wc_owned = locked.deref_mut();
                    if external_path.is_none() {
                        let mut ext_app_dir =
                            app_root(AppDataType::SharedData, &crate::MC_APP_INFO)
                                .expect("App Dirs error");
                        ext_app_dir.extend(["games", "com.mojang", "minecraftWorlds"].into_iter());

                        external_path = Some(ext_app_dir);
                    }
                    let external = external_path.unwrap();
                    wc_owned.world_path = external.clone();
                }
            }
        }
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
// We put these two here and in zero copy mode
// Because i really dont want this to take long
// So we use Cow and serde to go vroom
#[derive(Deserialize)]
pub(crate) struct Header {
    pub uuid: String,
}
#[derive(Deserialize)]
pub(crate) struct PackManifest {
    pub header: Header,
}
pub(crate) struct Pack {
    path: PathBuf,
    uuid: String,
}
pub(crate) fn prep_world_cache(path: &Path) -> Result<(), Error> {
    let mut locked = WORLD_CACHE.lock().expect("Mutex is poisoned!");
    let mut wc_owned = locked.deref_mut();
    // We lock this early to make sure we load our shaders
    let mut locked = SHADER_PATHS.lock().expect("Mutex is poisoned!");
    let mut sp_owned = locked.deref_mut();
    let gpack_path = path.join("world_resource_packs.json");
    let file = fs::read_to_string(&gpack_path)?;
    let apacks: Vec<ActivePack> = serde_json::from_str(&file)?;
    let vpacks: Vec<Pack> = get_packs_from_dir(&path.join("resource_packs"))?;
    let mut paths: HashMap<OsString, PathBuf> = HashMap::new();
    for ap in apacks {
        if let Some(vp) = vpacks.iter().find(|vp| vp.uuid == ap.pack_id) {
            let mut pack_paths = match mc_utils::scan_pack(&vp.path, ap.subpack) {
                Some(pack_paths) => pack_paths,
                None => continue,
            };
            log::trace!("World Pack paths is :{:#?}", &pack_paths);
            let filtered_packs: HashMap<OsString, PathBuf> = pack_paths
                .drain()
                .filter(|(k, _)| !paths.contains_key(k))
                .collect();
            log::trace!("Filtered world packs is:{:#?}", &filtered_packs);
            paths.extend(filtered_packs);
        }
    }
    wc_owned.last_world = path.file_name().unwrap().to_os_string();
    log::trace!("Extending shader packs with {:#?}", &paths);
    log::trace!("Sp Owned before extending is:{:#?}", &sp_owned);
    sp_owned.extend(paths);
    Ok(())
}
fn get_packs_from_dir(path: &Path) -> Result<Vec<Pack>, Error> {
    let paths = fs::read_dir(path)?;
    let mut packs = Vec::new();
    for dir in paths.flatten() {
        let path = dir.path();
        let mpath = path.join("manifest.json");
        let json = match fs::read_to_string(&mpath) {
            Ok(fdata) => fdata,
            Err(_) => continue,
        };
        let parsed_json: PackManifest = match serde_json::from_str(&json) {
            Ok(packmanifest) => packmanifest,
            Err(e) => {
                log::warn!(
                    "manifest with path [{:#?}] isnt valid with error {e}",
                    &mpath
                );
                continue;
            }
        };
        let mvpack = Pack {
            path,
            uuid: parsed_json.header.uuid,
        };
        packs.push(mvpack);
    }
    Ok(packs)
}
fn get_world_packs(path: &Path, subpack: Option<String>) -> Result<(), Error> {
    let pack_dirs = fs::read_dir(path)?;
    for dir in pack_dirs.flatten() {
        let mut path_to_shaders = dir.path();
        path_to_shaders.extend(["renderer", "materials"].into_iter());
    }
    Ok(())
}
pub(crate) fn update_global_sp(dataman: &mut DataManager, full: bool) {
    if full {
        dataman.parse_validpacks().expect("Cant update valid packs");
    }
    let data = dataman.shader_paths().expect("Cant update shader_paths");
    let mut locked_sp = SHADER_PATHS.lock().expect("Mutex got poisoned!");
    *locked_sp = data;
    log::info!("Updated global shader paths: {:#?}", &locked_sp);
    // We changed global sp so we put this to make hook know that
    // the sp is not with the world packs included anymore.
    let mut locked_wc = WORLD_CACHE.lock().expect("Mutex got poisoned!");
    let mut wc = locked_wc.deref_mut();
    wc.last_world = OsString::new();
}
