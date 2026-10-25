#[derive(Clone, Debug)]
pub struct History<T> {
    snapshots: Vec<T>,
    current: usize,
    limit: usize,
}

impl<T: Clone> History<T> {
    pub fn new(initial: T, limit: usize) -> Self {
        Self {
            snapshots: vec![initial],
            current: 0,
            limit: limit.max(2),
        }
    }

    pub fn push(&mut self, value: T) {
        self.snapshots.truncate(self.current + 1);
        self.snapshots.push(value);
        if self.snapshots.len() > self.limit {
            self.snapshots.remove(0);
        } else {
            self.current += 1;
        }
        self.current = self.current.min(self.snapshots.len() - 1);
    }

    pub fn undo(&mut self) -> Option<T> {
        if self.current == 0 {
            return None;
        }
        self.current -= 1;
        self.snapshots.get(self.current).cloned()
    }

    pub fn redo(&mut self) -> Option<T> {
        if self.current + 1 >= self.snapshots.len() {
            return None;
        }
        self.current += 1;
        self.snapshots.get(self.current).cloned()
    }

    pub fn can_undo(&self) -> bool {
        self.current > 0
    }

    pub fn can_redo(&self) -> bool {
        self.current + 1 < self.snapshots.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branching_after_undo_discards_the_redo_branch() {
        let mut history = History::new(0, 10);
        history.push(1);
        history.push(2);
        assert_eq!(history.undo(), Some(1));
        history.push(3);
        assert!(!history.can_redo());
        assert_eq!(history.undo(), Some(1));
    }

    #[test]
    fn history_keeps_the_configured_number_of_snapshots() {
        let mut history = History::new(0, 3);
        history.push(1);
        history.push(2);
        history.push(3);
        assert_eq!(history.undo(), Some(2));
        assert_eq!(history.undo(), Some(1));
        assert_eq!(history.undo(), None);
    }
}
