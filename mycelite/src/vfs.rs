use crate::replicator;
use journal::Journal;
use libsqlite_sys::c_str;
use libsqlite_sys::ffi;
use std::ffi::{c_char, c_int, c_void, CStr};
use std::mem;
use std::ptr;
use std::sync::{Arc, Mutex, MutexGuard};

macro_rules! vfs_vtable {
    ($name:expr) => {
        ffi::sqlite3_vfs {
            iVersion: 2,
            // initialized on extention load
            szOsFile: 0,
            // initialized on extention load
            mxPathname: 0,
            pNext: ptr::null_mut(),
            zName: c_str!($name),
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
        }
    };
}

#[repr(C)]
#[derive(Debug)]
pub struct MclVFS {
    base: ffi::sqlite3_vfs,
    read_only: bool,
    // initialized on extention load
    real: *mut ffi::sqlite3_vfs,
}

#[no_mangle]
#[used]
pub static mut MclVFSReader: MclVFS = MclVFS {
    base: vfs_vtable!("mycelite_reader"),
    read_only: true,
    // initialized on extention load
    real: ptr::null_mut(),
};

#[no_mangle]
#[used]
pub static mut MclVFSWriter: MclVFS = MclVFS {
    base: vfs_vtable!("mycelite_writer"),
    read_only: false,
    // initialized on extention load
    real: ptr::null_mut(),
};

impl MclVFS {
    /// Initialite MclVFS as a proxy to *real* VFS
    pub unsafe fn init(&mut self, real: *mut ffi::sqlite3_vfs) {
        // init VFS only once
        if self.real.is_null() {
            self.real = real;
            self.base.szOsFile = mem::size_of::<MclVFSFile>() as c_int + (*real).szOsFile;
            self.base.mxPathname = (*real).mxPathname;
        }
    }

    /// Get pointer to base vfs struct
    pub unsafe fn as_base(&mut self) -> *mut ffi::sqlite3_vfs {
        &mut self.base
    }

    /// Get pointer to real vfs
    unsafe fn as_real_ptr(base: *mut ffi::sqlite3_vfs) -> *mut ffi::sqlite3_vfs {
        (*base.cast::<Self>()).real
    }

    /// return reference to real vfs
    ///
    /// reference allow vfs function calls
    unsafe fn as_real_ref(base: *mut ffi::sqlite3_vfs) -> &'static mut ffi::sqlite3_vfs {
        &mut *Self::as_real_ptr(base)
    }

    unsafe fn from_raw_ptr(base: *mut ffi::sqlite3_vfs) -> &'static mut Self {
        &mut *(base.cast::<Self>())
    }
}

#[repr(C)]
struct MclVFSFile {
    base: ffi::sqlite3_file,
    journal: Option<mem::ManuallyDrop<Journal>>,
    read_only: bool,
    replicator: Option<mem::ManuallyDrop<replicator::ReplicatorHandle>>,
    mutex: Option<mem::ManuallyDrop<Arc<Mutex<()>>>>,
    mutex_guard: Option<mem::ManuallyDrop<MutexGuard<'static, ()>>>,
    vfs: *mut ffi::sqlite3_vfs,
    real: ffi::sqlite3_file,
}

impl MclVFSFile {
    /// init VFS File
    unsafe fn init(&mut self, vfs: *mut ffi::sqlite3_vfs) {
        self.vfs = vfs;
        self.read_only = MclVFS::from_raw_ptr(vfs).read_only;
        self.mutex = Some(mem::ManuallyDrop::new(Arc::new(Mutex::new(()))));
        self.mutex_guard = None
    }

    /// downcast pfile ptr to MclVFSFile struct ptr
    unsafe fn from_ptr(pfile: *mut ffi::sqlite3_file) -> &'static mut Self {
        &mut *(pfile as *mut MclVFSFile)
    }

    /// bootstrap journal
    ///
    /// happens only once on journal creation.
    fn bootstrap_journal(
        &self,
        journal: &mut Journal,
        database_path: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let db = page_parser::Database::new(database_path);
        let iter = match db.into_raw_page_iter() {
            Ok(iter) => iter,
            Err(e) => {
                if let Some(err) = e.downcast_ref::<std::io::Error>() {
                    if err.kind() == std::io::ErrorKind::NotFound {
                        // no database file - no need in bootstraping
                        return Ok(());
                    }
                }
                return Err(e);
            }
        };
        for res in iter {
            let (offset, page) = res?;
            let page = page.as_slice();
            journal.new_blob(offset, page)?;
        }
        journal.commit().map_err(Into::into)
    }

    fn setup_journal(
        &mut self,
        flags: c_int,
        zname: *const c_char,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if flags & ffi::SQLITE_OPEN_MAIN_DB == 0 {
            self.journal = None;
            self.replicator = None;
            return Ok(());
        }

        let database_path = unsafe { CStr::from_ptr(zname) }.to_str()?.to_owned();
        let journal_path = {
            let mut s = database_path.clone();
            s.push_str("-mycelial");
            s
        };
        let (journal, bootstrapped) = match Journal::try_from(&journal_path) {
            Ok(j) => (j, false),
            Err(e) if e.journal_not_exists() => {
                let mut journal = Journal::create(&journal_path)?;
                self.bootstrap_journal(&mut journal, &database_path)?;
                (journal, true)
            }
            Err(e) => return Err(e.into()),
        };
        self.journal = Some(mem::ManuallyDrop::new(journal));

        let lock = Arc::clone(self.mutex.as_ref().unwrap());
        self.replicator = Some(mem::ManuallyDrop::new(
            replicator::Replicator::new(&journal_path, database_path, self.read_only, lock).spawn(),
        ));

        if bootstrapped {
            if let Some(r) = self.replicator.as_mut() {
                r.new_snapshot();
            }
        }
        Ok(())
    }

    fn lock(&'static mut self) {
        if self.mutex_guard.is_some() {
            return;
        };
        if let Some(mutex) = self.mutex.as_ref() {
            self.mutex_guard = Some(mem::ManuallyDrop::new(mutex.lock().unwrap()))
        };
    }

    fn unlock(&mut self) {
        if self.mutex_guard.is_some() {
            self.mutex_guard.take().map(mem::ManuallyDrop::into_inner);
        }
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
    let file = MclVFSFile::from_ptr(file);
    file.init(vfs);
    if file.setup_journal(flags, zname).is_err() {
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
    file.unlock();
    file.mutex.take().map(mem::ManuallyDrop::into_inner);
    file.journal.take().map(mem::ManuallyDrop::into_inner);
    file.replicator.take().map(mem::ManuallyDrop::into_inner);
    (*file.real.pMethods).xClose.unwrap()(&mut file.real)
}

unsafe extern "C" fn mvfs_io_read(
    pfile: *mut ffi::sqlite3_file,
    buf: *mut c_void,
    amt: c_int,
    offset: ffi::sqlite_int64,
) -> c_int {
    let file = MclVFSFile::from_ptr(pfile);
    (*file.real.pMethods).xRead.unwrap()(&mut file.real, buf, amt, offset)
}

unsafe extern "C" fn mvfs_io_write(
    pfile: *mut ffi::sqlite3_file,
    buf: *const c_void,
    amt: c_int,
    offset: ffi::sqlite_int64,
) -> c_int {
    let file = MclVFSFile::from_ptr(pfile);
    if file.read_only && file.journal.is_some() {
        // FIXME: this is a hack for reader-only and virtual table
        if offset == 0 {
            return ffi::SQLITE_OK;
        } else {
            return ffi::SQLITE_READONLY;
        }
    }
    let result = match file.journal.as_mut() {
        Some(journal) => {
            let new_page = std::slice::from_raw_parts(buf.cast::<u8>(), amt as usize);
            let mut old_page = vec![0_u8; amt as usize];
            let mut iter =
                match MclVFSIO.xRead.unwrap()(pfile, old_page.as_mut_ptr().cast(), amt, offset) {
                    // existing page
                    ffi::SQLITE_OK => utils::get_diff(new_page, &old_page),
                    // new page
                    ffi::SQLITE_IOERR_SHORT_READ => utils::get_diff(new_page, &[]),
                    _other => return ffi::SQLITE_ERROR,
                };
            iter.try_for_each(|(mut diff_offset, diff)| {
                let diff_offset = diff_offset as i64 + offset;
                journal
                    .new_snapshot(amt as u32)
                    .and_then(|_| journal.new_blob(diff_offset as u64, diff))
            })
        }
        None => Ok(()),
    };
    if let Err(_e) = result {
        return ffi::SQLITE_ERROR;
    }
    (*file.real.pMethods).xWrite.unwrap()(&mut file.real, buf, amt, offset)
}

unsafe extern "C" fn mvfs_io_truncate(
    pfile: *mut ffi::sqlite3_file,
    size: ffi::sqlite3_int64,
) -> c_int {
    let file = MclVFSFile::from_ptr(pfile);
    (*file.real.pMethods).xTruncate.unwrap()(&mut file.real, size)
}

unsafe extern "C" fn mvfs_io_sync(pfile: *mut ffi::sqlite3_file, flags: c_int) -> c_int {
    let file = MclVFSFile::from_ptr(pfile);
    match file.journal.as_mut().map(|journal| journal.commit()) {
        None | Some(Ok(_)) => (),
        Some(Err(_e)) => return ffi::SQLITE_ERROR,
    };
    if let Some(replicator) = file.replicator.as_mut() {
        replicator.new_snapshot();
    }
    println!("xsync");
    (*file.real.pMethods).xSync.unwrap()(&mut file.real, flags)
}

unsafe extern "C" fn mvfs_io_file_size(
    pfile: *mut ffi::sqlite3_file,
    psize: *mut ffi::sqlite3_int64,
) -> c_int {
    let file = MclVFSFile::from_ptr(pfile);
    (*file.real.pMethods).xFileSize.unwrap()(&mut file.real, psize)
}

unsafe extern "C" fn mvfs_io_lock(pfile: *mut ffi::sqlite3_file, elock: c_int) -> c_int {
    let file = MclVFSFile::from_ptr(pfile);
    let real = (&mut file.real) as *mut ffi::sqlite3_file;
    // lock only main database file
    if file.journal.is_some() {
        file.lock();
    }
    (*(*real).pMethods).xLock.unwrap()(real, elock)
}

unsafe extern "C" fn mvfs_io_unlock(pfile: *mut ffi::sqlite3_file, elock: c_int) -> c_int {
    let file = MclVFSFile::from_ptr(pfile);
    if file.journal.is_some() {
        file.unlock()
    }
    (*file.real.pMethods).xUnlock.unwrap()(&mut file.real, elock)
}

unsafe extern "C" fn mvfs_io_check_reserved_lock(
    pfile: *mut ffi::sqlite3_file,
    out: *mut c_int,
) -> c_int {
    let file = MclVFSFile::from_ptr(pfile);
    (*file.real.pMethods).xCheckReservedLock.unwrap()(&mut file.real, out)
}

unsafe extern "C" fn mvfs_io_file_control(
    pfile: *mut ffi::sqlite3_file,
    op: c_int,
    p_arg: *mut c_void,
) -> c_int {
    let file = MclVFSFile::from_ptr(pfile);
    (*file.real.pMethods).xFileControl.unwrap()(&mut file.real, op, p_arg)
}

unsafe extern "C" fn mvfs_io_sector_size(pfile: *mut ffi::sqlite3_file) -> c_int {
    let file = MclVFSFile::from_ptr(pfile);
    (*file.real.pMethods).xSectorSize.unwrap()(&mut file.real)
}

unsafe extern "C" fn mvfs_io_device_characteristics(pfile: *mut ffi::sqlite3_file) -> c_int {
    let file = MclVFSFile::from_ptr(pfile);
    (*file.real.pMethods).xDeviceCharacteristics.unwrap()(&mut file.real)
}
