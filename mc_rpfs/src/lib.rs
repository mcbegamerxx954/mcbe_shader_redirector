use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::{fs, io};
use thiserror::Error;

// Keeps track and manages data about the minecraft Resource Pack Structure
pub struct DataManager {
    valid_packs: Vec<ValidPack>,
    valid_packs_path: PathBuf,
    global_packs_path: PathBuf,
}

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
struct GlobalPack {
    pack_id: String,
    subpack: Option<String>,
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
    // Get minecraft paths and create itself
    pub fn init_data(mcjsons_dir: &Path) -> Self {
        let valid_packs_path = mcjsons_dir.join("valid_known_packs.json");
        let global_packs_path = mcjsons_dir.join("global_resource_packs.json");
        Self {
            valid_packs: Vec::new(),
            valid_packs_path,
            global_packs_path,
        }
    }

    // Get valid packs from minecraft
    pub fn update_validpacks(&mut self) -> Result<(), Error> {
        let mut valid_packs: Vec<ValidPack> = vec_from_json(&self.valid_packs_path)?;
        if let Some(file_version) = valid_packs[0].file_version {
            assert!(file_version == 2);
            valid_packs.remove(0);
        };
        self.valid_packs = valid_packs;
        Ok(())
    }

    // Get a list of shader paths
    pub fn shader_paths(&self) -> Result<HashMap<OsString, PathBuf>, Error> {
        let global_packs: Vec<GlobalPack> = vec_from_json(&self.global_packs_path)?;
        log::info!("global_packs parsed: {:#?}", global_packs);
        let mut final_paths = HashMap::new();
        for pack in global_packs {
            if let Some(vp) = self.valid_packs.iter().find(|vp| pack.pack_id == vp.uuid) {
                let mut paths = match scan_pack(&vp.path, pack.subpack) {
                    Some(paths) => paths,
                    None => continue,
                };
                log::info!("scan pack paths : {:#?}", &paths);
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
    // Get shaders in pack directory
}
fn scan_pack(path: &str, subpack: Option<String>) -> Option<HashMap<OsString, PathBuf>> {
    let path = Path::new(path);
    let mut pack_files = match scan_path(path) {
        Ok(paths) => paths,
        Err(_) => return None,
    };
    if let Some(subpack) = subpack {
        let subpath = path.join("subpacks/").join(subpack);
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
    let dir_entries = fs::read_dir(&path.join("renderer/materials"))?;
    let mut paths: HashMap<OsString, PathBuf> = HashMap::new();
    for entry in dir_entries.flatten() {
        let path = entry.path();
        let file_name = entry.file_name();
        let bytes = file_name.as_encoded_bytes();
        if !paths.contains_key(&file_name) && bytes.ends_with(b".material.bin") {
            log::info!("scan_path found a valid shader path!: {:#?}", &path);
            paths.insert(file_name, path);
        }
    }
    Ok(paths)
}

pub(crate) fn vec_from_json<T: DeserializeOwned>(path: &Path) -> Result<Vec<T>, Error> {
    let json_file = fs::read_to_string(path).expect("path does not exist!!");
    let json_vec: Vec<T> = serde_json::from_str(&json_file)?;
    Ok(json_vec)
}
