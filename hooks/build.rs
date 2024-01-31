// This is to fix cross compilation on termux where
// theres no target specific linker so we pass target to the linker
//#[cfg(target_os = "android")]
fn main() {
    let target = std::env::var("TARGET").unwrap();
    println!(
        "cargo:rustc-link-arg=--target={}", target
    );
}
//#[cfg(not(target_os = "android"))]
//fn main() {
//}
