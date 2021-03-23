pub(crate) trait ArrayIndex {
    fn normalize(self, len: i64) -> usize;
}

impl ArrayIndex for i64 {
    fn normalize(self, len: i64) -> usize {
        let index = if self < 0 {
            len - len.min(-self)
        } else {
            (len - 1).min(self).max(0)
        };
        index as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }
}
