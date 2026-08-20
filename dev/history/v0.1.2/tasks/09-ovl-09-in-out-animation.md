# OVL-09 IN/OUTアニメーションを独立選択

## 目的

Overlayの登場・退場アニメーションを別々に選択できるようにする。

---

## 依存

OVL-01

---

## IN候補

```text
none
fade
slide-up
slide-down
slide-left
slide-right
pop
```

## OUT候補

```text
none
fade
slide-up
slide-down
slide-left
slide-right
shrink
```

---

## アニメーション計算を純粋関数化

```rust
#[derive(Debug, Clone, Copy)]
pub struct Transform {
    pub offset_x: f32,
    pub offset_y: f32,
    pub opacity: f32,
    pub scale: f32,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            offset_x: 0.0,
            offset_y: 0.0,
            opacity: 1.0,
            scale: 1.0,
        }
    }
}
```

### easing

```rust
fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

fn ease_in_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t.powi(3)
}
```

---

## IN

```rust
pub fn enter_transform(
    animation: OverlayInAnimation,
    progress: f32,
    distance: f32,
    start_scale: f32,
) -> Transform {
    let p = ease_out_cubic(progress);

    match animation {
        OverlayInAnimation::None => Transform::default(),

        OverlayInAnimation::Fade => Transform {
            opacity: p,
            ..Default::default()
        },

        OverlayInAnimation::SlideUp => Transform {
            offset_y: distance * (1.0 - p),
            opacity: p,
            ..Default::default()
        },

        OverlayInAnimation::SlideDown => Transform {
            offset_y: -distance * (1.0 - p),
            opacity: p,
            ..Default::default()
        },

        OverlayInAnimation::SlideLeft => Transform {
            offset_x: distance * (1.0 - p),
            opacity: p,
            ..Default::default()
        },

        OverlayInAnimation::SlideRight => Transform {
            offset_x: -distance * (1.0 - p),
            opacity: p,
            ..Default::default()
        },

        OverlayInAnimation::Pop => Transform {
            opacity: p,
            scale: start_scale + (1.0 - start_scale) * p,
            ..Default::default()
        },
    }
}
```

---

## OUT

```rust
pub fn exit_transform(
    animation: OverlayOutAnimation,
    progress: f32,
    distance: f32,
) -> Transform {
    let p = ease_in_cubic(progress);

    match animation {
        OverlayOutAnimation::None => Transform::default(),

        OverlayOutAnimation::Fade => Transform {
            opacity: 1.0 - p,
            ..Default::default()
        },

        OverlayOutAnimation::SlideUp => Transform {
            offset_y: -distance * p,
            opacity: 1.0 - p,
            ..Default::default()
        },

        OverlayOutAnimation::SlideDown => Transform {
            offset_y: distance * p,
            opacity: 1.0 - p,
            ..Default::default()
        },

        OverlayOutAnimation::SlideLeft => Transform {
            offset_x: -distance * p,
            opacity: 1.0 - p,
            ..Default::default()
        },

        OverlayOutAnimation::SlideRight => Transform {
            offset_x: distance * p,
            opacity: 1.0 - p,
            ..Default::default()
        },

        OverlayOutAnimation::Shrink => Transform {
            opacity: 1.0 - p,
            scale: 1.0 - (0.08 * p),
            ..Default::default()
        },
    }
}
```

---

## duration=0

0msはdivision by zeroを避ける。

```rust
fn progress(now: Instant, start: Instant, duration_ms: u64) -> f32 {
    if duration_ms == 0 {
        return 1.0;
    }

    let elapsed = now.saturating_duration_since(start).as_secs_f32() * 1000.0;
    (elapsed / duration_ms as f32).clamp(0.0, 1.0)
}
```

---

## 既存stack移動

新規Item IN/OUTと、既存Itemが1段移動するアニメーションは混同しない。

既存stack shiftの160ms仕様がある場合:

- 既存の移動補間を維持
- enter presetは新規行だけ
- exit presetはTTL終了行だけ

---

## Unit Test

```rust
#[test]
fn fade_enter_starts_transparent_and_ends_opaque() {
    let a = enter_transform(
        OverlayInAnimation::Fade,
        0.0,
        18.0,
        0.96,
    );
    let b = enter_transform(
        OverlayInAnimation::Fade,
        1.0,
        18.0,
        0.96,
    );

    assert_eq!(a.opacity, 0.0);
    assert_eq!(b.opacity, 1.0);
}
```

---

## 完了条件

- INとOUTを独立設定
- preset変更だけでrenderer構造を分岐だらけにしない
- `none`対応
- 0ms対応
- animation中だけtimer動作
- stable時の無駄描画なし

---

## 禁止

- CSS animation導入
- 1アニメーションごとに別Thread生成
- 常時60FPS
