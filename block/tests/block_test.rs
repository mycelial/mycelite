#[cfg(test)]
mod test {
    use block_macro::*;
    use block::Block;

    #[block(512)]
    struct S {}

    #[block(128)]
    enum E {}

    #[test]
    fn test_block() {
        assert_eq!(<S as Block>::block_size(), 512);
        assert_eq!(<E as Block>::block_size(), 128);
        assert_eq!(S::block_size(), 512);
        assert_eq!(E::block_size(), 128);
    }
}
