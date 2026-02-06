use crate::mc_utils::{DataError, DataManager};
use crate::platform::android::{get_storage_location, get_storage_path};
use crate::platform::storage::StorageLocation;
use crate::SHADER_PATHS;
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
pub static SHOULD_STOP: AtomicBool = AtomicBool::new(false);
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
        let should_stop = SHOULD_STOP.load(Ordering::Acquire);
        if should_stop {
            // Something happened that requires us to stop this thread
            return;
        }
        // Recieve a filesystem event
        let event = match event {
            Ok(event) => event,
            Err(e) => {
                log::info!("Skipping event error: {e}");
                continue;
            }
        };
        log::info!("Recieved interesting event: {:#?}", event);
        // Get the first filename in the event
        let Some(path) = event.paths.first() else {
            log::warn!("No event path found");
            continue;
        };
        let Some(file_name) = path.file_name() else {
            log::warn!("Event path has no filename");
            continue;
        };

        if &data_manager.active_packs_path != path {
            log::warn!("Wrong path detected, correcting..");
            let new_dataman =
                DataManager::init_data(path.clone(), data_manager.resourcepacks_dir.clone());
            data_manager = new_dataman;
        }
        // This means that Minecraft has changed or read the resource list, let's do it too
        if file_name == "global_resource_packs.json" && event.kind.is_modify() {
            log::info!("Active rpacks changed, updating..");

            if let Err(e) = update_global_sp(&mut data_manager) {
                log::warn!("Updating shader paths failed: {e}");
            };
        }
    }
}
fn update_global_sp<'guh>(dataman: &'guh mut DataManager) -> Result<(), DataError> {
    let time = Instant::now();

    let mut locked_sp = SHADER_PATHS.lock().unwrap_or_else(|err| err.into_inner()); //        .expect("The shader paths lock should never be poisoned");
    let data = dataman.shader_paths()?;
    // drop(dataman);
    //

    *locked_sp = data;
    log::info!(
        "Updated global shader paths in {}ms...",
        time.elapsed().as_millis()
    );
    Ok(())
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
