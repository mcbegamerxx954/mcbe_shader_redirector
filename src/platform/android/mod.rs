mod hooks;
pub mod storage;
use crate::hooking::{setup_hook, unsetup_hook};

use self::storage::{parse_storage_location, StorageLocation};
use super::errors::HookError;
use libc::c_void;
use libloading::{Library, Symbol};
use plt_rs::{collect_modules, DynamicLibrary};
use std::ffi::{CStr, OsStr};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

#[derive(Debug)]
struct JniPaths {
    internal_path: String,
    external_path: String,
}
type IsEduFn = unsafe extern "C" fn(jni::JNIEnv, jni::objects::JObject);
static JNI_PATHS: OnceLock<JniPaths> = OnceLock::new();
pub static EDU_BACKUP: OnceLock<MemBackup> = OnceLock::new();
extern "C" fn is_edu_hook(mut env: jni::JNIEnv, thiz: jni::objects::JObject) {
    let external_path = get_string_from_fn(&mut env, &thiz, "getExternalStoragePath");
    let internal_path = get_string_from_fn(&mut env, &thiz, "getInternalStoragePath");
    let paths = JniPaths {
        internal_path,
        external_path,
    };
    JNI_PATHS.set(paths).unwrap();
    let edu = EDU_BACKUP.get().unwrap();
    unsafe {
        unsetup_hook(edu.original_func_ptr as _, edu.backup_bytes);
        (edu.original_func_ptr)(env, thiz);
    }
}
fn get_string_from_fn(
    env: &mut jni::JNIEnv,
    instance: &jni::objects::JObject,
    fn_name: &str,
) -> String {
    let jstring = env
        .call_method(instance, fn_name, "()Ljava/lang/String;", &[])
        .unwrap()
        .l()
        .unwrap();
    let path_str = env.get_string(jstring.as_ref().into()).unwrap();
    path_str.to_str().unwrap().to_owned()
}
pub fn get_storage_location(options_path: &Path) -> Option<StorageLocation> {
    let int = match parse_storage_location(options_path) {
        Ok(location) => location,
        Err(e) => {
            log::info!("Cant parse storage: {e}");
            return None;
        }
    };
    StorageLocation::from_i8(int)
}

pub fn setup_logging() {
    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Trace),
    );
}
// Get the full path for a storage location
pub fn get_storage_path(location: StorageLocation) -> std::path::PathBuf {
    loop {
        if JNI_PATHS.get().is_some() {
            break;
        } else {
            log::warn!("we going slwepy time");
            std::thread::sleep(Duration::from_millis(500));
        }
    }

    let paths = JNI_PATHS.get().unwrap();
    let result = match location {
        StorageLocation::Internal => paths.internal_path.to_owned(),
        StorageLocation::External => paths.external_path.to_owned(),
    };
    log::info!("Jni path for {location:#?}: {}", &result);
    result.into()
}

// Get app directory for the current platform
pub fn get_path() -> std::path::PathBuf {
    get_storage_path(StorageLocation::Internal)
}
// Setup asset hooks
pub fn setup_hooks() -> Result<(), HookError> {
    const LIBNAME: &str = "libminecraftpe.so";
    let lib_entry = match find_lib(LIBNAME) {
        Some(lib) => lib,
        None => return Err(HookError::MissingLib(LIBNAME.to_string())),
    };
    let dyn_lib = match DynamicLibrary::initialize(lib_entry) {
        Ok(lib) => lib,
        Err(e) => return Err(HookError::OsError(format!("{e}"))),
    };
    // This is needed because plt_rs can do nothing about this one
    unsafe { special_hook(LIBNAME) };
    replace_plt_functions(
        &dyn_lib,
        &[
            ("AAssetManager_open", hooks::asset_open as *const _),
            ("AAsset_read", hooks::asset_read as *const _),
            ("AAsset_close", hooks::asset_close as *const _),
            ("AAsset_seek", hooks::asset_seek as *const _),
            ("AAsset_seek64", hooks::asset_seek64 as *const _),
            ("AAsset_getLength", hooks::asset_length as *const _),
            ("AAsset_getLength64", hooks::asset_length64 as *const _),
            (
                "AAsset_getRemainingLength",
                hooks::asset_remaining as *const _,
            ),
            (
                "AAsset_getRemainingLength64",
                hooks::asset_remaining64 as *const _,
            ),
            (
                "AAsset_openFileDescriptor",
                hooks::asset_fd_dummy as *const _,
            ),
            (
                "AAsset_openFileDescriptor64",
                hooks::asset_fd_dummy64 as *const _,
            ),
            ("AAsset_getBuffer", hooks::asset_get_buffer as *const _),
            ("AAsset_isAllocated", hooks::asset_is_alloc as *const _),
            ("fopen", open_hook as *const _),
        ],
    )?;
    log::info!("Finished Hooking");
    Ok(())
}
unsafe fn open_hook(filename: *const u8, mode: *const u8) -> *mut libc::FILE {
    let cfilename = CStr::from_ptr(filename);
    let Osstr = OsStr::from_bytes(&cfilename.to_bytes());
    let path = Path::new(Osstr);
    if path
        .file_name()
        .is_some_and(|osstr| osstr.as_encoded_bytes().ends_with(b"options.txt"))
    {
        log::info!("mc opened options.txt at {:?}", path);
    }
    libc::fopen(filename, mode)
}
// Backup of function ptr and its instructions
#[derive(Debug)]
struct MemBackup {
    backup_bytes: [u8; crate::hooking::BACKUP_LEN],
    original_func_ptr: IsEduFn,
}

unsafe impl Send for MemBackup {}
unsafe impl Sync for MemBackup {}

unsafe fn special_hook(libname: &str) {
    const IS_EDU: &[u8] = b"Java_com_mojang_minecraftpe_MainActivity_isEduMode\0";
    let lib = Library::new(libname).unwrap();
    let sym: Symbol<IsEduFn> = lib.get(IS_EDU).unwrap();
    let addr = *sym;
    let result = setup_hook(addr as _, is_edu_hook as _);
    let mbackup = MemBackup {
        backup_bytes: result,
        original_func_ptr: addr,
    };
    EDU_BACKUP.set(mbackup);
}
fn find_lib<'a>(target_name: &str) -> Option<plt_rs::LoadedLibrary<'a>> {
    let loaded_modules = collect_modules();
    loaded_modules
        .into_iter()
        .find(|lib| lib.name().contains(target_name))
}

fn replace_plt_functions(
    dyn_lib: &DynamicLibrary,
    functions: &[(&str, *const ())],
) -> Result<(), HookError> {
    let base_addr = dyn_lib.library().addr();
    for (fn_name, replacement) in functions {
        let Some(fn_plt) = dyn_lib.try_find_function(fn_name) else {
            log::warn!("Missing symbol: {fn_name}");
            continue;
        };
        log::info!("Hooking {}...", fn_name);
        replace_plt_function(base_addr, fn_plt.r_offset as usize, *replacement)?;
    }
    log::info!("Hooked {} functions.", functions.len());
    Ok(())
}
fn replace_plt_function(
    base_addr: usize,
    offset: usize,
    replacement: *const (),
) -> Result<(), HookError> {
    let plt_fn_ptr = (base_addr + offset) as *mut *mut c_void;
    let page_size = page_size::get();
    let plt_page = ((plt_fn_ptr as usize / page_size) * page_size) as *mut c_void;
    unsafe {
        // Set the memory page to read, write
        let prot_res = libc::mprotect(plt_page, page_size, libc::PROT_WRITE | libc::PROT_READ);
        if prot_res != 0 {
            return Err(HookError::OsError(
                "Mprotect error on setting rw".to_string(),
            ));
        }

        // Replace the function address
        plt_fn_ptr.write(replacement as *mut _);

        // Set the memory page protection back to read only
        let prot_res = libc::mprotect(plt_page, page_size, libc::PROT_READ);
        if prot_res != 0 {
            return Err(HookError::OsError(
                "Mprotect error on setting read only".to_string(),
            ));
        }
        Ok(())
    }
}
