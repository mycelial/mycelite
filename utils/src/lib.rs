use std::iter;

const GAP: usize = 16;

pub fn get_diff<'a>(
    new_page: &'a [u8],
    old_page: &'a [u8],
) -> impl Iterator<Item = (usize, &'a [u8])> + 'a {
    let iter = old_page
        .iter()
        .chain(iter::repeat::<&u8>(&0))
        .zip(new_page)
        .map(|(&old, &new)| (old, new))
        .enumerate();

    Diff {
        iter,
        gap: GAP,
        range: None,
    }
    .map(|(start, end)| (start, &new_page[start..=end]))
}

pub struct Diff<I> {
    iter: I,
    gap: usize,
    range: Option<(usize, usize)>,
}

impl<I> Iterator for Diff<I>
where
    I: Iterator<Item = (usize, (u8, u8))>,
{
    type Item = (usize, usize);

    fn next(&mut self) -> Option<Self::Item> {
        for item in self.iter.by_ref() {
            match item {
                (i, (old, new)) if old != new => {
                    self.range = match self.range {
                        None => Some((i, i)),
                        Some((start, _)) => Some((start, i)),
                    }
                }
                (i, _) => match self.range {
                    Some((_, end)) if end + self.gap < i => {
                        return self.range.take();
                    }
                    _ => {}
                },
            }
        }
        self.range.take()
    }
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
        let old_page: &[u8] = &[
            83, 81, 76, 105, 116, 101, 32, 102, 111, 114, 109, 97, 116, 32, 51, 0, 16, 0, 1, 1, 0,
            64, 32, 32, 0, 0, 0, 4, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 4, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 4, 0, 46, 99, 1, 13, 0, 0, 0, 1, 15,
            201, 0, 15, 201, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 53, 1, 6, 23, 21, 21,
            1, 79, 116, 97, 98, 108, 101, 116, 101, 115, 116, 116, 101, 115, 116, 2, 67, 82, 69,
            65, 84, 69, 32, 84, 65, 66, 76, 69, 32, 116, 101, 115, 116, 40, 110, 117, 109, 98, 101,
            114, 32, 105, 110, 116, 101, 103, 101, 114, 41,
        ];
        let new_page: &[u8] = &[
            83, 81, 76, 105, 116, 101, 32, 102, 111, 114, 109, 97, 116, 32, 51, 0, 16, 0, 1, 1, 0,
            64, 32, 32, 0, 0, 0, 5, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 4, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 0, 46, 99, 1, 13, 0, 0, 0, 1, 15,
            201, 0, 15, 201, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 53, 1, 6, 23, 21, 21,
            1, 79, 116, 97, 98, 108, 101, 116, 101, 115, 116, 116, 101, 115, 116, 2, 67, 82, 69,
            65, 84, 69, 32, 84, 65, 66, 76, 69, 32, 116, 101, 115, 116, 40, 110, 117, 109, 98, 101,
            114, 32, 105, 110, 116, 101, 103, 101, 114, 41,
        ];
        let results = get_diff(new_page, old_page);
        let expected: Vec<(usize, &[u8])> = vec![(27, &[5]), (95, &[5])];
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
    fn test_it_works_with_empty_old_page_and_new_page_all_zeros() {
        let old_page: &[u8] = &[];
        let new_page: &[u8] = &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

        let results = get_diff(new_page, old_page);
        let expected: Vec<(usize, &[u8])> = vec![];

        assert_eq!(results.collect::<Vec<(usize, &[u8])>>(), expected);
    }

    #[test]
    fn test_it_works_with_empty_old_page() {
        let old_page: &[u8] = &[];
        let new_page: &[u8] = &[0, 0, 0, 1, 1, 1, 0, 1, 0, 0, 0];

        let results = get_diff(new_page, old_page);
        let expected: Vec<(usize, &[u8])> = vec![(3, &[1, 1, 1, 0, 1])];

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
