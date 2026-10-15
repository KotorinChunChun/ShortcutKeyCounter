# ShortcutKeyCounter セキュリティ診断・改善実装レポート

診断日: 2026-10-15。対象: `88aea33` / feature/v0.1.5（Cargo製品版0.1.4）、開始時clean。入力保護gate、管理pipe、Tauri権限/CSP、Rust依存、追跡秘密を静的診断した。`dev/private`・`dev/temp`は除外。

## 確認した防御

- `apps/shortcut-key-counter/src/security_gate.rs:169`: password検査・失敗時の停止を入力許可判定へ反映。実パスワード入力を使う検証は行っていない。
- `crates/shortcut-windows/src/management_pipe.rs:86`: 所有利用者SIDだけのACL、`:108` はローカルクライアント限定、`:142` は同一session照合、`:164` は要求8192 bytes上限と期限を使用。
- `apps/shortcut-key-counter/tauri.conf.json:20`: ローカルmain画面のイベント購読/解除のみのcapabilityと、selfを基準とするCSPを定義。CSP有りだけで全XSSを否定していない。

## 依存照合結果と改善

既存Cargo.lockのregistry依存489項目のname/versionだけをOSV `/v1/querybatch` に照会した。ソース/ユーザーデータ/秘密は送信していない。9パッケージで計10識別子（glibの別名を重複排除すると9 advisory）が一致。cargo-auditは未導入のため使用せず、OSV結果とRustSecの一次資料を確認した。

|項目|証拠|判定と対応|
|---|---|---|
|glib 0.18.5 / RUSTSEC-2024-0429（GHSA-wrw7-89jp-8q8g）|`Cargo.lock:1223`、[RustSec](https://rustsec.org/advisories/RUSTSEC-2024-0429.html)|Iteratorのunsoundness。修正版>=0.20.0。`cargo tree --locked --offline --target x86_64-pc-windows-msvc -i glib` は依存経路なし。Windows製品での成立を確認していない。Linux対象にする場合はTauri/GTK依存経路を再評価する。|
|proc-macro-error 1.0.4 / RUSTSEC-2024-0370|`Cargo.lock:2561`、[RustSec](https://rustsec.org/advisories/RUSTSEC-2024-0370.html)|保守終了。Windows対象の逆依存なし。脆弱性の実害を示す件数に含めない。|
|rustybuzz 0.20.1 / RUSTSEC-2026-0206|`Cargo.lock:2815`、[RustSec](https://rustsec.org/advisories/RUSTSEC-2026-0206.html)|保守終了。Windowsのusvg→resvg→本体経路をcargo treeで確認。harfrust等への移行は上位ライブラリ・描画互換性を伴うため保留。|
|ttf-parser 0.25.1 / RUSTSEC-2026-0192|`Cargo.lock:4049`、[RustSec](https://rustsec.org/advisories/RUSTSEC-2026-0192.html)|保守終了。skrifa等への移行はフォント描画の互換性判断が必要。|
|unic-char-property / unic-char-range / unic-common / unic-ucd-ident / unic-ucd-version 0.9.0|`Cargo.lock:4070`以降。RUSTSEC-2025-0081、0075、0080、0100、0098。[RustSec代表項目](https://rustsec.org/advisories/RUSTSEC-2025-0081.html)|rust-unic群の保守終了情報。Tauri/HTML関連の上位依存更新と互換性を検討。今回の高優先修正対象ではない。|

保守終了8件は直ちに悪用可能な8脆弱性という意味ではない。最新アドバイザリとの照合はこの実行時点の結果であり、ソース全体の安全性を保証しない。高/致命的として直ちに直せるWindows製品の問題は今回確定していない。

## 秘密・改善実装・検証

追跡テキスト182ファイルのPEM秘密鍵/GitHub/AWS/OpenAI代表パターン値非表示検査は一致0。過去履歴/無視対象/一般パスワード/実DBは未検査。

- [x] 静的診断・OSV照合・Windows依存経路確認・文書差分検査。
- 製品コード/lock変更なし。依存移行はフォント/UI互換性の重要判断が必要なため保留。
- 文書のみのため形式的な製品ビルド・GUI試験は行っていない。実入力・DB・CSV・権限昇格・導入・公開・親gitlink更新なし。
