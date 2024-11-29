use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::ffi::OsString;
use std::fs::{File, FileType};
use std::path::{Path, PathBuf, MAIN_SEPARATOR};
use std::{fs, io};
use struson::json_path;
use struson::reader::{JsonReader, JsonStreamReader, ReaderSettings};
use thiserror::Error;
use walkdir::DirEntry;

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
    // We do not use serde because it is much more strict
    // than bedrock in terms of json parsing
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
    pub fn get_pack_files(&self, subpack: Option<String>) -> HashMap<PathBuf, PathBuf> {
        let mut files = get_files(&self.path);
        if let Some(subpack) = subpack {
            let subpack_files = get_files(&self.path.join(subpack));
            files.extend(subpack_files);
        }
        files
    }
}

fn get_files(path: &Path) -> HashMap<PathBuf, PathBuf> {
    let walker = walkdir::WalkDir::new(path);
    let iter = walker.into_iter().filter_entry(is_interesting).flatten();
    let mut files = HashMap::new();
    for entry in iter {
        let curr_path = entry.into_path();
        let resource_name = curr_path.strip_prefix(path).unwrap();
        files.insert(resource_name.to_path_buf(), curr_path);
    }
    files
}
fn is_interesting(entry: &DirEntry) -> bool {
    if entry.depth() == 1 {
        return entry.file_name() == "renderer"
            || entry.file_name() == "vanilla_cameras"
            || entry.file_name() == "hbui"
            || entry.file_name() == "custom_persona";
    }
    true
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
    pub fn shader_paths(&self) -> Result<HashMap<PathBuf, PathBuf>, DataError> {
        let global_packs: Vec<GlobalPack> = vec_from_json(&self.active_packs_path)?;
        log::info!("global_packs parsed: {:#?}", global_packs);
        let packs = self.get_installed_packs()?;
        log::info!("Installed packs: {packs:#?}");
        let mut final_paths = HashMap::new();
        // Explanation: we use .rev to reverse the iterator since this way we can avoid
        // some checks
        for pack in global_packs.into_iter().rev() {
            if let Some(vp) = find_valid_pack(&pack, &packs) {
                let paths = vp.get_pack_files(pack.subpack);
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
    let walker = walkdir::WalkDir::new(path).sort_by(compare);
    for entry in walker.into_iter().flatten() {
        if entry.file_name() == "manifest.json" && entry.file_type().is_file() {
            let mut path = entry.into_path();
            let _ = path.pop();
            return Some(path);
        }
    }
    None
}
fn compare(entry1: &DirEntry, entry2: &DirEntry) -> Ordering {
    let ftype1 = entry1.file_type();
    let ftype2 = entry2.file_type();
    if ftype1.is_file() && !ftype2.is_file() {
        return Ordering::Less;
    }
    if !ftype1.is_file() && ftype2.is_file() {
        return Ordering::Greater;
    }
    if ftype1.is_file() && ftype2.is_file() {
        return Ordering::Equal;
    }
    Ordering::Equal
}
pub(crate) fn vec_from_json<T: DeserializeOwned>(path: &Path) -> Result<Vec<T>, DataError> {
    let json_file = fs::read_to_string(path)?;
    let json_vec: Vec<T> = serde_json::from_str(&json_file)?;
    Ok(json_vec)
}
