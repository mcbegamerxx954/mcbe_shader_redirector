mod aasset_warcrimes;
mod common;
mod mc_utils;
use jni_sys::JNI_VERSION_1_6;
use libc::{off64_t, off_t};
use ndk_sys::AAsset;
use once_cell::sync::Lazy;
use plt_rs::{LinkMapView, MutableLinkMap};
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Mutex;
use std::thread;

static SHADER_PATHS: Lazy<Mutex<HashMap<OsString, PathBuf>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

#[cfg(not(feature = "dynamic_path"))]
#[no_mangle]
pub extern "system" fn JNI_OnLoad(_: *mut libc::c_void, _: *mut libc::c_void) -> libc::c_int {
    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Trace),
    );
    startup();
    JNI_VERSION_1_6
}
#[cfg(feature = "dynamic_path")]
#[no_mangle]
pub extern "system" fn JNI_OnLoad(vm: jni::JavaVM, _: *mut libc::c_void) -> libc::c_int {
    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Trace),
    );

    let env = vm.get_env().expect("Expected java env");
    let context = get_global_context(env);
    unsafe {
        ndk_context::initialize_android_context(
            vm.get_java_vm_pointer().cast(),
            context.as_raw().cast(),
        )
    };
    log::info!("Starting.....");
    startup();
    JNI_VERSION_1_6
}

#[cfg(feature = "dynamic_path")]
fn get_global_context(mut env: jni::JNIEnv) -> jni::objects::JObject {
    let activity_thread = env
        .find_class("android/app/ActivityThread")
        .expect("Cant find activitythread class");
    let curr_activity_thread = env
        .call_static_method(
            activity_thread,
            "currentActivityThread",
            "()Landroid/app/ActivityThread;",
            &[],
        )
        .expect("Expected activity thread")
        .l()
        .expect("Expected object from activity thread");
    let context = env
        .call_method(
            curr_activity_thread,
            "getApplication",
            "()Landroid/app/Application;",
            &[],
        )
        .expect("Expected android context")
        .l()
        .expect("Expected object from getapplication");
    context
}
#[cfg(feature = "dynamic_path")]
fn get_path() -> std::path::PathBuf {
    use app_dirs2::{app_root, AppDataType, AppInfo};
    const MC_APP_INFO: AppInfo = AppInfo {
        name: "minecraftpe",
        author: "mojang",
    };
    let mut app_dir = app_root(AppDataType::UserData, &MC_APP_INFO).unwrap();
    // remove some parts that we dont use
    let _ = app_dir.pop();
    let _ = app_dir.pop();
    // add the path we want to be in
    app_dir.extend(["games", "com.mojang", "minecraftpe"]);
    app_dir
}
#[cfg(not(feature = "dynamic_path"))]
fn get_path() -> std::path::PathBuf {
    use std::fs;
    let mut pkgname = fs::read_to_string("/proc/self/cmdline").unwrap();
    log::info!("pkgname is :{pkgname}");
    //pkgnames are only ascii
    let pkgtrim = pkgname.trim_matches(char::from(0));
    let path = "/data/data/".to_string() + pkgtrim + "/games/com.mojang/minecraftpe";
    let mut canon_path = fs::canonicalize(path).unwrap();
    // im fine with this
    canon_path
}
fn startup() {
    log::info!("Starting up!");
    std::panic::set_hook(Box::new(move |panic_info| {
        log::error!("Thread crashed: {}", panic_info);

        // Sadly plt-rs is very mean when it comes to using its stuff
        /*
        log::error!("Undoing hooks..");
        if let Err(e) = get_mut_map().restore(_aaset_orig) {
            log::error!("Unhooking aaset failed with: {e}");
        }
        if let Err(e) = get_mut_map().restore(_fopen_orig) {
            log::error!("Unhooking fopen failed with: {e}");
        }
        */
    }));
    setup_hooks().expect("Expected hook to work");
    log::info!("Finished hooking..");
    let _handler = thread::spawn(|| {
        log::info!("Hello from thread");
        common::setup_json_watcher(get_path());
    });
}
fn setup_hooks() -> Result<(), plt_rs::PltError> {
    let link_map = LinkMapView::from_dynamic_library("libminecraftpe.so").unwrap();
    let mut_lm = MutableLinkMap::from_view(link_map);
    let _asset_open = mut_lm.hook::<unsafe fn(
        *mut ndk_sys::AAssetManager,
        *const libc::c_char,
        libc::c_int,
    ) -> AAsset>(
        "AAssetManager_open",
        aasset_warcrimes::asset_open as *const _,
    )?;
    let _asset_read =
        mut_lm.hook::<unsafe fn(*mut AAsset, *mut libc::c_void, libc::size_t) -> libc::c_int>(
            "AAsset_read",
            aasset_warcrimes::asset_read as *const _,
        )?;
    let _aasset_close = mut_lm
        .hook::<unsafe fn(*mut AAsset)>("AAsset_close", aasset_warcrimes::asset_close as *const _);
    let _asset_seek = mut_lm.hook::<unsafe fn(*mut AAsset, off_t, libc::c_int) -> off_t>(
        "AAsset_seek",
        aasset_warcrimes::asset_seek as *const _,
    )?;
    let _asset_seek64 = mut_lm.hook::<unsafe fn(*mut AAsset, off64_t, libc::c_int) -> off64_t>(
        "AAsset_seek64",
        aasset_warcrimes::asset_seek64 as *const _,
    )?;
    let _asset_len = mut_lm.hook::<unsafe fn(*mut AAsset) -> off_t>(
        "AAsset_getLength",
        aasset_warcrimes::asset_length as *const _,
    )?;
    let _asset_len64 = mut_lm.hook::<unsafe fn(*mut AAsset) -> off64_t>(
        "AAsset_getLength64",
        aasset_warcrimes::asset_length64 as *const _,
    )?;
    let _aasset_rem = mut_lm.hook::<unsafe fn(*mut AAsset) -> off_t>(
        "AAsset_getRemainingLength",
        aasset_warcrimes::asset_remaining as *const _,
    )?;
    let _asset_rem64 = mut_lm.hook::<unsafe fn(*mut AAsset) -> off64_t>(
        "AAsset_getRemainingLength64",
        aasset_warcrimes::asset_remaining64 as *const _,
    )?;
    let _asset_fd = mut_lm.hook::<unsafe fn(*mut AAsset, *mut off_t, *mut off_t) -> libc::c_int>(
        "AAsset_openFileDescriptor",
        aasset_warcrimes::asset_fd_dummy as *const _,
    )?;
    let _asset_fd = mut_lm
        .hook::<unsafe fn(*mut AAsset, *mut off64_t, *mut off64_t) -> libc::c_int>(
            "AAsset_openFileDescriptor64",
            aasset_warcrimes::asset_fd_dummy64 as *const _,
        )?;
    let _asset_get_buf = mut_lm.hook::<unsafe fn(*mut AAsset) -> *const libc::c_void>(
        "AAsset_getBuffer",
        aasset_warcrimes::asset_get_buffer as *const _,
    )?;
    let _asset_is_alloc = mut_lm.hook::<unsafe fn(*mut AAsset) -> libc::c_int>(
        "AAsset_isAllocated",
        aasset_warcrimes::asset_is_alloc as *const _,
    )?;
    Ok(())
}
