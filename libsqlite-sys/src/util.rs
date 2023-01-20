use core::ffi::{c_char, c_int, CStr};

/// Iterator over Argv params
pub struct ArgvIter<'a> {
    cur: c_int,
    argc: c_int,
    argv: *mut *const c_char,
    _marker: core::marker::PhantomData<&'a ()>,
}

impl<'a> ArgvIter<'a> {
    pub fn new(argc: c_int, argv: *const *const c_char) -> Self {
        Self {
            cur: 0,
            _marker: core::marker::PhantomData,
            argc,
            argv: argv as *mut _,
        }
    }
}

impl<'a> Iterator for ArgvIter<'a> {
    type Item = &'a CStr;

    fn next(&mut self) -> Option<Self::Item> {
        if self.argc <= self.cur {
            return None;
        }
        let cstr = Some(unsafe { CStr::from_ptr(*self.argv) });
        self.cur += 1;
        self.argv = unsafe { self.argv.offset(1) };
        cstr
    }
}

#[macro_export]
macro_rules! c_str(
    ($e:expr) => {
        concat!($e, "\0").as_ptr() as *const core::ffi::c_char
    }
);
