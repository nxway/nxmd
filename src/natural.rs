//! Locale-independent Unicode lowercase keys with numeric runs (no integer overflow).
use std::cmp::Ordering;
#[derive(Clone, Eq, PartialEq)]
pub struct NaturalKey {
    folded: String,
    original: String,
}
impl NaturalKey {
    pub fn new(name: &str) -> Self {
        Self {
            folded: name
                .to_lowercase()
                .chars()
                .map(|c| digit(c).and_then(|d| char::from_digit(d, 10)).unwrap_or(c))
                .collect(),
            original: name.into(),
        }
    }
}
fn digit(c: char) -> Option<u32> {
    if c.is_ascii() {
        return c.to_digit(10);
    }
    // Unicode decimal digit blocks; all values in a block are consecutive 0..9.
    const ZERO: &[u32] = &[
        0x30, 0x660, 0x6f0, 0x7c0, 0x966, 0x9e6, 0xa66, 0xae6, 0xb66, 0xbe6, 0xc66, 0xce6, 0xd66,
        0xde6, 0xe50, 0xed0, 0xf20, 0x1040, 0x1090, 0x17e0, 0x1810, 0x1946, 0x19d0, 0x1a80, 0x1a90,
        0x1b50, 0x1bb0, 0x1c40, 0x1c50, 0xa620, 0xa8d0, 0xa900, 0xa9d0, 0xa9f0, 0xaa50, 0xabf0,
        0xff10, 0x104a0, 0x10d30, 0x11066, 0x110f0, 0x11136, 0x111d0, 0x112f0, 0x11450, 0x114d0,
        0x11650, 0x116c0, 0x11730, 0x118e0, 0x11950, 0x11c50, 0x11d50, 0x11da0, 0x11f50, 0x16a60,
        0x16ac0, 0x16b50, 0x1d7ce, 0x1d7d8, 0x1d7e2, 0x1d7ec, 0x1d7f6, 0x1e140, 0x1e2f0, 0x1e4f0,
        0x1e950, 0x1fbf0,
    ];
    ZERO.iter()
        .find_map(|&z| (c as u32).checked_sub(z).filter(|&n| n < 10))
}
fn compare(a: &str, b: &str) -> Ordering {
    let mut a = a.chars().peekable();
    let mut b = b.chars().peekable();
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (Some(x), Some(y)) if digit(x).is_some() && digit(y).is_some() => {
                let mut ar = Vec::new();
                let mut br = Vec::new();
                while let Some(n) = a.peek().and_then(|&c| digit(c)) {
                    ar.push(n);
                    a.next();
                }
                while let Some(n) = b.peek().and_then(|&c| digit(c)) {
                    br.push(n);
                    b.next();
                }
                let av = &ar[ar.iter().position(|&n| n != 0).unwrap_or(ar.len())..];
                let bv = &br[br.iter().position(|&n| n != 0).unwrap_or(br.len())..];
                let order = av.len().cmp(&bv.len()).then_with(|| av.cmp(bv));
                if order != Ordering::Equal {
                    return order;
                }
            }
            (Some(x), Some(y)) => {
                let order = x.cmp(&y);
                if order != Ordering::Equal {
                    return order;
                }
                a.next();
                b.next();
            }
            (x, y) => return x.cmp(&y),
        }
    }
}
impl Ord for NaturalKey {
    fn cmp(&self, other: &Self) -> Ordering {
        compare(&self.folded, &other.folded).then_with(|| self.original.cmp(&other.original))
    }
}
impl PartialOrd for NaturalKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn numeric_and_unicode() {
        let mut names = vec![
            "48. Millie",
            "5. St. Nora",
            "10. Folder",
            "2. Folder",
            "46. Folder",
            "9. Folder",
        ];
        names.sort_by_cached_key(|s| NaturalKey::new(s));
        assert_eq!(
            names,
            vec![
                "2. Folder",
                "5. St. Nora",
                "9. Folder",
                "10. Folder",
                "46. Folder",
                "48. Millie"
            ]
        );
        assert!(NaturalKey::new("École2") < NaturalKey::new("école10"));
        assert!(NaturalKey::new("日本語２") < NaturalKey::new("日本語１０"));
        assert!(NaturalKey::new("note٩") < NaturalKey::new("note١٠"));
        assert!(
            NaturalKey::new("n99999999999999999999") < NaturalKey::new("n100000000000000000000")
        );
    }
    #[test]
    fn total_order_ties() {
        let names = ["A2", "a2", "A02", "a02", "a0002"];
        let mixed = ["20", "a", "٣", "９", "١٠"];
        for a in mixed {
            for b in mixed {
                for c in mixed {
                    let (a, b, c) = (NaturalKey::new(a), NaturalKey::new(b), NaturalKey::new(c));
                    if a < b && b < c {
                        assert!(a < c);
                    }
                }
            }
        }
        for a in names {
            for b in names {
                assert_eq!(
                    NaturalKey::new(a).cmp(&NaturalKey::new(b)),
                    NaturalKey::new(b).cmp(&NaturalKey::new(a)).reverse()
                );
            }
        }
        let mut sorted = names;
        sorted.sort_by_cached_key(|s| NaturalKey::new(s));
        assert_eq!(sorted, ["A02", "A2", "a0002", "a02", "a2"]);
    }
}
