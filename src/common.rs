use crate::mc_utils::DataManager;
use crate::platform::android::{get_storage_location, get_storage_path};
use crate::platform::storage::StorageLocation;
use crate::SHADER_PATHS;
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub(crate) fn setup_json_watcher(path: PathBuf) {
    let current_location = match get_storage_location(&path.join("options.txt")) {
        Some(yayy) => yayy,
        None => StorageLocation::Internal,
    };
    let path = get_storage_path(current_location);
    let mut data_manager = setup_dataman(&path);
    if !data_manager.active_packs_path.exists() {
        data_manager.active_packs_path =
            setup_dataman(&get_storage_path(StorageLocation::Internal)).active_packs_path;
        if !data_manager.active_packs_path.exists() {
            log::info!("no active_packs file found, using internal and hoping for the best");
        }
        log::info!("global packs json not found, defaulting to internal storage");
    }
    startup_load(&mut data_manager);
    let (sender, reciever) = std::sync::mpsc::channel();
    let mut watcher = RecommendedWatcher::new(sender, Config::default()).unwrap();
    loop {
        if data_manager.active_packs_path.exists() {
            break;
        } else {
            std::thread::sleep(Duration::from_secs(5));
        }
    }
    watcher
        .watch(&data_manager.active_packs_path, RecursiveMode::NonRecursive)
        .unwrap();
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
            update_global_sp(&mut data_manager);
        }
    }
}
fn update_global_sp(dataman: &mut DataManager) {
    let mut locked_sp = SHADER_PATHS
        .lock()
        .expect("The shader paths lock should never be poisoned");
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
fn startup_load(dataman: &mut DataManager) {
    log::info!("Trying to load files eagerly");
    update_global_sp(dataman);
}
fn setup_dataman(mc_path: &Path) -> DataManager {
    let mut json_path = mc_path.to_path_buf();
    json_path.extend([
        "games",
        "com.mojang",
        "minecraftpe",
        "global_resource_packs.json",
    ]);
    let mut resourcepacks_path = mc_path.to_path_buf();
    resourcepacks_path.extend(["games", "com.mojang", "resource_packs"]);
    DataManager::init_data(json_path, resourcepacks_path)
}
