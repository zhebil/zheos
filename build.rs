fn main() {
    if std::env::var("TARGET").as_deref() == Ok("aarch64-unknown-none-softfloat") {
        println!("cargo:rustc-link-arg=-Tlinker.ld");
    }
    println!("cargo:rerun-if-changed=linker.ld");
}
