mod android;
mod common;
mod mc_utils;
use android::{aasset_hook, fopen_hook};
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

const MC_PATH: &str = "/data/user/0/com.mojang.minecraftpe/games/com.mojang/minecraftpe";
const JNI_VERSION_1_6: i32 = 65542;

fn get_mut_map<'a>() -> MutableLinkMap<'a> {
    let link_map = LinkMapView::from_dynamic_library("libminecraftpe.so").expect("open link map");

    MutableLinkMap::from_view(link_map)
}

#[no_mangle]
pub extern "system" fn JNI_OnLoad(_: *mut libc::c_void, _: *mut libc::c_void) -> libc::c_int {
    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Trace),
    );
    startup();
    JNI_VERSION_1_6
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
        let path = Path::new(MC_PATH);
        common::setup_json_watcher(path);
    });
}
