# OVL-04 Overlay表示位置の自動変更

## 目的

位置設定を`auto`にした場合、Overlayが一度完全に消えた次の表示セッションから位置をローテーションする。

---

## 依存

OVL-03

---

## 仕様

毎キー入力で位置を変えない。

```text
Session A
  Ctrl+C
  Ctrl+V
  Ctrl+Z
  → 全部同じ位置

全Item消滅

Session B
  A
  B
  C
  → 次の位置
```

初期ローテーション:

```text
top-left
→ top-center
→ top-right
→ middle-right
→ bottom-right
→ bottom-center
→ bottom-left
→ middle-left
→ center
→ top-left...
```

---

## 状態モデル

`OverlayManager`へ状態を追加。

```rust
pub struct AutoPositionState {
    index: usize,
    active_position: Option<OverlayPosition>,
}

const AUTO_POSITIONS: [OverlayPosition; 9] = [
    OverlayPosition::TopLeft,
    OverlayPosition::TopCenter,
    OverlayPosition::TopRight,
    OverlayPosition::MiddleRight,
    OverlayPosition::BottomRight,
    OverlayPosition::BottomCenter,
    OverlayPosition::BottomLeft,
    OverlayPosition::MiddleLeft,
    OverlayPosition::Center,
];
```

### セッション開始

```rust
impl AutoPositionState {
    pub fn begin_session(&mut self) -> OverlayPosition {
        if let Some(position) = self.active_position {
            return position;
        }

        let position = AUTO_POSITIONS[self.index % AUTO_POSITIONS.len()];
        self.active_position = Some(position);
        position
    }

    pub fn end_session(&mut self) {
        if self.active_position.take().is_some() {
            self.index = (self.index + 1) % AUTO_POSITIONS.len();
        }
    }
}
```

---

## Managerへの組込み

```rust
fn resolve_position(
    cfg: &OverlayConfig,
    auto: &mut AutoPositionState,
    has_items_before_event: bool,
) -> OverlayPosition {
    match cfg.position_mode {
        OverlayPositionMode::Fixed => cfg.fixed_position,
        OverlayPositionMode::Auto => {
            if !has_items_before_event {
                auto.begin_session()
            } else {
                auto.active_position.unwrap_or_else(|| auto.begin_session())
            }
        }
    }
}
```

全Itemが消滅した瞬間:

```rust
if state.items.is_empty() && had_items_before {
    auto_position.end_session();
    window.hide();
}
```

---

## 設定変更時

`auto -> fixed`:

```rust
auto_position.active_position = None;
```

`fixed -> auto`:

- 既存Itemがあれば現在セッション中はそのままでもよい。
- 次の空状態からauto開始。

挙動をテストで固定する。

---

## Unit Test

```rust
#[test]
fn auto_position_stays_fixed_during_session() {
    let mut state = AutoPositionState {
        index: 0,
        active_position: None,
    };

    let first = state.begin_session();
    let second = state.begin_session();

    assert_eq!(first, OverlayPosition::TopLeft);
    assert_eq!(second, OverlayPosition::TopLeft);

    state.end_session();

    assert_eq!(state.begin_session(), OverlayPosition::TopCenter);
}
```

---

## 完了条件

- 1セッション中に位置が飛ばない
- 全Item消滅後のみ次位置へ
- 9位置で循環
- fixed modeの挙動を変更しない
- マルチモニター時も「位置区分」は維持し、座標は現在モニターで再計算

---

## 禁止

- 毎キー入力でランダム移動
- 乱数依存
- 自動位置のための別タイマー追加
