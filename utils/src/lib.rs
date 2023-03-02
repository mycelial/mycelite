use std::iter;

const HEADER_SIZE: usize = 16;

pub fn get_diff<'a>(
    new_page: &'a [u8],
    old_page: &'a [u8],
) -> impl Iterator<Item = (usize, &'a [u8])> + 'a {
    let l = new_page.len();
    let o = old_page.len();

    let mut offset = 0;
    let mut offset_end = 0;

    old_page
        .iter()
        .chain(iter::repeat::<&u8>(&0))
        .zip(new_page)
        .enumerate()
        .filter_map(move |(i, values)| {
            // special case for when old page is empty
            if o == 0 && i == (l - 1) {
                return Some((0, &new_page[0..i + 1]));
            }
            if values.0 == values.1 {
                // if this value matches and the previous one did not, we know we just passed the end of one or more
                // values that didn't match. In that case, we note the position by setting `offset_end` to `i`
                if i != 0 && i < o && new_page[i - 1] != old_page[i - 1] {
                    offset_end = i;
                }
                // if we're HEADER_SIZE past the last section that needs changing, or at the end, we need to return the last blob of changes
                if offset_end + HEADER_SIZE == i || i == (l - 1) {
                    return Some((offset, &new_page[offset..offset_end]));
                }
            } else {
                // if this value doesn't match but the previous one did, we know we're at the beginning of one or more
                // values that don't match. we note the position by updating `offset` to `i`.
                if (i == 0 || (i < o && new_page[i - 1] == old_page[i - 1])) && (offset_end == 0 || offset_end + HEADER_SIZE < i) {
                    offset = i;
                }
                // normally, we add the values that need to be changed as soon as we see a matching value again, but
                // when we're on the last value that doesn't match, we need to have special handing to include it.
                if i == (l - 1) {
                    return Some((offset, &new_page[offset..i + 1]));
                }
            }
            None
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use quickcheck::{quickcheck, TestResult};

    #[test]
    fn it_works() {
        let expected: Vec<(usize, &[u8])> = vec![];
        let results = get_diff(&[], &[]);
        assert_eq!(results.collect::<Vec<(usize, &[u8])>>(), expected);
    }

    #[test]
    fn test_it_works_with_actual_data() {
        let old_page: &[u8] = &[0, 0, 0, 1, 1, 1, 2, 2, 2, 3, 3, 3];
        let new_page: &[u8] = &[0, 1, 2, 3, 1, 1, 1, 1, 2, 3, 1, 3];
        let results = get_diff(new_page, old_page);
        let expected: Vec<(usize, &[u8])> = vec![(1, &[1, 2, 3, 1, 1, 1, 1, 2, 3, 1])];
        assert_eq!(results.collect::<Vec<(usize, &[u8])>>(), expected);
    }

    #[test]
    fn test_it_works_with_small_gap_between_changed_values() {
        let old_page: &[u8] = &[
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
        ];
        let new_page: &[u8] = &[
            0, 1, 20, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 190, 20,
        ];
        let results = get_diff(new_page, old_page);
        let expected: Vec<(usize, &[u8])> = vec![(
            2,
            &[
                20, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 190,
            ],
        )];

        assert_eq!(results.collect::<Vec<(usize, &[u8])>>(), expected);
    }
    #[test]
    fn test_it_works_with_big_gap_between_changed_values() {
        let old_page: &[u8] = &[
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
        ];
        let new_page: &[u8] = &[
            0, 1, 20, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 200,
        ];

        let results = get_diff(new_page, old_page);
        let expected: Vec<(usize, &[u8])> = vec![(2, &[20]), (20, &[200])];

        assert_eq!(results.collect::<Vec<(usize, &[u8])>>(), expected);
    }

    #[test]
    fn test_it_works_with_values_at_end_changed() {
        let old_page: &[u8] = &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let new_page: &[u8] = &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1];

        let results = get_diff(new_page, old_page);
        let expected: Vec<(usize, &[u8])> = vec![(11, &[1])];

        assert_eq!(results.collect::<Vec<(usize, &[u8])>>(), expected);
    }

    #[test]
    fn test_it_works_with_empty_old_page() {
        let old_page: &[u8] = &[];
        let new_page: &[u8] = &[1, 0];

        let results = get_diff(new_page, old_page);
        let expected: Vec<(usize, &[u8])> = vec![(0, &[1, 0])];

        assert_eq!(results.collect::<Vec<(usize, &[u8])>>(), expected);
    }

    quickcheck! {
        fn prop_get_diff_when_pages_exist(new: Vec<u8>, old: Vec<u8>) -> TestResult {
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
            TestResult::from_bool(new == brand_new)
        }

        fn prop_get_diff_when_old_page_not_exists(new: Vec<u8>) -> TestResult {
            let old: Vec<u8> = vec![];
            let diff = get_diff(&new, &old);
            let mut brand_new = vec![0; new.len()];
            for (offset, bytes) in diff {
                for (i, val) in bytes.iter().enumerate() {
                    brand_new[offset + i] = *val;
                }
            }
            TestResult::from_bool(new == brand_new)
        }
    }
}
