//! 主入力キーの押下状態を追跡する純粋状態モジュール。

use std::collections::HashSet;

/// 主入力キーの押下集合。
#[derive(Clone, Debug, Default)]
pub struct PressedKeys {
    keys: HashSet<u16>,
}

impl PressedKeys {
    /// 空の押下集合を作る。
    pub fn new() -> Self {
        Self::default()
    }

    /// KeyDownを反映し、初回なら`true`、repeatなら`false`を返す。
    pub fn key_down(&mut self, vk: u16) -> bool {
        self.keys.insert(vk)
    }

    /// KeyUpを反映する。未押下のキーに対しても安全に呼び出せる。
    pub fn key_up(&mut self, vk: u16) {
        self.keys.remove(&vk);
    }

    /// キーが現在押下中か確認する。
    pub fn contains(&self, vk: u16) -> bool {
        self.keys.contains(&vk)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence_accepts_only_first_key_down_until_key_up() {
        let mut pressed = PressedKeys::new();

        assert!(pressed.key_down(0x41));
        assert!(!pressed.key_down(0x41));
        assert!(pressed.contains(0x41));

        pressed.key_up(0x41);
        assert!(!pressed.contains(0x41));
        assert!(pressed.key_down(0x41));

        pressed.key_down(0x42);
        pressed.key_up(0x41);
        assert!(!pressed.contains(0x41));
        assert!(pressed.contains(0x42));
        pressed.key_up(0x42);
        pressed.key_up(0x42);
        assert!(!pressed.contains(0x42));
    }
}
