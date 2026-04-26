use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct RingBuf<T> {
    cap: usize,
    data: VecDeque<T>,
}

impl<T> RingBuf<T> {
    pub fn new(cap: usize) -> Self {
        Self { cap, data: VecDeque::with_capacity(cap) }
    }

    pub fn push(&mut self, item: T) {
        if self.data.len() >= self.cap {
            self.data.pop_front();
        }
        self.data.push_back(item);
    }

    pub fn clear(&mut self) {
        self.data.clear();
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.data.iter()
    }
}
