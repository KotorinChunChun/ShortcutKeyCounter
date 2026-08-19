//! 左右の修飾キー状態を追跡する純粋状態モジュール。

/// Windows Virtual-Key code: 左右Ctrl。
pub const VK_CONTROL: u16 = 0x11;
pub const VK_LCONTROL: u16 = 0xA2;
pub const VK_RCONTROL: u16 = 0xA3;
/// Windows Virtual-Key code: 左右Alt。
pub const VK_MENU: u16 = 0x12;
pub const VK_LMENU: u16 = 0xA4;
pub const VK_RMENU: u16 = 0xA5;
/// Windows Virtual-Key code: 左右Shift。
pub const VK_SHIFT: u16 = 0x10;
pub const VK_LSHIFT: u16 = 0xA0;
pub const VK_RSHIFT: u16 = 0xA1;
/// Windows Virtual-Key code: 左右Win。
pub const VK_LWIN: u16 = 0x5B;
pub const VK_RWIN: u16 = 0x5C;

/// 左右の修飾キーを個別に保持する状態。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ModifierStateTracker {
    left_ctrl: bool,
    right_ctrl: bool,
    generic_ctrl: bool,
    left_alt: bool,
    right_alt: bool,
    generic_alt: bool,
    left_win: bool,
    right_win: bool,
    left_shift: bool,
    right_shift: bool,
    generic_shift: bool,
}

impl ModifierStateTracker {
    /// すべての修飾キーが離されている状態を作る。
    pub const fn new() -> Self {
        Self {
            left_ctrl: false,
            right_ctrl: false,
            generic_ctrl: false,
            left_alt: false,
            right_alt: false,
            generic_alt: false,
            left_win: false,
            right_win: false,
            left_shift: false,
            right_shift: false,
            generic_shift: false,
        }
    }

    /// 修飾キーのKeyDownを反映する。未知のVK値は無視する。
    pub fn key_down(&mut self, vk: u16) {
        self.set(vk, true);
    }

    /// 修飾キーのKeyUpを反映する。未知のVK値は無視する。
    pub fn key_up(&mut self, vk: u16) {
        self.set(vk, false);
    }

    /// shortcut-coreのModifierStateを生成するための畳み込みAPI。
    ///
    /// 引数の順序は`ctrl, alt, win, shift`であり、各値は左右いずれかが
    ///押されていれば`true`になる。呼び出し側はこのクロージャ内で
    ///`shortcut_core::model::ModifierState`を構築できる。
    pub fn fold<T>(&self, build: impl FnOnce(bool, bool, bool, bool) -> T) -> T {
        build(
            self.left_ctrl || self.right_ctrl || self.generic_ctrl,
            self.left_alt || self.right_alt || self.generic_alt,
            self.left_win || self.right_win,
            self.left_shift || self.right_shift || self.generic_shift,
        )
    }

    fn set(&mut self, vk: u16, pressed: bool) {
        match vk {
            VK_CONTROL => self.generic_ctrl = pressed,
            VK_LCONTROL => self.left_ctrl = pressed,
            VK_RCONTROL => self.right_ctrl = pressed,
            VK_MENU => self.generic_alt = pressed,
            VK_LMENU => self.left_alt = pressed,
            VK_RMENU => self.right_alt = pressed,
            VK_LWIN => self.left_win = pressed,
            VK_RWIN => self.right_win = pressed,
            VK_LSHIFT => self.left_shift = pressed,
            VK_RSHIFT => self.right_shift = pressed,
            VK_SHIFT => self.generic_shift = pressed,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence_tracks_left_and_right_keys_independently_then_folds_them() {
        let mut state = ModifierStateTracker::new();

        state.key_down(VK_LCONTROL);
        assert_eq!(
            state.fold(|ctrl, alt, win, shift| (ctrl, alt, win, shift)),
            (true, false, false, false)
        );

        state.key_down(VK_RCONTROL);
        state.key_up(VK_LCONTROL);
        assert_eq!(
            state.fold(|ctrl, alt, win, shift| (ctrl, alt, win, shift)),
            (true, false, false, false)
        );

        state.key_down(VK_RSHIFT);
        state.key_down(VK_LWIN);
        state.key_down(VK_LMENU);
        assert_eq!(
            state.fold(|ctrl, alt, win, shift| (ctrl, alt, win, shift)),
            (true, true, true, true)
        );

        state.key_up(VK_RCONTROL);
        state.key_up(VK_RSHIFT);
        state.key_up(VK_LWIN);
        state.key_up(VK_LMENU);
        assert_eq!(
            state.fold(|ctrl, alt, win, shift| (ctrl, alt, win, shift)),
            (false, false, false, false)
        );
    }
}
