#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameBudget {
    remaining: usize,
}

impl FrameBudget {
    pub const fn new(limit: usize) -> Self {
        Self { remaining: limit }
    }

    pub const fn remaining(&self) -> usize {
        self.remaining
    }

    pub fn try_take(&mut self, amount: usize) -> bool {
        if amount > self.remaining {
            return false;
        }

        self.remaining -= amount;
        true
    }

    pub fn refill(&mut self, limit: usize) {
        self.remaining = limit;
    }
}
