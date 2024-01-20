use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::fs::{File, OpenOptions};
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
    pub fn shader_paths(&self) -> Result<Vec<PathBuf>, Error> {
        let global_packs: Vec<GlobalPack> = vec_from_json(&self.global_packs_path)?;
        log::info!("global_packs parsed: {:#?}", global_packs);
        let mut paths_to_check = Vec::new();
        // we get paths from the valid_paths
        // TODO: Im sure we can be terser
        for gp in global_packs {
            if let Some(valid_pack) = &self.valid_packs.iter().find(|vp| vp.uuid == gp.pack_id) {
                log::info!("Found path to global pack: {}", valid_pack.path);
                paths_to_check.push(valid_pack.path.clone());
            }
        }
        // we get all dirs which have shaders
        // TODO: Improve this to be more efficient
        let shader_dirs = self.scan_paths(paths_to_check);

        Ok(shader_dirs)
    }
    // Get shaders in pack directory
    pub(crate) fn scan_paths(&self, pack_paths: Vec<String>) -> Vec<PathBuf> {
        // The fact that it failed means that its probably not worth searching
        let mut seen_filenames = Vec::new();
        let mut shader_paths = Vec::new();
        for pack_path in pack_paths {
            let dir_entries = match fs::read_dir(pack_path + "/renderer/materials/") {
                Ok(dir_entries) => dir_entries,
                Err(_) => continue,
            };
            let mut file_paths: Vec<PathBuf> = Vec::new();
            for entry in dir_entries.flatten() {
                let path = entry.path();
                log::info!("investigating path:{:#?}", &path);
                let file_name = entry.file_name();
                if seen_filenames.contains(&file_name) {
                    log::info!("we already have shader: {:?}", &file_name);
                    continue;
                }
                let fp_bytes = file_name.as_encoded_bytes();
                if fp_bytes.ends_with(b"material.bin") {
                    log::info!("Found shader, pushing path: {:#?}", &path);
                    seen_filenames.push(file_name);
                    file_paths.push(path);
                }
            }
            shader_paths.extend(file_paths);
        }
        shader_paths
    }
}

pub(crate) fn vec_from_json<T: DeserializeOwned>(path: &Path) -> Result<Vec<T>, Error> {
    let json_file = fs::read_to_string(path).expect("path does not exist!!");
    let json_vec: Vec<T> = serde_json::from_str(&json_file)?;
    Ok(json_vec)
}
