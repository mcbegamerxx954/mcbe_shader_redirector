use crate::SHADER_PATHS;
use crate::{mc_utils::ResourcePath, platform::OPTS};
use libc::{off64_t, off_t};
use materialbin::{
    bgfx_shader::BgfxShader,
    pass::{ShaderCodePlatform, ShaderStage},
    CompiledMaterialDefinition, MinecraftVersion,
};
use memchr::memmem::Finder;
use std::{borrow::Cow, ptr::NonNull, sync::atomic::Ordering};
//use ndk::asset::Asset;
use crate::LockResultExt;
use ndk::asset::{Asset, AssetManager};
use ndk_sys::{AAsset, AAssetManager};
use scroll::Pread;
use std::{
    collections::HashMap,
    ffi::{CStr, OsStr},
    fs::File,
    io::{self, Cursor, Read, Seek},
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, LazyLock, Mutex, OnceLock},
};
// This makes me feel wrong... but all we will do is compare the pointer
// and the struct will be used in a mutex so i guess this is safe??
#[derive(PartialEq, Eq, Hash)]
struct AAssetPtr(*const ndk_sys::AAsset);
unsafe impl Send for AAssetPtr {}

// the assets we want to intercept access to
static WANTED_ASSETS: LazyLock<Mutex<HashMap<AAssetPtr, CowFile>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
pub(crate) unsafe fn asset_open(
    man: *mut AAssetManager,
    fname: *const libc::c_char,
    mode: libc::c_int,
) -> *mut ndk_sys::AAsset {
    // This is where ub can happen, but we are merely a hook.
    let aasset = unsafe { ndk_sys::AAssetManager_open(man, fname, mode) };
    let c_str = unsafe { CStr::from_ptr(fname) };
    let raw_cstr = c_str.to_bytes();
    let os_str = OsStr::from_bytes(raw_cstr);
    let c_path: &Path = Path::new(os_str);
    let pointer = NonNull::new(man).unwrap();
    // let manager = unsafe { ndk::asset::AssetManager::from_ptr(pointer) };

    let Some(os_filename) = c_path.file_name() else {
        log::warn!("Path had no filename: {c_path:?}");
        return aasset;
    };
    let stripped_path = c_path.strip_prefix("assets/").unwrap_or(c_path);
    let replacement_list = [
        ("gui/dist/hbui/", "hbui/"),
        ("renderer/", "renderer/"),
        ("resource_packs/vanilla/cameras", "vanilla_cameras/"),
        ("skin_packs/persona", "custom_persona/"),
    ];
    for replacement in replacement_list {
        if let Ok(path) = stripped_path.strip_prefix(replacement.0) {
            let shader_paths = SHADER_PATHS.lock().unwrap();
            // this will be used if the joined path fits
            let mut bytes = [0; 128];
            // this will be used if the joined path does not fit in bytes var
            let mut planb = PathBuf::new();
            // Try to avoid allocation
            let path = opt_path_join(
                &mut bytes,
                Some(&mut planb),
                &[Path::new(replacement.1), path],
            );
            let aah = ResourcePath::new_nameless(Cow::Borrowed(path));
            // Try to get the file
            let filepath = match shader_paths.get(&aah) {
                Some(path) => path,
                None => {
                    log::info!("Cannot load file: {:?}", path);
                    return aasset;
                }
            };
            let file = match File::open(filepath.path()) {
                Ok(file) => file,
                Err(err) => {
                    log::warn!("Cannot open file: {err}");
                    return aasset;
                }
            };
            let buffer = CowFile::File(file);

            let mut wanted_lock = WANTED_ASSETS.lock().unwrap();
            wanted_lock.insert(AAssetPtr(aasset), buffer);
            return aasset;
        }
    }
    return aasset;
}
/// Join paths without allocating if possible, or
/// if the joined path does not fit the buffer then just
/// allocate instead
fn opt_path_join<'a>(
    bytes: &'a mut [u8; 128],
    pathbuf: Option<&'a mut PathBuf>,
    paths: &[&Path],
) -> &'a Path {
    let total_len: usize = paths.iter().map(|p| p.as_os_str().len()).sum();
    if total_len > bytes.len() {
        // panic!("fuck");
        let pathbuf = pathbuf.unwrap();
        for path in paths {
            pathbuf.push(path);
        }
        return pathbuf.as_path();
    }

    let mut len = 0;
    for path in paths {
        let osstr = path.as_os_str().as_bytes();
        (bytes[len..len + osstr.len()]).copy_from_slice(osstr);
        len += osstr.len();
    }
    let osstr = OsStr::from_bytes(&bytes[..len]);
    Path::new(osstr)
}

pub(crate) unsafe fn asset_seek64(
    aasset: *mut AAsset,
    off: off64_t,
    whence: libc::c_int,
) -> off64_t {
    let mut wanted_assets = WANTED_ASSETS.lock().unwrap();
    let file = match wanted_assets.get_mut(&AAssetPtr(aasset)) {
        Some(file) => file,
        None => return ndk_sys::AAsset_seek64(aasset, off, whence),
    };
    seek_facade(off, whence, file) as off64_t
}

pub(crate) unsafe fn asset_seek(aasset: *mut AAsset, off: off_t, whence: libc::c_int) -> off_t {
    let mut wanted_assets = WANTED_ASSETS.lock().unwrap();
    let file = match wanted_assets.get_mut(&AAssetPtr(aasset)) {
        Some(file) => file,
        None => return ndk_sys::AAsset_seek(aasset, off, whence),
    };
    // This code can be very deadly on large files,
    // but since NO replacement should surpass u32 max we should be fine...
    // i dont even think a mcpack can exceed that
    seek_facade(off.into(), whence, file) as off_t
}

pub(crate) unsafe fn asset_read(
    aasset: *mut AAsset,
    buf: *mut libc::c_void,
    count: libc::size_t,
) -> libc::c_int {
    let mut wanted_assets = WANTED_ASSETS.lock().unwrap();
    let file = match wanted_assets.get_mut(&AAssetPtr(aasset)) {
        Some(file) => file,
        None => return ndk_sys::AAsset_read(aasset, buf, count),
    };
    // Reuse buffer given by caller
    let rs_buffer = core::slice::from_raw_parts_mut(buf as *mut u8, count);
    let read_total = match file.read(rs_buffer) {
        Ok(n) => n,
        Err(e) => {
            log::warn!("failed fake aaset read: {e}");
            return -1 as libc::c_int;
        }
    };
    read_total as libc::c_int
}

pub(crate) unsafe fn asset_length(aasset: *mut AAsset) -> off_t {
    let wanted_assets = WANTED_ASSETS.lock().unwrap();
    let file = match wanted_assets.get(&AAssetPtr(aasset)) {
        Some(file) => file,
        None => return ndk_sys::AAsset_getLength(aasset),
    };
    file.len().unwrap() as off_t
}

pub(crate) unsafe fn asset_length64(aasset: *mut AAsset) -> off64_t {
    let wanted_assets = WANTED_ASSETS.lock().unwrap();
    let file = match wanted_assets.get(&AAssetPtr(aasset)) {
        Some(file) => file,
        None => return ndk_sys::AAsset_getLength64(aasset),
    };
    file.len().unwrap() as off64_t
}

pub(crate) unsafe fn asset_remaining(aasset: *mut AAsset) -> off_t {
    let mut wanted_assets = WANTED_ASSETS.lock().unwrap();
    let file = match wanted_assets.get_mut(&AAssetPtr(aasset)) {
        Some(file) => file,
        None => return ndk_sys::AAsset_getRemainingLength(aasset),
    };
    file.rem().unwrap() as off_t
}

pub(crate) unsafe fn asset_remaining64(aasset: *mut AAsset) -> off64_t {
    let mut wanted_assets = WANTED_ASSETS.lock().unwrap();
    let file = match wanted_assets.get_mut(&AAssetPtr(aasset)) {
        Some(file) => file,
        None => return ndk_sys::AAsset_getRemainingLength64(aasset),
    };
    file.rem().unwrap() as off64_t
}

pub(crate) unsafe fn asset_close(aasset: *mut AAsset) {
    let mut wanted_assets = WANTED_ASSETS.lock().unwrap();
    let _result = wanted_assets.remove(&AAssetPtr(aasset));
    ndk_sys::AAsset_close(aasset);
}

pub(crate) unsafe fn asset_get_buffer(aasset: *mut AAsset) -> *const libc::c_void {
    let mut wanted_assets = WANTED_ASSETS.lock().unwrap();
    let file = match wanted_assets.get_mut(&AAssetPtr(aasset)) {
        Some(file) => file,
        None => return ndk_sys::AAsset_getBuffer(aasset),
    };
    // Lets hope this does not go boom boom
    file.raw_buffer().unwrap().cast()
}

pub(crate) unsafe fn asset_fd_dummy(
    aasset: *mut AAsset,
    out_start: *mut off_t,
    out_len: *mut off_t,
) -> libc::c_int {
    let wanted_assets = WANTED_ASSETS.lock().unwrap();
    match wanted_assets.get(&AAssetPtr(aasset)) {
        Some(_) => {
            log::error!("WE GOT BUSTED NOOO");
            -1
        }
        None => ndk_sys::AAsset_openFileDescriptor(aasset, out_start, out_len),
    }
}

pub(crate) unsafe fn asset_fd_dummy64(
    aasset: *mut AAsset,
    out_start: *mut off64_t,
    out_len: *mut off64_t,
) -> libc::c_int {
    let wanted_assets = WANTED_ASSETS.lock().unwrap();
    match wanted_assets.get(&AAssetPtr(aasset)) {
        Some(_) => {
            log::error!("WE GOT BUSTED NOOO");
            -1
        }
        None => ndk_sys::AAsset_openFileDescriptor64(aasset, out_start, out_len),
    }
}

pub(crate) unsafe fn asset_is_alloc(aasset: *mut AAsset) -> libc::c_int {
    let wanted_assets = WANTED_ASSETS.lock().unwrap();
    match wanted_assets.get(&AAssetPtr(aasset)) {
        Some(_) => false as libc::c_int,
        None => ndk_sys::AAsset_isAllocated(aasset),
    }
}

fn seek_facade(offset: i64, whence: libc::c_int, file: &mut CowFile) -> i64 {
    let offset = match whence {
        libc::SEEK_SET => {
            //Lets check this so we dont mess up
            let u64_off = match u64::try_from(offset) {
                Ok(uoff) => uoff,
                Err(e) => {
                    log::error!("signed ({offset}) to unsigned failed: {e}");
                    return -1;
                }
            };
            io::SeekFrom::Start(u64_off)
        }
        libc::SEEK_CUR => io::SeekFrom::Current(offset),
        libc::SEEK_END => io::SeekFrom::End(offset),
        _ => {
            log::error!("Invalid seek whence");
            return -1;
        }
    };
    match file.seek(offset) {
        Ok(new_offset) => match new_offset.try_into() {
            Ok(int) => int,
            Err(err) => {
                log::error!("u64 ({new_offset}) to i64 failed: {err}");
                -1
            }
        },
        Err(err) => {
            log::error!("aasset seek failed: {err}");
            -1
        }
    }
}

// Struct that contains either a file or a buffer to read bytes from
enum CowFile {
    File(File),
    Buffer(Cursor<Vec<u8>>),
}
impl Read for CowFile {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::File(file) => file.read(buf),
            Self::Buffer(cursor) => cursor.read(buf),
        }
    }
}
impl Seek for CowFile {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        match self {
            Self::File(file) => file.seek(pos),
            Self::Buffer(cursor) => cursor.seek(pos),
        }
    }
}
impl CowFile {
    fn len(&self) -> Result<u64, io::Error> {
        Ok(match self {
            Self::File(file) => file.metadata()?.len(),
            Self::Buffer(cursor) => cursor.get_ref().len() as _,
        })
    }
    fn rem(&mut self) -> Result<u64, io::Error> {
        Ok(self.len()? - self.stream_position()?)
    }
    fn raw_buffer(&mut self) -> Result<*mut u8, io::Error> {
        let len = self.len()? as usize;
        let mut vec = match self {
            Self::File(file) => {
                let mut vec = Vec::with_capacity(len);
                file.read_to_end(&mut vec)?;
                vec
            }
            Self::Buffer(cursor) => cursor.get_ref().clone(),
        };

        let ptr = vec.as_mut_ptr();
        //        std::mem::forget(vec);
        Ok(ptr)
    }
}
