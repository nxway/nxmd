//! Allocate table columns from measured min-content/preferred widths.
//! If words cannot fit, keep min-content widths and let the table scroll.
pub(crate) fn column_widths(
    minimum: &[f32],
    preferred: &[f32],
    available: Option<f32>,
) -> Vec<f32> {
    assert_eq!(minimum.len(), preferred.len());
    let preferred: Vec<_> = minimum
        .iter()
        .zip(preferred)
        .map(|(&m, &p)| p.max(m))
        .collect();
    let min_total: f32 = minimum.iter().sum();
    let preferred_total: f32 = preferred.iter().sum();
    let Some(available) = available else {
        return preferred;
    };
    if preferred_total <= available {
        return preferred;
    }
    if min_total >= available {
        return minimum.to_vec();
    }
    let share = (available - min_total) / (preferred_total - min_total);
    minimum
        .iter()
        .zip(preferred)
        .map(|(&m, p)| m + (p - m) * share)
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn uses_available_space_without_crushing_words() {
        let widths = column_widths(&[60., 100., 80.], &[80., 700., 100.], Some(600.));
        assert!((widths.iter().sum::<f32>() - 600.).abs() < 0.01);
        assert!(widths[1] > 400.);
        assert!(widths.iter().zip([60., 100., 80.]).all(|(&w, m)| w >= m));
    }
    #[test]
    fn narrow_tables_overflow_and_wide_windows_keep_intrinsic_sizes() {
        assert_eq!(
            column_widths(&[80., 120.], &[160., 400.], Some(100.)),
            vec![80., 120.]
        );
        assert_eq!(
            column_widths(&[80., 120.], &[160., 400.], Some(1000.)),
            vec![160., 400.]
        );
        assert_eq!(
            column_widths(&[160., 120.], &[160., 400.], Some(360.)),
            vec![160., 200.]
        );
    }
}
