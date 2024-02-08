mod android;
mod common;
mod mc_utils;
use android::{aasset_hook, fopen_hook};
use jni_sys::JNI_VERSION_1_6;
use ndk_sys::AAssetManager;
use once_cell::sync::Lazy;
use plt_rs::{LinkMapView, MutableLinkMap};
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::thread;

static SHADER_PATHS: Lazy<Mutex<HashMap<OsString, PathBuf>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

fn get_mut_map<'a>() -> MutableLinkMap<'a> {
    let link_map = LinkMapView::from_dynamic_library("libminecraftpe.so").expect("open link map");

    MutableLinkMap::from_view(link_map)
}
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
fn get_path() -> &'static std::path::Path {
    Path::new("/data/user/0/com.mojang.minecraftpe/games/com.mojang/minecraftpe")
}
fn startup() {
    log::info!("Starting up!");
    let mut mutable_link_map = get_mut_map();
    let _aaset_orig =  mutable_link_map
            .hook::<unsafe fn(
                *mut AAssetManager,
                *const libc::c_char,
                libc::c_int,
            ) -> *mut ndk_sys::AAsset>("AAssetManager_open", aasset_hook as *const _)
            .unwrap()
            .unwrap();
    let mut mutable_link_map = get_mut_map();
    let _fopen_orig = mutable_link_map
        .hook::<unsafe fn(*const libc::c_char, *const libc::c_char) -> *mut libc::FILE>(
            "fopen",
            fopen_hook as *const _,
        )
        .unwrap()
        .unwrap();

    log::info!("Finished hooking");
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

    let _handler = thread::spawn(|| {
        log::info!("Hello from thread");
        common::setup_json_watcher(get_path());
    });
}
