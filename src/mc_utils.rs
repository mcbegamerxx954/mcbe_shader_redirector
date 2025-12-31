use json_strip_comments::{strip_comments_in_place, CommentSettings};
use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::ffi::{CStr, OsStr};
use std::fmt::Display;
use std::hash::Hash;
use std::ops::Range;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::{fmt, fs, io};
use tinyjson::{JsonParseError, JsonParser, JsonValue};
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
    version: Vec<f64>,
}
#[derive(Debug)]
pub enum PackParseError {
    //    #[error("Manifest parsing error")]
    JsonParse(JsonParseError),
    //    #[error("Io error while reading")]
    IoError(std::io::Error),
    //    #[error("Manifest is not valid")]
    InvalidManifest,
    //    CommentStrip(json_strip_comments::Erro)
    //    #[error("Error while parsing version")]
    //    VersionParse(std::num::ParseIntError),
}
macro_rules! from_error {
    ($dis:ident, $errorType:ty, $targetError:ty) => {
        impl From<$errorType> for $targetError {
            fn from(value: $errorType) -> Self {
                Self::$dis(value)
            }
        }
    };
}
from_error!(IoError, std::io::Error, PackParseError);
from_error!(JsonParse, JsonParseError, PackParseError);
impl Display for PackParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::JsonParse(e) => write!(f, "Manifest parsing error {e}"),
            Self::IoError(e) => write!(f, "Io error while reading: {e}"),
            Self::InvalidManifest => write!(f, "Manifest file is not valid"),
            // Self::VersionParse(e) => write!(f, "Failed parsing version: {e}"),
        }
    }
}

impl ValidPack {
    // We do not use serde because it is much more strict
    // than bedrock in terms of json parsing
    fn parse_manifest(pack_path: PathBuf) -> Result<Self, PackParseError> {
        let mut manifest = fs::read_to_string(pack_path.join("manifest.json"))?;
        strip_comments_in_place(&mut manifest, CommentSettings::c_style(), true)?;
        let mut json = tinyjson::JsonParser::new(manifest.chars()).parse()?;

        let header = match json.get_value_mut("header") {
            Some(yay) => yay,
            None => return Err(PackParseError::InvalidManifest),
        };
        let uuid = header.get_value_mut("uuid").and_then(|u| u.get_string());
        let version = header
            .get_value_mut("version")
            .and_then(|v| v.get_array())
            .and_then(|mut a| a.iter_mut().map(|v| v.get_number()).collect());
        if uuid.is_none() || version.is_none() {
            return Err(PackParseError::InvalidManifest);
        }
        Ok(Self {
            uuid: uuid.unwrap(),
            path: pack_path,
            version: version.unwrap(),
        })
    }
    pub fn get_pack_files(&self, subpack: Option<String>, set: &mut HashSet<ResourcePath>) {
        // We add the subpack first as it has priority over main pack
        if let Some(subpack) = subpack {
            let mut path = self.path.to_path_buf();
            path.extend(["subpacks", &subpack]);
            get_files(&path, set);
            //            files.extend(subpack_files);
        }
        // Any files that the subpack has will override these
        get_files(&self.path, set);
    }
}

fn get_files(path: &Path, file_list: &mut HashSet<ResourcePath>) {
    let walker = walkdir::WalkDir::new(path);
    let iter = walker.into_iter().filter_entry(is_interesting).flatten();
    //    let mut files = HashMap::new();
    for entry in iter {
        let curr_path = entry.into_path();
        let Some(resource_path) = ResourcePath::new(curr_path, &path) else {
            continue;
        };
        file_list.insert(resource_path);
    }
    //    files
}

fn wrapping_sub_ptr<T>(lhs: *const T, rhs: *const T) -> usize {
    let pointee_size = std::mem::size_of::<T>();
    (lhs as usize - rhs as usize) / pointee_size
}

pub fn range_of<T>(outer: &[T], inner: &[T]) -> Option<Range<usize>> {
    let outer = outer.as_ptr_range();
    let inner = inner.as_ptr_range();
    if outer.start <= inner.start && inner.end <= outer.end {
        Some(wrapping_sub_ptr(inner.start, outer.start)..wrapping_sub_ptr(inner.end, outer.start))
    } else {
        None
    }
}

pub struct ResourcePath<'a> {
    path: Cow<'a, Path>,
    resource_start: Range<usize>,
}
impl<'a> ResourcePath<'a> {
    pub fn new_nameless(path: Cow<'a, Path>) -> Self {
        let len = path.as_os_str().as_bytes().len();
        Self {
            path,
            resource_start: 0..len,
        }
    }
    pub fn new(path: PathBuf, prefix: &Path) -> Option<Self> {
        let strip = path.strip_prefix(prefix).ok()?;
        let bytes = path.as_os_str().as_encoded_bytes();
        let range = range_of(bytes, strip.as_os_str().as_bytes())?;
        Some(Self {
            path: Cow::Owned(path),
            resource_start: range,
        })
    }
    pub fn path(&self) -> &Path {
        self.path.as_ref()
    }
    pub fn resource_name(&self) -> &Path {
        let osbytes = self.path.as_os_str().as_encoded_bytes();
        let resource = &osbytes[self.resource_start.clone()];
        let osstr = OsStr::from_bytes(resource);
        Path::new(osstr)
    }
}
impl<'a> Hash for ResourcePath<'a> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let osbytes = self.path.as_os_str().as_encoded_bytes();
        let resource = &osbytes[self.resource_start.clone()];
        resource.hash(state);
    }
}
// Spoiler: This is Bullshit
impl<'a> PartialEq for ResourcePath<'a> {
    fn eq(&self, other: &Self) -> bool {
        self.resource_name() == other.resource_name()
    }
    fn ne(&self, other: &Self) -> bool {
        self.resource_name() != other.resource_name()
    }
}
impl<'a> Eq for ResourcePath<'a> {}
//impl Eq for ResourcePath {}
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
#[derive(Debug)]
struct GlobalPack {
    pack_id: String,
    subpack: Option<String>,
    version: Vec<f64>,
}
impl GlobalPack {
    fn parse(path: &Path) -> Result<Vec<Self>, DataError> {
        let data = fs::read_to_string(path)?;
        let mut json = JsonParser::new(data.chars()).parse()?;
        let mut objects = match json.get_array() {
            Some(yay) => yay,
            None => return Err(DataError::InvalidData("array")),
        };
        objects
            .iter_mut()
            .map(|v| GlobalPack::parse_one(v))
            .collect()
    }
    fn parse_one(val: &mut JsonValue) -> Result<Self, DataError> {
        let pack_id = val.get_value_mut("pack_id").and_then(|v| v.get_string());
        let subpack = val.get_value_mut("subpack").and_then(|v| v.get_string());
        let version = val
            .get_value_mut("version")
            .and_then(|v| v.get_array())
            .and_then(|mut a| a.iter_mut().map(|v| v.get_number()).collect());
        let Some(pack_id) = pack_id else {
            return Err(DataError::InvalidData("id"));
        };
        let Some(version) = version else {
            return Err(DataError::InvalidData("version"));
        };
        Ok(Self {
            pack_id,
            subpack,
            version,
        })
    }
}

#[derive(Debug)]
pub enum DataError {
    InvalidData(&'static str),
    JsonParse(JsonParseError),
    IoError(io::Error),
    ManifestParse(PackParseError),
}
impl Display for DataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidData(missing) => {
                write!(f, "Data file is invalid, field {missing} is missing")
            }
            Self::JsonParse(e) => write!(f, "Data file parsing error: {e}"),
            Self::IoError(e) => write!(f, "Io error while reading data: {e}"),
            Self::ManifestParse(e) => write!(f, "Error while parsing manifest file: {e}"),
        }
    }
}
from_error!(IoError, io::Error, DataError);
from_error!(ManifestParse, PackParseError, DataError);
from_error!(JsonParse, JsonParseError, DataError);
impl DataManager {
    // Get minecraft paths and create itself
    pub fn init_data(json_path: PathBuf, resourcepacks_path: PathBuf) -> Self {
        Self {
            resourcepacks_dir: resourcepacks_path,
            active_packs_path: json_path,
        }
    }

    // Get a list of shader paths
    pub fn shader_paths<'a>(&self) -> Result<HashSet<ResourcePath<'a>>, DataError> {
        let global_packs: Vec<GlobalPack> = GlobalPack::parse(&self.active_packs_path)?;
        log::info!("global_packs parsed: {:#?}", global_packs);
        let packs = self.get_installed_packs()?;
        log::info!("Installed packs: {packs:#?}");
        let mut final_paths = HashSet::new();
        // Explanation: we use .rev to reverse the iterator since this way we can avoid
        // some checks
        for pack in global_packs.into_iter().rev() {
            if let Some(vp) = find_valid_pack(&pack, &packs) {
                // We pass the hashset directly to avoid useless allocations that get dropped instantly
                vp.get_pack_files(pack.subpack, &mut final_paths);
            }
        }
        Ok(final_paths)
    }
    fn get_installed_packs(&self) -> Result<Vec<ValidPack>, DataError> {
        let pack_dirs = fs::read_dir(&self.resourcepacks_dir)?;
        let mut packs = Vec::new();
        for dir in pack_dirs.flatten() {
            if !dir.file_type()?.is_dir() {
                continue;
            }
            let manifest_path = match find_pack_folder(&dir.path()) {
                Some(found) => found,
                None => {
                    log::warn!("Cannot find pack manifest for dir: {:?}", dir.path());
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
        if valid_pack.uuid.eq_ignore_ascii_case(&global_pack.pack_id)
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

trait ValGetters {
    fn get_value_mut(&mut self, val_name: &str) -> Option<&mut JsonValue>;
    fn get_string(&mut self) -> Option<String>;
    fn get_array(&mut self) -> Option<Vec<JsonValue>>;
    fn get_number(&self) -> Option<f64>;
}
impl ValGetters for JsonValue {
    fn get_value_mut(&mut self, str: &str) -> Option<&mut JsonValue> {
        let object = match self {
            JsonValue::Object(o) => o,
            _ => return None,
        };
        match object.get_mut(str) {
            Some(h) => Some(h),
            None => None,
        }
    }
    // For efficiency, this will obliverate the value to return it
    fn get_string(&mut self) -> Option<String> {
        let object = match self {
            JsonValue::String(o) => o,
            _ => return None,
        };
        Some(std::mem::take(object))
    }

    // For efficiency, this will obliverate the value to return it
    fn get_array(&mut self) -> Option<Vec<JsonValue>> {
        let object = match self {
            JsonValue::Array(o) => o,
            _ => return None,
        };
        Some(std::mem::take(object))
    }

    fn get_number(&self) -> Option<f64> {
        match self {
            JsonValue::Number(o) => Some(*o),
            _ => None,
        }
    }
}
