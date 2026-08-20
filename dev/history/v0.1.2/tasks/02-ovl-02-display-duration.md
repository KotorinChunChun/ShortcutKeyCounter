# OVL-02 オーバーレイ表示時間を設定可能にする

## 目的

Overlay Itemの安定表示時間を固定値から設定値へ変更する。

---

## 依存

OVL-01

---

## 最初に探すコード

```powershell
rg -n "840|1_000|1_400|hold_ms|created_at|OverlayPhase|Stable|Fading" apps/shortcut-key-counter
```

---

## 仕様

```text
enter animation
↓
display_duration_ms
↓
exit animation
```

例:

```text
enter_ms = 160
display_duration_ms = 1500
exit_ms = 300

総存在時間 = 1960ms
```

`display_duration_ms` は「完全表示している時間」とする。

---

## サンプル実装

### Phase deadlineの計算

```rust
use std::time::{Duration, Instant};

pub struct OverlayTiming {
    pub enter: Duration,
    pub hold: Duration,
    pub exit: Duration,
}

impl OverlayTiming {
    pub fn from_config(cfg: &OverlayConfig) -> Self {
        Self {
            enter: Duration::from_millis(cfg.animation.enter_ms),
            hold: Duration::from_millis(cfg.display_duration_ms),
            exit: Duration::from_millis(cfg.animation.exit_ms),
        }
    }

    pub fn stable_at(&self, created_at: Instant) -> Instant {
        created_at + self.enter
    }

    pub fn exit_at(&self, created_at: Instant) -> Instant {
        created_at + self.enter + self.hold
    }

    pub fn remove_at(&self, created_at: Instant) -> Instant {
        created_at + self.enter + self.hold + self.exit
    }
}
```

### Phase判定

```rust
pub fn phase_at(
    now: Instant,
    created_at: Instant,
    timing: &OverlayTiming,
) -> Option<OverlayPhase> {
    if now < timing.stable_at(created_at) {
        Some(OverlayPhase::Entering)
    } else if now < timing.exit_at(created_at) {
        Some(OverlayPhase::Stable)
    } else if now < timing.remove_at(created_at) {
        Some(OverlayPhase::Fading)
    } else {
        None
    }
}
```

---

## 単独キー横連結との関係

OVL-08で同一行にキーが追加された場合は、その行の寿命を延長する。

そのためOverlayItemに次のような値を持たせられる構造にしておく。

```rust
pub struct OverlayItem {
    // ...
    pub created_at: Instant,
    pub last_input_at: Instant,
}
```

通常ショートカット:

```text
created_at == last_input_at
```

単独キー行:

```text
キー追加のたびに last_input_at を更新
```

OVL-02では既存Itemをすべてリセットする挙動はまだ入れない。

---

## Unit Test

時間比較はsleepではなく固定Instantで行う。

```rust
#[test]
fn timing_uses_configured_hold_time() {
    let mut cfg = OverlayConfig::default();
    cfg.display_duration_ms = 1500;
    cfg.animation.enter_ms = 200;
    cfg.animation.exit_ms = 300;

    let timing = OverlayTiming::from_config(&cfg);
    let t0 = Instant::now();

    assert_eq!(
        timing.remove_at(t0).duration_since(t0),
        Duration::from_millis(2000)
    );
}
```

---

## 完了条件

- hard-coded 840msをOverlay Configへ置換
- 100〜10000msの範囲で反映
- アイドル時timer停止を維持
- 新しい設定を反映するためにOverlay threadを再生成しない
- 設定reload後の次Itemから新設定が使用される

---

## 禁止

- DB DuplicateGuard 1秒を変更しない
- Local/Elevated 50ms dedupを変更しない
- `std::thread::sleep()`によるblocking animationを入れない
