pub fn get_diff<'a>(new_page: &'a [u8], old_page: &'a [u8]) -> impl Iterator<Item=(usize, &'a [u8])> + 'a {
    let l = old_page.len();

    let mut offset = 0;

    old_page
        .into_iter()
        .enumerate()
        .filter_map(move |(i, old_val)| {
            let new_val = new_page[i];
            match old_val == &new_val {
                true => {
                    // if this value matches and the previous one did not, we know we just passed the end of one or more
                    // values that didn't match. In that case, we return the offset as well as the values to be changed, which
                    // are the ones between `offset` and `i`, as results.
                    if i != 0 && new_page[i - 1] != old_page[i - 1] {
                        return Some((offset, &new_page[offset..i]));
                    }
                    None
                }
                false => {
                    // if this value doesn't match but the previous one did, we know we're at the beginning of one or more
                    // values that don't match. we note the position by updating `offset` to `i`.
                    if i == 0 || new_page[i - 1] == old_page[i - 1] {
                        offset = i;
                    }
                    // normally, we add the values that need to be changed as soon as we see a matching value again, but
                    // when we're on the last value that doesn't match, we need to have special handing to include it.
                    if i == (l - 1) {
                        return Some((offset, &new_page[offset..i + 1]));
                    }
                    None
                }
            }
        })
}

#[cfg(test)]
mod tests {

    use super::*;
    use quickcheck::{quickcheck, TestResult};

    #[test]
    fn it_works() {
        get_diff(&[], &[]);
    }

    #[test]
    fn test_it_works_with_actual_data() {
        let old_page: &[u8] = &[0, 0, 0, 1, 1, 1, 2, 2, 2, 3, 3, 3];
        let new_page: &[u8] = &[0, 1, 2, 3, 1, 1, 1, 1, 2, 3, 1, 3];
        let results = get_diff(new_page, old_page);
        let expected: Vec<(usize, &[u8])> = vec![(1, &[1, 2, 3]), (6, &[1, 1]), (10, &[1])];
        assert_eq!(expected, results);
    }

    #[test]
    fn test_it_works_with_values_at_end_changed() {
        let old_page: &[u8] = &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let new_page: &[u8] = &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1];
        let results = get_diff(new_page, old_page);
        let expected: Vec<(usize, &[u8])> = vec![(11, &[1])];
        assert_eq!(expected, results);
    }

    quickcheck! {
        fn prop_get_diff(new: Vec<u8>, old: Vec<u8>) -> TestResult {
            if new.len() != old.len() {
                return TestResult::discard();
            }
            let diff = get_diff(&new, &old);
            let mut brand_new = old.clone();
            for (offset, bytes) in diff {
                for (i, val) in bytes.iter().enumerate() {
                    brand_new[offset + i] = *val;
                }
            }
            return TestResult::from_bool(new == brand_new);
        }
    }
}
