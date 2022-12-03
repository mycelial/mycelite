## Block macro
Attribute macro for `block` crate

## Example
```rust
use block_macro::block;

#[block(512)]
struct S {

}

assert_eq!(S::block_size(), 512)
```
