//! Block trait
pub use block_macro::block;

pub trait Block {
    fn block_size() -> usize;

    /// size of instance of the block, for enums it's tag + size of variant arm
    ///
    /// only new-type enums are currently supported
    fn iblock_size(&self) -> usize {
        Self::block_size()
    }
}
