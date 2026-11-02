//! Fixed-capacity ring buffer for short-lived metric history. Old values are
//! overwritten automatically, so memory use is bounded regardless of uptime.

use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct RingBuffer<T> {
    items: VecDeque<T>,
    capacity: usize,
}

impl<T> RingBuffer<T> {
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            items: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub fn push(&mut self, item: T) {
        if self.items.len() == self.capacity {
            self.items.pop_front();
        }
        self.items.push_back(item);
    }

    /// Items oldest first.
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &T> + ExactSizeIterator {
        self.items.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overwrites_oldest_when_full() {
        let mut rb = RingBuffer::new(3);
        for i in 0..5 {
            rb.push(i);
        }
        assert_eq!(rb.iter().copied().collect::<Vec<_>>(), vec![2, 3, 4]);
    }

    #[test]
    fn keeps_everything_below_capacity() {
        let mut rb = RingBuffer::new(10);
        (0..6).for_each(|i| rb.push(i));
        assert_eq!(rb.iter().len(), 6);
        assert_eq!(rb.iter().next_back(), Some(&5));
    }

    #[test]
    fn zero_capacity_is_clamped_to_one() {
        let mut rb = RingBuffer::new(0);
        rb.push(1);
        rb.push(2);
        assert_eq!(rb.iter().copied().collect::<Vec<_>>(), vec![2]);
    }
}
