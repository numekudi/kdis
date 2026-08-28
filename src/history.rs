use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

const MAX_DISPLAY_DURATION_MS: u128 = 9_999;

/// Stable identity for a physical key during one press/release cycle.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct KeyId(String);

impl KeyId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

/// A key label within an active-key snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisplayKey {
    pub label: String,
    sort_code: u32,
}

/// One stable keyboard state. Its timer ends at the next state transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InputRow {
    pub id: u64,
    keys: Vec<DisplayKey>,
    pub started_at: Instant,
    pub duration: Option<Duration>,
}

impl InputRow {
    pub fn keys(&self) -> impl Iterator<Item = &DisplayKey> {
        self.keys.iter()
    }

    pub fn elapsed_at(&self, now: Instant) -> Duration {
        self.duration
            .unwrap_or_else(|| now.saturating_duration_since(self.started_at))
    }

    /// Keeps the overlay width stable even during very long held or idle states.
    pub fn display_millis_at(&self, now: Instant) -> u128 {
        self.elapsed_at(now)
            .as_millis()
            .min(MAX_DISPLAY_DURATION_MS)
    }

    pub fn is_current(&self) -> bool {
        self.duration.is_none()
    }
}

/// Input events are timestamped at the listener boundary to keep timing precise.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum KeySignal {
    Pressed {
        key: KeyId,
        label: String,
        sort_code: u32,
        at: Instant,
    },
    Released {
        key: KeyId,
        at: Instant,
    },
}

/// Records each change to the complete set of physically held keys.
pub struct KeyHistory {
    rows: VecDeque<InputRow>,
    active: HashMap<KeyId, DisplayKey>,
    next_id: u64,
    capacity: usize,
}

impl KeyHistory {
    pub fn new(capacity: usize, started_at: Instant) -> Self {
        assert!(capacity > 0, "history capacity must be positive");
        let mut rows = VecDeque::with_capacity(capacity);
        // An empty snapshot makes time with no held keys visible from startup.
        rows.push_front(InputRow {
            id: 0,
            keys: Vec::new(),
            started_at,
            duration: None,
        });
        Self {
            rows,
            active: HashMap::new(),
            next_id: 1,
            capacity,
        }
    }

    pub fn apply(&mut self, signal: KeySignal) {
        match signal {
            KeySignal::Pressed {
                key,
                label,
                sort_code,
                at,
            } => self.press(key, label, sort_code, at),
            KeySignal::Released { key, at } => self.release(&key, at),
        }
    }

    pub fn rows(&self) -> impl Iterator<Item = &InputRow> {
        self.rows.iter()
    }

    fn press(&mut self, key: KeyId, label: String, sort_code: u32, at: Instant) {
        // Key-repeat does not change the physical-key state.
        if self.active.contains_key(&key) {
            return;
        }

        self.finish_current_row(at);
        self.active.insert(key, DisplayKey { label, sort_code });
        self.push_active_snapshot(at);
    }

    fn release(&mut self, key: &KeyId, at: Instant) {
        if !self.active.contains_key(key) {
            return;
        }

        self.finish_current_row(at);
        self.active.remove(key);

        // An empty active set is an explicit idle state, not missing history.
        self.push_active_snapshot(at);
    }

    fn finish_current_row(&mut self, at: Instant) {
        let Some(row) = self.rows.front_mut().filter(|row| row.is_current()) else {
            return;
        };
        row.duration = Some(at.saturating_duration_since(row.started_at));
    }

    fn push_active_snapshot(&mut self, at: Instant) {
        let mut keys = self.active.values().cloned().collect::<Vec<_>>();
        keys.sort_by_key(|key| key.sort_code);

        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.rows.push_front(InputRow {
            id,
            keys,
            started_at: at,
            duration: None,
        });
        self.rows.truncate(self.capacity);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(key: &str, sort_code: u32, at: Instant) -> KeySignal {
        KeySignal::Pressed {
            key: KeyId::new(key),
            label: key.to_uppercase(),
            sort_code,
            at,
        }
    }

    fn release(key: &str, at: Instant) -> KeySignal {
        KeySignal::Released {
            key: KeyId::new(key),
            at,
        }
    }

    fn row_labels(history: &KeyHistory) -> Vec<String> {
        let mut labels = history
            .rows()
            .map(|row| row.keys().map(|key| key.label.as_str()).collect())
            .collect::<Vec<_>>();
        labels.reverse();
        labels
    }

    #[test]
    fn held_j_and_repeated_k_produce_each_keyboard_state() {
        let start = Instant::now();
        let mut history = KeyHistory::new(10, start);
        history.apply(press("j", 36, start));
        history.apply(press("k", 37, start + Duration::from_millis(10)));
        history.apply(release("k", start + Duration::from_millis(20)));
        history.apply(press("k", 37, start + Duration::from_millis(30)));
        history.apply(release("k", start + Duration::from_millis(40)));

        assert_eq!(row_labels(&history), ["", "J", "JK", "J", "JK", "J"]);
        let rows = history.rows().collect::<Vec<_>>();
        assert!(rows[0].is_current());
        assert!(rows[1..].iter().all(|row| !row.is_current()));
    }

    #[test]
    fn every_state_duration_ends_at_its_next_transition() {
        let start = Instant::now();
        let mut history = KeyHistory::new(5, start);
        history.apply(press("j", 36, start));
        history.apply(press("k", 37, start + Duration::from_millis(20)));
        history.apply(release("k", start + Duration::from_millis(55)));

        let rows = history.rows().collect::<Vec<_>>();
        assert_eq!(rows[1].duration, Some(Duration::from_millis(35)));
        assert_eq!(rows[2].duration, Some(Duration::from_millis(20)));
    }

    #[test]
    fn keys_in_each_snapshot_are_sorted_by_key_code() {
        let start = Instant::now();
        let mut history = KeyHistory::new(3, start);
        history.apply(press("k", 37, start));
        history.apply(press("j", 36, start + Duration::from_millis(1)));

        assert_eq!(row_labels(&history), ["", "K", "JK"]);
    }

    #[test]
    fn releasing_the_final_key_starts_an_idle_row() {
        let start = Instant::now();
        let mut history = KeyHistory::new(3, start);
        history.apply(press("j", 36, start));
        history.apply(release("j", start + Duration::from_millis(42)));

        let rows = history.rows().collect::<Vec<_>>();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].keys().count(), 0);
        assert!(rows[0].is_current());
        assert_eq!(rows[1].duration, Some(Duration::from_millis(42)));
    }

    #[test]
    fn repeated_press_does_not_create_another_state() {
        let start = Instant::now();
        let mut history = KeyHistory::new(3, start);
        for offset in [0, 20, 40] {
            history.apply(press("j", 36, start + Duration::from_millis(offset)));
        }

        assert_eq!(history.rows().count(), 2);
    }

    #[test]
    fn active_state_survives_old_rows_leaving_the_history() {
        let start = Instant::now();
        let mut history = KeyHistory::new(2, start);
        history.apply(press("j", 36, start));
        for offset in [10, 30, 50] {
            history.apply(press("k", 37, start + Duration::from_millis(offset)));
            history.apply(release("k", start + Duration::from_millis(offset + 10)));
        }
        history.apply(release("j", start + Duration::from_millis(80)));

        let rows = history.rows().collect::<Vec<_>>();
        assert!(rows[0].is_current());
        assert_eq!(rows[0].keys().count(), 0);
        assert!(!rows[1].is_current());
        assert_eq!(rows[1].duration, Some(Duration::from_millis(20)));
    }

    #[test]
    fn display_time_is_capped_at_9999_milliseconds() {
        let start = Instant::now();
        let history = KeyHistory::new(1, start);
        let idle_row = history.rows().next().unwrap();

        assert_eq!(
            idle_row.display_millis_at(start + Duration::from_secs(30)),
            9_999
        );
    }
}
