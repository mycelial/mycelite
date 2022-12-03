//! Block trait
pub use block_macro::block;

pub trait Block {
    fn block_size() -> usize;
}
