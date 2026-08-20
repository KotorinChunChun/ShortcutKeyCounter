# ドキュメント運用規則

更新日: 2026-08-20

本書は、ShortcutKeyCounter の文書配置、ファイル名、版管理の基準を定める。本文は日本語で記述し、ファイル名とリンク先はASCII文字で統一する。

## 1. 配置

| 場所 | 対象 | 正規ファイル |
|---|---|---|
| プロジェクトルート | 利用者が最初に読む資料 | `README.md` |
| プロジェクトルート | 配布ZIPへ同梱するプレーンテキスト | `README.txt` |
| `docs/` | 利用者・ビルド担当者向けの版に依存しない手順 | `docs/<topic>.md` |
| `dev/` | 現行版の要求、実装計画、進捗 | `vX.Y.Z-req.md` / `vX.Y.Z-imp.md` / `vX.Y.Z-task.md` |
| `dev/` | プロジェクト共通の開発規約 | `project-rules.md` |
| `dev/history/vX.Y.Z/` | 完了済み版の要求・実装・検証・報告 | 版付きファイル一式 |
| `dev/history/vX.Y.Z/tasks/` | 旧版の細分化タスク | `NN-<topic>.md` |

版付き資料を `docs/` や `dev/` 直下へ重複配置しない。完了した版は、次の版へ進む時点で `dev/history/vX.Y.Z/` へ移動する。

## 2. ファイル名

### 2.1 共通規則

- 固定名の `README.md` / `README.txt` / `LICENSE.txt` 以外は小文字ASCIIのkebab-caseを使う。
- 空白、日本語、全角記号、`#Uxxxx` のようなUnicodeエスケープ風表記、用途不明な略称は使わない。
- 日付は `YYYYMMDD` とし、版と組み合わせる場合は末尾に置く。
- ファイル名だけで役割が分かるようにし、アプリ名や版をディレクトリ名と重複させない。

### 2.2 版付き資料

基本形は次のとおりとする。

```text
vMAJOR.MINOR.PATCH-<kind>[-<topic>].md
```

`kind` は次を使用する。

| kind | 用途 | 例 |
|---|---|---|
| `req` | 要求仕様 | `v0.1.5-req.md` |
| `imp` | 実装計画 | `v0.1.5-imp.md` |
| `task` | 進捗・作業一覧 | `v0.1.5-task.md` |
| `design` | 設計資料 | `v0.1.1-design-overlay.md` |
| `report` | 実装・修正・テスト報告 | `v0.1.4-report-implementation.md` |
| `checklist` | 受入・リリース確認表 | `v0.1.4-checklist-windows-acceptance.md` |
| `audit` | 監査結果 | `v0.1.4-audit-static.txt` |
| `manifest` | 成果物一覧 | `v0.1.4-manifest-source.txt` |
| `status` | 版の状態記録 | `v0.1.4-status.md` |

修正対象や内容は `topic` にkebab-caseで記述する。日付付き報告は `v0.1.3-report-build-fix-20260817.md` のようにする。

### 2.3 タスクパッケージ

旧版を保存する場合も、タスク順を示す2桁番号を維持する。

```text
dev/history/v0.1.2/tasks/00-overview.md
dev/history/v0.1.2/tasks/01-ovl-01-settings-model.md
dev/history/v0.1.2/tasks/99-master-checklist.md
dev/history/v0.1.2/tasks/file-manifest.md
```

## 3. 正規文書と重複防止

- 要求の正は `vX.Y.Z-req.md` とする。差分要件も同じ版の `req` として保管する。
- 実装計画の正は `vX.Y.Z-imp.md` とする。実装後の結果は `vX.Y.Z-report-implementation.md` に分ける。
- テスト・受入・監査・成果物一覧は、それぞれ `report` / `checklist` / `audit` / `manifest` として分離する。
- 同一内容の複製を作らない。過去の成果物一覧に記録された旧パスは、当時のパッケージ構成を示すスナップショットとして扱う。

## 4. リンクと版更新

- Markdownのリンクはリンク元からの相対パスにする。
- ファイルを移動・改名したら、旧パスを `rg` で検索し、現行文書の参照をすべて更新する。
- 実装中の版だけを `dev/` 直下に置く。リリース後に次の版へ進んだら、旧版一式を `dev/history/` へ移動する。
- 本プロジェクトでは、製品のリリース版はソースのバージョン表記に従い、要求作業中の次版は開発中として `dev/<version>-req.md` に記録する。

## 5. 現在の配置

- 基準リリース: v0.1.4
- 現行要求: [`v0.1.5-req.md`](./v0.1.5-req.md)
- 共通規約: [`project-rules.md`](./project-rules.md)
- 利用者向け手順: [`../docs/README.md`](../docs/README.md)
- 完了版: [`history/`](./history/)
