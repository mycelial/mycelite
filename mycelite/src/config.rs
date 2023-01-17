//! mycelite configuration
use crate::deallocate;
use lazy_static::lazy_static;
use libsqlite_sys::{c_str, ffi};
use std::collections::BTreeMap;
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::marker::PhantomData;
use std::mem;
use std::sync::{Arc, Mutex};
use toml;

lazy_static! {
    pub(crate) static ref CONFIG_REGISTRY: Mutex<BTreeMap<String, Arc<Mutex<Config>>>> =
        Mutex::new(BTreeMap::new());
}

#[derive(Debug, Copy, Clone)]
pub(crate) struct ConfigRegistry {}

impl ConfigRegistry {
    pub fn new() -> Self {
        Self {}
    }

    pub fn register_config(self, database_path: &str) {
        let mut map = CONFIG_REGISTRY.lock().unwrap();
        if map.get(database_path).is_some() {
            return;
        }
        let mut config = Config::new(database_path);
        // FIXME: error is swallowed
        config.read().ok();
        map.insert(database_path.into(), Arc::new(Mutex::new(config)));
    }

    pub fn unregister_config(self, database_path: &str) {
        CONFIG_REGISTRY
            .lock()
            .map(|mut map| map.remove(database_path))
            .unwrap();
    }

    pub fn get(self, database_path: &str) -> Arc<Mutex<Config>> {
        self.register_config(database_path);
        CONFIG_REGISTRY
            .lock()
            .map(|map| Arc::clone(map.get(database_path).unwrap()))
            .unwrap()
    }
}

#[derive(Debug)]
pub(crate) struct ValueIter<'a> {
    cur: usize,
    total: usize,
    value: *mut *mut ffi::sqlite3_value,
    _marker: PhantomData<&'a ()>,
}

impl<'a> ValueIter<'a> {
    fn new(argc: c_int, value: *mut *mut ffi::sqlite3_value) -> Self {
        Self {
            cur: 0,
            total: argc as usize,
            value,
            _marker: PhantomData,
        }
    }
}

// FIXME: move to utils
#[derive(Debug, PartialEq)]
pub enum SqliteValue<'a> {
    I64(i64),
    Double(f64),
    Blob(&'a [u8]),
    Text(&'a str),
    Null,
}

impl<'a> SqliteValue<'a> {
    fn is_null(&self) -> bool {
        match self {
            Self::Null => true,
            _ => false,
        }
    }
}

// FIXME: move to utils
impl<'a> Iterator for ValueIter<'a> {
    type Item = SqliteValue<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cur >= self.total {
            return None;
        }
        let value = unsafe {
            let value = *self.value.offset(self.cur as isize);
            match { (*SQLITE3_API).value_type.unwrap()(value) } {
                ffi::SQLITE_TEXT => {
                    let (text, len) = (
                        (*SQLITE3_API).value_text.unwrap()(value),
                        (*SQLITE3_API).value_bytes.unwrap()(value) as usize,
                    );
                    SqliteValue::Text(std::str::from_utf8_unchecked(std::slice::from_raw_parts(
                        text, len,
                    )))
                }
                ffi::SQLITE_INTEGER => SqliteValue::I64((*SQLITE3_API).value_int64.unwrap()(value)),
                ffi::SQLITE_FLOAT => {
                    SqliteValue::Double((*SQLITE3_API).value_double.unwrap()(value))
                }
                ffi::SQLITE_NULL => SqliteValue::Null,
                ffi::SQLITE_BLOB => {
                    let blob = std::slice::from_raw_parts(
                        (*SQLITE3_API).value_blob.unwrap()(value) as *const u8,
                        (*SQLITE3_API).value_bytes.unwrap()(value) as usize,
                    );
                    SqliteValue::Blob(blob)
                }
                _ => unreachable!(),
            }
        };
        self.cur += 1;
        Some(value)
    }
}

// FIXME: move to utils
#[derive(Debug, PartialEq)]
enum UpdateType<'a> {
    Delete {
        row_id: SqliteValue<'a>,
    },
    Insert {
        row_id: SqliteValue<'a>,
        columns: Vec<SqliteValue<'a>>,
    },
    Update {
        row_id: SqliteValue<'a>,
        columns: Vec<SqliteValue<'a>>,
    },
}

// FIXME: move to utils
// https://www.sqlite.org/vtab.html#xupdate
impl<'a> From<(c_int, *mut *mut ffi::sqlite3_value)> for UpdateType<'a> {
    fn from((argc, argv): (c_int, *mut *mut ffi::sqlite3_value)) -> Self {
        let mut iter = ValueIter::new(argc, argv);
        let first = iter.next();
        let second = iter.next();
        let columns: Vec<SqliteValue<'a>> = iter.collect();
        match argc {
            1 if first.is_some() => Self::Delete {
                row_id: first.unwrap(),
            },
            v if v > 1 && first.is_some() && first.as_ref().unwrap().is_null() => Self::Insert {
                row_id: first.unwrap(),
                columns,
            },
            v if v > 1
                && first.is_some()
                && !first.as_ref().unwrap().is_null()
                && first == second =>
            {
                Self::Insert {
                    row_id: first.unwrap(),
                    columns,
                }
            }
            v if v > 1
                && first.is_some()
                && !first.as_ref().unwrap().is_null()
                && first != second =>
            {
                Self::Update {
                    row_id: first.unwrap(),
                    columns,
                }
            }
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Config {
    path: String,
    state: BTreeMap<String, String>,
}

impl Config {
    pub fn new<P: Into<String>>(database_path: P) -> Self {
        let path = {
            let mut path: String = database_path.into();
            path.push_str("-mycelite-config");
            path
        };
        Self {
            path: path.into(),
            state: BTreeMap::new(),
        }
    }

    pub fn from_path(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self::new(path))
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.state.get(key).map(|s| s.as_str())
    }

    fn insert(&mut self, key: &str, value: &str) {
        if Self::allowed_keys().contains(&key) {
            self.state.insert(key.to_string(), value.to_string());
        }
    }

    fn delete(&mut self, pos: usize) {
        if let Some(key) = Self::allowed_keys().get(pos) {
            self.state.remove(*key);
        };
    }

    fn write(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if !self.path.is_empty() {
            let value = toml::to_string(&self.state)?;
            std::fs::write(self.path.as_str(), value)?;
        }
        Ok(())
    }

    fn read(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let value = match std::fs::read_to_string(self.path.as_str()) {
            Ok(value) => value,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        let map = toml::from_str::<BTreeMap<String, String>>(&value)?;
        map.into_iter().for_each(|(key, value)| {
            self.state.insert(key, value);
        });
        Ok(())
    }

    fn allowed_keys() -> &'static [&'static str] {
        &["client_id", "domain", "endpoint", "secret"]
    }

    fn rows(&self) -> impl Iterator<Item = (i64, &str, &str)> {
        self.state.iter().map(|(k, v)| {
            (
                Self::allowed_keys().iter().position(|r| r == k).unwrap() as i64,
                k.as_str(),
                v.as_str(),
            )
        })
    }
}

#[repr(C)]
struct VTab {
    vtab: ffi::sqlite3_vtab,
    database_path: String,
}

impl VTab {
    unsafe fn new(database_path: String) -> Self {
        Self {
            vtab: mem::zeroed(),
            database_path,
        }
    }

    unsafe fn as_mut(ptr: *mut ffi::sqlite3_vtab) -> &'static mut Self {
        &mut *ptr.cast::<Self>()
    }

    unsafe fn from_raw(ptr: *mut ffi::sqlite3_vtab) -> Box<Self> {
        Box::from_raw(ptr.cast::<Self>())
    }

    fn into_raw(self) -> *mut ffi::sqlite3_vtab {
        Box::into_raw(Box::new(self)).cast()
    }
}

#[repr(C)]
struct VTabCursor {
    cur: ffi::sqlite3_vtab_cursor,
    offset: usize,
    rows: Vec<(i64, String, String)>,
}

impl VTabCursor {
    unsafe fn new(config_path: &str) -> Self {
        let config = ConfigRegistry::new().get(config_path);
        let rows = config
            .lock()
            .unwrap()
            .rows()
            .map(|(rowid, k, v)| (rowid, k.to_owned(), v.to_owned()))
            .collect();
        Self {
            cur: mem::zeroed(),
            offset: 0,
            rows,
        }
    }

    unsafe fn as_mut(ptr: *mut ffi::sqlite3_vtab_cursor) -> &'static mut Self {
        &mut *ptr.cast::<Self>()
    }

    unsafe fn from_raw(ptr: *mut ffi::sqlite3_vtab_cursor) -> Box<Self> {
        Box::from_raw(ptr.cast::<Self>())
    }

    fn into_raw(self) -> *mut ffi::sqlite3_vtab_cursor {
        Box::into_raw(Box::new(self)).cast()
    }
}

unsafe extern "C" fn x_connect(
    db: *mut ffi::sqlite3,
    _p_aux: *mut c_void,
    _argc: c_int,
    _argv: *const *const c_char,
    pp_vtab: *mut *mut ffi::sqlite3_vtab,
    _err: *mut *mut c_char,
) -> c_int {
    let rc = (*SQLITE3_API).declare_vtab.unwrap()(
        db,
        c_str!("CREATE TABLE mycelite_config(key text, value text)"),
    );
    if rc != ffi::SQLITE_OK {
        return rc;
    };
    let database_path = CStr::from_ptr((*SQLITE3_API).db_filename.unwrap()(db, c_str!("main")))
        .to_string_lossy()
        .to_string();
    *pp_vtab = VTab::new(database_path).into_raw();
    ffi::SQLITE_OK
}

unsafe extern "C" fn x_best_index(
    _p_vtab: *mut ffi::sqlite3_vtab,
    index_info: *mut ffi::sqlite3_index_info,
) -> c_int {
    ffi::SQLITE_OK
}

unsafe extern "C" fn x_disconnect(p_vtab: *mut ffi::sqlite3_vtab) -> c_int {
    VTab::from_raw(p_vtab);
    ffi::SQLITE_OK
}

unsafe extern "C" fn x_open(
    p_vtab: *mut ffi::sqlite3_vtab,
    pp_cursor: *mut *mut ffi::sqlite3_vtab_cursor,
) -> c_int {
    let vtab = VTab::as_mut(p_vtab);
    *pp_cursor = VTabCursor::new(vtab.database_path.as_str()).into_raw();
    ffi::SQLITE_OK
}

unsafe extern "C" fn x_close(p_cursor: *mut ffi::sqlite3_vtab_cursor) -> c_int {
    VTabCursor::from_raw(p_cursor);
    ffi::SQLITE_OK
}

unsafe extern "C" fn x_filter(
    p_cursor: *mut ffi::sqlite3_vtab_cursor,
    _idx_num: c_int,
    _idx_str: *const c_char,
    _argc: c_int,
    _argv: *mut *mut ffi::sqlite3_value,
) -> c_int {
    let mut cursor = VTabCursor::as_mut(p_cursor);
    cursor.offset = 0;
    ffi::SQLITE_OK
}

unsafe extern "C" fn x_next(p_cursor: *mut ffi::sqlite3_vtab_cursor) -> c_int {
    let cursor = VTabCursor::as_mut(p_cursor);
    cursor.offset += 1;
    ffi::SQLITE_OK
}

unsafe extern "C" fn x_column(
    p_cursor: *mut ffi::sqlite3_vtab_cursor,
    p_ctx: *mut ffi::sqlite3_context,
    n: c_int,
) -> c_int {
    let cursor = VTabCursor::as_mut(p_cursor);
    let row = match cursor.rows.get(cursor.offset) {
        Some(row) => row,
        None => return ffi::SQLITE_ERROR,
    };
    let value = match n {
        0 => row.1.clone(),
        1 => row.2.clone(),
        _ => return ffi::SQLITE_ERROR,
    };
    let len = value.len();
    let cs = CString::from_vec_unchecked(value.into_bytes());
    (*SQLITE3_API).result_text.unwrap()(p_ctx, cs.into_raw(), len as c_int, Some(deallocate));
    ffi::SQLITE_OK
}

unsafe extern "C" fn x_eof(p_cursor: *mut ffi::sqlite3_vtab_cursor) -> c_int {
    let cursor = VTabCursor::as_mut(p_cursor);
    (cursor.offset >= cursor.rows.len()) as c_int
}

unsafe extern "C" fn x_rowid(
    p_cursor: *mut ffi::sqlite3_vtab_cursor,
    p_rowid: *mut ffi::sqlite_int64,
) -> c_int {
    let cursor = VTabCursor::as_mut(p_cursor);
    *p_rowid = cursor.rows.get(cursor.offset).unwrap().0;
    ffi::SQLITE_OK
}

unsafe extern "C" fn x_update(
    vtab: *mut ffi::sqlite3_vtab,
    argc: c_int,
    value: *mut *mut ffi::sqlite3_value,
    _p_rowid: *mut ffi::sqlite3_int64,
) -> c_int {
    let vtab = VTab::as_mut(vtab);
    let config = ConfigRegistry::new().get(vtab.database_path.as_str());
    let mut config = config.lock().unwrap();
    match UpdateType::from((argc, value)) {
        UpdateType::Delete {
            row_id: SqliteValue::I64(row_id),
        } => config.delete(row_id as usize),
        UpdateType::Update { columns, .. } => match (columns.get(0), columns.get(1)) {
            (Some(SqliteValue::Text(key)), Some(SqliteValue::Text(value))) => {
                config.insert(key, value)
            }
            _ => {
                return ffi::SQLITE_MISUSE;
            }
        },
        UpdateType::Insert { columns, .. } => match (columns.get(0), columns.get(1)) {
            (Some(SqliteValue::Text(key)), Some(SqliteValue::Text(value))) => {
                config.insert(key, value)
            }
            _ => {
                return ffi::SQLITE_MISUSE;
            }
        },
        _ => return ffi::SQLITE_MISUSE,
    }
    ffi::SQLITE_OK
}

unsafe extern "C" fn x_begin(_p_vtab: *mut ffi::sqlite3_vtab) -> c_int {
    ffi::SQLITE_OK
}

unsafe extern "C" fn x_sync(p_vtab: *mut ffi::sqlite3_vtab) -> c_int {
    let vtab = VTab::as_mut(p_vtab);
    let config = ConfigRegistry::new().get(vtab.database_path.as_str());
    let mut config = config.lock().unwrap();
    if config.write().is_err() {
        return ffi::SQLITE_ERROR;
    };
    ffi::SQLITE_OK
}

unsafe extern "C" fn x_commit(_p_vtab: *mut ffi::sqlite3_vtab) -> c_int {
    ffi::SQLITE_OK
}

unsafe extern "C" fn x_rollback(_p_vtab: *mut ffi::sqlite3_vtab) -> c_int {
    ffi::SQLITE_OK
}

static mut SQLITE3_API: *mut ffi::sqlite3_api_routines = std::ptr::null_mut();

pub unsafe fn init(
    db: *mut ffi::sqlite3,
    _err: *mut *mut c_char,
    api: *mut ffi::sqlite3_api_routines,
) -> c_int {
    SQLITE3_API = api;

    static CONFIG_VTABLE: ffi::sqlite3_module = ffi::sqlite3_module {
        iVersion: 0,
        xCreate: None,
        xDestroy: None,
        xConnect: Some(x_connect),
        xDisconnect: Some(x_disconnect),
        xBestIndex: Some(x_best_index),
        xOpen: Some(x_open),
        xClose: Some(x_close),
        xFilter: Some(x_filter),
        xNext: Some(x_next),
        xEof: Some(x_eof),
        xColumn: Some(x_column),
        xRowid: Some(x_rowid),
        xUpdate: Some(x_update),
        xBegin: Some(x_begin),
        xSync: Some(x_sync),
        xCommit: Some(x_commit),
        xRollback: Some(x_rollback),
        xFindFunction: None,
        xRename: None,
        xSavepoint: None,
        xRelease: None,
        xRollbackTo: None,
        xShadowName: None,
    };

    (*SQLITE3_API).create_module.unwrap()(
        db,
        c_str!("mycelite_config"),
        &CONFIG_VTABLE,
        std::ptr::null_mut() as *mut c_void,
    )
}
