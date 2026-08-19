# OVL-05 Overlay背景色・透明度

## 目的

キーキャップの背後にOverlayコンテナ背景を描画し、全体の視認性を上げる。

---

## 依存

OVL-01

---

## 最初に探すコード

```powershell
rg -n "renderer|ARGB|premultipl|alpha|UpdateLayeredWindow|Rounded|key.svg|modifier.svg" apps crates
```

---

## 仕様

設定:

```json
"background": {
  "color": "#202020",
  "opacity": 0.8
}
```

Overlay全体:

```text
┌──────────────────────────────┐
│      [Ctrl] + [Shift] + [P]  │
│             [Ctrl] + [C]     │
└──────────────────────────────┘
```

コンテナ:

- 角丸
- 内側padding
- 背景色
- opacity
- 必要なら既存shadowを維持

---

## 色パース

```rust
#[derive(Debug, Clone, Copy)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

pub fn parse_hex_with_opacity(
    color: &str,
    opacity: f32,
) -> Option<Rgba> {
    let hex = color.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }

    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    let a = (opacity.clamp(0.0, 1.0) * 255.0).round() as u8;

    Some(Rgba { r, g, b, a })
}
```

---

## Premultiplied alpha

`UpdateLayeredWindow`側がpremultiplied alphaを期待する既存実装なら、必ず既存方式へ合わせる。

```rust
pub fn premultiply(c: Rgba) -> Rgba {
    let a = c.a as u16;
    Rgba {
        r: ((c.r as u16 * a + 127) / 255) as u8,
        g: ((c.g as u16 * a + 127) / 255) as u8,
        b: ((c.b as u16 * a + 127) / 255) as u8,
        a: c.a,
    }
}
```

---

## Rounded Rect描画の疑似コード

既存rendererがsoftware bitmapの場合:

```rust
renderer.fill_rounded_rect(
    Rect::new(0, 0, width, height),
    scaled_corner_radius,
    premultiply(background),
);
```

GDI/GDI+/Direct2D等の既存レイヤーを使っている場合は、その描画APIへ合わせる。

**新しい大きな描画ライブラリを追加しない。**

---

## Cache

背景色やopacityが変わったらshortcut cacheが古くなる可能性がある。

設定reload時:

```rust
asset_cache.keep_svg_templates();
shortcut_cache.clear();
request_redraw();
```

背景とキー描画が別レイヤーなら必要最小限のinvalidateにする。

---

## Unit Test

```rust
#[test]
fn parses_background_color() {
    let c = parse_hex_with_opacity("#202020", 0.8).unwrap();
    assert_eq!((c.r, c.g, c.b), (32, 32, 32));
    assert!((c.a as i32 - 204).abs() <= 1);
}
```

---

## 完了条件

- `#RRGGBB`を設定可能
- opacity 0.0〜1.0
- 不正色はdefault fallback
- キー文字・SVGのalphaを壊さない
- 背景を増やしてもWindow click-through維持
- 設定reloadで即時反映可能

---

## 禁止

- Layered Window全体のalphaだけを下げてキー文字まで薄くしない
- 透明度0でWindow hit-testを有効にしない
- WebView/CSS Overlayへ移行しない
