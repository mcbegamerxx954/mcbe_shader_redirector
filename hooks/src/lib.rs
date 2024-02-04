mod android;
mod common;
mod mc_utils;
use android::{aasset_hook, fopen_hook};
use app_dirs2::{app_root, AppDataType, AppInfo};
use jni::sys::{jint, JNI_VERSION_1_6};
use jni::{objects::JObject, JNIEnv, JavaVM};
use ndk_sys::AAssetManager;
use once_cell::sync::Lazy;
use plt_rs::{LinkMapView, MutableLinkMap};
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Mutex;
use std::thread;

static SHADER_PATHS: Lazy<Mutex<HashMap<OsString, PathBuf>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

const MC_APP_INFO: AppInfo = AppInfo {
    name: "minecraftpe",
    author: "mojang",
};

fn get_mut_map<'a>() -> MutableLinkMap<'a> {
    let link_map = LinkMapView::from_dynamic_library("libminecraftpe.so").expect("open link map");

    MutableLinkMap::from_view(link_map)
}

#[no_mangle]
pub extern "system" fn JNI_OnLoad(vm: JavaVM, _: *mut libc::c_void) -> jint {
    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Trace),
    );
    log::info!("Our JNI_OnLoad got called");
    let env = vm.get_env().expect("Expected java env");
    let context = get_global_context(env);
    unsafe {
        ndk_context::initialize_android_context(
            vm.get_java_vm_pointer().cast(),
            context.as_raw().cast(),
        )
    };
    log::info!("Starting.....");
    let _handler = thread::spawn(startup);
    JNI_VERSION_1_6
}

fn get_global_context(mut env: JNIEnv) -> JObject {
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

pub fn startup() {
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
    let mut app_dir = app_root(AppDataType::UserData, &MC_APP_INFO).unwrap();
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
        let _ = app_dir.pop();
        let _ = app_dir.pop();
        app_dir.extend(["games", "com.mojang", "minecraftpe"]);
        log::info!("path is: {:#?}", &app_dir);
        common::setup_json_watcher(app_dir);
    });
}
