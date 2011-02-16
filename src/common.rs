use crate::mc_utils::DataManager;
use crate::platform::android::{self, get_storage_location, get_storage_path};
use crate::platform::storage::StorageLocation;
use crate::SHADER_PATHS;
use notify::event::{AccessKind, AccessMode, EventKind};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::fs::File;
use std::path::{Path, PathBuf};

pub(crate) fn setup_json_watcher(path: PathBuf) {
    let current_location = match get_storage_location(&path.join("options.txt")) {
        Some(yayy) => yayy,
        None => StorageLocation::Internal,
    };
    let mut path = get_storage_path(current_location);
    path.extend(["games", "com.mojang", "minecraftpe"]);
    log::info!("location = {current_location:#?}");
    if !path.join("valid_known_packs.json").exists() {
        log::warn!("Options storage invalid, cowardly defaulting to internal");
        path = get_storage_path(StorageLocation::Internal);
        path.extend(["games", "com.mojang", "minecraftpe"]);
    }

    let mut data_manager = DataManager::init_data(&path);
    let (sender, reciever) = crossbeam_channel::unbounded();
    let mut watcher = RecommendedWatcher::new(sender, Config::default()).unwrap();
    setup_watches(
        &mut watcher,
        &path,
        &["valid_known_packs.json", "global_resource_packs.json"],
    );
    for event in reciever {
        let event = match event {
            Ok(event) => event,
            Err(e) => {
                log::info!("Skipping event error: {e}");
                continue;
            }
        };
        log::info!("Recieved interesting event: {:#?}", event);
        let Some(file_name) = event.paths.first().and_then(|p| p.file_name()) else {
            log::warn!("Event path is empty or with no filename");
            continue;
        };

        if file_name == "global_resource_packs.json" {
            log::info!("Active rpacks changed, updating..");
            update_global_sp(&mut data_manager, false);
        }
        if file_name == "valid_known_packs.json" {
            log::info!("Valid rpackschanged, updating active packs too..");
            update_global_sp(&mut data_manager, true);
        }
    }
}
fn update_global_sp(dataman: &mut DataManager, full: bool) {
    let mut locked_sp = SHADER_PATHS
        .lock()
        .expect("The shader paths lock should never be poisoned");
    if full {
        if let Err(e) = dataman.update_validpacks() {
            log::warn!("Cant update valid packs: {:#?}", e);
            return;
        };
    }
    let data = match dataman.shader_paths() {
        Ok(spaths) => spaths,
        Err(e) => {
            log::warn!("Cant update shader paths: {:#?}", e);
            return;
        }
    };
    *locked_sp = data;
    log::info!("Updated global shader paths: {:#?}", &locked_sp);
}
fn setup_watches(watcher: &mut impl Watcher, path: &Path, files: &[&str]) {
    for file in files {
        let path = path.join(file);
        if !path.exists() {
            File::create(&path).unwrap();
        }
        watcher.watch(&path, RecursiveMode::NonRecursive).unwrap();
    }
}
