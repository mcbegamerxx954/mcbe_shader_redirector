use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::collections::HashMap;
use std::ffi::OsString;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::{fs, io};
use struson::json_path;
use struson::reader::{JsonReader, JsonStreamReader, ReaderSettings};
use thiserror::Error;

// Keeps track and manages data about the minecraft Resource Pack Structure
#[derive(Debug)]
pub struct DataManager {
    pub resourcepacks_dir: PathBuf,
    pub active_packs_path: PathBuf,
}

// A pack that minecraft verified as valid
#[derive(Debug)]
pub struct ValidPack {
    uuid: String,
    path: PathBuf,
    version: Vec<u32>,
}
#[derive(Error, Debug)]
pub enum PackParseError {
    #[error("Manifest parsing error")]
    JsonParse(#[from] struson::reader::ReaderError),
    #[error("Io error while reading")]
    IoError(#[from] std::io::Error),
    #[error("Manifest is not valid")]
    InvalidManifest,
    #[error("Error while parsing version")]
    VersionParse(#[from] std::num::ParseIntError),
}
impl ValidPack {
    fn parse_manifest(pack_path: PathBuf) -> Result<Self, PackParseError> {
        let manifest = File::open(pack_path.join("manifest.json"))?;
        let mut settings = ReaderSettings::default();
        settings.allow_comments = true;
        let mut json = JsonStreamReader::new_custom(manifest, settings);
        json.seek_to(&json_path!["header"])?;
        json.begin_object()?;
        let mut uuid = None;
        let mut version = None;
        loop {
            match json.next_name()? {
                "uuid" => uuid = Some(json.next_string()?),
                "version" => {
                    json.begin_array()?;
                    let mut numbers: Vec<u32> = Vec::new();
                    while json.has_next()? {
                        let workaround = json.next_number()?;
                        numbers.push(workaround?);
                    }
                    json.end_array()?;
                    version = Some(numbers);
                }
                _ => {
                    json.skip_value()?;
                }
            }
            if !json.has_next()? {
                break;
            }
        }
        json.end_object()?;
        if uuid.is_none() || version.is_none() {
            return Err(PackParseError::InvalidManifest);
        }
        Ok(Self {
            uuid: uuid.unwrap(),
            path: pack_path,
            version: version.unwrap(),
        })
    }
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
    #[error("Expected valid globalpack json")]
    JsonError(#[from] serde_json::Error),
    #[error("Io error while reading json")]
    IoError(#[from] io::Error),
    #[error("Failed parsing pack manifest")]
    ManifestParse(#[from] PackParseError),
}

impl DataManager {
    // Get minecraft paths and create itself
    pub fn init_data(json_path: PathBuf, resourcepacks_path: PathBuf) -> Self {
        Self {
            resourcepacks_dir: resourcepacks_path,
            active_packs_path: json_path,
        }
    }

    // Get a list of shader paths
    pub fn shader_paths(&self) -> Result<HashMap<OsString, PathBuf>, DataError> {
        let global_packs: Vec<GlobalPack> = vec_from_json(&self.active_packs_path)?;
        log::info!("global_packs parsed: {:#?}", global_packs);
        let packs = self.get_installed_packs()?;
        log::info!("Installed packs: {packs:#?}");
        let mut final_paths = HashMap::new();
        for pack in global_packs {
            if let Some(vp) = find_valid_pack(&pack, &packs) {
                let mut paths = match scan_pack(&vp.path, pack.subpack) {
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
    fn get_installed_packs(&self) -> Result<Vec<ValidPack>, DataError> {
        let pack_dirs = fs::read_dir(&self.resourcepacks_dir)?;
        let mut packs = Vec::new();
        for pack in pack_dirs.flatten() {
            let manifest_path = match find_pack_folder(&pack.path()) {
                Some(found) => found,
                None => {
                    log::warn!("Cannot find pack manifest for dir: {:?}", pack.path());
                    continue;
                }
            };
            let validpack = match ValidPack::parse_manifest(manifest_path) {
                Ok(pack) => pack,
                Err(err) => {
                    log::info!("Pack manifest parse failed: {err}");
                    continue;
                }
            };
            packs.push(validpack);
        }
        Ok(packs)
    }
}
fn find_valid_pack<'a>(
    global_pack: &GlobalPack,
    valid_packs: &'a [ValidPack],
) -> Option<&'a ValidPack> {
    for valid_pack in valid_packs {
        if valid_pack.uuid.to_lowercase() == global_pack.pack_id.to_lowercase()
            && valid_pack.version == global_pack.version
        {
            return Some(valid_pack);
        }
    }
    None
}

// This is rare, but can happen
fn find_pack_folder(path: &Path) -> Option<PathBuf> {
    let walker = walkdir::WalkDir::new(path);
    for entry in walker.into_iter().flatten() {
        if entry.file_name() == "manifest.json" {
            let mut path = entry.into_path();
            let _ = path.pop();
            return Some(path);
        }
    }
    None
}
fn scan_pack(
    path: &Path,
    subpack: Option<String>,
) -> Result<HashMap<OsString, PathBuf>, io::Error> {
    log::trace!("Scanning path: {}", path.display());
    let mut found_paths = HashMap::new();
    let mut main_path = Path::new(path).join("renderer");
    main_path.push("materials");
    // Scan main path if it exists
    if main_path.is_dir() {
        found_paths = scan_path(&main_path)?;
        log::info!("Main path had shaders");
    }
    // Scan subpack path if it exists
    if let Some(subpack) = subpack {
        let mut subpath = Path::new(path).join("subpacks");
        // Doing it like this prevents allocs + its more crossplatform
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
        // This check failing is very bad
        #[cfg(target_os = "android")]
        if metadata.len() >= usize::MAX as u64 {
            continue;
        }
        // Check if its... well a file
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
