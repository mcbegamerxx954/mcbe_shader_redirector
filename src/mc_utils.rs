use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::{fs, io};
use thiserror::Error;
use walkdir::WalkDir;

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
    version: String,
}

// A active global pack
#[derive(Deserialize, Debug)]
struct GlobalPack {
    pack_id: String,
    subpack: Option<String>,
    version: Vec<u32>,
}

#[derive(Debug, Error)]
pub enum DataError {
    #[error("Expected valid json")]
    JsonError(#[from] serde_json::Error),
    #[error("Io error while reading json")]
    IoError(#[from] io::Error),
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
    pub fn update_validpacks(&mut self) -> Result<(), DataError> {
        let mut valid_packs: Vec<ValidPack> = vec_from_json(&self.valid_packs_path)?;
        if let Some(file_version) = valid_packs[0].file_version {
            assert!(file_version == 2);
            valid_packs.remove(0);
        };
        self.valid_packs = valid_packs;
        Ok(())
    }

    // Get a list of shader paths
    pub fn shader_paths(&self) -> Result<HashMap<OsString, PathBuf>, DataError> {
        let global_packs: Vec<GlobalPack> = vec_from_json(&self.global_packs_path)?;
        log::info!("global_packs parsed: {:#?}", global_packs);
        let mut final_paths = HashMap::new();
        for pack in global_packs {
            if let Some(vp) = find_valid_pack(&pack, &self.valid_packs) {
                let Some(path) = handle_weird_directory(Path::new(&vp.path)) else {
                    log::warn!("Did not find pack in path... skipping");
                    continue;
                };
                let mut paths = match scan_pack(&path, pack.subpack) {
                    Ok(paths) => paths,
                    Err(e) => {
                        log::error!("Path scanning error: {e}");
                        continue;
                    }
                };
                paths.retain(|k, _| !final_paths.contains_key(k));
                log::info!("shader paths are: {:#?}", &paths);
                final_paths.extend(paths);
            }
        }

        Ok(final_paths)
    }
    // Get shaders in pack directory
}
fn find_valid_pack<'a>(
    global_pack: &GlobalPack,
    valid_packs: &'a Vec<ValidPack>,
) -> Option<&'a ValidPack> {
    for valid_pack in valid_packs {
        if valid_pack.uuid == global_pack.pack_id
            && valid_pack.version == process_version_array(&global_pack.version)
        {
            return Some(valid_pack);
        }
    }
    None
}
fn process_version_array(version: &Vec<u32>) -> String {
    let mut version_str = String::new();
    for int in version {
        version_str.push_str(&format!("{int}."))
    }
    let _ = version_str.pop();
    version_str
}
fn handle_weird_directory(path: &Path) -> Option<PathBuf> {
    WalkDir::new(path)
        .into_iter()
        .flatten()
        .find(|e| e.file_name().as_encoded_bytes() == b"manifest.json")
        .and_then(|e| e.path().parent().and_then(|e| Some(e.to_path_buf())))
}
fn scan_pack(
    path: &Path,
    subpack: Option<String>,
) -> Result<HashMap<OsString, PathBuf>, io::Error> {
    log::trace!("Scanning path: {:#?}", path);
    let mut found_paths = HashMap::new();
    let mut main_path = path.join("renderer");
    main_path.push("materials");
    if main_path.is_dir() {
        found_paths.extend(scan_path(&main_path)?);
        log::info!("Main path had shaders");
    }
    if let Some(subpack) = subpack {
        let mut subpath = Path::new(path).join("subpacks");
        subpath.extend([&subpack, "renderer", "materials"]);
        if subpath.is_dir() {
            found_paths.extend(scan_path(&subpath)?);
        }
    }
    Ok(found_paths)
}
fn scan_path(path: &Path) -> Result<HashMap<OsString, PathBuf>, io::Error> {
    let dir_entries = fs::read_dir(path)?;
    let mut paths: HashMap<OsString, PathBuf> = HashMap::new();
    for entry in dir_entries.flatten() {
        let path = entry.path();
        let osfile_name = entry.file_name();
        // Some very important checks are done here
        let metadata = entry.metadata()?;
        // Check if len is larger than usize
        #[cfg(target_os = "android")]
        if metadata.len() >= usize::MAX as u64 {
            continue;
        }
        if !metadata.is_file() {
            continue;
        }
        // Mojang won't use non utf8 i know it
        let Some(file_name) = osfile_name.to_str() else {
            continue;
        };
        if !paths.contains_key(&osfile_name) && file_name.ends_with(".material.bin") {
            log::info!("scan_path found a valid shader path!: {:#?}", &path);
            paths.insert(osfile_name, path);
        }
    }
    Ok(paths)
}
pub(crate) fn vec_from_json<T: DeserializeOwned>(path: &Path) -> Result<Vec<T>, DataError> {
    let json_file = fs::read_to_string(path)?;
    let json_vec: Vec<T> = serde_json::from_str(&json_file)?;
    Ok(json_vec)
}
