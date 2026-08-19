use std::collections::HashMap;
use std::time::{Duration, Instant};

/// 同じ正規化済みshortcutの短時間重複を抑制する。
#[derive(Debug)]
pub struct DuplicateGuard {
    last_recorded: HashMap<String, Instant>,
    threshold: Duration,
}

impl Default for DuplicateGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl DuplicateGuard {
    /// 仕様で定めた1秒閾値のguardを作成する。
    pub fn new() -> Self {
        Self::with_threshold(Duration::from_secs(1))
    }

    /// テストや用途に応じた閾値でguardを作成する。
    pub fn with_threshold(threshold: Duration) -> Self {
        Self {
            last_recorded: HashMap::new(),
            threshold,
        }
    }

    /// 実行中の設定変更を次イベントから反映する。
    pub fn set_threshold(&mut self, threshold: Duration) {
        self.threshold = threshold;
        if threshold.is_zero() {
            self.last_recorded.clear();
        }
    }

    pub fn threshold(&self) -> Duration {
        self.threshold
    }

    /// DB commit前に、記録可能かどうかだけを判定する。
    pub fn should_accept(&self, shortcut: &str, now: Instant) -> bool {
        self.last_recorded
            .get(shortcut)
            .map(|last| now.checked_duration_since(*last).unwrap_or_default() >= self.threshold)
            .unwrap_or(true)
    }

    /// DB commit成功後の時刻を記録する。
    pub fn mark_recorded(&mut self, shortcut: String, committed_at: Instant) {
        self.last_recorded.insert(shortcut, committed_at);
    }

    /// 計測再開時などに重複状態を破棄する。
    pub fn clear(&mut self) {
        self.last_recorded.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_event_is_accepted() {
        let guard = DuplicateGuard::new();
        assert!(guard.should_accept("Ctrl+C", Instant::now()));
    }

    #[test]
    fn event_within_half_second_is_rejected_after_mark() {
        let mut guard = DuplicateGuard::new();
        let start = Instant::now();
        guard.mark_recorded("Ctrl+C".to_owned(), start);

        assert!(!guard.should_accept("Ctrl+C", start + Duration::from_millis(500)));
    }

    #[test]
    fn event_at_one_second_is_accepted() {
        let mut guard = DuplicateGuard::new();
        let start = Instant::now();
        guard.mark_recorded("Ctrl+C".to_owned(), start);

        assert!(guard.should_accept("Ctrl+C", start + Duration::from_secs(1)));
    }

    #[test]
    fn different_shortcut_is_accepted_immediately() {
        let mut guard = DuplicateGuard::new();
        let start = Instant::now();
        guard.mark_recorded("Ctrl+C".to_owned(), start);

        assert!(guard.should_accept("Ctrl+V", start + Duration::from_millis(100)));
    }

    #[test]
    fn zero_threshold_disables_duplicate_suppression() {
        let mut guard = DuplicateGuard::new();
        let start = Instant::now();
        guard.mark_recorded("Ctrl+C".to_owned(), start);
        guard.set_threshold(Duration::ZERO);
        assert!(guard.should_accept("Ctrl+C", start));
    }

    #[test]
    fn clear_allows_same_shortcut_again() {
        let mut guard = DuplicateGuard::new();
        let start = Instant::now();
        guard.mark_recorded("Ctrl+C".to_owned(), start);
        guard.clear();

        assert!(guard.should_accept("Ctrl+C", start + Duration::from_millis(100)));
    }
}
