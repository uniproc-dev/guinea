use std::collections::VecDeque;

#[derive(Clone, Debug, PartialEq)]
pub struct RingSeries {
    capacity: usize,
    points: VecDeque<(u64, f32)>,
}

impl RingSeries {
    pub fn new(capacity: usize) -> Self {
        Self { capacity: capacity.max(1), points: VecDeque::with_capacity(capacity) }
    }

    pub fn push(&mut self, point: (u64, f32)) {
        if self.points.len() >= self.capacity {
            self.points.pop_front();
        }
        self.points.push_back(point);
    }

    pub fn len(&self) -> usize {
        self.points.len()
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// Snapshot for `Series::points` - a chart's render/hit-test logic
    /// works over a contiguous slice, not a deque.
    pub fn as_points(&self) -> Vec<(u64, f32)> {
        self.points.iter().copied().collect()
    }
}

impl Default for RingSeries {
    fn default() -> Self {
        Self::new(120)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_under_capacity_keeps_everything_in_order() {
        let mut ring = RingSeries::new(3);
        ring.push((1, 1.0));
        ring.push((2, 2.0));

        assert_eq!(ring.as_points(), vec![(1, 1.0), (2, 2.0)]);
    }

    #[test]
    fn push_past_capacity_evicts_the_oldest_point() {
        let mut ring = RingSeries::new(3);
        ring.push((1, 1.0));
        ring.push((2, 2.0));
        ring.push((3, 3.0));
        ring.push((4, 4.0));

        assert_eq!(ring.as_points(), vec![(2, 2.0), (3, 3.0), (4, 4.0)]);
        assert_eq!(ring.len(), 3);
    }
}
