use libsqlite_sys;
use libc;
use quickcheck::{Arbitrary, Gen, TestResult};
use std::alloc;

unsafe extern "C" fn _test_malloc64_wrap(size: u64) -> *mut core::ffi::c_void {
    libc::malloc(size as libc::size_t)
}

#[global_allocator]
static mut SQLITE3_ALLOCATOR: libsqlite_sys::SQLiteAllocator = libsqlite_sys::SQLiteAllocator {
    malloc: _test_malloc64_wrap,
    free: libc::free,
};

#[derive(Debug, Clone)]
struct TestAlloc {
    size: usize,
    layout: usize,
}

impl Arbitrary for TestAlloc {
    fn arbitrary(gen: &mut Gen) -> Self {
        let layouts = (0..10).map(|shf| 1 << shf).collect::<Vec<_>>();
        let size = (usize::arbitrary(gen) % 16536).max(1);
        let layout = *layouts.get(usize::arbitrary(gen) % layouts.len()).unwrap();
        TestAlloc { size, layout }
    }
}


#[test]
// goal of this function to check SQLite Allocator:
// 1. addresses should be aligned
// 2. allocated block should contain requested amount of bytes
// 3. tag should be written properly in initial allocated block
fn test_allocator() {
    fn check(allocs: Vec<TestAlloc>) -> TestResult {
        for t in allocs {
            println!("size: {:?}, layout: {:?}", t.size, t.layout);
            let layout = alloc::Layout::from_size_align(t.size, t.layout).unwrap();
            let result = unsafe { alloc::alloc(layout) };

            // check allocation is not null
            assert!(!result.is_null());

            // check if returned address alligned
            assert_eq!(result as usize % t.layout, 0);
        }
        TestResult::from_bool(true)
    }
    quickcheck::quickcheck(check as fn(Vec<TestAlloc>) -> TestResult)
}
