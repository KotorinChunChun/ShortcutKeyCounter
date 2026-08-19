# ShellExecuteW ビルドエラー修正報告（2026-08-16）

## 発生エラー

`windows` crate 0.62.2 の `ShellExecuteW` 第1引数は `Option<HWND>` だが、`crates/shortcut-windows/src/shell_open.rs` で `HWND::default()` を直接渡していたため E0308 が発生した。

## 修正

```rust
ShellExecuteW(
    None,
    PCWSTR(operation.as_ptr()),
    PCWSTR(target.as_ptr()),
    PCWSTR::null(),
    PCWSTR::null(),
    SW_SHOWNORMAL,
)
```

あわせて不要になった `use windows::Win32::Foundation::HWND;` を削除した。
`elevation.rs` の `ShellExecuteW` は既に `None` を使用していたため変更不要。

## 回帰防止

`scripts/build-release.ps1` に `shell_open.rs` の `HWND::default()` を禁止する静的Gateを追加した。通常の `cargo check` でも同種の型不一致を検出する。

## 実機での確認コマンド

```powershell
cargo check --workspace --all-targets --all-features
cargo build --release -p shortcut-key-counter -p shortcut-key-capture
```
