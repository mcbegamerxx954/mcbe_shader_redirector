use crate::platform::OPTS;
use crate::SHADER_PATHS;
use libc::{off64_t, off_t};
use materialbin::{
    bgfx_shader::BgfxShader, pass::ShaderStage, CompiledMaterialDefinition, MinecraftVersion,
};
use memchr::memmem::Finder;
use std::{ptr::NonNull, sync::atomic::Ordering};
//use ndk::asset::Asset;
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
static MC_VERSION: OnceLock<Option<MinecraftVersion>> = OnceLock::new();
static IS_1_21_100: AtomicBool = AtomicBool::new(false);
fn get_current_mcver(man: ndk::asset::AssetManager) -> Option<MinecraftVersion> {
    let mut file = match get_uitext(man) {
        Some(asset) => asset,
        None => {
            log::error!("Shader fixing is disabled as RenderChunk was not found");
            return None;
        }
    };
    let mut buf = Vec::with_capacity(file.length());
    if let Err(e) = file.read_to_end(&mut buf) {
        log::error!("Something is wrong with AssetManager, mc detection failed: {e}");
        return None;
    };

    for version in materialbin::ALL_VERSIONS.into_iter().rev() {
        if let Ok(_shader) = buf.pread_with::<CompiledMaterialDefinition>(0, version) {
            log::info!("Mc version is {version}");
            if memchr::memmem::find(&buf, b"v_dithering").is_some() {
                log::warn!("mc is 1.21.100 and higher");
                IS_1_21_100.store(true, Ordering::Release);
            }
            return Some(version);
        };
    }
    log::warn!("Cannot detect mc version, autofix disabled");
    None
}

// Try to open UIText.material.bin to guess Minecraft shader version
fn get_uitext(man: ndk::asset::AssetManager) -> Option<Asset> {
    const NEW: &CStr = c"assets/renderer/materials/RenderChunk.material.bin";
    const OLD: &CStr = c"renderer/materials/RenderChunk.material.bin";
    for path in [NEW, OLD] {
        if let Some(asset) = man.open(path) {
            return Some(asset);
        }
    }
    None
}
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
            // Try to get the file
            let filepath = match shader_paths.get(path) {
                Some(path) => path,
                None => {
                    log::info!("Cannot load file: {:?}", path);
                    return aasset;
                }
            };
            let buffer = if os_filename.as_encoded_bytes().ends_with(b".material.bin") {
                let file = match std::fs::read(filepath) {
                    Ok(file) => file,
                    Err(err) => {
                        log::info!("Cannot open shader file: {err}");
                        return aasset;
                    }
                };
                let result = match process_material(man, &file) {
                    Some(updated) => updated,
                    None => file,
                };
                CowFile::Buffer(Cursor::new(result))
            } else {
                let file = match File::open(filepath) {
                    Ok(file) => file,
                    Err(err) => {
                        log::warn!("Cannot open file: {err}");
                        return aasset;
                    }
                };
                CowFile::File(file)
            };
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
fn process_material(man: *mut AAssetManager, data: &[u8]) -> Option<Vec<u8>> {
    let mcver = MC_VERSION.get_or_init(|| {
        let ptr = NonNull::new(man).unwrap();
        let manager = unsafe { AssetManager::from_ptr(ptr) };
        get_current_mcver(manager)
    });
    // Just ignore if no Minecraft version was found
    let mcver = (*mcver)?;
    let opts = OPTS.lock().unwrap();
    for version in opts.autofixer_versions.iter() {
        let version = *version;
        let mut material: CompiledMaterialDefinition = match data.pread_with(0, version) {
            Ok(data) => data,
            Err(e) => {
                log::trace!("[version] Parsing failed: {e}");
                continue;
            }
        };
        let needs_lightmap_fix = IS_1_21_100.load(Ordering::Acquire)
            && version != MinecraftVersion::V1_21_110
            && (material.name == "RenderChunk" || material.name == "RenderChunkPrepass")
            && opts.handle_lightmaps;
        let needs_sampler_fix = material.name == "RenderChunk"
            && mcver >= MinecraftVersion::V1_20_80
            && version <= MinecraftVersion::V1_19_60
            && opts.handle_texturelods;
        // Prevent some work
        if version == mcver && !needs_lightmap_fix && !needs_sampler_fix {
            log::info!("Did not fix mtbin, mtversion: {version}");
            return None;
        }
        if needs_lightmap_fix {
            handle_lightmaps(&mut material);
            log::warn!("Had to fix lightmaps for RenderChunk");
        }
        if needs_sampler_fix {
            handle_samplers(&mut material);
        }
        let mut output = Vec::with_capacity(data.len());
        if let Err(e) = material.write(&mut output, mcver) {
            log::trace!("[version] Write error: {e}");
            return None;
        }
        return Some(output);
    }

    None
}
fn handle_lightmaps(materialbin: &mut CompiledMaterialDefinition) {
    let finder = Finder::new(b"void main");
    let finder1 = Finder::new(b"v_lightmapUV = a_texcoord1;");
    let finder2 = Finder::new(b"v_lightmapUV=a_texcoord1;");
    let finder3 = Finder::new(b"#define a_texcoord1 ");
    let replace_with = b"#define a_texcoord1 vec2(uvec2(uvec2(round(a_texcoord1 * 65535.0)).y >> 4u, uvec2(round(a_texcoord1 * 65535.0)).y) & uvec2(15u,15u)) * vec2_splat(0.066666670143604278564453125);";

    //     let replace_with = b"
    // #define a_texcoord1 vec2(fract(a_texcoord1.x*15.9375)+0.0001,floor(a_texcoord1.x*15.9375)*0.0625+0.0001)
    // void main";
    for (_, pass) in &mut materialbin.passes {
        for variants in &mut pass.variants {
            for (stage, code) in &mut variants.shader_codes {
                if stage.stage == ShaderStage::Vertex {
                    let blob = &mut code.bgfx_shader_data;
                    let Ok(mut bgfx) = blob.pread::<BgfxShader>(0) else {
                        continue;
                    };
                    if finder3.find(&bgfx.code).is_some()
                        || (finder1.find(&bgfx.code).is_none()
                            && finder2.find(&bgfx.code).is_none())
                    {
                        continue;
                    };

                    replace_bytes(&mut bgfx.code, &finder, b"void main", replace_with);
                    blob.clear();
                    let _unused = bgfx.write(blob);
                }
            }
        }
    }
}
fn handle_samplers(materialbin: &mut CompiledMaterialDefinition) {
    let pattern = b"void main ()";
    let replace_with = b"
#if __VERSION__ >= 300
 #define texture(tex,uv) textureLod(tex,uv,0.0)
#else
 #define texture2D(tex,uv) texture2DLod(tex,uv,0.0)
#endif
void main ()";
    let finder = Finder::new(pattern);
    for (_passes, pass) in &mut materialbin.passes {
        if _passes == "AlphaTest" || _passes == "Opaque" {
            for variants in &mut pass.variants {
                for (stage, code) in &mut variants.shader_codes {
                    if stage.stage == ShaderStage::Fragment && stage.platform_name == "ESSL_100" {
                        log::info!("handle_samplers");
                        let mut bgfx: BgfxShader = code.bgfx_shader_data.pread(0).unwrap();
                        replace_bytes(&mut bgfx.code, &finder, pattern, replace_with);
                        code.bgfx_shader_data.clear();
                        bgfx.write(&mut code.bgfx_shader_data).unwrap();
                    }
                }
            }
        }
    }
}

fn replace_bytes(codebuf: &mut Vec<u8>, finder: &Finder, pattern: &[u8], replace_with: &[u8]) {
    let sus = match finder.find(codebuf) {
        Some(yay) => yay,
        None => {
            println!("oops");
            return;
        }
    };
    codebuf.splice(sus..sus + pattern.len(), replace_with.iter().cloned());
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
