//! Various helpers around sqlite VTables
use crate::ffi;
use crate::sqlite_value::{SqliteValue, SqliteValueIter};
use core::ffi::c_int;

#[derive(Debug)]
pub enum UpdateType<'a> {
    Delete {
        row_id: SqliteValue<'a>,
    },
    Insert {
        row_id: SqliteValue<'a>,
        columns: SqliteValueIter<'a>,
    },
    Update {
        row_id: SqliteValue<'a>,
        columns: SqliteValueIter<'a>,
    },
}

// https://www.sqlite.org/vtab.html#xupdate
impl<'a>
    From<(
        c_int,
        *mut *mut ffi::sqlite3_value,
        *mut ffi::sqlite3_api_routines,
    )> for UpdateType<'a>
{
    fn from(
        (argc, argv, api): (
            c_int,
            *mut *mut ffi::sqlite3_value,
            *mut ffi::sqlite3_api_routines,
        ),
    ) -> Self {
        let mut iter = SqliteValueIter::new(argc, argv, api);
        let first = iter.next();
        let second = iter.next();
        match argc {
            1 if first.is_some() => Self::Delete {
                row_id: first.unwrap(),
            },
            v if v > 1 && first.is_some() && first.as_ref().unwrap().is_null() => Self::Insert {
                row_id: first.unwrap(),
                columns: iter,
            },
            v if v > 1
                && first.is_some()
                && !first.as_ref().unwrap().is_null()
                && first == second =>
            {
                Self::Insert {
                    row_id: first.unwrap(),
                    columns: iter,
                }
            }
            v if v > 1
                && first.is_some()
                && !first.as_ref().unwrap().is_null()
                && first != second =>
            {
                Self::Update {
                    row_id: first.unwrap(),
                    columns: iter,
                }
            }
            _ => unreachable!(),
        }
    }
}
