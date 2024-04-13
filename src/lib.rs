mod common;
mod mc_utils;
mod platform;
use once_cell::sync::Lazy;
use platform::get_path;
use platform::setup_hooks;

use std::collections::HashMap;
use std::ffi::{CStr, OsString};
use std::path::PathBuf;
use std::sync::Mutex;
use std::{fs, thread};

static SHADER_PATHS: Lazy<Mutex<HashMap<OsString, PathBuf>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

#[ctor::ctor]
fn start_lib() {
    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Trace),
    );
    startup();
}

fn startup() {
    log::info!("Starting up!");
    std::panic::set_hook(Box::new(move |panic_info| {
        log::error!("Thread crashed: {}", panic_info);
    }));
    setup_hooks().unwrap();
    log::info!("Finished hooking..");
    let mut path = get_path();
    path.extend(["games", "com.mojang", "minecraftpe"]);
    log::info!("non verified path: {:#?}", &path);
    if !path.exists() {
        if let Err(e) = fs::create_dir_all(&path) {
            log::error!("Fatal: path to minecraftpe cant be created: {e}");
            log::error!("Quitting..");
            return;
        }
    }
    log::debug!("path is : {:#?}", &path);
    let _handler = thread::spawn(|| {
        log::info!("Hello from thread");
        common::setup_json_watcher(path);
    });
}
