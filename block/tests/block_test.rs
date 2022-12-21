use block::Block;
use block_macro::*;

#[block(512)]
struct S {}

#[block]
enum E {
    E(S),
}

#[test]
fn test_block_size() {
    assert_eq!(<S as Block>::block_size(), 512);
    assert_eq!(<E as Block>::block_size(), 4);
    assert_eq!(S::block_size(), 512);
    assert_eq!(E::block_size(), 4);
}

#[block]
enum NewTypeEnum {
    S(S),
    E(E),
}

#[test]
fn test_new_type_enum() {
    assert_eq!(<NewTypeEnum as Block>::block_size(), 4);

    let instance = NewTypeEnum::S(S {});
    assert_eq!(instance.iblock_size(), 512 + 4);

    let instance = NewTypeEnum::E(E::E(S {}));
    assert_eq!(instance.iblock_size(), 4 + 4 + 512);
}
