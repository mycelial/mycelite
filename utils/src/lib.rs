// calculates diff between two sqlite pages
pub fn get_diff<'a>(new_page: &'a [u8], old_page: &'a [u8]) -> Vec<(usize, &'a [u8])> {
    let mut res: Vec<(usize, &[u8])> = Vec::new();

    // TODO?: currently just assuming pages are the same length
    let mut i: usize = 0;
    let l = old_page.len();

    let mut offset = 0;
    let mut in_change_block = false;

    loop {
        if i == l && in_change_block {
            res.push((offset, &new_page[offset..i]));
            in_change_block = false
        }
        if i >= l {
            break;
        }

        if new_page[i] != old_page[i] {
            if !in_change_block {
                in_change_block = true;
                offset = i;
            }
        }
        if new_page[i] == old_page[i] {
            if in_change_block {
                res.push((offset, &new_page[offset..i]));
                in_change_block = false
            }
        }

        i += 1
    }

    res
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
    fn test_it_works_with_data() {
        let old_page: &[u8] = &[0, 0, 0, 1, 1, 1, 2, 2, 2, 3, 3, 3];
        let new_page: &[u8] = &[0, 1, 2, 3, 1, 1, 1, 1, 2, 3, 1, 3];
        let results = get_diff(new_page, old_page);
        let expected: Vec<(usize, &[u8])> = vec![(1, &[1, 2, 3]), (6, &[1, 1]), (10, &[1])];
        assert_eq!(expected, results);
    }

    quickcheck! {
        fn prop_get_diff(p1: Vec<u8>, p2: Vec<u8>) -> TestResult {
            if p1.len() != p2.len() {
                return TestResult::discard();
            }
            let diff = get_diff(&p1, &p2);
            let diff_exists = diff.len() > 0;
            let inputs_equal = p1 == p2;
            return TestResult::from_bool(inputs_equal != diff_exists);
        }
    }
}
