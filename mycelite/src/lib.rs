#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(unused_variables)]

mod replicator;

use journal::Journal;
use libsqlite_sys::c_str;
use libsqlite_sys::ffi;
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::mem;
use std::os::unix::fs::FileExt;
use std::ptr;

libsqlite_sys::setup!();

#[repr(C)]
struct MclVFS {
    base: ffi::sqlite3_vfs,
    real: *mut ffi::sqlite3_vfs,
}

#[no_mangle]
#[used]
static mut MclVFS: MclVFS = MclVFS {
    base: ffi::sqlite3_vfs {
        iVersion: 2,
        // initialized on extention load
        szOsFile: 0,
        // initialized on extention load
        mxPathname: 0,
        pNext: ptr::null_mut(),
        zName: c_str!("mycelite"),
        pAppData: ptr::null_mut(),
        xOpen: Some(mvfs_open),
        xDelete: Some(mvfs_delete),
        xAccess: Some(mvfs_access),
        xFullPathname: Some(mvfs_full_pathname),
        xDlOpen: Some(mvfs_dlopen),
        xDlError: Some(mvfs_dlerror),
        xDlSym: Some(mvfs_dlsym),
        xDlClose: Some(mvfs_dlclose),
        xRandomness: Some(mvfs_randomness),
        xSleep: Some(mvfs_sleep),
        xCurrentTime: Some(mvfs_current_time),
        xGetLastError: Some(mvfs_get_last_error),
        xCurrentTimeInt64: Some(mvfs_current_time_i64),
        xSetSystemCall: None,
        xGetSystemCall: None,
        xNextSystemCall: None,
    },
    // initialized on extention load
    real: ptr::null_mut(),
};

impl MclVFS {
    /// Initialite MclVFS as a proxy to *real* VFS
    unsafe fn init(real: *mut ffi::sqlite3_vfs) {
        MclVFS.real = real;
        MclVFS.base.szOsFile = mem::size_of::<MclVFSFile>() as c_int + (&*real).szOsFile;
        MclVFS.base.mxPathname = (&*real).mxPathname;
    }

    /// Get pointer to base vfs struct
    unsafe fn as_base() -> *mut ffi::sqlite3_vfs {
        &mut MclVFS.base
    }

    /// Get pointer to real vfs
    unsafe fn as_real_ptr(base: *mut ffi::sqlite3_vfs) -> *mut ffi::sqlite3_vfs {
        MclVFS.real
    }

    /// return reference to real vfs
    ///
    /// reference allow vfs function calls
    unsafe fn as_real_ref(base: *mut ffi::sqlite3_vfs) -> &'static mut ffi::sqlite3_vfs {
        &mut *Self::as_real_ptr(base)
    }
}

#[derive(Debug)]
#[repr(C)]
struct MclVFSFile {
    base: ffi::sqlite3_file,
    journal: Option<std::mem::ManuallyDrop<Journal>>,
    read_only: bool,
    replicator: Option<std::mem::ManuallyDrop<replicator::ReplicatorHandle>>,
    vfs: *mut ffi::sqlite3_vfs,
    real: ffi::sqlite3_file,
}

impl MclVFSFile {
    /// downcast pfile ptr to MclVFSFile struct ptr
    unsafe fn from_ptr(pfile: *mut ffi::sqlite3_file) -> &'static mut Self {
        &mut *(pfile as *mut MclVFSFile)
    }

    unsafe fn setup_journal(
        &mut self,
        flags: c_int,
        zname: *const c_char,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.read_only = std::env::var("MYCELIAL_WRITER").unwrap_or("false".into()) == "false";
        if flags & ffi::SQLITE_OPEN_MAIN_DB == 0 {
            self.journal = None;
            self.replicator = None;
            return Ok(());
        }

        let database_path = CStr::from_ptr(zname).to_str()?.to_owned();
        let journal_path = {
            let mut s = database_path.clone();
            s.push_str("-mycelial");
            s
        };

        self.journal = Some(std::mem::ManuallyDrop::new(
            Journal::try_from(&journal_path).or_else(|_e| Journal::create(&journal_path))?,
        ));
        let url = std::env::var("MYCELIAL_SYNC_BACKEND").unwrap_or("http://localhost:8080".into());
        self.replicator = Some(std::mem::ManuallyDrop::new(
            replicator::Replicator::new(url, &journal_path, database_path, self.read_only).spawn()
        ));
        Ok(())
    }
}

// VFS methods

unsafe extern "C" fn mvfs_open(
    vfs: *mut ffi::sqlite3_vfs,
    zname: *const c_char,
    file: *mut ffi::sqlite3_file,
    flags: c_int,
    p_out_flags: *mut c_int,
) -> c_int {
    let real = MclVFS::as_real_ref(vfs);
    let file = MclVFSFile::from_ptr(file);
    file.vfs = vfs;
    if let Err(_) = file.setup_journal(flags, zname) {
        return ffi::SQLITE_ERROR;
    }
    file.base.pMethods = &MclVFSIO as *const _;
    MclVFS::as_real_ref(vfs).xOpen.unwrap()(
        MclVFS::as_real_ptr(vfs),
        zname,
        &mut file.real,
        flags,
        p_out_flags,
    )
}

unsafe extern "C" fn mvfs_delete(
    vfs: *mut ffi::sqlite3_vfs,
    zname: *const c_char,
    sync_dir: c_int,
) -> c_int {
    MclVFS::as_real_ref(vfs).xDelete.unwrap()(MclVFS::as_real_ptr(vfs), zname, sync_dir)
}

unsafe extern "C" fn mvfs_access(
    vfs: *mut ffi::sqlite3_vfs,
    zname: *const c_char,
    flags: c_int,
    p_res_out: *mut c_int,
) -> c_int {
    MclVFS::as_real_ref(vfs).xAccess.unwrap()(MclVFS::as_real_ptr(vfs), zname, flags, p_res_out)
}

unsafe extern "C" fn mvfs_full_pathname(
    vfs: *mut ffi::sqlite3_vfs,
    zname: *const c_char,
    n_out: c_int,
    z_out: *mut c_char,
) -> c_int {
    MclVFS::as_real_ref(vfs).xFullPathname.unwrap()(MclVFS::as_real_ptr(vfs), zname, n_out, z_out)
}

unsafe extern "C" fn mvfs_dlopen(
    vfs: *mut ffi::sqlite3_vfs,
    zfilename: *const c_char,
) -> *mut c_void {
    MclVFS::as_real_ref(vfs).xDlOpen.unwrap()(MclVFS::as_real_ptr(vfs), zfilename)
}

unsafe extern "C" fn mvfs_dlerror(
    vfs: *mut ffi::sqlite3_vfs,
    n_byte: c_int,
    z_err_msg: *mut c_char,
) {
    MclVFS::as_real_ref(vfs).xDlError.unwrap()(MclVFS::as_real_ptr(vfs), n_byte, z_err_msg)
}

unsafe extern "C" fn mvfs_dlsym(
    vfs: *mut ffi::sqlite3_vfs,
    p: *mut c_void,
    z_symbol: *const c_char,
) -> Option<unsafe extern "C" fn(vfs: *mut ffi::sqlite3_vfs, p: *mut c_void, z_symbol: *const c_char)>
{
    MclVFS::as_real_ref(vfs).xDlSym.unwrap()(MclVFS::as_real_ptr(vfs), p, z_symbol)
}

unsafe extern "C" fn mvfs_dlclose(vfs: *mut ffi::sqlite3_vfs, p_handle: *mut c_void) {
    MclVFS::as_real_ref(vfs).xDlClose.unwrap()(MclVFS::as_real_ptr(vfs), p_handle);
}

unsafe extern "C" fn mvfs_randomness(
    vfs: *mut ffi::sqlite3_vfs,
    n_byte: c_int,
    z_buf_out: *mut c_char,
) -> c_int {
    MclVFS::as_real_ref(vfs).xRandomness.unwrap()(MclVFS::as_real_ptr(vfs), n_byte, z_buf_out)
}

unsafe extern "C" fn mvfs_sleep(vfs: *mut ffi::sqlite3_vfs, micros: c_int) -> c_int {
    MclVFS::as_real_ref(vfs).xSleep.unwrap()(MclVFS::as_real_ptr(vfs), micros)
}

unsafe extern "C" fn mvfs_current_time(vfs: *mut ffi::sqlite3_vfs, p_timeout: *mut f64) -> c_int {
    MclVFS::as_real_ref(vfs).xCurrentTime.unwrap()(MclVFS::as_real_ptr(vfs), p_timeout)
}

unsafe extern "C" fn mvfs_get_last_error(
    vfs: *mut ffi::sqlite3_vfs,
    a: c_int,
    b: *mut c_char,
) -> c_int {
    MclVFS::as_real_ref(vfs).xGetLastError.unwrap()(MclVFS::as_real_ptr(vfs), a, b)
}

unsafe extern "C" fn mvfs_current_time_i64(
    vfs: *mut ffi::sqlite3_vfs,
    p: *mut ffi::sqlite3_int64,
) -> c_int {
    MclVFS::as_real_ref(vfs).xCurrentTimeInt64.unwrap()(MclVFS::as_real_ptr(vfs), p)
}

#[no_mangle]
#[used]
static MclVFSIO: ffi::sqlite3_io_methods = ffi::sqlite3_io_methods {
    iVersion: 1,
    xClose: Some(mvfs_io_close),
    xRead: Some(mvfs_io_read),
    xWrite: Some(mvfs_io_write),
    xTruncate: Some(mvfs_io_truncate),
    xSync: Some(mvfs_io_sync),
    xFileSize: Some(mvfs_io_file_size),
    xLock: Some(mvfs_io_lock),
    xUnlock: Some(mvfs_io_unlock),
    xCheckReservedLock: Some(mvfs_io_check_reserved_lock),
    xFileControl: Some(mvfs_io_file_control),
    xSectorSize: Some(mvfs_io_sector_size),
    xDeviceCharacteristics: Some(mvfs_io_device_characteristics),

    xShmMap: None,
    xShmLock: None,
    xShmBarrier: None,
    xShmUnmap: None,
    xFetch: None,
    xUnfetch: None,
};

unsafe extern "C" fn mvfs_io_close(pfile: *mut ffi::sqlite3_file) -> c_int {
    let file = MclVFSFile::from_ptr(pfile);
    file.journal.take().map(std::mem::ManuallyDrop::into_inner);
    file.replicator
        .take()
        .map(std::mem::ManuallyDrop::into_inner);
    (&*file.real.pMethods).xClose.unwrap()(&mut file.real)
}

unsafe extern "C" fn mvfs_io_read(
    pfile: *mut ffi::sqlite3_file,
    buf: *mut c_void,
    amt: c_int,
    offset: ffi::sqlite_int64,
) -> c_int {
    let file = MclVFSFile::from_ptr(pfile);
    (&*file.real.pMethods).xRead.unwrap()(&mut file.real, buf, amt, offset)
}

unsafe extern "C" fn mvfs_io_write(
    pfile: *mut ffi::sqlite3_file,
    buf: *const c_void,
    amt: c_int,
    offset: ffi::sqlite_int64,
) -> c_int {
    let file = MclVFSFile::from_ptr(pfile);
    if file.read_only {
        return ffi::SQLITE_READONLY;
    }
    let result = file.journal.as_mut().map(|journal| {
        journal.add_page(
            offset as u64,
            std::slice::from_raw_parts(buf as *const u8, amt as usize),
        )
    });
    match result {
        None | Some(Ok(_)) => (),
        Some(Err(_e)) => return ffi::SQLITE_ERROR,
    };
    (&*file.real.pMethods).xWrite.unwrap()(&mut file.real, buf, amt, offset)
}

unsafe extern "C" fn mvfs_io_truncate(
    pfile: *mut ffi::sqlite3_file,
    size: ffi::sqlite3_int64,
) -> c_int {
    let file = MclVFSFile::from_ptr(pfile);
    (&*file.real.pMethods).xTruncate.unwrap()(&mut file.real, size)
}

unsafe extern "C" fn mvfs_io_sync(pfile: *mut ffi::sqlite3_file, flags: c_int) -> c_int {
    let file = MclVFSFile::from_ptr(pfile);
    match file.journal.as_mut().map(|journal| journal.commit()) {
        None | Some(Ok(_)) => (),
        Some(Err(e)) => return ffi::SQLITE_ERROR,
    };
    file.replicator
        .as_mut()
        .map(|replicator| replicator.new_snapshot());
    (&*file.real.pMethods).xSync.unwrap()(&mut file.real, flags)
}

unsafe extern "C" fn mvfs_io_file_size(
    pfile: *mut ffi::sqlite3_file,
    psize: *mut ffi::sqlite3_int64,
) -> c_int {
    let file = MclVFSFile::from_ptr(pfile);
    (&*file.real.pMethods).xFileSize.unwrap()(&mut file.real, psize)
}

unsafe extern "C" fn mvfs_io_lock(pfile: *mut ffi::sqlite3_file, elock: c_int) -> c_int {
    let file = MclVFSFile::from_ptr(pfile);
    (&*file.real.pMethods).xLock.unwrap()(&mut file.real, elock)
}

unsafe extern "C" fn mvfs_io_unlock(pfile: *mut ffi::sqlite3_file, elock: c_int) -> c_int {
    let file = MclVFSFile::from_ptr(pfile);
    (&*file.real.pMethods).xUnlock.unwrap()(&mut file.real, elock)
}

unsafe extern "C" fn mvfs_io_check_reserved_lock(
    pfile: *mut ffi::sqlite3_file,
    out: *mut c_int,
) -> c_int {
    let file = MclVFSFile::from_ptr(pfile);
    (&*file.real.pMethods).xCheckReservedLock.unwrap()(&mut file.real, out)
}

unsafe extern "C" fn mvfs_io_file_control(
    pfile: *mut ffi::sqlite3_file,
    op: c_int,
    p_arg: *mut c_void,
) -> c_int {
    let file = MclVFSFile::from_ptr(pfile);
    (&*file.real.pMethods).xFileControl.unwrap()(&mut file.real, op, p_arg)
}

unsafe extern "C" fn mvfs_io_sector_size(pfile: *mut ffi::sqlite3_file) -> c_int {
    let file = MclVFSFile::from_ptr(pfile);
    (&*file.real.pMethods).xSectorSize.unwrap()(&mut file.real)
}

unsafe extern "C" fn mvfs_io_device_characteristics(pfile: *mut ffi::sqlite3_file) -> c_int {
    let file = MclVFSFile::from_ptr(pfile);
    (&*file.real.pMethods).xDeviceCharacteristics.unwrap()(&mut file.real)
}

#[no_mangle]
pub unsafe fn sqlite3_mycelite_init(
    db: *mut ffi::sqlite3,
    err: *mut *mut c_char,
    api: *mut ffi::sqlite3_api_routines,
) -> c_int {
    libsqlite_sys::init!(api);

    MclVFS::init(
        (&*SQLITE3_API)
            .vfs_find
            .map(|f| f(ptr::null_mut()))
            .unwrap(),
    );
    (&*SQLITE3_API)
        .vfs_register
        .map(|f| f(MclVFS::as_base(), 1));

    ffi::SQLITE_OK_LOAD_PERMANENTLY
}
