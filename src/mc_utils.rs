// use json_strip_comments::{strip_comments_in_place, CommentSettings};
use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::ffi::{CStr, OsStr};
use std::fmt::Display;
use std::fs::File;
use std::hash::Hash;
use std::num::ParseIntError;
use std::ops::Range;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::{fmt, fs, io};
use struson::json_path;
use struson::reader::{JsonReader, JsonStreamReader, ReaderError, ReaderSettings};
// use tinyjson::{JsonParseError, JsonParser, JsonValue};
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
from_error!(JsonParse, ReaderError, PackParseError);
from_error!(VersionParse, ParseIntError, PackParseError);
impl Display for PackParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::JsonParse(e) => write!(f, "Manifest parsing error {e}"),
            Self::IoError(e) => write!(f, "Io error while reading: {e}"),
            Self::InvalidManifest => write!(f, "Manifest file is not valid"),
            Self::VersionParse(e) => write!(f, "Failed parsing version: {e}"),
        }
    }
}

#[derive(Debug)]
pub enum PackParseError {
    //    #[error("Manifest parsing error")]
    JsonParse(struson::reader::ReaderError),
    //    #[error("Io error while reading")]
    IoError(std::io::Error),
    //    #[error("Manifest is not valid")]
    InvalidManifest,
    //    #[error("Error while parsing version")]
    VersionParse(std::num::ParseIntError),
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
    version: Vec<u32>,
}
impl GlobalPack {
    fn parse(path: &Path) -> Result<Vec<Self>, DataError> {
        let manifest = File::open(path)?;
        let mut settings = ReaderSettings::default();
        settings.allow_comments = true;
        let mut json = JsonStreamReader::new_custom(manifest, settings);
        json.begin_array()?;
        let mut global_packs = Vec::new();
        while json.has_next()? {
            json.begin_object()?;
            global_packs.push(GlobalPack::parse_one(&mut json)?);
            json.end_object()?;
        }
        json.end_array()?;
        Ok(global_packs)
    }
    fn parse_one(json: &mut impl JsonReader) -> Result<Self, DataError> {
        let mut pack_id = None;
        let mut subpack = None;
        let mut version = None;
        while json.has_next()? {
            match json.next_name()? {
                "pack_id" => pack_id = Some(json.next_string()?),
                "subpack" => subpack = Some(json.next_string()?),
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
        }

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
    JsonParse(ReaderError),
    IoError(io::Error),
    IntConvert(ParseIntError),
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
            Self::IntConvert(e) => write!(f, "Error wgile parsing int: {e}"),
            Self::ManifestParse(e) => write!(f, "Error while parsing manifest file: {e}"),
        }
    }
}
from_error!(IoError, io::Error, DataError);
from_error!(ManifestParse, PackParseError, DataError);
from_error!(JsonParse, ReaderError, DataError);
from_error!(IntConvert, ParseIntError, DataError);
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
