mod android;
use android::{aasset_hook, fopen_hook, watch_jsons};
use app_dirs2::{app_dir, AppDataType, AppInfo};
use core::panic;
use jni::sys::{jint, JNINativeMethod, JNI_VERSION_1_6};
use jni::{objects::JObject, JNIEnv, JavaVM};
use libc::c_void;
use ndk_sys::AAssetManager;
use plt_rs::{LinkMapView, MutableLinkMap};
use std::panic::set_hook;
use std::path::PathBuf;
use std::thread::{self, Thread};
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
    /*    let ipath: String = env
        .get_string(&internal_path)
        .expect("Java error while getting ipath")
        .into();
    let epath: String = env
        .get_string(&external_path)
        .expect("Java error while getting ipath")
        .into(); */
    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Trace),
    );
    log::info!("got called");
    let mut env = vm.get_env().expect("Expected java env");
    let context = get_global_context(env);
    log::info!("got global context");
    unsafe {
        ndk_context::initialize_android_context(
            vm.get_java_vm_pointer().cast(),
            context.as_raw().cast(),
        )
    };
    log::info!("Starting.....");
    let handler = thread::spawn(|| startup());
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
    log::info!("We are initialized");
    let mut mutable_link_map = get_mut_map();
    let aaset_orig =
        mutable_link_map
            .hook::<unsafe fn(
                *mut AAssetManager,
                *const libc::c_char,
                libc::c_int,
            ) -> *mut ndk_sys::AAsset>("AAssetManager_open", aasset_hook as *const _)
            .unwrap()
            .unwrap();
    let mut mutable_link_map = get_mut_map();
    let fopen_orig = mutable_link_map
        .hook::<unsafe fn(*const libc::c_char, *const libc::c_char) -> *mut libc::FILE>(
            "fopen",
            fopen_hook as *const _,
        )
        .unwrap()
        .unwrap();
    log::info!("Finished hooking");
    // fucking shit why dont you work or give me a
    // understandable error instead of 6657 you shit
    let app_dir = app_dir(AppDataType::UserData, &MC_APP_INFO, "");
    if app_dir.is_err() {
        // We prolly got a jvm exception soo
        let vm = unsafe {
            JavaVM::from_raw(ndk_context::android_context().vm() as *mut jni::sys::JavaVM).unwrap()
        };
        let env = vm.get_env().unwrap();
        if env.exception_check().unwrap() {
            env.exception_describe().unwrap();
            panic!();
        }
    }
    //let app_dir_ptsd = PathBuf::from(ipath);
    let handler = thread::spawn(|| {
        set_hook(Box::new(|_| {
            log::error!("ok i die");
        }));
        log::info!("Hello from thread");
        let mut app_dir = app_dir.expect("app_dirs is err");
        let _ = app_dir.pop();
        let _ = app_dir.pop();
        app_dir.push("games/com.mojang/minecraftpe");
        log::info!("path is: {:#?}", &app_dir);
        watch_jsons(app_dir);
    });
}
