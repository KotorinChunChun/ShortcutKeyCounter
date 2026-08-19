# OVL-10 Overlay最大表示行数を可変化

## 目的

固定3行を設定値`max_rows`へ変更する。

範囲:

```text
1〜10
```

---

## 依存

OVL-01

---

## 最初に探すコード

```powershell
rg -n "max_items|MAX_ITEMS|3usize|items.len|pop_front|VecDeque" apps/shortcut-key-counter/src/overlay
```

---

## サンプル実装

```rust
fn enforce_max_rows(
    items: &mut VecDeque<OverlayItem>,
    max_rows: usize,
) {
    let max_rows = max_rows.clamp(1, 10);

    while items.len() > max_rows {
        items.pop_front();
    }
}
```

新規追加後:

```rust
items.push_back(item);
enforce_max_rows(&mut items, config.max_rows);
```

---

## 設定を3→1へ変更した場合

設定reload直後に現在表示中のItemにも適用する。

```rust
pub fn apply_config(
    &mut self,
    new_config: OverlayConfig,
) {
    self.config = new_config;
    enforce_max_rows(&mut self.items, self.config.max_rows);
    self.invalidate_layout();
}
```

3→5では既存Itemを復活させない。

---

## transient exit item

v0.1.1に「最古itemの短い退場用transient」がある場合、
`max_rows`判定へそのtransientを数えない。

```text
logical rows <= max_rows
+
optional transient exit animation
```

とする。

---

## Window高さ

`max_rows`そのものから固定高さを作らず、実際のlogical rowsから計算する。

```rust
let row_count = items.len();
let height =
    padding_top
    + padding_bottom
    + row_count as i32 * row_height
    + row_count.saturating_sub(1) as i32 * item_gap;
```

単独キー列も1rowと数える。

---

## Unit Test

```rust
#[test]
fn max_rows_discards_oldest_logical_row() {
    let mut items = VecDeque::from([
        item(1),
        item(2),
        item(3),
        item(4),
    ]);

    enforce_max_rows(&mut items, 3);

    assert_eq!(items.len(), 3);
    assert_eq!(items.front().unwrap().id, 2);
}
```

---

## 完了条件

- 1〜10
- default=3
- 4件目の旧挙動をmax_rowsで一般化
- 設定縮小時に即時trim
- 単独キーSequenceは1行扱い
- Windowサイズが内容に追従

---

## 禁止

- 行数上限を解除しない
- 全履歴をOverlay stateへ溜め続けない
