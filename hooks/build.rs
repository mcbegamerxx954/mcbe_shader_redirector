fn main() {
    let target = std::env::var("TARGET").unwrap();
    println!("cargo:rustc-link-arg=--target={}", target);
}
