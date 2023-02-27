use std::io::Cursor;

// TODO: There's a comment in the docs indicating this shouldn't be used in production.
// "These simple functions will do everything in one go and are thus not recommended for use cases
// outside of prototyping/testing as real world data can have any size and thus result in very large
// memory allocations for the output Vector. Consider using miniz_oxide via flate2 which makes it easy
// to do streaming (de)compression or the low-level streaming functions instead."
use miniz_oxide::deflate::compress_to_vec;
use miniz_oxide::inflate::decompress_to_vec;

pub fn get_compressed_diff(new_page: &[u8], old_page: &[u8]) -> Vec<u8> {
    compress(get_diff(new_page, old_page))
}

pub fn apply_compressed_diff(old_page: &[u8], diff: Vec<u8>, expected_size: usize) -> Vec<u8> {
    apply_diff(old_page, decompress(&diff), expected_size)
}

// calculates diff between two sqlite pages
pub fn get_diff(new_page: &[u8], old_page: &[u8]) -> Vec<u8> {
    let mut cursor = Cursor::new(Vec::new());
    bsdiff::diff::diff(&old_page, &new_page, &mut cursor).unwrap();
    cursor.into_inner()
}

// applies a diff on top of a page to get the new page
pub fn apply_diff(old_page: &[u8], diff: Vec<u8>, expected_size: usize) -> Vec<u8> {
    let mut patched = vec![0; expected_size];
    let mut cursor = Cursor::new(diff);

    bsdiff::patch::patch(&old_page, &mut cursor, &mut patched).unwrap();
    patched
}

pub fn compress(diff: Vec<u8>) -> Vec<u8> {
    compress_to_vec(&diff, 6)
}

pub fn decompress(compressed: &Vec<u8>) -> Vec<u8> {
    decompress_to_vec(compressed).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use quickcheck::quickcheck;
    use rand::{distributions::Uniform, Rng};

    fn get_random(size: usize) -> Vec<u8> {
        let mut rng = rand::thread_rng();
        let range = Uniform::new(0, std::u8::MAX);

        let vals: Vec<u8> = (0..size).map(|_| rng.sample(&range)).collect();
        vals
    }

    quickcheck! {
        fn prop_get_and_apply_diff(p1: Vec<u8>, p2: Vec<u8>) -> bool {
            let s = p1.len();
            println!("p1: {:?}", p1);
            println!("p2: {:?}", p2);
            let compressed_diff = get_compressed_diff(&p1, &p2);
            println!("compressed: {:?}", compressed_diff);
            let applied = apply_compressed_diff(&p2, compressed_diff, s);
            println!("applied: {:?}", applied);
            p1 == applied
        }
    }

    #[test]
    fn test_diff() {
        let one: &[u8] = &[1, 2, 3, 4, 5, 6, 7];
        let two: &[u8] = &[1, 2, 4, 4, 7, 6, 7];

        let diff = get_diff(two, one);
        let expected: Vec<u8> = vec![
            7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 128, 0, 0, 1, 0,
            2, 0, 0,
        ];
        assert_eq!(diff, expected);
        let compressed_diff = get_compressed_diff(two, one);
        let expected_compressed_diff =
            vec![99, 103, 64, 5, 76, 16, 170, 129, 129, 129, 17, 196, 6, 0];
        assert_eq!(compressed_diff, expected_compressed_diff);
    }

    #[test]
    fn test_that_applying_diff_results_in_original() {
        let original_page = get_random(4096);
        let new_page = get_random(4096);

        let compressed_diff = get_compressed_diff(&new_page, &original_page);
        assert_ne!(original_page, compressed_diff);
        assert_ne!(new_page, compressed_diff);

        let applied = apply_compressed_diff(&original_page, compressed_diff, new_page.len());
        assert_eq!(new_page, applied)
    }

    #[test]
    fn test_compressed_diff_is_smaller_with_100_values_changed() {
        let original_page = get_random(4096);
        let mut new_page = original_page.clone();
        for i in 1..100 {
            new_page[500 + i] = 100 + i as u8;
        }

        let compressed_diff = get_compressed_diff(&new_page, &original_page);
        assert!(original_page.len() > compressed_diff.len())
    }

    #[test]
    fn test_compressed_diff_is_smaller_with_1000_values_changed() {
        let original_page = get_random(4096);
        let mut new_page = original_page.clone();
        for i in 1..1000 {
            new_page[i + 2 * i] = 100;
        }

        let compressed_diff = get_compressed_diff(&new_page, &original_page);
        assert!(original_page.len() > compressed_diff.len())
    }

    #[test]
    fn test_compressed_diff_is_big_with_almost_all_values_changed() {
        let original_page = get_random(4096);
        let new_page = get_random(4096);

        let compressed_diff = get_compressed_diff(&new_page, &original_page);
        // compressed diff is larger than the new page
        assert!(original_page.len() < compressed_diff.len())
    }
}
