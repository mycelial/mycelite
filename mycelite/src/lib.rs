#![allow(clippy::missing_safety_doc)]

mod config;
mod replicator;
mod vfs;
use libsqlite_sys::ffi;
use std::ffi::{c_char, c_int};
use once_cell::sync::OnceCell;

static INITIALIZED: OnceCell<bool> = OnceCell::new();

libsqlite_sys::setup!();

#[no_mangle]
pub unsafe fn sqlite3_mycelite_init(
    _db: *mut ffi::sqlite3,
    _err: *mut *mut c_char,
    api: *mut ffi::sqlite3_api_routines,
) -> c_int {
    if INITIALIZED.set(true).is_err() {
        // already initialized
        return ffi::SQLITE_OK
    }

    libsqlite_sys::init!(api);

    // sqlite default vfs
    let default_vfs = (*SQLITE3_API).vfs_find.unwrap()(std::ptr::null_mut());

    // init writer
    vfs::MclVFSWriter.init(default_vfs);
    (*SQLITE3_API).vfs_register.unwrap()(vfs::MclVFSWriter.as_base(), 0);

    // init reader and set reader as a new default vfs
    vfs::MclVFSReader.init(default_vfs);
    (*SQLITE3_API).vfs_register.unwrap()(vfs::MclVFSReader.as_base(), 1);

    // init config registry

    ffi::SQLITE_OK_LOAD_PERMANENTLY
}

#[no_mangle]
pub unsafe fn mycelite_config(
    db: *mut ffi::sqlite3,
    err: *mut *mut c_char,
    api: *mut ffi::sqlite3_api_routines,
) -> c_int {
    libsqlite_sys::init!(api);

    // init configuration vtab for given db handle
    config::init(db, err)
}
