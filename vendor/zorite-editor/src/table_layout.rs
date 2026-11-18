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
/// Caret geometry for table cells, with downstream affinity at a soft wrap.
pub(crate) fn cell_position(
    line: &gpui::WrappedLineLayout,
    index: usize,
    line_height: gpui::Pixels,
) -> gpui::Point<gpui::Pixels> {
    for (row, boundary) in line.wrap_boundaries.iter().enumerate() {
        let glyph = &line.unwrapped_layout.runs[boundary.run_ix].glyphs[boundary.glyph_ix];
        if glyph.index == index {
            return gpui::point(gpui::px(0.), line_height * (row + 1));
        }
    }
    line.position_for_index(index, line_height)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn caret_at_wrap_start_matches_hit_test_visual_row() {
        use gpui::{
            FontId, GlyphId, LineLayout, ShapedGlyph, ShapedRun, WrapBoundary, WrappedLineLayout,
            point, px,
        };
        let line = WrappedLineLayout {
            unwrapped_layout: std::sync::Arc::new(LineLayout {
                width: px(60.),
                len: 6,
                runs: vec![ShapedRun {
                    font_id: FontId(0),
                    glyphs: (0..6)
                        .map(|i| ShapedGlyph {
                            id: GlyphId(0),
                            index: i,
                            position: point(px(i as f32 * 10.), px(0.)),
                            is_emoji: false,
                        })
                        .collect(),
                }],
                ..Default::default()
            }),
            wrap_width: Some(px(20.)),
            wrap_boundaries: vec![
                WrapBoundary {
                    run_ix: 0,
                    glyph_ix: 2,
                },
                WrapBoundary {
                    run_ix: 0,
                    glyph_ix: 4,
                },
            ]
            .into(),
        };
        for row in 0..3 {
            let target = point(px(0.), px(row as f32 * 24. + 1.));
            let source = line.closest_index_for_position(target, px(24.)).unwrap();
            assert_eq!(source, row * 2);
            assert_eq!(
                cell_position(&line, source, px(24.)),
                point(px(0.), px(row as f32 * 24.))
            );
        }
    }

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
