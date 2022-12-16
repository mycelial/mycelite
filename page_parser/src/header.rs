//! [Sqlite Database Header]<https://www.sqlite.org/fileformat.html#the_database_header>

use block::block;
use serde::{Deserialize, Serialize};
use serde_sqlite;

/// sqlite database header
#[derive(Debug, Clone, Serialize, Deserialize)]
#[block(100)]
pub struct Header {
    /// sqlite header magic: 'SQLite format 3\0'
    pub magic: [u8; 16],
    /// sqlite page size, values of power of two between 512 and 32768 inclusive, or the value 1 representing a page size of 65536
    pub page_size: u16,
    /// file format write version: 1 for legacy, 2 for WAL
    pub write_version: u8,
    /// file format read vresion: 1 for legacy, 2 for WAL
    pub read_version: u8,
    // reserved
    _reserved_1: u8,
    /// max embedded payload fraction, must be 64
    pub max_embedded_payload_fraction: u8,
    /// min embedded payload fraction, must be 32
    pub min_embedded_payload_fraction: u8,
    /// leaf payload fraction
    pub leaf_payload_fraction: u8,
    /// file change counter
    pub file_change_counter: u32,
    /// size of the database file in pages, the "in-header database size"
    pub database_size: u32,
    /// page number of the first freelist trunk page
    #[serde(
        deserialize_with = "serde_sqlite::de::zero_as_none",
        serialize_with = "serde_sqlite::se::none_as_zero"
    )]
    pub first_freelist_page_num: Option<u32>,
    /// total number of freelist pages
    pub freelist_pages_total: u32,
    /// schema cookie
    pub schema_cookie: u32,
    /// schema format number, supported values are 1, 2, 3 and 4
    pub schema_format_num: u32,
    /// default page cache size
    pub default_page_cache_size: u32,
    /// page number of largest root b-tree page when in auto-vacuum or incremental vacuum modes, zero otherwise
    pub largest_root: u32,
    /// db text encoding:
    /// UTF-8    - 1
    /// UTF-16le - 2
    /// UTF-16be - 3
    pub text_encoding: u32,
    /// user version, set by user version pragma
    pub user_version: u32,
    /// incremental vacuum mode flag, true if not 0, false otherwize
    pub inc_vacuum_mode: u32,
    /// application id, set by pragma application id
    pub application_id: u32,
    // reserved
    _reserved_2: [u8; 20],
    // offset: 72, size: 20
    /// version of sqlite which modified database recently
    pub version_valid_for_number: u32,
    /// sqlite version number
    pub version: u32,
}

impl Header {
    pub fn page_size(&self) -> u32 {
        match self.page_size {
            1 => 0x10000,
            v => v as u32,
        }
    }
}
