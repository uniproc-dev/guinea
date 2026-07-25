use super::Series;


pub fn bounds(series: &[Series]) -> Option<(u64, u64, f32, f32)> {
    let mut points = series.iter().flat_map(|s| s.points.iter().copied());
    let (first_t, first_v) = points.next()?;
    let mut acc = (first_t, first_t, first_v, first_v);
    for (t, v) in points {
        acc.0 = acc.0.min(t);
        acc.1 = acc.1.max(t);
        acc.2 = acc.2.min(v);
        acc.3 = acc.3.max(v);
    }
    Some(acc)
}


pub fn nearest_point(points: &[(u64, f32)], target: u64) -> Option<(u64, f32)> {
    if points.is_empty() {
        return None;
    }

    let idx = points.partition_point(|&(t, _)| t < target);
    Some(match idx {
        0 => points[0],
        n if n == points.len() => points[n - 1],
        n => {
            let (before, after) = (points[n - 1], points[n]);
            if target - before.0 <= after.0 - target { before } else { after }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chart::Interpolation;
    use windows_canvas::ColorF;

    fn series(points: &[(u64, f32)]) -> Series {
        Series {
            color: ColorF::BLACK,
            interpolation: Interpolation::Linear,
            fill: None,
            points: points.to_vec(),
        }
    }

    #[test]
    fn bounds_spans_every_series() {
        let all = bounds(&[series(&[(0, 1.0), (10, 5.0)]), series(&[(5, -2.0), (20, 3.0)])]).unwrap();
        assert_eq!(all, (0, 20, -2.0, 5.0));
    }

    #[test]
    fn bounds_is_none_when_every_series_is_empty() {
        assert_eq!(bounds(&[series(&[]), series(&[])]), None);
    }

    #[test]
    fn nearest_point_picks_the_closer_neighbor() {
        let points = [(0, 0.0), (10, 1.0), (20, 2.0)];
        assert_eq!(nearest_point(&points, 4), Some((0, 0.0)));
        assert_eq!(nearest_point(&points, 6), Some((10, 1.0)));
        assert_eq!(nearest_point(&points, 10), Some((10, 1.0)));
    }

    #[test]
    fn nearest_point_clamps_to_the_ends() {
        let points = [(10, 0.0), (20, 1.0)];
        assert_eq!(nearest_point(&points, 0), Some((10, 0.0)));
        assert_eq!(nearest_point(&points, 100), Some((20, 1.0)));
    }

    #[test]
    fn nearest_point_of_empty_series_is_none() {
        assert_eq!(nearest_point(&[], 5), None);
    }
}
