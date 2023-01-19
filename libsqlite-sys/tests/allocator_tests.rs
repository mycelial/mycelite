use libc;
use libsqlite_sys;
use quickcheck::{Arbitrary, Gen, TestResult};
use std::alloc;

#[global_allocator]
static mut SQLITE3_ALLOCATOR: libsqlite_sys::SQLiteAllocator = libsqlite_sys::SQLiteAllocator {
    malloc: _test_malloc64_wrap,
    free: libc::free,
};

unsafe extern "C" fn _test_malloc64_wrap(size: u64) -> *mut core::ffi::c_void {
    libc::malloc(size as libc::size_t)
}

#[derive(Debug, Clone)]
struct TestAlloc {
    size: usize,
    layout: usize,
}

impl Arbitrary for TestAlloc {
    fn arbitrary(gen: &mut Gen) -> Self {
        let layouts = (0..10).map(|shf| 1 << shf).collect::<Vec<_>>();
        let size = usize::arbitrary(gen) % 0x0000_8000;
        let layout = *layouts.get(usize::arbitrary(gen) % layouts.len()).unwrap();
        TestAlloc { size, layout }
    }
}

const PTR_SIZE: isize = std::mem::size_of::<usize>() as isize;

#[test]
// goal of this function to check SQLite Allocator:
// 1. allocated addresses should be aligned
// 2. allocated block should contain requested amount of bytes
// 3. tag should be written properly in initial allocated block
fn test_allocator() {
    fn check(allocs: Vec<TestAlloc>) -> TestResult {
        let mut allocated = allocs
            .into_iter()
            .map(|t| {
                let layout = alloc::Layout::from_size_align(t.size, t.layout).unwrap();
                let result = unsafe { alloc::alloc(layout) };

                // check allocation is not null
                assert!(!result.is_null());

                // check if returned address alligned
                assert_eq!(result as usize % t.layout, 0);

                // address is always aligned by at least pointer size
                if t.layout < std::mem::size_of::<usize>() {
                    assert_eq!(result as usize % (PTR_SIZE as usize), 0);
                }

                // find original block address by reading address of result - usize
                let real_block_addr: usize = unsafe {
                    *(result.offset(-(std::mem::size_of::<usize>() as isize)) as *mut usize)
                };
                // +----------------------------------------
                // | real_addr | padding | header | result |
                // +----------------------------------------
                //      ^___________________|
                //  real_addr - pointer to block provided by 'malloc'
                //  padding   - calculated offset for block alignment
                //  header    - contains value of real_addr, to call 'free' properly
                //  result    - some value calcuated on top of real_add + layout

                // real block address is always smaller than resulting address, since at least we need
                // to store header, which contains real address of the allocated block
                assert!(real_block_addr < result as usize);

                // SQLiteAllocator asks for block of size + layout in order to properly align final block
                // resulting address should always between (real_address, real_address + layout]
                assert!(real_block_addr + t.layout.max(PTR_SIZE as usize) >= result as usize);

                // smoke test? cast allocated block to slice of bytes, zero all stuff
                let slice: &mut [u8] = unsafe { std::slice::from_raw_parts_mut(result, t.size) };
                slice.iter_mut().for_each(|x| *x = 0);
                assert_eq!(slice.iter().map(|x| (*x) as usize).sum::<usize>(), 0);

                (result, layout)
            })
            .collect::<Vec<_>>();

        // deallocate
        while let Some((addr, layout)) = allocated.pop() {
            unsafe { alloc::dealloc(addr, layout) };
        }
        TestResult::from_bool(true)
    }
    quickcheck::quickcheck(check as fn(Vec<TestAlloc>) -> TestResult);
}
