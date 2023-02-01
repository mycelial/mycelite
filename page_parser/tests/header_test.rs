//! validate sqlite header deserializer/serialiser.
//! deserialized version compared against manually parsed version.
//! serialized version should produce exact header is was deserialized from.

use page_parser::Header;

use std::ffi::CStr;

// real sqlite3 header
static HEADER: [u8; 100] = [
    0x53, 0x51, 0x4c, 0x69, 0x74, 0x65, 0x20, 0x66, 0x6f, 0x72, 0x6d, 0x61, 0x74, 0x20, 0x33, 0x00,
    0x10, 0x00, 0x01, 0x01, 0x00, 0x40, 0x20, 0x20, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x04,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
    0x00, 0x2e, 0x63, 0x00,
];

/// manually parsed header
#[derive(Debug, Clone)]
pub struct TestHeader {
    pub magic: [u8; 16],
    pub page_size: u32,
    pub write_version: u8,
    pub read_version: u8,
    pub max_embedded_payload_fraction: u8,
    pub min_embedded_payload_fraction: u8,
    pub leaf_payload_fraction: u8,
    pub file_change_counter: u32,
    pub db_size: u32,
    pub first_free_page_num: Option<u32>,
    pub freelist_total: u32,
    pub schema_cookie: u32,
    pub schema_format_num: u32,
    pub default_page_cache_size: u32,
    pub largest_root: u32,
    pub text_encoding: u32,
    pub user_version: u32,
    pub inc_vacuum_mode: u32,
    pub application_id: u32,
    pub version_valid_for_number: u32,
    pub version: u32,
}

macro_rules! slc {
    ($buf:ident, $offset:expr, $len:expr) => {
        $buf[$offset..($offset + $len)]
    };
    ($buf:ident, $offset:expr, $len:expr, $t:ty) => {
        <$t>::from_be_bytes(slc!($buf, $offset, $len).try_into()?)
    };
}

impl TryFrom<&[u8; 100]> for TestHeader {
    type Error = Box<dyn std::error::Error>;

    fn try_from(buf: &[u8; 100]) -> Result<Self, Self::Error> {
        Ok(Self::new(
            {
                let mut magic = [0_u8; 16];
                magic.copy_from_slice(&buf[..16]);
                magic
            }, // header
            slc!(buf, 16, 2, u16),                               // page size
            slc!(buf, 18, 1, u8),                                // write_version
            slc!(buf, 19, 1, u8),                                // read_version
            slc!(buf, 21, 1, u8),                                // max_embedded_payload_fraction
            slc!(buf, 22, 1, u8),                                // min_embedded_payload_fraction
            slc!(buf, 23, 1, u8),                                // leaf_payload_fraction
            slc!(buf, 24, 4, u32),                               // file_change_counter
            slc!(buf, 28, 4, u32),                               // db_size
            slc!(buf, 32, 4, u32).checked_sub(1).map(|x| x + 1), // first_free_page_num
            slc!(buf, 36, 4, u32),                               // freelist_total
            slc!(buf, 40, 4, u32),                               // schema_cookie
            slc!(buf, 44, 4, u32),                               // schema_format_num
            slc!(buf, 48, 4, u32),                               // default_page_cache
            slc!(buf, 52, 4, u32),                               // largest_root
            slc!(buf, 56, 4, u32),                               // text_encoding
            slc!(buf, 60, 4, u32),                               // user_version
            slc!(buf, 64, 4, u32),                               // inc_vacuum_mode
            slc!(buf, 68, 4, u32),                               // application_id
            slc!(buf, 92, 4, u32),                               // version_valid_for_number
            slc!(buf, 96, 4, u32),                               // version
        ))
    }
}

impl TestHeader {
    pub fn new(
        magic: [u8; 16],
        page_size: u16,
        write_version: u8,
        read_version: u8,
        max_embedded_payload_fraction: u8,
        min_embedded_payload_fraction: u8,
        leaf_payload_fraction: u8,
        file_change_counter: u32,
        db_size: u32,
        first_free_page_num: Option<u32>,
        freelist_total: u32,
        schema_cookie: u32,
        schema_format_num: u32,
        default_page_cache_size: u32,
        largest_root: u32,
        text_encoding: u32,
        user_version: u32,
        inc_vacuum_mode: u32,
        application_id: u32,
        version_valid_for_number: u32,
        version: u32,
    ) -> Self {
        Self {
            magic,
            page_size: Self::to_page_size(page_size),
            write_version,
            read_version,
            max_embedded_payload_fraction,
            min_embedded_payload_fraction,
            leaf_payload_fraction,
            file_change_counter,
            db_size,
            first_free_page_num,
            freelist_total,
            schema_cookie,
            schema_format_num,
            default_page_cache_size,
            largest_root,
            text_encoding,
            user_version,
            inc_vacuum_mode,
            application_id,
            version_valid_for_number,
            version,
        }
    }

    /// Get real page size
    ///
    /// Field stores only 2 bytes, to max value to represent is 65535
    /// To specify page size of value 65536 - 0x0001 value is used
    fn to_page_size(value: u16) -> u32 {
        match value {
            1 => 65536,
            v => v as u32,
        }
    }
}

#[test]
fn header_deserialize_serialize() {
    let header = serde_sqlite::from_bytes::<Header>(HEADER.as_slice());
    assert!(header.is_ok(), "{header:?}");
    let header = header.unwrap();

    let test_header = <TestHeader as TryFrom<_>>::try_from(&HEADER);
    assert!(test_header.is_ok(), "{test_header:?}");
    let test_header = test_header.unwrap();

    // check magic
    assert_eq!(header.magic, test_header.magic);
    assert_eq!(
        CStr::from_bytes_with_nul(header.magic.as_slice()),
        CStr::from_bytes_with_nul(b"SQLite format 3\0")
    );

    // check page size
    assert_eq!(header.page_size(), test_header.page_size);

    // check write version
    assert_eq!(header.write_version, test_header.write_version);

    // check read version
    assert_eq!(header.read_version, test_header.read_version);

    // check max embedded payload fraction
    assert_eq!(
        header.max_embedded_payload_fraction,
        test_header.max_embedded_payload_fraction
    );

    // check min embedded payload fraction
    assert_eq!(
        header.min_embedded_payload_fraction,
        test_header.min_embedded_payload_fraction
    );

    // check leaf payload fraction
    assert_eq!(
        header.leaf_payload_fraction,
        test_header.leaf_payload_fraction
    );

    // check file change counter
    assert_eq!(header.file_change_counter, test_header.file_change_counter);

    // check db size
    assert_eq!(header.database_size, test_header.db_size);

    // check first free page num
    assert_eq!(
        header.first_freelist_page_num,
        test_header.first_free_page_num
    );

    // check freelist total
    assert_eq!(header.freelist_pages_total, test_header.freelist_total);

    // check schema cookie
    assert_eq!(header.schema_cookie, test_header.schema_cookie);

    // check schema format num
    assert_eq!(header.schema_format_num, test_header.schema_format_num);

    // check default page cache size
    assert_eq!(
        header.default_page_cache_size,
        test_header.default_page_cache_size
    );

    // check largest root
    assert_eq!(header.largest_root, test_header.largest_root);

    // check text encoding
    assert_eq!(header.text_encoding, { test_header.text_encoding });

    // check user version
    assert_eq!(header.user_version, test_header.user_version);

    // check inc vacuum mode
    assert_eq!(header.inc_vacuum_mode, test_header.inc_vacuum_mode);

    // check application id
    assert_eq!(header.application_id, test_header.application_id);

    // check version valid for number
    assert_eq!(
        header.version_valid_for_number,
        test_header.version_valid_for_number
    );

    // check version
    assert_eq!(header.version, test_header.version);

    // check serialized header equals original header
    let bytes = serde_sqlite::to_bytes(&header);
    assert!(bytes.is_ok(), "{bytes:?}");
    let bytes = bytes.unwrap();
    assert_eq!(bytes, HEADER);
}
