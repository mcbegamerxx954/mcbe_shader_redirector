use crate::mc_utils::DataManager;
use crate::SHADER_PATHS;
use notify::event::{AccessKind, AccessMode, EventKind};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;

pub(crate) fn setup_json_watcher(app_dir: &Path) {
    let mut dataman = DataManager::init_data(app_dir);
    let (sender, reciever) = crossbeam_channel::unbounded();
    let mut watcher = RecommendedWatcher::new(sender, Config::default()).unwrap();
    watcher.watch(app_dir, RecursiveMode::NonRecursive).unwrap();

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
        debug_assert!(!event.paths.is_empty());
        let file_name = match event.paths[0].file_name() {
            Some(file_name) => file_name,
            None => {
                log::warn!("Event path is not a filename");
                continue;
            }
        };
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
fn update_global_sp(dataman: &mut DataManager, full: bool) {
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
    // If this happened mcbe crashed, somehow....
    let mut locked_sp = SHADER_PATHS.lock().unwrap();
    *locked_sp = data;
    log::info!("Updated global shader paths: {:#?}", &locked_sp);
}
