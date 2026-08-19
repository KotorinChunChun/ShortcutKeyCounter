# OVL-08 単独キー連続入力を同じ行の右へ追加

## 目的

単独キーが連続した場合、1キー1行にせず、同一行へ横方向に追加する。

例:

```text
A
B
C
```

ではなく:

```text
[A] [B] [C]
```

ショートカットが来たら単独キー列を閉じる。

---

## 依存

OVL-06, OVL-07

---

## Event / Itemモデル

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverlayRowKind {
    Shortcut,
    SingleKeySequence,
}

pub struct OverlayItem {
    pub id: u64,
    pub kind: OverlayRowKind,
    pub tokens: Vec<String>,
    pub created_at: Instant,
    pub last_input_at: Instant,
    pub phase: OverlayPhase,
}
```

ショートカット:

```rust
tokens = vec!["Ctrl+C".into()]
kind = Shortcut
```

通常キー列:

```rust
tokens = vec!["A", "B", "C"]
kind = SingleKeySequence
```

---

## Aggregator

```rust
const SINGLE_KEY_SEQUENCE_GAP: Duration = Duration::from_millis(700);
const MAX_SINGLE_KEY_TOKENS: usize = 20;

pub fn append_or_create_single_key(
    items: &mut VecDeque<OverlayItem>,
    key: String,
    now: Instant,
    next_id: u64,
) {
    if let Some(last) = items.back_mut() {
        let can_append =
            last.kind == OverlayRowKind::SingleKeySequence
            && now.duration_since(last.last_input_at) <= SINGLE_KEY_SEQUENCE_GAP
            && last.tokens.len() < MAX_SINGLE_KEY_TOKENS;

        if can_append {
            last.tokens.push(key);
            last.last_input_at = now;
            last.phase = OverlayPhase::Stable;
            return;
        }
    }

    items.push_back(OverlayItem {
        id: next_id,
        kind: OverlayRowKind::SingleKeySequence,
        tokens: vec![key],
        created_at: now,
        last_input_at: now,
        phase: OverlayPhase::Entering,
    });
}
```

---

## ショートカットで列を閉じる

ショートカットは必ず新規行。

```rust
fn push_shortcut(
    items: &mut VecDeque<OverlayItem>,
    shortcut: String,
    now: Instant,
    next_id: u64,
) {
    items.push_back(OverlayItem {
        id: next_id,
        kind: OverlayRowKind::Shortcut,
        tokens: vec![shortcut],
        created_at: now,
        last_input_at: now,
        phase: OverlayPhase::Entering,
    });
}
```

次の単独キーは新しい行になる。

---

## TTL

単独キー列へ追加時は、その行のTTLを延長する。

Phase判定基準:

```rust
pub fn item_timing_origin(item: &OverlayItem) -> Instant {
    match item.kind {
        OverlayRowKind::Shortcut => item.created_at,
        OverlayRowKind::SingleKeySequence => item.last_input_at,
    }
}
```

これにより:

```text
A
0.2s B
0.3s C
```

ならCからdisplay durationを再カウント。

---

## 横幅上限

長時間タイピングでWindowが横へ無限拡張しないよう制限する。

推奨:

```text
MAX_SINGLE_KEY_TOKENS = 20
```

または描画後幅で:

```text
max_row_width = min(work_area_width * 0.8, 1200px scaled)
```

上限到達時は**新しい単独キー行を開始**。

古いtokenを消して同じ行を使い続けない。

---

## Renderer

```rust
fn layout_single_key_sequence(tokens: &[String]) -> RowLayout {
    let mut x = 0.0;
    let mut keys = Vec::new();

    for token in tokens {
        let key = layout_keycap(token, x);
        x += key.width + SINGLE_KEY_GAP;
        keys.push(key);
    }

    RowLayout { keys, width: x, /* ... */ }
}
```

同じキーでも位置が異なるため、shortcut最終bitmap cacheと完全共有できない場合がある。

キーキャップ単位cacheは再利用する。

---

## Unit Test

```rust
#[test]
fn consecutive_single_keys_append_to_same_row() {
    let t0 = Instant::now();
    let mut items = VecDeque::new();

    append_or_create_single_key(&mut items, "A".into(), t0, 1);
    append_or_create_single_key(
        &mut items,
        "B".into(),
        t0 + Duration::from_millis(100),
        2,
    );

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].tokens, vec!["A", "B"]);
}

#[test]
fn shortcut_breaks_single_key_sequence() {
    // A B / Ctrl+C / D
    // -> 3 rows
}
```

---

## 完了条件

- 連続単独キーは横へ追加
- shortcutは独立行
- shortcut後の単独キーは新規行
- token追加でTTL延長
- Window幅が無限拡張しない
- DBへの影響なし

---

## 禁止

- 入力文字列を文章としてファイル保存しない
- Unicode/IME text capture機能へ拡張しない
- Password field判定など別仕様を勝手に入れない
