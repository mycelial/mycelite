use block::block;
use serde::Deserialize;
use serde_sqlite::Error;
use serde_sqlite::{from_bytes, from_reader};

#[derive(Debug, Deserialize, PartialEq)]
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
    #[serde(deserialize_with = "serde_sqlite::de::zero_as_none")]
    n: Option<u64>,
    #[serde(deserialize_with = "serde_sqlite::de::zero_as_none")]
    s: Option<u64>,
}

#[test]
#[rustfmt::skip]
fn test_deserialization_from_bytes() {
    let block = &[
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
        /* padding */ 0x01, 0x02, 0x03, 0x04, 0x05
    ];
    let decoded = from_bytes::<ValidStruct>(block);
    assert!(decoded.is_ok());
    let decoded = decoded.unwrap();
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
    assert_eq!(decoded, header);
}

#[test]
#[rustfmt::skip]
fn test_deserialization_from_reader() {
    let block = &[
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
        /* padding */ 0x00, 0x00, 0x00, 0x00, 0x00
    ];
    let decoded = from_reader::<ValidStruct, _>(std::io::Cursor::new(block));
    assert!(decoded.is_ok());
    let decoded = decoded.unwrap();
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
    assert_eq!(decoded, header);
}

#[test]
#[rustfmt::skip]
fn test_deserialization_error() {
    // incomplete block (padding is missing)
    let block = &[
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
    ];
    assert!(matches!(from_bytes::<ValidStruct>(block), Err(Error::IoError(_))));
    assert!(matches!(
        from_reader::<ValidStruct, _>(std::io::Cursor::new(block)),
        Err(Error::IoError(_)))
    );
}

#[derive(Debug, Deserialize, PartialEq)]
#[block(8)]
struct S {}

#[derive(Debug, Deserialize, PartialEq)]
#[block]
enum A {
    F(ValidStruct),
    S(S),
}

#[derive(Debug, Deserialize, PartialEq)]
#[block]
enum B {
    A(A),
}

#[test]
#[rustfmt::skip]
fn test_deserialization_newtype_enum() {
    // test first variant
    let block = &[
        /* tag     */ 0x00, 0x00, 0x00, 0x00,
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
        /* padding */ 0x01, 0x02, 0x03, 0x04, 0x05
    ];
    let res = from_bytes::<A>(block);
    assert!(res.is_ok(), "{res:?}");
    let f = res.unwrap();
    assert_eq!(
        A::F(ValidStruct{
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
        }),
        f
    );


    let block = &[
        /* tag     */ 0x00, 0x00, 0x00, 0x01,
        /* padding */ 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00
    ];
    let res = from_bytes::<A>(block);
    assert!(res.is_ok(), "{res:?}");
    let s = res.unwrap();
    assert_eq!(A::S(S{}), s);


    let block = &[
        /* tag B   */ 0x00, 0x00, 0x00, 0x00,
        /* tag A   */ 0x00, 0x00, 0x00, 0x01,
        /* padding */ 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00
    ];

    let res = from_bytes::<B>(block);
    assert!(res.is_ok(), "{res:?}");
    let b = res.unwrap();
    assert_eq!(B::A(A::S(S{})), b);
}
