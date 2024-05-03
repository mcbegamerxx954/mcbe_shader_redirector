mod common;
mod mc_utils;
mod platform;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Mutex;
use std::{fs, thread};

static SHADER_PATHS: Lazy<Mutex<HashMap<OsString, PathBuf>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

// A quick startpoint for the library, mostly there because
// unwinding up here is ub, + give a good panic message
#[ctor::ctor]
fn ctor() {
    thread::spawn(safe_setup);
}
// Make sure that ub cant happen when unwinding
// and provide usefull info
fn safe_setup() {
    platform::setup_logging();
    std::panic::set_hook(Box::new(move |panic_info| {
        log::error!("Thread crashed: {}", panic_info);
    }));
    let start = std::panic::catch_unwind(|| {
        startup();
    });
    if let Err(e) = start {
        if let Ok(err) = e.downcast::<String>() {
            log::error!("Thread crash, error: {err}");
        }
    }
}
fn startup() {
    log::info!("Starting up!");
    platform::setup_hooks().unwrap();
    log::info!("Finished hooking..");
    let mut path = platform::get_path();
    path.extend(["games", "com.mojang", "minecraftpe"]);
    log::info!("non verified path: {:#?}", &path);
    if !path.exists() {
        if let Err(e) = fs::create_dir_all(&path) {
            log::error!("Fatal: path to minecraftpe cant be created: {e}");
            log::error!("Quitting..");
            return;
        }
    }
    log::debug!("path is: {:#?}", &path);
    common::setup_json_watcher(path);
}
