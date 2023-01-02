use block::{block, Block};
use serde::Serialize;
use serde_sqlite::{to_bytes, to_writer, Error};

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
    #[serde(serialize_with = "serde_sqlite::se::none_as_zero")]
    n: Option<u64>,
    #[serde(serialize_with = "serde_sqlite::se::none_as_zero")]
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
    assert_eq!(
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
    );
}

#[test]
#[rustfmt::skip]
fn test_valid_serialization_to_writer() {
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
    
    let mut buf = vec![0xff; 72];
    let res = to_writer(buf.as_mut_slice(), &header);
    assert!(res.is_ok(), "{:?}", res);

    assert_eq!(
        buf.as_slice(),
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
        /*  extra  */ 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff
        ]
    );
}

#[derive(Debug, Serialize)]
#[block(4)]
struct InvalidStruct {
    v: u64,
}

#[test]
/// serialized struct contains more bytes than size provided to block macro
fn test_invalid_serialization() {
    let err = to_bytes(&InvalidStruct { v: 0 });
    assert!(matches!(err, Err(Error::IoError(_))));
    let err = err.unwrap_err();
    assert_eq!(
        err.to_string(),
        "IoError(Custom { kind: Other, error: \"block size overflow\" })"
    );
}

#[test]
fn test_invalid_serialization_to_writer() {
    let mut buf = vec![0xff; 128];
    let err = to_writer(buf.as_mut_slice(), &InvalidStruct { v: 0 });
    assert!(matches!(err, Err(Error::IoError(_))));
    let err = err.unwrap_err();
    assert_eq!(
        err.to_string(),
        "IoError(Custom { kind: Other, error: \"block size overflow\" })"
    );
}

// enum serialization

#[derive(Serialize)]
#[block(32)]
struct FirstVariant {
    f: u64,
    s: u32,
    t: [u8; 2],
}

#[derive(Serialize)]
#[block(16)]
struct SecondVariant {
    f: i64,
    s: i64,
}

// newtype enum, wraps existing structures
// serializer needs to produce blocks of different size + prefix to hold enum discriminant
#[derive(Serialize)]
#[block]
enum NewTypeEnum {
    F(FirstVariant),
    S(SecondVariant),
}

#[test]
#[rustfmt::skip]
fn test_enum_newtype_serialization() {
    let fv = NewTypeEnum::F(FirstVariant { f: 0, s: 1, t: [0; 2] });
    let fv_res= serde_sqlite::to_bytes(&fv);
    assert!(fv_res.is_ok());
    let fv_bytes = fv_res.unwrap();
    assert_eq!(fv_bytes.len(), fv.iblock_size());
    assert_eq!(
        fv_bytes.as_slice(),
        &[
        /* tag     */ 0x00, 0x00, 0x00, 0x00,
        /* f       */ 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        /* s       */ 0x00, 0x00, 0x00, 0x01,
        /* t       */ 0x00, 0x00,

        /*  block  */ 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        /*         */ 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        /* padding */ 0x00, 0x00
        ]
    );

    let sv = NewTypeEnum::S(SecondVariant{ f: 0, s: 1});
    let sv_res= serde_sqlite::to_bytes(&sv);
    assert!(sv_res.is_ok());
    let sv_bytes = sv_res.unwrap();
    assert_eq!(
        sv_bytes.as_slice(),
        &[
        /* tag     */ 0x00, 0x00, 0x00, 0x01,
        /* f       */ 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        /* s       */ 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01
        ]
    );
    assert_eq!(sv_bytes.len(), sv.iblock_size());
}
