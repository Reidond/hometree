use std::collections::BTreeSet;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct Debounce<T> {
    window: Duration,
    last_event: Option<Instant>,
    pending: BTreeSet<T>,
}

impl<T> Debounce<T>
where
    T: Ord + Clone,
{
    pub fn new(window: Duration) -> Self {
        Self {
            window,
            last_event: None,
            pending: BTreeSet::new(),
        }
    }

    pub fn push(&mut self, value: T, now: Instant) {
        self.last_event = Some(now);
        self.pending.insert(value);
    }

    pub fn is_due(&self, now: Instant) -> bool {
        match self.last_event {
            Some(last) => now.duration_since(last) >= self.window,
            None => false,
        }
    }

    pub fn drain(&mut self) -> Vec<T> {
        let items: Vec<T> = self.pending.iter().cloned().collect();
        self.pending.clear();
        self.last_event = None;
        items
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::Debounce;
    use std::time::{Duration, Instant};

    #[test]
    fn debounce_coalesces_and_flushes() {
        let mut debouncer = Debounce::new(Duration::from_millis(100));
        let start = Instant::now();
        debouncer.push("a", start);
        debouncer.push("b", start);
        assert!(!debouncer.is_due(start));
        let later = start + Duration::from_millis(100);
        assert!(debouncer.is_due(later));
        let drained = debouncer.drain();
        assert_eq!(drained, vec!["a", "b"]);
        assert!(debouncer.is_empty());
    }
}
