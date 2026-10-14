# 開発資料

開発者・保守担当者向けの資料をまとめる。利用者向けの説明は、プロジェクトルートの [`README.md`](../README.md) と [`docs/README.md`](../docs/README.md) を参照する。

## 現行

- 必須ルール・資料目録: [`../AGENTS.md`](../AGENTS.md)
- 文書運用規則: [`documentation-rules.md`](./documentation-rules.md)
- 配布手順: [`RELEASE_RULE.md`](./RELEASE_RULE.md)
- 運用スクリプト: [`scripts/`](./scripts/)
- v0.1.5 要求仕様: [`v0.1.5-req.md`](./v0.1.5-req.md)
- v0.1.5 実装計画: [`v0.1.5-imp.md`](./v0.1.5-imp.md)

## 次期版計画

- v0.2.0 要求仕様: [`v0.2.0-req.md`](./v0.2.0-req.md)
- v0.2.0 実装計画: [`v0.2.0-imp.md`](./v0.2.0-imp.md)
- v0.2.0 タスク一覧: [`v0.2.0-task.md`](./v0.2.0-task.md)
- 将来計画・未確定アイデア: [`PLAN_IDEAS.md`](./PLAN_IDEAS.md)

v0.2.0は計画のみを確定した次期版であり、v0.1.5のリリースと履歴移動が完了するまで実装しない。個別設計への索引は`v0.2.0-imp.md`を正とする。

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

詳細な文書命名規則は [`documentation-rules.md`](./documentation-rules.md)、必須の開発ルールは [`../AGENTS.md`](../AGENTS.md) を正とする。

## プロジェクト固有の実装配置

汎用テンプレートの `src/` に相当する実装領域は、Rust workspaceの責務分離を維持するため、本プロジェクトでは `apps/` と `crates/` に分ける。配布成果物は `release/Publish/` に保存し、`release/` 全体をGit追跡対象外とする。`dev/private/` と `dev/temp/` は目録へ個別資料を載せず、通常の調査対象から除外する。
