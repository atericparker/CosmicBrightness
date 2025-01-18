use std::{env, path::PathBuf};
//you need to have libddcutil installed to make this build. 
fn main() {
    println!("cargo:rerun-if-changed=wrapper.h");
    println!("cargo:rerun-if-env-changed=DDCUTIL_PATH");

    // Adjust include path(s) if needed:
    // e.g., if ddcutil headers are in /usr/local/include, add clang_arg("-I/usr/local/include").
    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        // Because your header defines everything with "ddca_..." or "DDCA_..."
        .allowlist_function("ddca_.*")
        .allowlist_type("DDCA_.*")
        .allowlist_var("DDCA_.*")
        // The line below helps the linker find ddcutil if needed
        .clang_arg("-I/usr/include") 
        // Generate the bindings now
        .generate()
        .expect("Unable to generate ddcutil bindings");
    println!("cargo:rustc-link-lib=ddcutil");
    // Write them to `$OUT_DIR/bindings.rs`.
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write ddcutil bindings!");
}
