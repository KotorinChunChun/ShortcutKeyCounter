//! KotorinToysと共通のウィンドウタイトル規約。
pub struct WindowTitle;
impl WindowTitle {
    pub fn create(screen: Option<&str>) -> String {
        let product = format!("ShortcutKeyCounter - ver {}", env!("CARGO_PKG_VERSION"));
        screen
            .filter(|s| !s.is_empty())
            .map_or_else(|| product.clone(), |s| format!("{s} - {product}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn common_title_uses_product_version() {
        assert_eq!(
            WindowTitle::create(None),
            format!("ShortcutKeyCounter - ver {}", env!("CARGO_PKG_VERSION"))
        );
        assert_eq!(
            WindowTitle::create(Some("設定")),
            format!(
                "設定 - ShortcutKeyCounter - ver {}",
                env!("CARGO_PKG_VERSION")
            )
        );
    }
}
