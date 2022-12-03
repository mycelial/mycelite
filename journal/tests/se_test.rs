use block::{block, Block};
use journal::{to_bytes, Error};
use serde::Serialize;

#[derive(Debug, Serialize)]
#[block(64)]
struct ValidStruct {
    b: bool,
    u_8: u8,
    u_16: u16,
    u_32: u32,
    u_64: u64,
    i_8: i8,
    i_16: i16,
    i_32: i32,
    i_64: i64,
    f_32: f32,
    f_64: f64,
    #[serde(serialize_with = "journal::se::custom_option")]
    n: Option<u64>,
    #[serde(serialize_with = "journal::se::custom_option")]
    s: Option<u64>,
}

#[test]
#[rustfmt::skip]
fn test_valid_serialization() {
    let header = ValidStruct {
        b: true,
        u_8: 2,
        u_16: 0x0102_u16,
        u_32: 0x01020304_u32,
        u_64: 0x0102030405060708_u64,
        i_8: -1,
        i_16: -2,
        i_32: -3,
        i_64: -4,
        f_32: -0.0,
        f_64: f64::INFINITY,
        n: None,
        s: Some(1),
    };
    // to_bytes
    let res = to_bytes(&header);
    assert!(res.is_ok(), "{:?}", res);

    let header = res.unwrap();
    assert_eq!(header.len(), ValidStruct::block_size());
    assert!(matches!(
        header.as_slice(),
        &[ 
        /* b       */ 0x01,
        /* u_8     */ 0x02,
        /* u_16    */ 0x01, 0x02,
        /* u_32    */ 0x01, 0x02, 0x03, 0x04,
        /* u_64    */ 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08,
        /* i_8     */ 0xff,
        /* i_16    */ 0xff, 0xfe,
        /* i_32    */ 0xff, 0xff, 0xff, 0xfd,
        /* i_64    */ 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xfc,
        /* f_32    */ 0x80, 0x00, 0x00, 0x00,
        /* f_64    */ 0x7f, 0xf0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        /* n<u64>  */ 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        /* s<u64>  */ 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
        /* block   */ 
        /* padding */ 0x00, 0x00, 0x00, 0x00, 0x00,
        ]
    ));
}

#[derive(Debug, Serialize)]
#[block(4)]
struct InvalidStruct {
    v: u64,
}

#[test]
fn test_invalid_serialization() {
    // serialized struct contains more bytes than size provided to block macro
    assert!(matches!(
        to_bytes(&InvalidStruct { v: 0 }),
        Err(Error::IoError(_))
    ));
}
