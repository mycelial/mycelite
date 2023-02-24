//! calculates diff between two sqlite pages
pub fn get_diff(new_page: &[u8], old_page: &[u8]) ->  Vec<()> {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_not_works() {
        get_diff(&[], &[]);
    }
}
