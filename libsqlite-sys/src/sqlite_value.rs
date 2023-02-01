//! Wrapper for sqlite3_value struct
use crate::{ffi, iter};
use core::ffi::c_int;

#[derive(Debug, PartialEq)]
pub enum SqliteValue<'a> {
    I64(i64),
    Double(f64),
    Blob(&'a [u8]),
    Text(&'a str),
    Null,
}

impl<'a> SqliteValue<'a> {
    pub fn is_null(&self) -> bool {
        match self {
            Self::Null => true,
            _ => false,
        }
    }
}

/// Iterator over *mut *mut ffi::sqlite3_value
#[derive(Debug)]
pub struct SqliteValueIter<'a> {
    iter: iter::PtrIter<'a, *mut ffi::sqlite3_value>,
    api: *mut ffi::sqlite3_api_routines,
}

impl<'a> SqliteValueIter<'a> {
    pub fn new(
        argc: c_int,
        value: *mut *mut ffi::sqlite3_value,
        api: *mut ffi::sqlite3_api_routines,
    ) -> Self {
        Self {
            iter: iter::PtrIter::new(argc, value),
            api,
        }
    }
}

impl<'a> Iterator for SqliteValueIter<'a> {
    type Item = SqliteValue<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let value = match self.iter.next() {
            Some(v) => v,
            None => return None,
        };
        let value = unsafe {
            match { (*self.api).value_type.unwrap()(value) } {
                ffi::SQLITE_TEXT => {
                    let (text, len) = (
                        (*self.api).value_text.unwrap()(value),
                        (*self.api).value_bytes.unwrap()(value) as usize,
                    );
                    SqliteValue::Text(core::str::from_utf8_unchecked(core::slice::from_raw_parts(
                        text, len,
                    )))
                }
                ffi::SQLITE_INTEGER => SqliteValue::I64((*self.api).value_int64.unwrap()(value)),
                ffi::SQLITE_FLOAT => SqliteValue::Double((*self.api).value_double.unwrap()(value)),
                ffi::SQLITE_NULL => SqliteValue::Null,
                ffi::SQLITE_BLOB => {
                    let blob = core::slice::from_raw_parts(
                        (*self.api).value_blob.unwrap()(value) as *const u8,
                        (*self.api).value_bytes.unwrap()(value) as usize,
                    );
                    SqliteValue::Blob(blob)
                }
                _ => unreachable!(),
            }
        };
        Some(value)
    }
}
