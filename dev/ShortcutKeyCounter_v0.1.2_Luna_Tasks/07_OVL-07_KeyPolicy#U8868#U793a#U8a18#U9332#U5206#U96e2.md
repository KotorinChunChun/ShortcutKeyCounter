# OVL-07 キーごとの「無視 / 表示のみ / 表示＋記録」

## 目的

キー入力の表示と記録を分離する。

内部ポリシー:

```text
Ignore
DisplayOnly
DisplayAndRecord
```

---

## 依存

OVL-06

---

## 仕様

| Policy | Overlay | DB |
|---|---:|---:|
| Ignore | NO | NO |
| DisplayOnly | YES | NO |
| DisplayAndRecord | YES | YES |

### デフォルト

- 通常単独キー: `DisplayOnly`
- 従来記録対象ショートカット: `DisplayAndRecord`
- 現在無視されている入力: 原則`Ignore`

---

## 型

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KeyAction {
    Ignore,
    DisplayOnly,
    DisplayAndRecord,
}
```

---

## PolicyResolver

新規候補:

```text
apps/shortcut-key-counter/src/key_policy.rs
```

```rust
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct KeyPolicyConfig {
    pub default_single_key: KeyAction,
    pub default_shortcut: KeyAction,
    pub overrides: HashMap<String, KeyAction>,
}

impl Default for KeyPolicyConfig {
    fn default() -> Self {
        Self {
            default_single_key: KeyAction::DisplayOnly,
            default_shortcut: KeyAction::DisplayAndRecord,
            overrides: HashMap::new(),
        }
    }
}

impl KeyPolicyConfig {
    pub fn resolve(
        &self,
        identity: &str,
        kind: OverlayInputKind,
    ) -> KeyAction {
        if let Some(action) = self.overrides.get(identity) {
            return *action;
        }

        match kind {
            OverlayInputKind::SingleKey => self.default_single_key,
            OverlayInputKind::Shortcut => self.default_shortcut,
        }
    }
}
```

---

## JSON例

```json
{
  "version": 1,
  "default_single_key": "display_only",
  "default_shortcut": "display_and_record",
  "overrides": {
    "Escape": "ignore",
    "F5": "display_only",
    "Ctrl+C": "display_and_record",
    "Ctrl+V": "display_and_record"
  }
}
```

保存先候補:

```text
%LOCALAPPDATA%\ShortcutKeyCounter\overlay_key_policy.json
```

既存Key Mapping JSONと役割を混ぜない。

---

## Captureフロー

```rust
let candidate = build_overlay_candidate(&input)?;
let action = policy.resolve(candidate.identity(), candidate.kind);

match action {
    KeyAction::Ignore => {
        return;
    }

    KeyAction::DisplayOnly => {
        overlay.try_send(candidate);
        return;
    }

    KeyAction::DisplayAndRecord => {
        overlay.try_send(candidate.clone());

        // ここから既存recording path
        if let Some(shortcut) = candidate.normalized_shortcut.as_deref() {
            record_shortcut_existing_path(shortcut, &candidate.application)?;
        }
    }
}
```

### 注意

実際には`return`するとrealtime diagnostics等を飛ばす場合があるため、
既存pipelineに合わせてbooleanを分けたほうが安全。

推奨:

```rust
pub struct KeyDecision {
    pub display: bool,
    pub record: bool,
}
```

```rust
impl From<KeyAction> for KeyDecision {
    fn from(action: KeyAction) -> Self {
        match action {
            KeyAction::Ignore => Self {
                display: false,
                record: false,
            },
            KeyAction::DisplayOnly => Self {
                display: true,
                record: false,
            },
            KeyAction::DisplayAndRecord => Self {
                display: true,
                record: true,
            },
        }
    }
}
```

---

## 矛盾状態を作らない

UIでは「表示」と「記録」の2チェックに見せてもよいが、内部では次へ正規化する。

```text
display=0 record=0 → Ignore
display=1 record=0 → DisplayOnly
display=1 record=1 → DisplayAndRecord
display=0 record=1 → 禁止
```

`record=ON`へしたら自動的に`display=ON`にする。

---

## Unit Test

```rust
#[test]
fn single_key_defaults_to_display_only() {
    let cfg = KeyPolicyConfig::default();

    assert_eq!(
        cfg.resolve("A", OverlayInputKind::SingleKey),
        KeyAction::DisplayOnly
    );
}

#[test]
fn shortcut_defaults_to_display_and_record() {
    let cfg = KeyPolicyConfig::default();

    assert_eq!(
        cfg.resolve("Ctrl+C", OverlayInputKind::Shortcut),
        KeyAction::DisplayAndRecord
    );
}
```

---

## DB回帰試験

既存fixtureで、ポリシー導入前後のショートカット記録件数が同じこと。

追加:

```text
A DisplayOnly × 100回
→ DB 0件増加

Ctrl+C DisplayAndRecord
→ 従来通り記録

Escape Ignore
→ Overlayなし / DBなし
```

---

## 完了条件

- 3状態を実装
- 通常キーはdefault表示のみ
- 従来shortcutはdefault表示＋記録
- `DisplayOnly`がDBへ流れない
- 設定reload可能
- 既存key mapping / DB shortcut identityを変更しない

---

## 禁止

- 既存DBスキーマのshortcut意味を変更しない
- 表示名JSONをpolicy identityとして使わない
- Overlay label変更でpolicy keyが変わらないようにする
