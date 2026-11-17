// test-exe-ShortcutKeyCounter-ui-background.ps1 から呼ばれるUI試験本体。
// 非表示で起動したWebViewへDevTools Protocolで接続し、UIと同じTauri command呼び出し、
// 画面遷移、JS例外を検査する。デスクトップへウィンドウは出さない。
// 使い方: node test-exe-ShortcutKeyCounter-ui-background.mjs <port> <isolatedRoot> <outDir>
import fs from 'node:fs'
import path from 'node:path'
import { DatabaseSync } from 'node:sqlite'

const [port, isolatedRoot, outDir] = process.argv.slice(2)
const root = path.resolve(isolatedRoot).toLowerCase()
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms))
const report = { checks: [], pages: [], errors: [], console: [], notes: [] }
const isInsideRoot = file => path.resolve(file).toLowerCase().startsWith(root + path.sep)

function record(name, pass, detail) {
  report.checks.push({ name, pass, detail })
  console.log(`${pass ? 'PASS' : 'FAIL'}  ${name}${detail === undefined ? '' : `  ${typeof detail === 'string' ? detail : JSON.stringify(detail)}`}`)
}

// --- DevTools Protocol接続
let target
for (let attempt = 0; attempt < 120 && !target; attempt += 1) {
  try {
    const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json()
    target = targets.find(t => t.type === 'page' && /tauri\.localhost|index\.html/.test(t.url))
  } catch { /* WebView not ready */ }
  if (!target) await sleep(500)
}
if (!target) throw new Error('WebViewのDevToolsターゲットが見つかりません。')
const ws = new WebSocket(target.webSocketDebuggerUrl)
await new Promise((resolve, reject) => { ws.onopen = resolve; ws.onerror = reject })
let nextId = 1
const pending = new Map()
ws.onmessage = event => {
  const message = JSON.parse(event.data)
  if (message.id && pending.has(message.id)) {
    pending.get(message.id)(message)
    pending.delete(message.id)
  } else if (message.method === 'Runtime.exceptionThrown') {
    const details = message.params.exceptionDetails
    report.errors.push(details.exception?.description ?? details.text)
  } else if (message.method === 'Runtime.consoleAPICalled' && message.params.type === 'error') {
    report.console.push(message.params.args.map(a => a.value ?? a.description).join(' '))
  }
}
const send = (method, params = {}, timeoutMs = 30000) => new Promise((resolve, reject) => {
  const id = nextId++
  const timer = setTimeout(() => { pending.delete(id); reject(new Error(`${method} timeout`)) }, timeoutMs)
  pending.set(id, message => { clearTimeout(timer); resolve(message) })
  ws.send(JSON.stringify({ id, method, params }))
})
async function evaluate(expression) {
  const response = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true })
  const exception = response.result?.exceptionDetails
  if (exception) throw new Error(exception.exception?.description ?? exception.text)
  return response.result?.result?.value
}
// UIと同じ window.__TAURI__.core.invoke を使う。エラーは CommandError か文字列。
const invoke = (command, args = {}) => evaluate(`window.__TAURI__.core.invoke(${JSON.stringify(command)}, ${JSON.stringify(args)})
  .then(value => ({ ok: true, value }), error => ({ ok: false, error: typeof error === 'string' ? { message: error } : error }))`)
async function expectOk(name, command, args, verify = () => true) {
  const result = await invoke(command, args)
  const pass = result.ok && verify(result.value) !== false
  record(name, pass, result.ok ? undefined : result.error)
  return result.ok ? result.value : undefined
}
async function expectError(name, command, args, code) {
  const result = await invoke(command, args)
  record(name, !result.ok && (!code || result.error?.code === code), result.ok ? 'unexpected success' : result.error?.code ?? result.error?.message)
}

await send('Runtime.enable')
await send('Page.enable')
for (let attempt = 0; attempt < 60 && !(await evaluate(`document.readyState === 'complete' && !!window.__TAURI__?.core`)); attempt += 1) {
  await sleep(500)
}
await sleep(1500)

// --- 1. 隔離の確認。保存先が隔離rootの外なら、以降の書き込み系を実行せずに中止する。
const paths = await invoke('get_app_paths')
const isolated = paths.ok && [paths.value.data_directory, paths.value.database_path].every(isInsideRoot)
record('保存先が隔離rootの中にある', isolated, paths.ok ? paths.value.data_directory : paths.error)
if (!isolated) {
  fs.writeFileSync(path.join(outDir, 'report.json'), JSON.stringify(report, null, 2))
  process.exit(3)
}
report.notes.push(`document.visibilityState=${await evaluate('document.visibilityState')}`)

// --- 2. 隔離した本番DBへ、日付の異なる実績を投入する（サンプル生成の母集団にもなる）
const seedDays = ['2026-11-10', '2026-11-11', '2026-11-12']
{
  const db = new DatabaseSync(paths.value.database_path)
  db.exec('PRAGMA busy_timeout = 5000')
  const daily = db.prepare('INSERT INTO shortcut_daily(date, shortcut, application, count) VALUES (?, ?, ?, ?)')
  const event = db.prepare('INSERT INTO shortcut_events(occurred_at, shortcut, application) VALUES (?, ?, ?)')
  const rows = [[seedDays[0], 'Ctrl+C', 'Code.exe', 3], [seedDays[1], 'Ctrl+V', 'Code.exe', 2], [seedDays[2], 'Ctrl+S', 'EXCEL.EXE', 4]]
  for (const [date, shortcut, application, count] of rows) {
    daily.run(date, shortcut, application, count)
    for (let index = 0; index < count; index += 1) event.run(`${date} 09:${String(index).padStart(2, '0')}:00.000`, shortcut, application)
  }
  db.close()
}

// --- 3. 読み取り系command
for (const command of ['get_app_info', 'get_capture_status', 'get_capture_health', 'get_settings', 'get_database_environment',
  'get_excluded_apps', 'get_overlay_settings', 'get_overlay_monitors', 'get_overlay_key_labels', 'get_overlay_key_policy',
  'get_key_mapping', 'get_mapping_edit_session', 'mapping_edit_session_status', 'list_mapping_profiles', 'get_usage_graph_colors',
  'get_sample_source_status', 'get_daily_export_filter', 'preview_delete_yesterday_and_earlier', 'poll_key_search', 'get_data_bounds']) {
  await expectOk(`読み取り: ${command}`, command)
}
await expectOk('読み取り: get_realtime_logs (camelCase引数)', 'get_realtime_logs', { afterSequence: 0, limit: 10 })
await expectError('引数名はcamelCaseだけを受け付ける', 'get_realtime_logs', { after_sequence: 0, limit: 10 })
await expectOk('計測は隔離試験で停止している', 'get_capture_status', {}, status => status.enabled === false)
await expectOk('Overlayは隔離試験で無効', 'get_overlay_settings', {}, settings => settings.enabled === false)

// --- 4. 統計（隔離本番DB）と期間検証
const [start, end] = [seedDays[0], seedDays[2]]
await expectOk('統計: dashboard', 'get_dashboard_summary', { start, end }, v => v.total_count === 9)
await expectOk('統計: 1日だけの期間', 'get_dashboard_summary', { start: seedDays[1], end: seedDays[1] }, v => v.total_count === 2)
await expectOk('統計: ランキング', 'get_shortcut_ranking', { start, end, application: null, limit: 10 }, v => v[0]?.shortcut === 'Ctrl+S')
await expectOk('統計: 拡張ランキング', 'get_shortcut_ranking_extended', { start, end, application: null, limit: 10 })
await expectOk('統計: アプリランキング', 'get_application_ranking', { start, end, limit: 10 }, v => v.length === 2)
await expectOk('統計: アプリ統計', 'get_application_statistics', { start, end })
await expectOk('統計: 日別推移', 'get_daily_trend', { start, end, shortcut: null, application: null }, v => v.length === 3)
await expectOk('統計: 時間別推移は24枠', 'get_hourly_trend', { start, end }, v => v.points.length === 24)
await expectOk('統計: ショートカット一覧', 'list_shortcuts', { start, end }, v => v.length === 3)
await expectOk('統計: ショートカット詳細', 'get_shortcut_detail', { shortcut: 'Ctrl+C', start, end }, v => v.total_count === 3)
await expectOk('統計: 統合ランキング', 'get_rankings_v014', { range: 'all', application: null })
await expectOk('統計: 使用率グラフ', 'get_usage_graph_v014', { mode: 'keys', range: 'all', identities: ['Ctrl+C'] }, v => v.series.length === 1)
await expectError('統計: 逆順の期間を拒否', 'get_dashboard_summary', { start: end, end: start }, 'DB_QUERY_FAILED')
await expectError('統計: 日付形式を検証', 'get_dashboard_summary', { start: '2026/11/10', end }, 'INVALID_DATE')
await expectError('統計: 期間キーを検証', 'get_rankings_v014', { range: 'bad', application: null }, 'INVALID_RANGE')

// --- 5. 出力系（隔離rootのDownloadsへ出力されること）。UI本体の api.js を経由する。
const ui = `(await import('/js/api.js')).api`
const exportResult = await evaluate(`(async () => {
  const api = ${ui}
  const call = promise => promise.then(value => ({ ok: true, value }), error => ({ ok: false, error }))
  return {
    oneDay: await call(api.exportDaily(undefined, '${seedDays[1]}', '${seedDays[1]}', 'ui-background')),
    allDays: await call(api.exportDaily(undefined, undefined, undefined, 'ui-background-all')),
    events: await call(window.__TAURI__.core.invoke('export_events_csv')),
    backup: await call(window.__TAURI__.core.invoke('backup_database')),
    keyMapping: await call(window.__TAURI__.core.invoke('export_key_mapping_json')),
  }
})()`)
for (const [label, result] of Object.entries(exportResult)) {
  record(`出力: ${label} は隔離rootへ書かれる`, result.ok && isInsideRoot(result.value.path), result.ok ? result.value.path : result.error)
}
const csvDates = file => [...new Set(fs.readFileSync(file, 'utf8').replace(/^﻿/, '').trim().split(/\r?\n/).slice(1).map(line => line.split(',')[0]))]
if (exportResult.oneDay.ok) {
  const dates = csvDates(exportResult.oneDay.value.path)
  record('日次CSV: UIで指定した期間だけを出力する', dates.length === 1 && dates[0] === seedDays[1], dates)
}
if (exportResult.allDays.ok) record('日次CSV: 期間未指定は全期間', csvDates(exportResult.allDays.value.path).length === 3)

// --- 6. 設定・除外アプリ（隔離した設定DB）
await expectOk('設定: テーマ変更', 'set_theme', { theme: 'light' }, v => v.theme === 'light')
await expectError('設定: 不正なテーマを拒否', 'set_theme', { theme: 'blue' }, 'SETTINGS_FAILED')
await expectOk('設定: テーマを戻す', 'set_theme', { theme: 'dark' })
await expectOk('設定: 連続入力抑制時間', 'set_duplicate_guard_seconds', { seconds: 1.5 }, v => v.duplicate_guard_seconds === 1.5)
await expectError('設定: 範囲外の抑制時間を拒否', 'set_duplicate_guard_seconds', { seconds: 11 }, 'SETTINGS_FAILED')
await expectOk('設定: 保持日数', 'set_detail_event_retention_days', { days: 7 })
await expectOk('除外: 追加', 'add_excluded_app', { executableName: 'ui-background-test.exe' }, v => v.some(a => a.executable_name === 'ui-background-test.exe'))
await expectError('除外: 不正な名前を拒否', 'add_excluded_app', { executableName: '..\\evil.exe' }, 'INVALID_EXECUTABLE_NAME')
await expectOk('除外: 無効化', 'set_excluded_app_enabled', { executableName: 'ui-background-test.exe', enabled: false })
await expectOk('除外: 削除', 'remove_user_excluded_app', { executableName: 'ui-background-test.exe' }, v => !v.some(a => a.executable_name === 'ui-background-test.exe'))
await expectOk('設定: グラフ色', 'set_usage_graph_colors', { colors: { 'Ctrl+C': '#112233' } })
await expectError('設定: 不正なグラフ色を拒否', 'set_usage_graph_colors', { colors: { 'Ctrl+C': 'red' } }, 'SETTINGS_FAILED')

// --- 7. キーマッピング編集セッション（未保存ガード）
const session = await expectOk('マッピング: 編集開始', 'begin_mapping_edit_session', {}, v => v.active && !v.dirty)
if (session) {
  await expectError('マッピング: 二重開始を拒否', 'begin_mapping_edit_session', {}, 'MAPPING_EDIT_ACTIVE')
  const draft = structuredClone(session.draft)
  draft.mapping.mouse.left_click = !draft.mapping.mouse.left_click
  await expectOk('マッピング: 下書き適用で未保存になる', 'apply_mapping_edit_draft', { draft }, v => v.dirty)
  await expectError('マッピング: 未保存のまま終了できない', 'end_mapping_edit_session', {}, 'MAPPING_EDIT_DIRTY')
  await expectOk('マッピング: 取り消し', 'cancel_mapping_edit_session', {}, v => !v.dirty)
  await expectOk('マッピング: 終了', 'end_mapping_edit_session', {}, v => !v.active)
}

// --- 8. 検証DBへ切り替え、隔離本番DBを母集団にサンプル生成
await expectOk('DB: 検証DBへ切替', 'switch_database_environment', { environment: 'test' }, v => v.is_test && isInsideRoot(v.path))
await expectOk('DB: サンプル生成', 'generate_sample_data', {}, v => v.events_generated > 0)
await expectOk('DB: 本番DBへ戻す', 'switch_database_environment', { environment: 'production' }, v => !v.is_test)

// --- 9. 全画面を巡回してJS例外を確認する
const pages = await evaluate(`[...document.querySelectorAll('.nav button[data-page]')].map(b => b.dataset.page)`)
for (const id of pages) {
  const before = report.errors.length
  await evaluate(`document.querySelector('.nav button[data-page="${id}"]').click()`)
  await sleep(1200)
  const active = await evaluate(`document.querySelector('.nav button.active')?.dataset.page`)
  let screenshot = 'unavailable'
  try {
    const shot = await send('Page.captureScreenshot', { format: 'png' }, 5000)
    if (shot.result?.data) {
      fs.writeFileSync(path.join(outDir, `page-${id}.png`), Buffer.from(shot.result.data, 'base64'))
      screenshot = `page-${id}.png`
    }
  } catch { /* 非表示WebViewでは描画されない場合がある */ }
  const newErrors = report.errors.slice(before)
  report.pages.push({ id, active, screenshot, newErrors })
  record(`画面: ${id}`, active === id && newErrors.length === 0, newErrors.length ? newErrors : undefined)
}
record('JS例外が発生していない', report.errors.length === 0, report.errors.length ? report.errors : undefined)

ws.close()
fs.writeFileSync(path.join(outDir, 'report.json'), JSON.stringify(report, null, 2))
const failed = report.checks.filter(check => !check.pass)
console.log(`\n${report.checks.length - failed.length}/${report.checks.length} checks passed.`)
process.exit(failed.length === 0 ? 0 : 1)
