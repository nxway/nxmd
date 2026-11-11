/// Preserve the active document when another tab closes; prefer the right
/// neighbor when closing the active tab, or the left neighbor at the end.
pub fn active_after_close(active: usize, closing: usize, len: usize) -> usize {
    if len <= 1 {
        0
    } else if closing < active {
        active - 1
    } else {
        active.min(len - 2)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn close_target_keeps_selection() {
        assert_eq!(active_after_close(2, 0, 4), 1);
        assert_eq!(active_after_close(0, 2, 4), 0);
        assert_eq!(active_after_close(1, 1, 4), 1);
        assert_eq!(active_after_close(3, 3, 4), 2);
        assert_eq!(active_after_close(0, 0, 1), 0);
    }
}
