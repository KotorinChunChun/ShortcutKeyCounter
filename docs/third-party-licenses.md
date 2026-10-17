# 第三者ライセンスと素材

本体と所有者自作の画像・SVGはMITで提供する。依存ライブラリにはそれぞれのライセンスが適用される。

- [依存の表示文・ライセンス全文](../third-party/THIRD-PARTY-NOTICES.txt)
- [版・SPDX式・出典・ハッシュの台帳](../third-party/manifest.json)
- [画像・SVGの出典確認](../third-party/ASSET-PROVENANCE.md)
- [WebView2 SDKのライセンス](../third-party/webview2-sdk/LICENSE.txt)と[NOTICE](../third-party/webview2-sdk/NOTICE.txt)
- [Rust標準ライブラリの表示文](../third-party/rust-standard-library/COPYRIGHT-library.html)と[使用版](../third-party/rust-standard-library/SOURCES.md)

対象はCargo.lockから解決したWindows x64の依存（ビルド用・開発用も保守的に含む）。MIT、Apache-2.0、BSD、Zlib、Unicode等の本文・著作権表示を保持する。選択式は上流の式をそのまま掲載し、ANDで要求される表示も含める。上流crateに本文がないものは、crateの`.cargo_vcs_info.json`が示す固定コミットから取得した本文と出典を`third-party/upstream/`に保存する。

MPL-2.0対象のcssparser、cssparser-macros、dtoa-short、option-ext、selectorsは未改変のcrate原本を`third-party/sources/`へ同梱する。`.crate`はtar.gz形式であり、一般のtarツールで展開できる。台帳の各URLからも同じ版のソースを取得できる。原本SHA-256はCargo.lockと照合する。

WebView2 SDK 1.0.3650.58の表示文は公式NuGetパッケージから取得した。WebView2 RuntimeやWindowsのフォント自体を配布するものではない。UIは自前のTypeScript/HTML/CSSで、TypeScriptコンパイラーはビルド用ツールとして使用しZIPへ同梱しない。

## 更新手順

```powershell
cargo metadata --locked --offline --format-version 1 --filter-platform x86_64-pc-windows-msvc > dev/history/v0.1.5/logs/license-review-20261017/metadata.json
python dev/scripts/update-third-party-notices.py dev/history/v0.1.5/logs/license-review-20261017/metadata.json
```

Python 3.11以上、Cargoの原本キャッシュを使用する。新しい依存の本文が欠落する場合は生成を停止するため、上流の正しい版で許諾を確認する。台帳生成後、式・本文・ネイティブ同梱物をレビューする。ZIP生成は古いCargo.lockの台帳やMPL原本欠落を拒否する。製品機能・安全性の検証とは別の工程である。

参照した公式条件: [Apache-2.0](https://www.apache.org/licenses/LICENSE-2.0)、[MPL-2.0](https://www.mozilla.org/en-US/MPL/2.0/)、[WebView2 SDK 1.0.3650.58](https://www.nuget.org/packages/Microsoft.Web.WebView2/1.0.3650.58)。
