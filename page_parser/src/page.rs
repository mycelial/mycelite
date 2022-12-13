//! Sqlite Page

/// Sqlite Raw Page
///
/// Just a chunk of bytes representing sqlite database page
#[derive(Debug)]
pub struct RawPage(Vec<u8>);

impl RawPage {
    pub fn new(page: Vec<u8>) -> Self {
        Self(page)
    }

    pub fn as_slice(&self) -> &[u8] {
        self.0.as_slice()
    }
}
