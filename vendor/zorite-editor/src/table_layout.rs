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
    // Columns whose whole contents fit in an equal share (short labels,
    // numbers, units) get their small deficit first. Prose shares the rest.
    let fair = available / minimum.len() as f32;
    let compact_extra: f32 = minimum
        .iter()
        .zip(&preferred)
        .filter(|(_, p)| **p <= fair)
        .map(|(m, p)| p - m)
        .sum();
    let spare = available - min_total;
    let compact_share = if compact_extra > spare {
        spare / compact_extra
    } else {
        1.
    };
    let flexible_extra = preferred_total - min_total - compact_extra;
    let share = if flexible_extra > 0. {
        (spare - compact_extra).max(0.) / flexible_extra
    } else {
        0.
    };
    minimum
        .iter()
        .zip(preferred)
        .map(|(&m, p)| m + (p - m) * if p <= fair { compact_share } else { share })
        .collect()
}
/// Visit the selected horizontal interval on each visual row. Wrap boundaries
/// belong to the upstream row for an end and the downstream row for a start.
pub(crate) fn selection_bands(
    line: &gpui::WrappedLineLayout,
    range: std::ops::Range<usize>,
    mut emit: impl FnMut(usize, gpui::Pixels, gpui::Pixels),
) {
    let layout = &line.unwrapped_layout;
    let mut start = 0;
    for (row, end) in line
        .wrap_boundaries
        .iter()
        .map(|b| layout.runs[b.run_ix].glyphs[b.glyph_ix].index)
        .chain(std::iter::once(layout.len))
        .enumerate()
    {
        let a = range.start.max(start);
        let b = range.end.min(end);
        if a < b {
            let origin = layout.x_for_index(start);
            let x0 = layout.x_for_index(a) - origin;
            let x1 = layout.x_for_index(b) - origin;
            if x1 > x0 {
                emit(row, x0, x1);
            }
        }
        if end >= range.end {
            break;
        }
        start = end;
    }
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
        let mut bands = Vec::new();
        selection_bands(&line, 1..5, |row, left, right| {
            bands.push((row, left, right))
        });
        assert_eq!(
            bands,
            [
                (0, px(10.), px(20.)),
                (1, px(0.), px(20.)),
                (2, px(0.), px(10.))
            ]
        );
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
        assert_eq!((widths[0], widths[2]), (80., 100.));
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
