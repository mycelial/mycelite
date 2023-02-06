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

    let pkg_conf = pkg_config::Config::new()
        .print_system_cflags(false)
        .print_system_libs(false)
        .cargo_metadata(false)
        .probe("sqlite3")
        .expect("installation of sqlite3 required");

    let include_paths = pkg_conf
        .include_paths
        .iter()
        .map(|p| format!("-I{}", p.to_str().expect("")))
        .collect::<Vec<_>>();

    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        .clang_args(include_paths.as_slice())
        .use_core()
        .ctypes_prefix("core::ffi")
        .parse_callbacks(Box::new(ParseCallbacks {}))
        .generate()
        .expect("failed to generate bindings");

    bindings
        .write_to_file(out_path)
        .expect("Could'n write bindings");
}
