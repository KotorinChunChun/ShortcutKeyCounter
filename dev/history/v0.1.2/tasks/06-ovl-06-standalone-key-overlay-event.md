# OVL-06 通常の単独キーもOverlayへ送る

## 目的

`A`, `B`, `1`, `Enter`, `F5`, `Space`, `Backspace`, 矢印などの単独キーを、
DBへ記録せずOverlayへ送れる入力イベントとして扱う土台を作る。

このタスクではまだポリシーUIは作らない。

---

## 依存

OVL-01

---

## 最初に探すコード

```powershell
rg -n "is_recordable|normalize|CapturedInput|KeyEvent|shortcut|modifier|keyboard_hook" apps crates
rg -n "try_show|OverlayEvent" apps
```

---

## 問題

v0.1.1のOverlay通知位置が`is_recordable`の後なら、通常キーは手前で捨てられる。

v0.1.2ではOverlay用の入力表現をDB記録可否より前に生成する。

---

## Eventモデル拡張

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverlayInputKind {
    Shortcut,
    SingleKey,
}

#[derive(Debug, Clone)]
pub struct OverlayEvent {
    pub display_key: String,
    pub normalized_shortcut: Option<String>,
    pub application: String,
    pub source: CaptureSource,
    pub occurred_at: Instant,
    pub kind: OverlayInputKind,
}
```

### 例

`Ctrl+C`:

```rust
OverlayEvent {
    display_key: "Ctrl+C".into(),
    normalized_shortcut: Some("Ctrl+C".into()),
    kind: OverlayInputKind::Shortcut,
    // ...
}
```

`A`:

```rust
OverlayEvent {
    display_key: "A".into(),
    normalized_shortcut: None,
    kind: OverlayInputKind::SingleKey,
    // ...
}
```

---

## 単独キー正規化

OSのVirtual-Key値をそのまま表示しない。

```rust
pub fn normalize_single_key(vk: u32) -> Option<String> {
    match vk {
        0x41..=0x5A => char::from_u32(vk).map(|c| c.to_string()),
        0x30..=0x39 => char::from_u32(vk).map(|c| c.to_string()),
        VK_RETURN => Some("Enter".into()),
        VK_SPACE => Some("Space".into()),
        VK_BACK => Some("Backspace".into()),
        VK_ESCAPE => Some("Escape".into()),
        VK_TAB => Some("Tab".into()),
        VK_LEFT => Some("ArrowLeft".into()),
        VK_RIGHT => Some("ArrowRight".into()),
        VK_UP => Some("ArrowUp".into()),
        VK_DOWN => Some("ArrowDown".into()),
        VK_F1..=VK_F24 => Some(format!("F{}", vk - VK_F1 + 1)),
        _ => None,
    }
}
```

実際には既存keyboard key mapping関数があれば再利用する。

---

## 修飾キー単独

デフォルト方針:

```text
Ctrl単独
Shift単独
Alt単独
Win単独
```

は**単独キー表示対象外**を推奨。

理由:

- ショートカット入力中にノイズが増える
- `Ctrl`を押した後`C`で`Ctrl`, `Ctrl+C`の二重表示になる

実装:

```rust
fn is_modifier_only(key: &str) -> bool {
    matches!(key, "Ctrl" | "Shift" | "Alt" | "Win")
}
```

将来KeyPolicyで明示的に許可できる設計でもよい。

---

## Capture処理の概念

```rust
let app = resolve_foreground_application()?;

if excluded_apps.contains(&app) {
    return;
}

let overlay_candidate = build_overlay_candidate(&input);

if let Some(candidate) = overlay_candidate {
    overlay.try_send(candidate);
}

// 既存DB処理はこの後
if !is_recordable(&input) {
    return;
}

let shortcut = normalize_shortcut(&input)?;
// existing key mapping
// existing DB duplicate guard
// existing record
```

### 重要

`overlay.try_send()`はHook callbackをblockingしない。

---

## Local/Elevated dedup

通常キーでも同一物理入力がLocal/Elevated両方から届き得る。

既存50ms dedup keyを拡張する。

```rust
fn overlay_dedup_identity(event: &OverlayEvent) -> (&str, &str) {
    (&event.display_key, &event.application)
}
```

`normalized_shortcut == None`でもdedup可能にする。

---

## テスト

```rust
#[test]
fn single_letter_becomes_display_event_but_not_recordable_shortcut() {
    let input = test_key_down('A');

    let event = build_overlay_candidate(&input).unwrap();

    assert_eq!(event.kind, OverlayInputKind::SingleKey);
    assert_eq!(event.display_key, "A");
    assert!(event.normalized_shortcut.is_none());
}
```

---

## 完了条件

- 通常キーからOverlay candidateを生成可能
- DB recordabilityとは分離
- 単独修飾キーはデフォルト抑制
- 除外アプリでは表示しない
- Local/Elevated二重表示なし
- DB件数はこの変更前後で変わらない

---

## 禁止

- `A`をDBのshortcutとして保存しない
- 文字入力内容を組み立てて文章ログとして保持しない
- IME確定文字を取得する仕組みを追加しない
