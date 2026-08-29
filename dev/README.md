# 開発資料

開発者・保守担当者向けの資料をまとめる。利用者向けの説明は、プロジェクトルートの [`README.md`](../README.md) と [`docs/README.md`](../docs/README.md) を参照する。

## 現行

- 共通規約: [`project-rules.md`](./project-rules.md)
- 文書運用規則: [`documentation-rules.md`](./documentation-rules.md)
- v0.1.5 要求仕様: [`v0.1.5-req.md`](./v0.1.5-req.md)
- v0.1.5 実装計画・報告: [`v0.1.5-imp.md`](./v0.1.5-imp.md)
- 共通テンプレート: [`templates/`](./templates/)

実装を開始した版では、同じ版の `vX.Y.Z-imp.md` を作成する。要求が複数の作業へ分かれる場合だけ `vX.Y.Z-task.md` を追加する。

## 履歴

完了した版の資料は [`history/`](./history/) 以下に版ごとに保存する。

| 版 | 内容 |
|---|---|
| v0.1.0 | 初期要求、実装、進捗、受入資料 |
| v0.1.1 | Overlay設計・実装 |
| v0.1.2 | Overlay拡張の実装計画とタスク |
| v0.1.3 | 要求、ビルド修正、開発報告 |
| v0.1.4 | 差分要求、実装計画、実装・テスト・受入報告 |

詳細な命名規則は [`documentation-rules.md`](./documentation-rules.md) を正とする。

## プロジェクト固有の実装配置

汎用テンプレートの `src/` に相当する実装領域は、Rust workspaceの責務分離を維持するため、本プロジェクトでは `apps/` と `crates/` に分ける。配布成果物は `release/Publish/` に保存し、`release/` 全体をGit追跡対象外とする。
