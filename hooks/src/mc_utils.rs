
use serde::Deserialize;
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::{fs, io};
use thiserror::Error;

// A pack that minecraft verified as valid
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct ValidPack {
    file_version: Option<u32>,
    uuid: String,
    path: String,
}

// A active global pack
#[derive(Deserialize, Debug)]
pub struct ActivePack {
    pub pack_id: String,
    pub subpack: Option<String>,
}
pub struct DataManager {
    valid_packs: Vec<ValidPack>,
    valid_packs_path: PathBuf,
    active_packs_path: PathBuf,
}
#[derive(Error, Debug)]
pub enum Error {
    //    #[error("Getting minecraft dir failed")]
    //    AppDirsError(#[from] app_dirs2::AppDirsError);
    #[error("Failed to deserialize json")]
    JsonError(#[from] serde_json::Error),
    #[error("Io error while reading json")]
    ReadError(#[from] io::Error),
}
impl DataManager {
    pub fn new(path: &Path) -> Self {
        let valid_packs_path = path.join("valid_resource_packs.json");
        let active_packs_path = path.join("global_resource_packs.json");
        Self {
            valid_packs: Vec::new(),
            valid_packs_path,
            active_packs_path,
        }
    }
    // Get valid packs from minecraft
    pub fn parse_validpacks(&mut self) -> Result<(), Error> {
        let file = fs::read_to_string(&self.valid_packs_path)?;
        let mut valid_packs: Vec<ValidPack> = serde_json::from_str(&file)?;
        if let Some(file_version) = valid_packs[0].file_version {
            assert!(file_version == 2);
            valid_packs.remove(0);
        };
        self.valid_packs = valid_packs;
        Ok(())
    }
    // Get a list of shader paths
    pub fn shader_paths(&mut self) -> Result<HashMap<OsString, PathBuf>, Error> {
        let file = fs::read_to_string(&self.active_packs_path)?;
        let active_packs: Vec<ActivePack> = serde_json::from_str(&file)?;
        let mut final_paths = HashMap::new();
        for pack in active_packs {
            if let Some(valid_pack) = self.valid_packs.iter().find(|vp| vp.uuid == pack.pack_id) {
                let mut paths = match scan_pack(&valid_pack.path, pack.subpack) {
                    Some(paths) => paths,
                    None => continue,
                };
                log::info!("scan pack paths : {:#?}", &paths);
                // Idrk if this is efficient or not but i guess not
                let filt_paths: HashMap<OsString, PathBuf> = paths
                    .drain()
                    .filter(|(k, _)| !final_paths.contains_key(k))
                    .collect();
                log::info!("fil paths is :{:#?}", &filt_paths);
                final_paths.extend(filt_paths);
            }
        }

        Ok(final_paths)
    }
}
// Get shaders in pack directory

/// Scan a pack and returns paths containing shaders, or none if theres an error
pub fn scan_pack<T: AsRef<Path>>(
    path: T,
    subpack: Option<String>,
) -> Option<HashMap<OsString, PathBuf>> {
    let path = path.as_ref();
    let mut pack_files = match scan_path(path) {
        Ok(paths) => paths,
        Err(_) => return None,
    };
    if let Some(subpack) = subpack {
        let mut subpath = path.join("subpacks");
        subpath.push(subpack);
        let sub_files = match scan_path(&subpath) {
            Ok(subpaths) => subpaths,
            Err(_) => return None,
        };
        log::info!("expanding pack files with :{:#?}", &sub_files);
        pack_files.extend(sub_files);
    }
    Some(pack_files)
}
fn scan_path(path: &Path) -> Result<HashMap<OsString, PathBuf>, io::Error> {
    let mut path = path.join("renderer");
    path.push("materials");
    let dir_entries = fs::read_dir(path)?;
    let mut paths: HashMap<OsString, PathBuf> = HashMap::new();
    for entry in dir_entries.flatten() {
        let path = entry.path();
        let file_name = entry.file_name();
        let bytes = file_name.as_encoded_bytes();
        if bytes.ends_with(b".material.bin") {
            log::info!("scan_path found a valid shader path!: {:#?}", &path);
            paths.insert(file_name, path);
        }
    }
    Ok(paths)
}
