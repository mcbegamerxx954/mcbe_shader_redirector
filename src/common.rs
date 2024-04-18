use crate::mc_utils::DataManager;
use crate::SHADER_PATHS;
use notify::event::{AccessKind, AccessMode, EventKind};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;

pub(crate) fn setup_json_watcher<T: AsRef<Path>>(app_dir: T) {
    let path: &Path = app_dir.as_ref();
    let mut data_manager = DataManager::init_data(path);
    let (sender, reciever) = crossbeam_channel::unbounded();
    let mut watcher = RecommendedWatcher::new(sender, Config::default()).unwrap();
    watcher.watch(path, RecursiveMode::NonRecursive).unwrap();

    for event in reciever {
        let event = match event {
            Ok(event) => event,
            Err(e) => {
                log::info!("Skipping event error: {e}");
                continue;
            }
        };
        if event.kind != EventKind::Access(AccessKind::Close(AccessMode::Write)) {
            log::info!("Skipping event..");
            continue;
        }
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
