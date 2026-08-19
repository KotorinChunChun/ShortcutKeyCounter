# ShortcutKeyCounter v0.1.2 Master Checklist

## Wave A

- [x] OVL-01 設定モデル拡張
- [x] fmt
- [x] clippy
- [x] cargo test

## Wave B

- [x] OVL-02 表示時間
- [x] OVL-03 9位置
- [x] OVL-05 背景色
- [x] OVL-06 単独キーOverlayEvent
- [x] OVL-09 IN/OUT animation
- [x] OVL-10 最大行数
- [x] fmt
- [x] clippy
- [x] cargo test

## Wave C

- [x] OVL-04 自動位置
- [x] OVL-07 KeyPolicy
- [x] OVL-08 単独キー横連結
- [x] OVL-11 クイックOFF
- [x] fmt
- [x] clippy
- [x] cargo test

## Wave D

- [x] OVL-12 設定画面/API
- [x] TypeScript strict build
- [x] Tauri command登録漏れなし
- [x] UI ID参照漏れなし
- [x] fmt
- [x] clippy
- [x] cargo test

## Wave E

- [x] OVL-13 migration
- [x] v1 config migration
- [x] KeyPolicy初期生成
- [ ] 9位置実機
- [ ] auto rotation
- [ ] multi-monitor
- [ ] DPI
- [ ] Elevated
- [ ] Excluded apps
- [ ] DB regression
- [ ] quick OFF
- [ ] idle load
- [ ] rapid input
- [x] build-release.ps1（既存release exeを停止せず、`-CargoTargetDirectory`指定でrelease build/ZIPまでPASS）

## Release blockers

次の1つでもNOならリリース不可。

- [x] 通常キーがDBへ混入しない
- [x] DisplayOnlyがDBへ混入しない
- [x] DisplayAndRecord shortcutが従来通り記録される
- [x] DB 1秒DuplicateGuardを変更していない
- [x] Local/Elevated 50ms dedupを維持
- [x] 除外アプリではOverlayもDBも動かない
- [x] Overlayがfocusを奪わない
- [x] click-through維持
- [x] Overlay idle時busy loopなし
- [x] 旧overlay-theme.jsonを壊さず移行

## 実機未確認

- [ ] Windowsデスクトップ上の9位置、multi-monitor、DPI切替
- [ ] tray click、Elevated UAC、focus/click-through実機
- [ ] rapid input負荷、idle CPU/memory
