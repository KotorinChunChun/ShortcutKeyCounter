use std::time::Instant;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OverlayCaptureSource {
    Local,
    Elevated,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OverlayInputKind {
    Shortcut,
    SingleKey,
}

#[derive(Clone, Debug)]
pub struct OverlayEvent {
    pub display_key: String,
    pub normalized_shortcut: Option<String>,
    pub application: String,
    pub source: OverlayCaptureSource,
    pub occurred_at: chrono::NaiveDateTime,
    pub received_at: Instant,
    pub kind: OverlayInputKind,
    /// trueなら新しい行、falseなら直前の行へ続けて表示する。
    pub display_break: bool,
}

impl OverlayEvent {
    pub fn identity(&self) -> &str {
        self.normalized_shortcut
            .as_deref()
            .unwrap_or(self.display_key.as_str())
    }
}
