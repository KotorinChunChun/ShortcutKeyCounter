use serde::{Deserialize, Serialize};

/// Overlayを配置する3×3グリッドの位置。
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum OverlayPosition {
    TopLeft,
    TopCenter,
    TopRight,
    MiddleLeft,
    Center,
    MiddleRight,
    BottomLeft,
    #[default]
    BottomCenter,
    BottomRight,
}
