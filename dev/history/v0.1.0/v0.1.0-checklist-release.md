# v0.1.0 Release Checklist
- [ ] cargo fmt --all -- --check
- [ ] cargo clippy --workspace --all-targets --all-features -- -D warnings
- [ ] cargo test --workspace --all-features
- [ ] release build (ShortcutKeyCounter.exe / ShortcutKeyCapture.exe)
- [ ] UI smoke / Dashboard + 5 ranking views + detail/settings/help / capture ON-OFF
- [ ] privacy audit
- [ ] CSV / backup / delete acceptance
- [ ] Elevated / helper kill / Local fallback
- [ ] power lifecycle
- [ ] clean VM install / update / uninstall keep / uninstall delete
- [ ] ZIP has no DB
- [ ] 24h soak
- [ ] 72h soak

## UI / WebView lifecycle

- [ ] `scripts/test-webview-lifecycle.ps1` PASS
- [ ] × close destroys the main window instead of hiding it
- [ ] `ShortcutKeyCounter.exe` remains alive after UI close
- [ ] descendant `msedgewebview2.exe` count reaches zero after close
- [ ] notification-area icon double-click recreates and focuses the UI
- [ ] second close releases WebView2 again
- [ ] Realtime Log works after UI recreation
- [ ] Key Mapping persists and still filters capture after UI recreation

## Added UI features

- [ ] white 3D `C` single-key icon is visible in exe/tray
- [ ] sidebar shows version instead of Local usage analytics
- [ ] Realtime Log contains no normal text/window title/URL
- [ ] Key Mapping save + JSON export PASS
- [ ] CSV/DB/JSON completion toast opens the containing folder
- [ ] Help page version/author/GitHub/basic usage display correctly

## Elevated / startup / state-sync regression

- [ ] Elevated OFF: normal app shortcuts are recorded
- [ ] Elevated ON: normal app shortcuts continue to be recorded
- [ ] Elevated ON: elevated Notepad/Terminal shortcuts are recorded through `ShortcutKeyCapture.exe`
- [ ] Elevated OFF operation returns without UI freeze
- [ ] helper kill leaves normal capture running and health becomes degraded/local fallback
- [ ] Tray capture stop updates an open WebView immediately
- [ ] WebView capture stop updates tray menu/tooltip immediately
- [ ] `ShortcutKeyCounter.exe -hide` starts without native main window/WebView2
- [ ] `ShortcutKeyCounter.exe` without arguments shows UI on first launch
- [ ] second launch does not create a second backend process and opens Settings in the existing process
- [ ] Ranking navigation is two-level and includes daily/key/app/realtime/matrix
- [ ] Help shows KotoriChun and the ShortcutKeyCounter GitHub repository
