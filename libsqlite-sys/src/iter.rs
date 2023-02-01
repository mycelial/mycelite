//! Iterator over sqlite argc/argv pair
use core::ffi::c_int;
use core::marker::PhantomData;

/// Iterator over sqlite pointers
#[derive(Debug)]
pub struct PtrIter<'a, T> {
    offset: usize,
    len: usize,
    ptr: *const T,
    _marker: PhantomData<&'a ()>,
}

impl<'a, T> Iterator for PtrIter<'a, T>
where
    T: Copy,
{
    type Item = T;

    fn next(&mut self) -> Option<T> {
        if self.offset >= self.len {
            return None;
        }
        let item = unsafe { *self.ptr.add(self.offset) };
        self.offset += 1;
        Some(item)
    }
}

impl<'a, T> PtrIter<'a, T> {
    pub fn new(len: c_int, ptr: *const T) -> Self {
        Self {
            offset: 0,
            len: len as usize,
            ptr,
            _marker: PhantomData,
        }
    }
}
