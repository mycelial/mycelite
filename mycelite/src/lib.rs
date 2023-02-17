#![allow(clippy::missing_safety_doc)]

mod config;
mod replicator;
mod vfs;
use libsqlite_sys::ffi;
use once_cell::sync::OnceCell;
use std::ffi::{c_char, c_int};

struct DefaultVfs(*mut ffi::sqlite3_vfs);

unsafe impl Sync for DefaultVfs {}
unsafe impl Send for DefaultVfs {}

static DEFAULT_VFS: OnceCell<DefaultVfs> = OnceCell::new();

libsqlite_sys::setup!();

#[no_mangle]
pub unsafe fn sqlite3_mycelite_init(
    db: *mut ffi::sqlite3,
    err: *mut *mut c_char,
    api: *mut ffi::sqlite3_api_routines,
) -> c_int {
    mycelite_writer(db, err, api);
    mycelite_reader(db, err, api);
    ffi::SQLITE_OK_LOAD_PERMANENTLY
}

#[no_mangle]
pub unsafe fn mycelite_reader(
    _db: *mut ffi::sqlite3,
    _err: *mut *mut c_char,
    api: *mut ffi::sqlite3_api_routines,
) -> c_int {
    libsqlite_sys::init!(api);
    let default_vfs = (*SQLITE3_API).vfs_find.unwrap()(std::ptr::null_mut());
    DEFAULT_VFS.set(DefaultVfs(default_vfs)).ok();

    vfs::MclVFSReader.init(DEFAULT_VFS.get_unchecked().0);
    (*SQLITE3_API).vfs_register.unwrap()(vfs::MclVFSReader.as_base(), 1);
    ffi::SQLITE_OK_LOAD_PERMANENTLY
}

#[no_mangle]
pub unsafe fn mycelite_writer(
    _db: *mut ffi::sqlite3,
    _err: *mut *mut c_char,
    api: *mut ffi::sqlite3_api_routines,
) -> c_int {
    libsqlite_sys::init!(api);
    let default_vfs = (*SQLITE3_API).vfs_find.unwrap()(std::ptr::null_mut());
    DEFAULT_VFS.set(DefaultVfs(default_vfs)).ok();

    vfs::MclVFSWriter.init(DEFAULT_VFS.get_unchecked().0);
    (*SQLITE3_API).vfs_register.unwrap()(vfs::MclVFSWriter.as_base(), 1);
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
