#![no_std]
#[allow(non_upper_case_globals)]
#[allow(non_camel_case_types)]
#[allow(non_snake_case)]
pub mod ffi;
pub mod util;

use core::alloc::{GlobalAlloc, Layout};
use core::ffi::c_void;

pub struct SQLiteAllocator {
    pub malloc: unsafe extern "C" fn(u64) -> *mut c_void,
    pub free: unsafe extern "C" fn(*mut c_void),
}

const PTR_ISIZE: isize = core::mem::size_of::<usize>() as isize;
const PTR_USIZE: usize = core::mem::size_of::<usize>();

unsafe impl GlobalAlloc for SQLiteAllocator {
    // v--------------------------|
    // ------------------------------------------------------------
    // |     padding     |       ptr        |  aligned mem block  |
    // ------------------------------------------------------------
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let align = layout.align();
        let header_size = match align {
            align if align <= PTR_USIZE => PTR_USIZE,
            align => align,
        };
        let size = header_size + layout.size();
        let block = (self.malloc)(size as u64) as *mut u8;
        if block.is_null() {
            return block;
        }
        let padding = match (block as usize) % layout.align() {
            0 => header_size,
            v => align - v,
        } as isize;
        *(block.offset(padding - PTR_ISIZE) as *mut usize) = block as usize;
        block.offset(padding)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        // address of tag
        let addr = (ptr as usize - PTR_USIZE) as *mut usize;
        // address of original block
        let block = (*addr) as *mut c_void;
        (self.free)(block)
    }
}

/// Setup SQLITE3_API3 and SQLITE_ALLOCATOR symbols
#[macro_export]
macro_rules! setup {
    () => {
        static mut SQLITE3_API: *mut libsqlite_sys::ffi::sqlite3_api_routines =
            core::ptr::null_mut();

        // stub
        unsafe extern "C" fn _libsqlite3_stub_malloc(_: u64) -> *mut core::ffi::c_void {
            panic!("libsqlite3 not initialized");
        }

        unsafe extern "C" fn _libsqlite_stub_free(_: *mut core::ffi::c_void) {
            panic!("libsqlite3 not initialized");
        }

        #[global_allocator]
        static mut SQLITE_ALLOCATOR: libsqlite_sys::SQLiteAllocator =
            libsqlite_sys::SQLiteAllocator {
                malloc: _libsqlite3_stub_malloc,
                free: _libsqlite_stub_free,
            };

        use core::alloc::GlobalAlloc;
        pub unsafe extern "C" fn deallocate(ptr: *mut core::ffi::c_void) {
            SQLITE_ALLOCATOR.dealloc(
                ptr as *mut u8,
                core::alloc::Layout::from_size_align_unchecked(0, 0),
            )
        }
    };
}

/// Init SQLITE3_API and redefine GLOBAL_ALLOC functions to point to sqlite3_malloc64/sqlite3_free
#[macro_export]
macro_rules! init {
    ($global:expr) => {
        SQLITE3_API = $global;

        SQLITE_ALLOCATOR.malloc = (*$global).malloc64.unwrap();
        SQLITE_ALLOCATOR.free = (*$global).free.unwrap();
    };
}
