fn main() {
    let target = std::env::var("TARGET").unwrap_or_default();

    if target.starts_with("aarch64-unknown-none") {
        println!("cargo:rustc-link-arg=-Tlinker.ld");
    }

    println!("cargo:rerun-if-changed=linker.ld");
}
