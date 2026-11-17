/*
 * Copyright (c) 2006-Present, Redis Ltd.
 * All rights reserved.
 *
 * Licensed under your choice of (a) the Redis Source Available License 2.0
 * (RSALv2); or (b) the Server Side Public License v1 (SSPLv1); or (c) the
 * GNU Affero General Public License v3 (AGPLv3).
 */

pub(crate) fn insertion_index(index: i64, len: usize) -> redis_module::RedisResult<usize> {
    let len = len as i64;
    let index = if index < 0 { len + index } else { index };
    if !(0..=len).contains(&index) {
        return Err(redis_module::RedisError::Str("ERR index out of bounds"));
    }
    Ok(index as usize)
}

pub trait ArrayIndex {
    fn normalize(self, len: i64) -> usize;
}

impl ArrayIndex for i64 {
    fn normalize(self, len: i64) -> usize {
        let index = if self < 0 {
            len - len.min(self.saturating_neg())
        } else if len > 0 {
            (len - 1).min(self)
        } else {
            0
        };
        index as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insertion_index() {
        for (index, len, expected) in [(0, 0, 0), (3, 3, 3), (-1, 3, 2), (-3, 3, 0)] {
            assert_eq!(insertion_index(index, len).unwrap(), expected);
        }
        for (index, len) in [
            (1, 0),
            (-1, 0),
            (4, 3),
            (-4, 3),
            (i64::MIN, 3),
            (i64::MAX, 3),
        ] {
            assert!(insertion_index(index, len).is_err());
        }
    }

    #[test]
    fn test_index() {
        // [0,1,2,3,4]
        assert_eq!((-6).normalize(5), 0);
        assert_eq!((-5).normalize(5), 0);
        assert_eq!((-2).normalize(5), 3);
        assert_eq!((-1).normalize(5), 4);
        assert_eq!(0.normalize(5), 0);
        assert_eq!(1.normalize(5), 1);
        assert_eq!(4.normalize(5), 4);
        assert_eq!(5.normalize(5), 4);
        assert_eq!(6.normalize(5), 4);
        assert_eq!(0.normalize(0), 0);
        assert_eq!((-1).normalize(0), 0);
        assert_eq!(1.normalize(0), 0);
    }

    #[test]
    fn test_index_out_of_i64_range() {
        // `i64::MIN` cannot be negated, so it must not be negated unguarded
        assert_eq!(i64::MIN.normalize(5), 0);
        assert_eq!(i64::MIN.normalize(0), 0);
        assert_eq!((i64::MIN + 1).normalize(5), 0);
        assert_eq!((i64::MIN + 1).normalize(0), 0);
        assert_eq!(i64::MAX.normalize(5), 4);
        assert_eq!(i64::MAX.normalize(0), 0);
    }
}
