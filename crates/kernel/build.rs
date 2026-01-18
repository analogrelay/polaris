fn main() {
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let script_path = std::path::Path::new(&manifest_dir).join(format!("linker-{}.ld", arch));
    println!("cargo:rustc-link-arg=-T{}", script_path.display());
    println!("cargo:rerun-if-changed={}", script_path.display());
}
