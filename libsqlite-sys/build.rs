use std::path::Path;

#[derive(Debug)]
pub struct ParseCallbacks {}

impl bindgen::callbacks::ParseCallbacks for ParseCallbacks {
    fn int_macro(&self, name: &str, _value: i64) -> Option<bindgen::callbacks::IntKind> {
        if name.starts_with("SQLITE_") {
            return Some(bindgen::callbacks::IntKind::Int);
        }
        None
    }
}

fn main() {
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let out_path = Path::new(&out_dir).join("bindings.rs");

    println!("cargo:rerun-if-changed=wrapper.h");
    println!("cargo:rerun-if-changed=build.rs");

    #[cfg(feature = "link")]
    {
        println!("cargo:rustc-link-lib=dylib=sqlite3");
    }

    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        .use_core()
        .ctypes_prefix("core::ffi")
        .parse_callbacks(Box::new(ParseCallbacks {}))
        .generate()
        .expect("failed to generate bindings");

    bindings
        .write_to_file(out_path)
        .expect("Could'n write bindings");
}
