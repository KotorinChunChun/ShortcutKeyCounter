import { api } from './api.js';
const titles = {
    dashboard: 'ダッシュボード', 'daily-ranking': '日刊ランキング', ranking: 'ランキング', usage: '使用率グラフ',
    applications: 'アプリ別ランキング', matrix: 'キー × アプリ見取り図',
    'shortcut-detail': 'ショートカット詳細', 'realtime-log': 'リアルタイムログ', 'key-mapping': 'キーマッピング',
    excluded: '除外アプリ', data: 'データ管理', settings: '設定', help: 'ヘルプ'
};
const rankingPages = ['ranking', 'usage', 'realtime-log', 'matrix'];
const rangeState = {
    dashboard: 'today', ranking: 'all', applications: 'all', matrix: '7', 'shortcut-detail': 'today'
};
let activePage = 'dashboard';
let polling = false;
let toastTimer = 0;
let cachedBounds = null;
let currentKeyMapping = null;
let currentKeyPolicy = null;
let realtimeSequence = 0;
let realtimeEntries = [];
let appInfo = null;
let elevatedBusy = false;
let realtimeLoading = false;
let realtimeReloadPending = false;
let currentOverlaySettings = null;
let currentOverlayMonitors = [];
let currentOverlayLabels = null;
let currentDatabaseEnvironment = null;
const selectedModifierColumns = new Set();
const activeMappingCategories = new Set(['alnum', 'modifier', 'function', 'system']);
let analyticsRange = 'today';
let analyticsSnapshot = null;
const analyticsSort = { key: { column: '', dir: 'none' }, app: { column: '', dir: 'none' } };
let usageMode = 'keys';
let usageRange = 'today';
let usageIdentities = [];
const usageEnabled = new Set();
let usageColors = {};
let dailyExportApplications = [];
const dailyExportSelected = new Set();
let usageSnapshot = null;
function applyTheme(theme) {
    document.documentElement.dataset.theme = theme;
    window.localStorage.setItem('shortcut-key-counter-theme', theme);
}
const uiScaleSteps = [0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2];
const uiScaleStorageKey = 'shortcut-key-counter-ui-scale';
const minUiScale = uiScaleSteps[0] ?? 1;
const maxUiScale = uiScaleSteps[uiScaleSteps.length - 1] ?? 1;
let uiScale = 1;
function applyUiScale(scale) {
    uiScale = Math.min(Math.max(scale, minUiScale), maxUiScale);
    document.documentElement.style.zoom = String(uiScale);
    window.localStorage.setItem(uiScaleStorageKey, String(uiScale));
}
function loadUiScale() {
    const cached = Number(window.localStorage.getItem(uiScaleStorageKey));
    if (Number.isFinite(cached) && uiScaleSteps.includes(cached))
        applyUiScale(cached);
    else
        applyUiScale(1);
}
function changeUiScale(direction) {
    const currentIndex = uiScaleSteps.reduce((closest, value, index) => Math.abs(value - uiScale) < Math.abs((uiScaleSteps[closest] ?? 1) - uiScale) ? index : closest, 0);
    const nextIndex = Math.min(Math.max(currentIndex + direction, 0), uiScaleSteps.length - 1);
    applyUiScale(uiScaleSteps[nextIndex] ?? uiScale);
}
function handleUiZoomShortcut(event) {
    if (!event.ctrlKey || event.altKey || event.metaKey)
        return;
    const isPlus = event.key === '+' || (event.code === 'Equal' && event.shiftKey);
    const isMinus = event.key === '-' || event.code === 'Minus';
    if (!isPlus && !isMinus)
        return;
    event.preventDefault();
    event.stopImmediatePropagation();
    changeUiScale(isPlus ? 1 : -1);
}
const cachedTheme = window.localStorage.getItem('shortcut-key-counter-theme');
if (cachedTheme === 'dark' || cachedTheme === 'light')
    applyTheme(cachedTheme);
loadUiScale();
function el(id) {
    const node = document.getElementById(id);
    if (!node)
        throw new Error(`Missing element: ${id}`);
    return node;
}
function escapeHtml(value) {
    return String(value ?? '').replace(/[&<>'"]/g, char => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', "'": '&#39;', '"': '&quot;' }[char] ?? char));
}
function showToast(message, action) {
    const toast = el('toast');
    el('toastText').textContent = message;
    const actionButton = el('toastAction');
    actionButton.style.display = action ? '' : 'none';
    actionButton.textContent = action?.label ?? 'フォルダーを開く';
    actionButton.onclick = action ? () => void action.run().catch(error => showToast(errorMessage(error))) : null;
    toast.classList.toggle('has-action', Boolean(action));
    toast.classList.add('show');
    window.clearTimeout(toastTimer);
    toastTimer = window.setTimeout(() => toast.classList.remove('show'), action ? 8000 : 3200);
}
function errorMessage(error) {
    if (typeof error === 'object' && error !== null && 'message' in error) {
        return String(error.message);
    }
    return String(error);
}
function formatNumber(value) { return new Intl.NumberFormat('ja-JP').format(value); }
function formatDuration(seconds) {
    const safe = Math.max(0, Math.round(seconds));
    const h = Math.floor(safe / 3600), m = Math.floor((safe % 3600) / 60);
    if (h)
        return `${h}時間${m ? ` ${m}分` : ''}`;
    return `${m}分`;
}
function todayText(date = new Date()) {
    const y = date.getFullYear();
    const m = String(date.getMonth() + 1).padStart(2, '0');
    const d = String(date.getDate()).padStart(2, '0');
    return `${y}-${m}-${d}`;
}
function addDays(dateText, offset) {
    const [y, m, d] = dateText.split('-').map(Number);
    if (!y || !m || !d)
        return dateText;
    const date = new Date(y, m - 1, d);
    date.setDate(date.getDate() + offset);
    return todayText(date);
}
async function resolveRange(key) {
    const end = todayText();
    if (key === 'today')
        return { start: end, end };
    if (key === '7')
        return { start: addDays(end, -6), end };
    if (key === '30')
        return { start: addDays(end, -29), end };
    cachedBounds ?? (cachedBounds = await api.dataBounds());
    return {
        start: cachedBounds.first_date ?? end,
        end: cachedBounds.last_date ?? end,
    };
}
function rangeLabel(range) {
    return range.start === range.end ? range.start : `${range.start} ～ ${range.end}`;
}
function kbd(shortcut) {
    return `<span class="kbd">${shortcut.split('+').map(key => `<span class="key">${escapeHtml(key)}</span>`).join('+')}</span>`;
}
function emptyRow(columns, message = 'データがありません。') {
    return `<tr><td colspan="${columns}" class="empty">${escapeHtml(message)}</td></tr>`;
}
function renderBars(container, rows) {
    if (!rows.length) {
        container.innerHTML = '<div class="empty">データがありません。</div>';
        return;
    }
    const max = Math.max(...rows.map(row => row.count), 1);
    container.innerHTML = rows.map(row => `<div class="app-row"><span class="exe">${escapeHtml(row.name)}</span><div class="bar-track"><div class="bar" style="width:${Math.max(2, row.count / max * 100)}%"></div></div><b>${formatNumber(row.count)}</b></div>`).join('');
}
function renderChart(container, points) {
    if (!points.length) {
        container.innerHTML = '<div class="empty" style="width:100%">データがありません。</div>';
        return;
    }
    const max = Math.max(...points.map(point => point.count), 1);
    container.innerHTML = points.map(point => {
        const [, month, day] = point.date.split('-');
        const label = month && day ? `${Number(month)}/${Number(day)}` : point.date;
        const height = point.count === 0 ? 1 : Math.max(3, point.count / max * 100);
        return `<div class="day" title="${escapeHtml(point.date)}: ${formatNumber(point.count)}"><span style="height:${height}%"></span><label>${escapeHtml(label)}</label></div>`;
    }).join('');
}
async function refreshCaptureStatus() {
    try {
        const [status, health] = await Promise.all([api.captureStatus(), api.captureHealth()]);
        renderCaptureStatus(status, health);
    }
    catch (error) {
        const chip = el('statusChip');
        chip.innerHTML = '<span style="width:7px;height:7px;border-radius:50%;background:#ef6b73"></span><span>状態不明</span>';
        el('healthLine').textContent = errorMessage(error);
    }
}
function renderCaptureStatus(status, health) {
    const chip = el('statusChip');
    const button = el('captureBtn');
    chip.innerHTML = status.enabled
        ? '<span class="dot"></span><span>計測中</span>'
        : '<span style="width:7px;height:7px;border-radius:50%;background:#ef6b73"></span><span>停止中</span>';
    chip.classList.toggle('stopped', !status.enabled);
    button.textContent = status.enabled ? '計測を停止' : '計測を再開';
    button.classList.toggle('off', !status.enabled);
    const source = health.source === 'elevated' ? '通常権限 + 管理者権限Capture' : health.source === 'local_fallback' ? '通常権限（helper切断）' : '通常権限Capture';
    el('healthLine').textContent = `${source}${status.dropped_queue_items ? ` / queue drop ${status.dropped_queue_items}` : ''}`;
}
async function loadDashboard() {
    const range = await resolveRange(rangeState.dashboard ?? 'today');
    const [summary, trend, apps, shortcuts] = await Promise.all([
        api.dashboard(range.start, range.end),
        api.dailyTrend(range.start, range.end),
        api.applicationRanking(range.start, range.end, 4),
        api.shortcutRankingExtended(range.start, range.end, null, 5),
    ]);
    let topApp = '';
    if (summary.top_shortcut) {
        const detail = await api.shortcutDetail(summary.top_shortcut.shortcut, range.start, range.end);
        topApp = detail.applications[0]?.application ?? '';
    }
    el('dashboardCards').innerHTML = `
    <div class="card"><div class="card-label">ショートカット利用回数</div><div class="card-value">${formatNumber(summary.total_count)}</div><div class="card-note">${escapeHtml(rangeLabel(range))}</div></div>
    <div class="card"><div class="card-label">ユニークショートカット</div><div class="card-value">${formatNumber(summary.unique_shortcuts)}</div><div class="card-note">選択期間の種類数</div></div>
    <div class="card"><div class="card-label">利用アプリ</div><div class="card-value">${formatNumber(summary.unique_applications)}</div><div class="card-note">記録対象アプリ数</div></div>
    <div class="card"><div class="card-label">最多ショートカット</div><div class="card-value small">${summary.top_shortcut ? kbd(summary.top_shortcut.shortcut) : '—'}</div><div class="card-note">${summary.top_shortcut ? `${formatNumber(summary.top_shortcut.count)}回${topApp ? ` / ${escapeHtml(topApp)}` : ''}` : 'データなし'}</div></div>`;
    el('dashboardTrendLabel').textContent = rangeLabel(range);
    renderChart(el('dashboardTrend'), trend);
    renderBars(el('dashboardApps'), apps.map(row => ({ name: row.application, count: row.count })));
    renderShortcutRanking(el('dashboardRanking'), shortcuts);
}
function renderShortcutRanking(container, rows) {
    if (!rows.length) {
        container.innerHTML = emptyRow(5);
        return;
    }
    const max = Math.max(...rows.map(row => row.count), 1);
    container.innerHTML = rows.map((row, index) => `<tr class="clickable" data-shortcut="${escapeHtml(row.shortcut)}"><td>${index + 1}</td><td>${kbd(row.shortcut)}</td><td><b>${formatNumber(row.count)}</b></td><td>${row.application_count}</td><td><div class="bar-track"><div class="bar" style="width:${row.count / max * 100}%"></div></div></td></tr>`).join('');
    container.querySelectorAll('tr[data-shortcut]').forEach(row => row.addEventListener('click', () => {
        const shortcut = row.dataset.shortcut;
        if (shortcut)
            void openShortcutDetail(shortcut);
    }));
}
function treeRatio(value, total) {
    return total > 0 ? `${(value / total * 100).toFixed(1)}%` : '0.0%';
}
function removeTreeChildren(body, parentIndex) {
    body.querySelectorAll(`[data-tree-child="${parentIndex}"]`).forEach(node => node.remove());
}
function renderKeyTree(body, rows, range, showApplicationCount, applicationFilter = null) {
    const columns = showApplicationCount ? 5 : 4;
    if (!rows.length) {
        body.innerHTML = emptyRow(columns);
        return;
    }
    const total = rows.reduce((sum, row) => sum + row.count, 0);
    body.innerHTML = rows.map((row, index) => `<tr class="tree-parent clickable" data-tree-index="key-${index}" data-shortcut="${escapeHtml(row.shortcut)}"><td>${index + 1}</td><td><span class="tree-arrow">▸</span>${kbd(row.shortcut)}</td><td><b>${formatNumber(row.count)}</b></td>${showApplicationCount ? `<td>${row.application_count}</td>` : ''}<td>${treeRatio(row.count, total)}</td></tr>`).join('');
    body.querySelectorAll('tr[data-shortcut]').forEach(parent => parent.addEventListener('click', async () => {
        const shortcut = parent.dataset.shortcut;
        const parentIndex = parent.dataset.treeIndex;
        if (!shortcut || !parentIndex)
            return;
        const arrow = parent.querySelector('.tree-arrow');
        if (parent.classList.toggle('expanded') === false) {
            removeTreeChildren(body, parentIndex);
            if (arrow)
                arrow.textContent = '▸';
            return;
        }
        if (arrow)
            arrow.textContent = '▾';
        try {
            const detail = await api.shortcutDetail(shortcut, range.start, range.end);
            const applications = applicationFilter ? detail.applications.filter(row => row.application === applicationFilter) : detail.applications;
            const childHtml = applications.length
                ? applications.map(app => `<tr class="tree-child" data-tree-child="${parentIndex}"><td></td><td><span class="tree-branch">└</span><span class="exe">${escapeHtml(app.application)}</span></td><td>${formatNumber(app.count)}</td>${showApplicationCount ? '<td></td>' : ''}<td>${treeRatio(app.count, detail.total_count)}</td></tr>`).join('')
                : `<tr class="tree-child" data-tree-child="${parentIndex}"><td></td><td colspan="${columns - 1}" class="muted">内訳がありません。</td></tr>`;
            parent.insertAdjacentHTML('afterend', childHtml);
        }
        catch (error) {
            parent.classList.remove('expanded');
            if (arrow)
                arrow.textContent = '▸';
            showToast(errorMessage(error));
        }
    }));
}
function renderApplicationTree(body, rows, range, showUniqueShortcuts, showUsage = false) {
    const columns = 4 + (showUniqueShortcuts ? 1 : 0) + (showUsage ? 1 : 0);
    if (!rows.length) {
        body.innerHTML = emptyRow(columns);
        return;
    }
    const total = rows.reduce((sum, row) => sum + row.count, 0);
    body.innerHTML = rows.map((row, index) => `<tr class="tree-parent clickable" data-tree-index="app-${index}" data-application="${escapeHtml(row.application)}"><td>${index + 1}</td><td><span class="tree-arrow">▸</span><span class="exe">${escapeHtml(row.application)}</span></td><td><b>${formatNumber(row.count)}</b></td>${showUniqueShortcuts ? `<td>${row.unique_shortcuts}</td>` : ''}${showUsage ? `<td>${formatDuration(row.active_seconds)}</td>` : ''}<td>${treeRatio(row.count, total)}</td></tr>`).join('');
    body.querySelectorAll('tr[data-application]').forEach(parent => parent.addEventListener('click', async () => {
        const application = parent.dataset.application;
        const parentIndex = parent.dataset.treeIndex;
        if (!application || !parentIndex)
            return;
        const arrow = parent.querySelector('.tree-arrow');
        if (parent.classList.toggle('expanded') === false) {
            removeTreeChildren(body, parentIndex);
            if (arrow)
                arrow.textContent = '▸';
            return;
        }
        if (arrow)
            arrow.textContent = '▾';
        try {
            const shortcuts = await api.shortcutRankingExtended(range.start, range.end, application, 100);
            const rowTotal = rows.find(item => item.application === application)?.count ?? 0;
            const childHtml = shortcuts.length
                ? shortcuts.map(row => `<tr class="tree-child" data-tree-child="${parentIndex}"><td></td><td><span class="tree-branch">└</span>${kbd(row.shortcut)}</td><td>${formatNumber(row.count)}</td>${showUniqueShortcuts ? '<td></td>' : ''}${showUsage ? '<td></td>' : ''}<td>${treeRatio(row.count, rowTotal)}</td></tr>`).join('')
                : `<tr class="tree-child" data-tree-child="${parentIndex}"><td></td><td colspan="${columns - 1}" class="muted">内訳がありません。</td></tr>`;
            parent.insertAdjacentHTML('afterend', childHtml);
        }
        catch (error) {
            parent.classList.remove('expanded');
            if (arrow)
                arrow.textContent = '▸';
            showToast(errorMessage(error));
        }
    }));
}
async function loadDailyRanking() {
    const dateInput = el('dailyRankingDate');
    if (!dateInput.value)
        dateInput.value = todayText();
    const date = dateInput.value || todayText();
    const range = { start: date, end: date };
    const [keys, apps] = await Promise.all([
        api.shortcutRankingExtended(date, date, null, 100),
        api.applicationStatistics(date, date),
    ]);
    renderKeyTree(el('dailyKeyRankingBody'), keys, range, false);
    renderApplicationTree(el('dailyAppRankingBody'), apps, range, false);
}
function comparisonHtml(value) {
    if (value === null || !Number.isFinite(value))
        return '<span class="delta-flat">—</span>';
    const cls = value > 0 ? 'delta-up' : value < 0 ? 'delta-down' : 'delta-flat';
    return `<span class="${cls}">${value > 0 ? '+' : ''}${value.toFixed(1)}%</span>`;
}
function sortAnalyticsRows(rows, state) {
    if (!state.column || state.dir === 'none')
        return rows.slice();
    const column = state.column;
    return rows.slice().sort((a, b) => {
        const av = a[column], bv = b[column];
        let result = 0;
        if (typeof av === 'string' && typeof bv === 'string')
            result = av.localeCompare(bv, 'ja');
        else
            result = Number(av ?? Number.NEGATIVE_INFINITY) - Number(bv ?? Number.NEGATIVE_INFINITY);
        return state.dir === 'desc' ? -result : result;
    });
}
function syncAnalyticsSortButtons() {
    document.querySelectorAll('.sort-button-v014').forEach(button => {
        const table = button.closest('table')?.id === 'keyRankingV014' ? 'key' : 'app';
        const state = analyticsSort[table];
        button.dataset.dir = state.column === button.dataset.sort ? state.dir : 'none';
    });
}
function renderRankingV014(snapshot) {
    analyticsSnapshot = snapshot;
    document.querySelectorAll('.comparison-header').forEach(button => {
        const icon = button.querySelector('.sort-icons')?.outerHTML ?? '';
        button.innerHTML = `${escapeHtml(snapshot.comparison_label ?? '比較')} ${icon}`;
    });
    const keyRows = sortAnalyticsRows(snapshot.key_rows, analyticsSort.key);
    el('keyRankingV014Body').innerHTML = keyRows.length ? keyRows.map((row, index) => `<tr><td>${index + 1}</td><td>${kbd(row.shortcut)}</td><td><b>${formatNumber(row.count)}</b></td><td>${row.average.toLocaleString('ja-JP', { maximumFractionDigits: 1 })}</td><td>${row.ratio.toFixed(1)}%</td><td>${comparisonHtml(row.comparison_rate)}</td></tr>`).join('') : emptyRow(6);
    const appRows = sortAnalyticsRows(snapshot.application_rows, analyticsSort.app);
    el('appRankingV014Body').innerHTML = appRows.length ? appRows.map((row, index) => `<tr><td>${index + 1}</td><td class="exe">${escapeHtml(row.application)}</td><td><b>${formatNumber(row.count)}</b></td><td>${formatNumber(row.key_type_count)}</td><td>${formatDuration(row.active_seconds)}</td><td>${row.ratio.toFixed(1)}%</td><td>${comparisonHtml(row.comparison_rate)}</td></tr>`).join('') : emptyRow(7);
    syncAnalyticsSortButtons();
}
async function loadRanking() {
    const filter = el('analyticsAppFilter');
    const selected = filter.value;
    const snapshot = await api.rankingsV014(analyticsRange, selected || null);
    filter.innerHTML = '<option value="">すべて</option>' + snapshot.applications.map(name => `<option value="${escapeHtml(name)}">${escapeHtml(name)}</option>`).join('');
    if (selected && snapshot.applications.includes(selected))
        filter.value = selected;
    renderRankingV014(snapshot);
}
function usageIdentityKey(identity) { return `${usageMode === 'keys' ? 'key' : 'app'}:${identity}`; }
function usageDefaultColor(index) {
    const palette = ['#2563eb', '#e11d48', '#16a34a', '#9333ea', '#ea580c', '#0891b2', '#ca8a04', '#4f46e5', '#be123c', '#0f766e'];
    return palette[index % palette.length] ?? '#2563eb';
}
function usageColor(identity, index) { return usageColors[usageIdentityKey(identity)] ?? usageDefaultColor(index); }
async function ensureUsageCatalog(reset = false) {
    const ranking = await api.rankingsV014(usageRange, null);
    const next = usageMode === 'keys' ? ranking.key_rows.map(row => row.shortcut) : ranking.application_rows.map(row => row.application);
    const changed = reset || next.join('\u0000') !== usageIdentities.join('\u0000');
    usageIdentities = next;
    if (changed) {
        usageEnabled.clear();
        usageIdentities.slice(0, 5).forEach(identity => usageEnabled.add(identity));
    }
    else {
        for (const identity of [...usageEnabled])
            if (!usageIdentities.includes(identity))
                usageEnabled.delete(identity);
    }
}
function renderUsageSeriesList() {
    el('usageSeriesTitle').textContent = usageMode === 'keys' ? 'キーランキング' : 'アプリランキング';
    el('usageGraphTitle').textContent = usageMode === 'keys' ? '上位キーの使用回数推移' : '上位アプリのショートカット使用回数推移';
    const list = el('usageSeriesList');
    list.innerHTML = usageIdentities.length ? usageIdentities.map((identity, index) => `<div class="usage-series-item ${usageEnabled.has(identity) ? '' : 'off'}" data-identity="${escapeHtml(identity)}"><span class="usage-rank">${index + 1}</span><span class="usage-check">✓</span><span>${escapeHtml(identity)}</span><button class="usage-swatch" data-color-identity="${escapeHtml(identity)}" style="background:${usageColor(identity, index)}" title="系列色を変更"></button></div>`).join('') : '<div class="empty">データがありません。</div>';
}
function drawUsageGraph() {
    const canvas = el('usageCanvas');
    const parent = canvas.parentElement;
    const rect = parent?.getBoundingClientRect();
    const ratio = window.devicePixelRatio || 1;
    const cssWidth = Math.max(500, Math.floor(rect?.width ?? 900) - 24);
    const cssHeight = 410;
    canvas.style.width = `${cssWidth}px`;
    canvas.style.height = `${cssHeight}px`;
    canvas.width = Math.floor(cssWidth * ratio);
    canvas.height = Math.floor(cssHeight * ratio);
    const ctx = canvas.getContext('2d');
    if (!ctx)
        return;
    ctx.scale(ratio, ratio);
    ctx.clearRect(0, 0, cssWidth, cssHeight);
    const snapshot = usageSnapshot;
    if (!snapshot || !snapshot.buckets.length || !snapshot.series.length) {
        ctx.fillStyle = getComputedStyle(document.documentElement).getPropertyValue('--muted').trim() || '#8fa0b3';
        ctx.font = '13px Segoe UI';
        ctx.fillText('表示するデータがありません。', 24, 36);
        return;
    }
    const left = 50, right = 18, top = 18, bottom = 48, width = cssWidth - left - right, height = cssHeight - top - bottom;
    const max = Math.max(1, ...snapshot.series.flatMap(series => series.points.map(point => point.count)));
    const style = getComputedStyle(document.documentElement), line = style.getPropertyValue('--line').trim() || '#263241', muted = style.getPropertyValue('--muted').trim() || '#8fa0b3';
    ctx.strokeStyle = line;
    ctx.lineWidth = 1;
    ctx.fillStyle = muted;
    ctx.font = '10px Segoe UI';
    for (let i = 0; i <= 4; i++) {
        const y = top + height * i / 4;
        ctx.beginPath();
        ctx.moveTo(left, y);
        ctx.lineTo(left + width, y);
        ctx.stroke();
        ctx.fillText(formatNumber(Math.round(max * (1 - i / 4))), 4, y + 3);
    }
    const labelEvery = Math.max(1, Math.ceil(snapshot.buckets.length / 8));
    snapshot.buckets.forEach((bucket, i) => { if (i % labelEvery !== 0 && i !== snapshot.buckets.length - 1)
        return; const x = left + (snapshot.buckets.length <= 1 ? 0 : width * i / (snapshot.buckets.length - 1)); ctx.fillText(bucket, x - 12, top + height + 22); });
    snapshot.series.forEach((series, index) => { ctx.strokeStyle = usageColor(series.identity, usageIdentities.indexOf(series.identity)); ctx.lineWidth = 2; ctx.beginPath(); series.points.forEach((point, i) => { const x = left + (series.points.length <= 1 ? 0 : width * i / (series.points.length - 1)); const y = top + height - (point.count / max * height); if (i === 0)
        ctx.moveTo(x, y);
    else
        ctx.lineTo(x, y); }); ctx.stroke(); });
    el('usageAxisNote').textContent = `横軸: ${snapshot.bucket_kind === 'hour' ? '時間' : snapshot.bucket_kind === 'day' ? '日' : '月'} / 縦軸: 回数`;
    el('usageLegend').innerHTML = snapshot.series.map((series, index) => `<span class="usage-legend-item"><i class="usage-legend-dot" style="background:${usageColor(series.identity, usageIdentities.indexOf(series.identity))}"></i>${escapeHtml(series.identity)}</span>`).join('');
}
async function loadUsage(resetCatalog = false) {
    if (!Object.keys(usageColors).length)
        usageColors = await api.usageGraphColors().catch(() => ({}));
    await ensureUsageCatalog(resetCatalog);
    renderUsageSeriesList();
    const selected = usageIdentities.filter(identity => usageEnabled.has(identity)).slice(0, 20);
    usageSnapshot = await api.usageGraphV014(usageMode, usageRange, selected);
    drawUsageGraph();
}
async function loadApplications() {
    const range = await resolveRange(rangeState.applications ?? 'all');
    const rows = await api.applicationStatistics(range.start, range.end);
    const total = rows.reduce((sum, row) => sum + row.count, 0);
    const top = rows.slice(0, 3);
    const other = rows.slice(3).reduce((sum, row) => sum + row.count, 0);
    const cards = [...top.map(row => ({ name: row.application, count: row.count })), ...(other ? [{ name: 'その他', count: other }] : [])];
    el('applicationCards').innerHTML = cards.length ? cards.map(row => `<div class="card"><div class="card-label exe">${escapeHtml(row.name)}</div><div class="card-value">${formatNumber(row.count)}</div><div class="card-note">${total ? (row.count / total * 100).toFixed(1) : '0.0'}%</div></div>`).join('') : '<div class="card"><div class="card-label">データ</div><div class="card-value">0</div><div class="card-note">まだ記録がありません</div></div>';
    renderApplicationTree(el('applicationBody'), rows, range, true, true);
}
async function loadShortcutDetail(preferred) {
    const range = await resolveRange(rangeState['shortcut-detail'] ?? 'today');
    const select = el('shortcutSelect');
    const before = preferred ?? select.value;
    const catalog = await api.listShortcuts(range.start, range.end);
    select.innerHTML = catalog.length ? catalog.map(row => `<option value="${escapeHtml(row.shortcut)}">${escapeHtml(row.shortcut)} (${formatNumber(row.count)})</option>`).join('') : '<option value="">データなし</option>';
    const values = catalog.map(row => row.shortcut);
    if (before && values.includes(before))
        select.value = before;
    const shortcut = select.value;
    if (!shortcut) {
        el('selectedShortcut').textContent = '—';
        el('detailTotal').textContent = '0';
        el('detailAppCount').textContent = '0';
        renderBars(el('detailApps'), []);
        renderChart(el('detailTrend'), []);
        return;
    }
    const detail = await api.shortcutDetail(shortcut, range.start, range.end);
    el('selectedShortcut').innerHTML = kbd(detail.shortcut);
    el('detailTotal').textContent = formatNumber(detail.total_count);
    el('detailAppCount').textContent = formatNumber(detail.applications.length);
    renderBars(el('detailApps'), detail.applications.map(row => ({ name: row.application, count: row.count })));
    renderChart(el('detailTrend'), detail.daily);
}
async function openShortcutDetail(shortcut) {
    goPage('shortcut-detail');
    await loadShortcutDetail(shortcut);
}
function renderExcluded(rows) {
    const body = el('excludeBody');
    if (!rows.length) {
        body.innerHTML = emptyRow(4);
        return;
    }
    body.innerHTML = rows.map(row => `<tr><td class="exe">${escapeHtml(row.executable_name)}</td><td><span class="pill">${escapeHtml(row.source)}</span></td><td><span class="${row.enabled ? 'status-ok' : 'status-off'}">${row.enabled ? '除外中' : '解除済み'}</span></td><td>${row.source === 'user' ? `<button class="btn ghost" data-remove="${escapeHtml(row.executable_name)}">削除</button>` : `<button class="btn ghost" data-toggle="${escapeHtml(row.executable_name)}" data-enabled="${row.enabled}">${row.enabled ? '解除' : '再度除外'}</button>`}</td></tr>`).join('');
    body.querySelectorAll('button[data-remove]').forEach(button => button.addEventListener('click', async () => {
        const name = button.dataset.remove;
        if (!name)
            return;
        try {
            renderExcluded(await api.removeExcludedApp(name));
            showToast(`${name} を除外リストから削除しました。`);
        }
        catch (error) {
            showToast(errorMessage(error));
        }
    }));
    body.querySelectorAll('button[data-toggle]').forEach(button => button.addEventListener('click', async () => {
        const name = button.dataset.toggle;
        const enabled = button.dataset.enabled === 'true';
        if (!name)
            return;
        if (enabled && !window.confirm('デフォルト除外を解除しますか？\n機密入力を扱う可能性があるアプリです。'))
            return;
        try {
            renderExcluded(await api.setExcludedEnabled(name, !enabled));
            showToast(`${name} の除外設定を更新しました。`);
        }
        catch (error) {
            showToast(errorMessage(error));
        }
    }));
}
async function loadExcluded() { renderExcluded(await api.excludedApps()); }
function setSwitch(node, enabled) { node.classList.toggle('on', enabled); }
function renderRepeatCounterPreview(settings) {
    const theme = settings.repeat_counter;
    const badge = el('repeatCounterBadge');
    badge.style.backgroundColor = theme.background_color;
    badge.style.color = theme.text_color;
    badge.style.setProperty('--repeat-accent', theme.accent_color);
    badge.style.fontSize = `${theme.font_size}px`;
    badge.style.transform = `rotate(${theme.rotation_deg}deg)`;
    badge.style.borderRadius = theme.shape === 'pill' ? '999px' : theme.shape === 'rounded' ? '7px' : '2px';
}
function syncRepeatCounterDraftFromUi() {
    if (!currentOverlaySettings)
        return;
    const bg = el('repeatCounterBackground').value.trim();
    const text = el('repeatCounterText').value.trim();
    const accent = el('repeatCounterAccent').value.trim();
    currentOverlaySettings.repeat_counter = {
        ...currentOverlaySettings.repeat_counter,
        background_color: bg, text_color: text, accent_color: accent,
        shape: el('repeatCounterShape').value,
        rotation_deg: Number(el('repeatCounterRotation').value),
        font_size: Number(el('repeatCounterFontSize').value),
    };
    renderRepeatCounterPreview(currentOverlaySettings);
}
function renderOverlaySettings(settings) {
    currentOverlaySettings = structuredClone(settings);
    setSwitch(el('overlaySwitch'), settings.enabled);
    el('overlayDuration').value = (settings.display_duration_ms / 1000).toFixed(1);
    el('overlayMaxRows').value = String(settings.max_rows);
    el('overlayBackgroundColor').value = settings.background_color;
    el('overlayBackgroundColorPicker').value = settings.background_color;
    el('overlayBackgroundOpacity').value = String(Math.round(settings.background_opacity * 100));
    el('overlayOpacityValue').textContent = `${Math.round(settings.background_opacity * 100)}%`;
    el('overlayDisplayMode').value = settings.display_mode;
    el('overlayMonitorMode').value = settings.monitor_mode;
    renderOverlayMonitorOptions(settings);
    el('overlayEnterAnimation').value = settings.enter_animation;
    el('overlayExitAnimation').value = settings.exit_animation;
    el('overlayEnterMs').value = String(settings.enter_ms);
    el('overlayExitMs').value = String(settings.exit_ms);
    el('repeatCounterBackground').value = settings.repeat_counter.background_color;
    el('repeatCounterBackgroundPicker').value = settings.repeat_counter.background_color;
    el('repeatCounterText').value = settings.repeat_counter.text_color;
    el('repeatCounterTextPicker').value = settings.repeat_counter.text_color;
    el('repeatCounterAccent').value = settings.repeat_counter.accent_color;
    el('repeatCounterAccentPicker').value = settings.repeat_counter.accent_color;
    el('repeatCounterShape').value = settings.repeat_counter.shape;
    el('repeatCounterRotation').value = String(settings.repeat_counter.rotation_deg);
    el('repeatCounterFontSize').value = String(settings.repeat_counter.font_size);
    renderRepeatCounterPreview(settings);
    document.querySelectorAll('#overlayPositionMode button[data-mode]').forEach(button => {
        button.classList.toggle('active', button.dataset.mode === settings.position_mode);
    });
    const grid = el('overlayPositionGrid');
    grid.classList.toggle('is-auto', settings.position_mode === 'auto');
    grid.setAttribute('aria-label', settings.position_mode === 'auto' ? '自動配置で許可する位置（複数選択）' : '固定表示位置（単一選択）');
    grid.querySelectorAll('button[data-position]').forEach(button => {
        const position = button.dataset.position;
        const active = settings.position_mode === 'auto' ? settings.allowed_positions.includes(position) : settings.fixed_position === position;
        button.classList.toggle('active', active);
        button.classList.toggle('preferred', settings.position_mode === 'auto' && settings.preferred_position === position);
        button.disabled = false;
        button.setAttribute('aria-pressed', String(active));
        button.title = settings.position_mode === 'auto' && settings.preferred_position === position ? '優先位置' : '';
    });
}
function renderOverlayMonitorOptions(settings) {
    const select = el('overlayMonitorSelect');
    const current = settings.fixed_monitor_id;
    if (!currentOverlayMonitors.length) {
        select.innerHTML = '<option value="">モニター情報を取得できません</option>';
    }
    else {
        const choose = settings.monitor_mode === 'fixed' && !current && currentOverlayMonitors.length > 1
            ? '<option value="">モニターを選択...</option>'
            : '';
        select.innerHTML = choose + currentOverlayMonitors.map((monitor, index) => `<option value="${escapeHtml(monitor.id)}">${index + 1}: ${escapeHtml(monitor.label)}</option>`).join('');
    }
    if (current && currentOverlayMonitors.some(monitor => monitor.id === current))
        select.value = current;
    else if (currentOverlayMonitors.length === 1)
        select.value = currentOverlayMonitors[0]?.id ?? '';
    else
        select.value = '';
    select.disabled = settings.monitor_mode !== 'fixed' || settings.display_mode === 'active-window' || !currentOverlayMonitors.length;
}
function overlaySettingsFromUi() {
    if (!currentOverlaySettings)
        return null;
    const mode = document.querySelector('#overlayPositionMode button.active')?.dataset.mode;
    const durationSeconds = Number(el('overlayDuration').value);
    const maxRows = Number(el('overlayMaxRows').value);
    const opacity = Number(el('overlayBackgroundOpacity').value) / 100;
    const enterMs = Number(el('overlayEnterMs').value);
    const exitMs = Number(el('overlayExitMs').value);
    const color = el('overlayBackgroundColor').value.trim();
    const displayMode = el('overlayDisplayMode').value;
    const monitorMode = el('overlayMonitorMode').value;
    const fixedMonitorId = el('overlayMonitorSelect').value;
    const repeatRotation = Number(el('repeatCounterRotation').value);
    const repeatFontSize = Number(el('repeatCounterFontSize').value);
    if (!mode || !displayMode || !monitorMode || !Number.isFinite(durationSeconds) || !Number.isFinite(maxRows) || !Number.isFinite(opacity) || !Number.isFinite(enterMs) || !Number.isFinite(exitMs) || !Number.isFinite(repeatRotation) || !Number.isFinite(repeatFontSize))
        return null;
    const activePositions = Array.from(document.querySelectorAll('#overlayPositionGrid button.active'))
        .map(button => button.dataset.position);
    let fixedPosition = currentOverlaySettings.fixed_position;
    let allowedPositions = currentOverlaySettings.allowed_positions.slice();
    let preferredPosition = currentOverlaySettings.preferred_position;
    if (mode === 'fixed') {
        fixedPosition = activePositions[0] ?? fixedPosition;
    }
    else {
        allowedPositions = activePositions;
        if (!allowedPositions.includes(preferredPosition))
            preferredPosition = allowedPositions[0] ?? preferredPosition;
    }
    return {
        ...currentOverlaySettings,
        display_duration_ms: Math.round(durationSeconds * 1000),
        display_mode: displayMode,
        monitor_mode: monitorMode,
        fixed_monitor_id: fixedMonitorId,
        position_mode: mode,
        fixed_position: fixedPosition,
        allowed_positions: allowedPositions,
        preferred_position: preferredPosition,
        max_rows: Math.round(maxRows),
        background_color: color,
        background_opacity: opacity,
        enter_animation: el('overlayEnterAnimation').value,
        exit_animation: el('overlayExitAnimation').value,
        enter_ms: Math.round(enterMs),
        exit_ms: Math.round(exitMs),
        repeat_counter: {
            background_color: el('repeatCounterBackground').value.trim(),
            text_color: el('repeatCounterText').value.trim(),
            accent_color: el('repeatCounterAccent').value.trim(),
            shape: el('repeatCounterShape').value,
            rotation_deg: repeatRotation,
            font_size: repeatFontSize,
        },
    };
}
function validateOverlaySettings(settings) {
    if (settings.display_duration_ms < 100 || settings.display_duration_ms > 10000)
        return '表示時間は0.1〜10秒で指定してください。';
    if (settings.max_rows < 1 || settings.max_rows > 10)
        return '最大表示行数は1〜10行で指定してください。';
    if (!/^#[0-9a-fA-F]{6}$/.test(settings.background_color))
        return '背景色は #RRGGBB 形式で指定してください。';
    if (settings.background_opacity < 0 || settings.background_opacity > 1)
        return '背景透明度は0〜100%で指定してください。';
    if (settings.enter_ms < 0 || settings.enter_ms > 2000 || settings.exit_ms < 0 || settings.exit_ms > 2000)
        return 'アニメーション時間は0〜2000msで指定してください。';
    if (settings.position_mode === 'auto' && settings.allowed_positions.length === 0)
        return 'アクティブウィンドウ回避では、許可位置を1つ以上選択してください。';
    if (settings.monitor_mode === 'fixed' && settings.display_mode === 'desktop' && !settings.fixed_monitor_id)
        return '固定モニターを選択してください。';
    for (const [label, color] of [['背景色', settings.repeat_counter.background_color], ['文字色', settings.repeat_counter.text_color], ['枠／影色', settings.repeat_counter.accent_color]]) {
        if (!/^#[0-9a-fA-F]{6}$/.test(color))
            return `連打カウンターの${label}は #RRGGBB 形式で指定してください。`;
    }
    if (settings.repeat_counter.rotation_deg < -20 || settings.repeat_counter.rotation_deg > 20)
        return '連打カウンターの回転角は-20〜20°で指定してください。';
    if (settings.repeat_counter.font_size < 8 || settings.repeat_counter.font_size > 32)
        return '連打カウンターのフォントサイズは8〜32pxで指定してください。';
    return null;
}
async function saveOverlaySettingsFromUi(showMessage = true) {
    const settings = overlaySettingsFromUi();
    if (!settings)
        return;
    const validationError = validateOverlaySettings(settings);
    if (validationError) {
        showToast(validationError);
        renderOverlaySettings(currentOverlaySettings ?? settings);
        return;
    }
    currentOverlaySettings = await api.setOverlaySettings(settings);
    // select操作中のDOMを作り直さない。保存後の値は次回画面表示時に再同期する。
    if (showMessage)
        showToast('オーバーレイ設定を保存しました。');
}
async function renderSettings(settings) {
    const [resolvedSettings, status, health, paths, overlaySettings, monitors] = await Promise.all([
        settings ? Promise.resolve(settings) : api.settings(), api.captureStatus(), api.captureHealth(), api.appPaths(), api.overlaySettings(), api.overlayMonitors()
    ]);
    applyTheme(resolvedSettings.theme);
    document.querySelectorAll('#themeControl button[data-theme]').forEach(button => {
        button.classList.toggle('active', button.dataset.theme === resolvedSettings.theme);
    });
    setSwitch(el('autoStartSwitch'), resolvedSettings.auto_start);
    setSwitch(el('elevatedSwitch'), resolvedSettings.capture_elevated);
    setSwitch(el('overlaySwitch'), resolvedSettings.overlay_enabled);
    setSwitch(el('usageTrackingSwitch'), resolvedSettings.usage_tracking_enabled);
    el('duplicateGuardSeconds').value = resolvedSettings.duplicate_guard_seconds.toFixed(1);
    el('retentionDays').value = String(resolvedSettings.detail_event_retention_days);
    currentOverlayMonitors = monitors;
    renderOverlaySettings(overlaySettings);
    el('dbPath').textContent = paths.database_path;
    renderCaptureStatus(status, health);
}
function outcomeLabel(outcome) {
    return { recorded: '記録', display_only: '表示のみ', ignored: 'ポリシー無視', duplicate: '重複抑制', sample: 'サンプル', excluded: '除外アプリ', mapping_disabled: 'マッピングOFF', unsupported: '未対応', invalid_application: 'アプリ不明', ineligible: '対象外' }[outcome] ?? outcome;
}
function renderRealtimeLog() {
    const body = el('realtimeLogBody');
    if (!realtimeEntries.length) {
        body.innerHTML = emptyRow(4, 'ショートカット入力を待っています。');
        return;
    }
    body.innerHTML = realtimeEntries.slice().reverse().map(entry => `<tr><td class="exe">${escapeHtml(entry.occurred_at.split(' ')[1] ?? entry.occurred_at)}</td><td>${kbd(entry.shortcut)}</td><td class="exe">${escapeHtml(entry.application)}</td><td><span class="pill outcome-${escapeHtml(entry.outcome)}">${escapeHtml(outcomeLabel(entry.outcome))}</span></td></tr>`).join('');
}
async function loadRealtimeLog() {
    if (realtimeLoading) {
        realtimeReloadPending = true;
        return;
    }
    realtimeLoading = true;
    try {
        do {
            realtimeReloadPending = false;
            const incoming = await api.realtimeLogs(realtimeSequence, 500);
            if (incoming.length) {
                realtimeSequence = incoming[incoming.length - 1]?.sequence ?? realtimeSequence;
                realtimeEntries.push(...incoming);
                if (realtimeEntries.length > 500)
                    realtimeEntries = realtimeEntries.slice(-500);
            }
            renderRealtimeLog();
        } while (realtimeReloadPending);
    }
    finally {
        realtimeLoading = false;
    }
}
async function loadMatrix() {
    const range = await resolveRange(rangeState.matrix ?? '7');
    const [keys, apps] = await Promise.all([
        api.shortcutRankingExtended(range.start, range.end, null, 8),
        api.applicationRanking(range.start, range.end, 6),
    ]);
    el('matrixRangeLabel').textContent = rangeLabel(range);
    const head = el('matrixHead');
    const body = el('matrixBody');
    if (!keys.length || !apps.length) {
        head.innerHTML = '<tr><th>Shortcut</th></tr>';
        body.innerHTML = emptyRow(1);
        return;
    }
    const perApp = await Promise.all(apps.map(async (app) => ({
        application: app.application,
        rows: await api.shortcutRankingExtended(range.start, range.end, app.application, 200),
    })));
    const maps = new Map(perApp.map(group => [group.application, new Map(group.rows.map(row => [row.shortcut, row.count]))]));
    let maxCell = 1;
    for (const key of keys)
        for (const app of apps)
            maxCell = Math.max(maxCell, maps.get(app.application)?.get(key.shortcut) ?? 0);
    head.innerHTML = `<tr><th>Shortcut</th>${apps.map(app => `<th class="exe">${escapeHtml(app.application)}</th>`).join('')}<th>合計</th></tr>`;
    body.innerHTML = keys.map(key => `<tr><td>${kbd(key.shortcut)}</td>${apps.map(app => {
        const count = maps.get(app.application)?.get(key.shortcut) ?? 0;
        const heat = count / maxCell;
        return `<td class="heat-cell" style="--heat:${heat.toFixed(3)}" title="${escapeHtml(key.shortcut)} / ${escapeHtml(app.application)}: ${formatNumber(count)}">${formatNumber(count)}</td>`;
    }).join('')}<td><b>${formatNumber(key.count)}</b></td></tr>`).join('');
}
const modifierPatterns = [
    { id: 'none', label: 'なし', parts: [] },
    { id: 'ctrl', label: 'Ctrl', parts: ['Ctrl'] },
    { id: 'alt', label: 'Alt', parts: ['Alt'] },
    { id: 'win', label: 'Win', parts: ['Win'] },
    { id: 'shift', label: 'Shift', parts: ['Shift'] },
    { id: 'ctrl-alt', label: 'Ctrl+Alt', parts: ['Ctrl', 'Alt'] },
    { id: 'ctrl-win', label: 'Ctrl+Win', parts: ['Ctrl', 'Win'] },
    { id: 'ctrl-shift', label: 'Ctrl+Shift', parts: ['Ctrl', 'Shift'] },
    { id: 'alt-win', label: 'Alt+Win', parts: ['Alt', 'Win'] },
    { id: 'alt-shift', label: 'Alt+Shift', parts: ['Alt', 'Shift'] },
    { id: 'win-shift', label: 'Win+Shift', parts: ['Win', 'Shift'] },
    { id: 'ctrl-alt-win', label: 'Ctrl+Alt+Win', parts: ['Ctrl', 'Alt', 'Win'] },
    { id: 'ctrl-alt-shift', label: 'Ctrl+Alt+Shift', parts: ['Ctrl', 'Alt', 'Shift'] },
    { id: 'ctrl-win-shift', label: 'Ctrl+Win+Shift', parts: ['Ctrl', 'Win', 'Shift'] },
    { id: 'alt-win-shift', label: 'Alt+Win+Shift', parts: ['Alt', 'Win', 'Shift'] },
    { id: 'ctrl-alt-win-shift', label: 'Ctrl+Alt+Win+Shift', parts: ['Ctrl', 'Alt', 'Win', 'Shift'] },
];
const keyActionOptions = [
    ['ignore', 'なし'], ['continue_display', '続けて表示'], ['display_break', '表示して改行'], ['record_and_display_break', '記録と表示改行'],
];
function keyCategory(name, virtualKey) {
    if (/^[A-Z0-9]$/.test(name))
        return ['alnum', 'アルファベット・数字'];
    if (/^(Ctrl|Control|Alt|Shift|Win|LControl|RControl|LShift|RShift|LAlt|RAlt|LWin|RWin)$/i.test(name))
        return ['modifier', '修飾キー'];
    if (/^F(?:[1-9]|1[0-9]|2[0-4])$/.test(name))
        return ['function', 'ファンクションキー'];
    if (virtualKey && ((virtualKey >= 0x41 && virtualKey <= 0x5A) || (virtualKey >= 0x30 && virtualKey <= 0x39)))
        return ['alnum', 'アルファベット・数字'];
    return ['system', 'システム・その他'];
}
function policyRows(mapping) {
    const keyboard = mapping.keyboard.map(entry => {
        const [category, categoryLabel] = keyCategory(entry.name, entry.virtual_key);
        return { key: entry.name, label: entry.name, code: `${entry.virtual_key} / 0x${entry.virtual_key.toString(16).toUpperCase().padStart(2, '0')}`, category, categoryLabel };
    });
    const mouse = [
        ['MouseLeft', 'MouseLeft', '1 / 0x01'], ['MouseRight', 'MouseRight', '2 / 0x02'], ['MouseMiddle', 'MouseMiddle', '4 / 0x04'],
        ['MouseX1', 'MouseX1', '5 / 0x05'], ['MouseX2', 'MouseX2', '6 / 0x06'],
        ['WheelUp', 'WheelUp', 'WM 0x020A'], ['WheelDown', 'WheelDown', 'WM 0x020A'], ['WheelLeft', 'WheelLeft', 'WM 0x020E'], ['WheelRight', 'WheelRight', 'WM 0x020E'],
    ];
    return [...keyboard, ...mouse.map(([key, label, code]) => ({ key, label, code, category: 'system', categoryLabel: 'システム・その他' }))];
}
function policyIdentity(pattern, key) { return pattern.parts.length ? `${pattern.parts.join('+')}+${key}` : key; }
function defaultPolicyAction(policy, pattern) { return pattern.parts.length ? policy.default_shortcut : policy.default_single_key; }
function safeSingleAction(action) { return action === 'record_and_display_break' ? 'display_break' : action; }
function keyActionOptionsHtml(selected, allowRecord) {
    const normalized = allowRecord ? selected : safeSingleAction(selected);
    return keyActionOptions.filter(([value]) => allowRecord || value !== 'record_and_display_break').map(([value, label]) => `<option value="${value}" ${normalized === value ? 'selected' : ''}>${label}</option>`).join('');
}
function renderKeyMapping(mapping, policy, labels) {
    currentKeyMapping = structuredClone(mapping);
    currentKeyPolicy = structuredClone(policy);
    currentOverlayLabels = structuredClone(labels);
    el('keyboardLayout').value = labels.keyboard_layout;
    const head = el('keyPolicyMatrixHead');
    const body = el('keyPolicyMatrixBody');
    head.innerHTML = `<tr><th class="key-code-col">キーコード</th><th class="key-name-col">テキス名</th><th class="key-label-col">オーバーレイ表示名</th><th class="key-category-col">分類</th>${modifierPatterns.map(pattern => `<th class="key-policy-pattern ${selectedModifierColumns.has(pattern.id) ? 'selected-column' : ''}" data-pattern="${pattern.id}" title="クリックで一括変更対象列を選択">${escapeHtml(pattern.label)}</th>`).join('')}</tr>`;
    const rows = policyRows(mapping);
    body.innerHTML = rows.map(row => `<tr data-category="${row.category}" data-key="${escapeHtml(row.key)}"><td class="key-code-col key-code-text">${escapeHtml(row.code)}</td><th class="key-name-col">${escapeHtml(row.label)}</th><td class="key-label-col"><input class="input key-label-input" data-key-label="${escapeHtml(row.key)}" value="${escapeHtml(labels.keys[row.key] ?? row.key)}" /></td><td class="key-category-col"><span class="category-pill">${escapeHtml(row.categoryLabel)}</span></td>${modifierPatterns.map(pattern => {
        const identity = policyIdentity(pattern, row.key);
        const fallback = defaultPolicyAction(policy, pattern);
        const selected = policy.overrides[identity] ?? fallback;
        const selectedColumn = selectedModifierColumns.has(pattern.id);
        return `<td class="policy-cell ${selectedColumn ? 'selected-column' : ''}" data-pattern-cell="${pattern.id}"><select class="select key-policy-select ${pattern.id === 'none' ? 'security-locked' : ''}" data-policy-identity="${escapeHtml(identity)}" data-policy-default="${pattern.id === 'none' ? safeSingleAction(fallback) : fallback}" data-pattern="${pattern.id}" title="${pattern.id === 'none' ? '修飾なし単独入力は永続記録できません' : ''}">${keyActionOptionsHtml(selected, pattern.id !== 'none')}</select></td>`;
    }).join('')}</tr>`).join('');
    applyMappingFilters();
    updateMappingSummary();
}
function mappingFromUi() {
    return structuredClone(currentKeyMapping ?? { version: 1, keyboard: [], mouse: { left_click: true, right_click: true, middle_click: true, x1_click: true, x2_click: true, wheel_up: true, wheel_down: true, wheel_left: true, wheel_right: true } });
}
function policyFromUi() {
    const policy = structuredClone(currentKeyPolicy ?? { version: 2, default_single_key: 'continue_display', default_shortcut: 'record_and_display_break', overrides: {} });
    document.querySelectorAll('#keyPolicyMatrixBody select[data-policy-identity]').forEach(select => {
        const identity = select.dataset.policyIdentity;
        const fallback = select.dataset.policyDefault;
        const pattern = select.dataset.pattern;
        if (!identity)
            return;
        const chosen = pattern === 'none' ? safeSingleAction(select.value) : select.value;
        if (fallback && chosen === fallback)
            delete policy.overrides[identity];
        else
            policy.overrides[identity] = chosen;
    });
    return policy;
}
function labelsFromUi() {
    const labels = structuredClone(currentOverlayLabels ?? { version: 2, keyboard_layout: 'japanese-109', keys: {} });
    document.querySelectorAll('#keyPolicyMatrixBody input[data-key-label]').forEach(input => {
        const key = input.dataset.keyLabel;
        if (!key)
            return;
        labels.keys[key] = input.value.trim() || key;
    });
    return labels;
}
function visibleMappingRows() { return Array.from(document.querySelectorAll('#keyPolicyMatrixBody tr:not(.mapping-hidden)')); }
function applyMappingFilters() {
    document.querySelectorAll('#keyPolicyMatrixBody tr[data-category]').forEach(row => row.classList.toggle('mapping-hidden', !activeMappingCategories.has(row.dataset.category ?? '')));
    updateMappingSummary();
}
function updateMappingSelectionStyles() {
    document.querySelectorAll('[data-pattern], [data-pattern-cell]').forEach(node => {
        const id = node.dataset.pattern ?? node.dataset.patternCell ?? '';
        node.classList.toggle('selected-column', selectedModifierColumns.has(id));
    });
    const summary = el('mappingSelectedColumns');
    summary.classList.toggle('error', selectedModifierColumns.size === 0);
    summary.textContent = selectedModifierColumns.size
        ? `選択列: ${selectedModifierColumns.size}列。列見出しをクリックして複数選択できます。`
        : '列を選択してください。列見出しをクリックして複数選択できます。';
}
function updateMappingSummary() {
    updateMappingSelectionStyles();
}
function bulkMappingChange(action) {
    const rows = visibleMappingRows();
    const patterns = modifierPatterns.filter(pattern => selectedModifierColumns.has(pattern.id));
    if (!rows.length)
        return showToast('現在表示中のキーがありません。');
    if (!patterns.length)
        return showToast('一括変更する修飾パターン列を見出しから選択してください。');
    const operation = action === 'continue_display' ? '続けて表示' : action === 'record_and_display_break' ? '記録と表示改行' : action === 'ignore' ? 'すべて非表示' : action === 'reset' ? '既定に戻す' : 'すべて表示';
    const cells = rows.length * patterns.length;
    if (!window.confirm(`${operation}を実行しますか？
対象行: ${rows.length}
選択列: ${patterns.map(p => p.label).join(', ')}
変更セル数: ${cells}

この操作は多数の設定を変更します。`))
        return;
    for (const row of rows)
        for (const pattern of patterns) {
            const select = row.querySelector(`select[data-pattern="${pattern.id}"]`);
            if (!select)
                continue;
            if (action === 'reset')
                select.value = select.dataset.policyDefault ?? (pattern.id === 'none' ? 'continue_display' : 'record_and_display_break');
            else
                select.value = pattern.id === 'none' ? safeSingleAction(action) : action;
        }
    updateMappingSummary();
}
async function loadKeyMapping() {
    const [mapping, policy, labels] = await Promise.all([api.keyMapping(), api.overlayKeyPolicy(), api.overlayKeyLabels()]);
    renderKeyMapping(mapping, policy, labels);
}
function browserKeySearchIdentity(event) {
    if (event.isComposing || event.repeat)
        return null;
    const key = event.key;
    if (/^[a-zA-Z0-9]$/.test(key))
        return key.toUpperCase();
    if (/^F(?:[1-9]|1[0-9]|2[0-4])$/.test(key))
        return key;
    const byKey = {
        Enter: 'Enter', Tab: 'Tab', Escape: 'Esc', Esc: 'Esc', Backspace: 'Backspace', Delete: 'Delete', Insert: 'Insert',
        Home: 'Home', End: 'End', PageUp: 'PageUp', PageDown: 'PageDown', ArrowLeft: 'Left', ArrowRight: 'Right',
        ArrowUp: 'Up', ArrowDown: 'Down', ' ': 'Space', CapsLock: 'CapsLock', Pause: 'Pause', PrintScreen: 'PrintScreen',
        ScrollLock: 'ScrollLock', NumLock: 'NumLock', ContextMenu: 'Apps',
    };
    if (byKey[key])
        return byKey[key];
    const byCode = {
        Semicolon: 'Semicolon', Equal: 'Equal', Comma: 'Comma', Minus: 'Minus', Period: 'Period', Slash: 'Slash',
        Backquote: 'Backquote', BracketLeft: 'LeftBracket', Backslash: 'Backslash', BracketRight: 'RightBracket', Quote: 'Quote',
        NumpadMultiply: 'NumMultiply', NumpadAdd: 'NumAdd', NumpadSubtract: 'NumSubtract', NumpadDecimal: 'NumDecimal',
        NumpadDivide: 'NumDivide',
    };
    if (/^Numpad[0-9]$/.test(event.code))
        return `Num${event.code.slice(-1)}`;
    return byCode[event.code] ?? null;
}
function revealMappingSearchResult(identity) {
    document.querySelectorAll('#keyPolicyMatrixBody tr').forEach(row => row.classList.remove('mapping-search-hit'));
    const row = Array.from(document.querySelectorAll('#keyPolicyMatrixBody tr[data-key]'))
        .find(candidate => candidate.dataset.key === identity);
    if (!row)
        return false;
    const category = row.dataset.category;
    if (category && !activeMappingCategories.has(category)) {
        activeMappingCategories.add(category);
        document.querySelector(`#mappingCategoryFilter button[data-category="${category}"]`)?.classList.add('active');
        applyMappingFilters();
    }
    row.classList.add('mapping-search-hit');
    row.scrollIntoView({ block: 'center', behavior: 'smooth' });
    window.setTimeout(() => row.classList.remove('mapping-search-hit'), 1800);
    return true;
}
async function searchMappingPhysicalKey() {
    const button = el('mappingKeySearch');
    if (button.disabled)
        return;
    button.disabled = true;
    const original = button.textContent;
    button.textContent = 'キー入力待ち...';
    let frontendIdentity = null;
    const keyListener = (event) => {
        const identity = browserKeySearchIdentity(event);
        if (!identity)
            return;
        event.preventDefault();
        event.stopImmediatePropagation();
        frontendIdentity = identity;
    };
    const mouseListener = (event) => {
        const identity = event.button === 0 ? 'MouseLeft' : event.button === 1 ? 'MouseMiddle' : event.button === 2 ? 'MouseRight' : event.button === 3 ? 'MouseX1' : event.button === 4 ? 'MouseX2' : null;
        if (!identity)
            return;
        event.preventDefault();
        event.stopImmediatePropagation();
        frontendIdentity = identity;
    };
    const wheelListener = (event) => {
        const identity = Math.abs(event.deltaX) > Math.abs(event.deltaY)
            ? (event.deltaX < 0 ? 'WheelLeft' : 'WheelRight')
            : (event.deltaY < 0 ? 'WheelUp' : 'WheelDown');
        event.preventDefault();
        event.stopImmediatePropagation();
        frontendIdentity = identity;
    };
    try {
        await api.beginKeySearch();
        window.addEventListener('keydown', keyListener, true);
        window.addEventListener('mousedown', mouseListener, true);
        window.addEventListener('wheel', wheelListener, { capture: true, passive: false });
        let identity = null;
        // Backend global Capture is authoritative. The WebView listener is a UI fallback so
        // ordinary keyboard input still finds a row even if hook result delivery is delayed.
        for (let attempt = 0; attempt < 200 && activePage === 'key-mapping'; attempt += 1) {
            await new Promise(resolve => window.setTimeout(resolve, 50));
            identity = await api.pollKeySearch();
            if (!identity && frontendIdentity)
                identity = frontendIdentity;
            if (identity)
                break;
        }
        await api.cancelKeySearch();
        if (!identity) {
            showToast('キー入力検索を終了しました。');
            return;
        }
        if (!revealMappingSearchResult(identity)) {
            showToast(`${identity} はキーマッピング一覧にありません。`);
            return;
        }
        showToast(`${identity} の設定行へ移動しました。`);
    }
    catch (error) {
        await api.cancelKeySearch().catch(() => undefined);
        showToast(errorMessage(error));
    }
    finally {
        window.removeEventListener('keydown', keyListener, true);
        window.removeEventListener('mousedown', mouseListener, true);
        window.removeEventListener('wheel', wheelListener, true);
        button.disabled = false;
        button.textContent = original;
    }
}
async function refreshDatabaseEnvironment() {
    const snapshot = await api.databaseEnvironment();
    currentDatabaseEnvironment = snapshot;
    const badge = el('dbBadge');
    badge.hidden = !snapshot.is_test;
    badge.textContent = '検証DB';
    document.querySelectorAll('#databaseSwitch button[data-db]').forEach(button => button.classList.toggle('active', button.dataset.db === snapshot.environment));
    const activePath = document.getElementById('activeDbPath');
    if (activePath)
        activePath.textContent = `${snapshot.display_name}: ${snapshot.path}`;
    const backup = document.getElementById('backupDbName');
    if (backup)
        backup.textContent = snapshot.display_name;
    return snapshot;
}
async function loadDataManagement() {
    const [environment, source] = await Promise.all([refreshDatabaseEnvironment(), api.sampleSourceStatus()]);
    const button = el('generateSample');
    button.disabled = !environment.is_test || !source.can_generate;
    el('sampleSourceText').textContent = !environment.is_test ? 'サンプルデータは検証DBでのみ生成できます。' : source.can_generate ? `本番DBの実績を母集団に使用: アプリ ${source.application_count} / ショートカット ${source.shortcut_count}` : '元データとなるアプリ／ショートカット実績がありません。';
}
function updateDailyExportSelectionSummary() {
    el('dailyExportCount').textContent = `${dailyExportSelected.size} / ${dailyExportApplications.length}件を選択中`;
}
function createAnonymousExportName() {
    const bytes = new Uint8Array(8);
    window.crypto.getRandomValues(bytes);
    return `匿名-${Array.from(bytes, byte => byte.toString(16).padStart(2, '0')).join('')}`;
}
function renderDailyExportFilter() {
    const list = el('dailyExportList');
    list.innerHTML = dailyExportApplications.length
        ? dailyExportApplications.map(application => `<label class="daily-export-option"><input type="checkbox" data-daily-export-application="${escapeHtml(application)}" ${dailyExportSelected.has(application) ? 'checked' : ''}><span title="${escapeHtml(application)}">${escapeHtml(application)}</span></label>`).join('')
        : '<div class="daily-export-empty">日次集計にアプリがありません。</div>';
    list.querySelectorAll('input[data-daily-export-application]').forEach(input => input.addEventListener('change', () => {
        const application = input.dataset.dailyExportApplication;
        if (!application)
            return;
        if (input.checked)
            dailyExportSelected.add(application);
        else
            dailyExportSelected.delete(application);
        updateDailyExportSelectionSummary();
    }));
    updateDailyExportSelectionSummary();
}
async function openDailyExportFilter() {
    const modal = el('dailyExportModal');
    const confirm = el('confirmDailyExport');
    confirm.disabled = true;
    modal.classList.add('show');
    try {
        const snapshot = await api.getDailyExportFilter();
        dailyExportApplications = snapshot.applications;
        dailyExportSelected.clear();
        const saved = snapshot.selected;
        for (const application of dailyExportApplications) {
            if (saved === null || saved.includes(application))
                dailyExportSelected.add(application);
        }
        const startInput = el('dailyExportStartDate');
        const endInput = el('dailyExportEndDate');
        const today = todayText();
        startInput.value = snapshot.first_date ?? today;
        endInput.value = snapshot.last_date ?? today;
        startInput.min = snapshot.first_date ?? '';
        startInput.max = snapshot.last_date ?? '';
        endInput.min = snapshot.first_date ?? '';
        endInput.max = snapshot.last_date ?? '';
        el('dailyExportName').value = snapshot.name;
        renderDailyExportFilter();
    }
    catch (error) {
        modal.classList.remove('show');
        showToast(errorMessage(error));
    }
    finally {
        confirm.disabled = false;
    }
}
function closeDailyExportFilter() {
    el('dailyExportModal').classList.remove('show');
}
async function loadHelp() {
    appInfo ?? (appInfo = await api.appInfo());
    el('helpVersion').textContent = `v${appInfo.version}`;
    el('helpAuthor').textContent = appInfo.author;
    el('helpGithubUrl').textContent = appInfo.github_url;
    el('brandVersion').textContent = `v${appInfo.version}`;
}
async function loadPage(page = activePage) {
    try {
        if (page === 'dashboard')
            await loadDashboard();
        else if (page === 'daily-ranking')
            await loadDailyRanking();
        else if (page === 'ranking')
            await loadRanking();
        else if (page === 'usage')
            await loadUsage();
        else if (page === 'applications')
            await loadApplications();
        else if (page === 'matrix')
            await loadMatrix();
        else if (page === 'shortcut-detail')
            await loadShortcutDetail();
        else if (page === 'realtime-log')
            await loadRealtimeLog();
        else if (page === 'key-mapping')
            await loadKeyMapping();
        else if (page === 'excluded')
            await loadExcluded();
        else if (page === 'data')
            await loadDataManagement();
        else if (page === 'settings')
            await renderSettings();
        else if (page === 'help')
            await loadHelp();
    }
    catch (error) {
        showToast(errorMessage(error));
    }
}
function goPage(id) {
    if (!(id in titles))
        id = 'dashboard';
    activePage = id;
    document.querySelectorAll('.page').forEach(page => page.classList.toggle('active', page.id === id));
    document.querySelectorAll('.nav button[data-page]').forEach(button => button.classList.toggle('active', button.dataset.page === id));
    const rankingTree = el('rankingNavTree');
    rankingTree.classList.toggle('active', rankingPages.includes(id));
    if (rankingPages.includes(id))
        rankingTree.classList.add('open');
    el('topTitle').textContent = titles[id] ?? 'ShortcutKeyCounter';
    void loadPage(id);
}
async function withButton(button, action) {
    if (button.disabled)
        return;
    const text = button.textContent;
    button.disabled = true;
    button.textContent = '処理中...';
    try {
        await action();
    }
    finally {
        button.disabled = false;
        button.textContent = text;
    }
}
function bindEvents() {
    window.addEventListener('keydown', handleUiZoomShortcut, true);
    document.querySelectorAll('.nav button').forEach(button => button.addEventListener('click', () => {
        if (button.dataset.page)
            goPage(button.dataset.page);
    }));
    el('rankingNavToggle').addEventListener('click', () => el('rankingNavTree').classList.toggle('open'));
    el('dailyRankingDate').addEventListener('change', () => void loadDailyRanking());
    document.querySelectorAll('[data-page-range]').forEach(group => group.addEventListener('click', event => {
        const button = event.target.closest('button[data-range]');
        if (!button)
            return;
        const page = group.dataset.pageRange;
        if (!page || !(page in rangeState))
            return;
        group.querySelectorAll('button').forEach(node => node.classList.remove('active'));
        button.classList.add('active');
        rangeState[page] = button.dataset.range;
        void loadPage(page);
    }));
    el('analyticsRange').addEventListener('click', event => {
        const button = event.target.closest('button[data-range]');
        if (!button)
            return;
        el('analyticsRange').querySelectorAll('button').forEach(node => node.classList.remove('active'));
        button.classList.add('active');
        analyticsRange = button.dataset.range;
        analyticsSort.key = { column: '', dir: 'none' };
        analyticsSort.app = { column: '', dir: 'none' };
        void loadRanking();
    });
    el('analyticsAppFilter').addEventListener('change', () => { analyticsSort.key = { column: '', dir: 'none' }; void loadRanking(); });
    document.querySelectorAll('.sort-button-v014').forEach(button => button.addEventListener('click', () => {
        const scope = button.closest('table')?.id === 'keyRankingV014' ? 'key' : 'app';
        const state = analyticsSort[scope];
        const next = state.column !== button.dataset.sort || state.dir === 'none' ? 'desc' : state.dir === 'desc' ? 'asc' : 'none';
        state.column = next === 'none' ? '' : button.dataset.sort;
        state.dir = next;
        if (analyticsSnapshot)
            renderRankingV014(analyticsSnapshot);
    }));
    el('usageMode').addEventListener('click', event => {
        const button = event.target.closest('button[data-mode]');
        if (!button)
            return;
        el('usageMode').querySelectorAll('button').forEach(node => node.classList.remove('active'));
        button.classList.add('active');
        usageMode = button.dataset.mode;
        void loadUsage(true);
    });
    el('usageRange').addEventListener('click', event => {
        const button = event.target.closest('button[data-range]');
        if (!button)
            return;
        el('usageRange').querySelectorAll('button').forEach(node => node.classList.remove('active'));
        button.classList.add('active');
        usageRange = button.dataset.range;
        void loadUsage(true);
    });
    el('usageSeriesList').addEventListener('click', event => {
        const target = event.target;
        const swatch = target.closest('button[data-color-identity]');
        if (swatch) {
            event.stopPropagation();
            const identity = swatch.dataset.colorIdentity;
            if (!identity)
                return;
            const input = document.createElement('input');
            input.type = 'color';
            input.value = usageColor(identity, usageIdentities.indexOf(identity));
            input.style.position = 'fixed';
            input.style.left = '-1000px';
            document.body.appendChild(input);
            input.addEventListener('input', () => { usageColors[usageIdentityKey(identity)] = input.value; swatch.style.background = input.value; drawUsageGraph(); });
            input.addEventListener('change', () => { void api.setUsageGraphColors(usageColors).catch(error => showToast(errorMessage(error))); input.remove(); }, { once: true });
            input.click();
            return;
        }
        const item = target.closest('.usage-series-item');
        const identity = item?.dataset.identity;
        if (!item || !identity)
            return;
        if (usageEnabled.has(identity))
            usageEnabled.delete(identity);
        else
            usageEnabled.add(identity);
        item.classList.toggle('off', !usageEnabled.has(identity));
        void loadUsage(false);
    });
    el('captureBtn').addEventListener('click', async () => {
        const current = await api.captureStatus().catch(() => null);
        if (!current)
            return showToast('計測状態を取得できませんでした。');
        try {
            const status = await api.setCaptureEnabled(!current.enabled);
            renderCaptureStatus(status, await api.captureHealth());
            showToast(status.enabled ? '計測を再開しました。' : '計測を停止しました。この状態は再起動後には引き継がれません。');
        }
        catch (error) {
            showToast(errorMessage(error));
        }
    });
    el('showAllRanking').addEventListener('click', () => goPage('ranking'));
    el('shortcutSelect').addEventListener('change', () => void loadShortcutDetail());
    el('addExclude').addEventListener('click', async () => {
        const input = el('excludeInput');
        const name = input.value.trim();
        if (!name)
            return showToast('実行ファイル名を入力してください。');
        try {
            renderExcluded(await api.addExcludedApp(name));
            input.value = '';
            showToast(`${name} を除外対象に追加しました。`);
        }
        catch (error) {
            showToast(errorMessage(error));
        }
    });
    el('excludeInput').addEventListener('keydown', event => { if (event.key === 'Enter')
        el('addExclude').click(); });
    el('exportDaily').addEventListener('click', () => void openDailyExportFilter());
    el('dailyExportSelectAll').addEventListener('click', () => {
        dailyExportApplications.forEach(application => dailyExportSelected.add(application));
        renderDailyExportFilter();
    });
    el('dailyExportClearAll').addEventListener('click', () => {
        dailyExportSelected.clear();
        renderDailyExportFilter();
    });
    el('dailyExportAnonymous').addEventListener('click', () => {
        const input = el('dailyExportName');
        input.value = createAnonymousExportName();
        input.focus();
    });
    el('cancelDailyExport').addEventListener('click', closeDailyExportFilter);
    el('confirmDailyExport').addEventListener('click', event => void withButton(event.currentTarget, async () => {
        const applications = [...dailyExportSelected];
        const startDate = el('dailyExportStartDate').value;
        const endDate = el('dailyExportEndDate').value;
        const name = el('dailyExportName').value.trim();
        if (!startDate || !endDate)
            return showToast('開始日と終了日を指定してください。');
        if (startDate > endDate)
            return showToast('開始日は終了日以前にしてください。');
        if (!applications.length && !window.confirm('アプリが1件も選択されていません。ヘッダーだけのCSVを出力しますか？'))
            return;
        await api.setDailyExportFilter(applications, name);
        const result = await api.exportDaily(applications, startDate, endDate, name);
        closeDailyExportFilter();
        showToast(`${result.path} を出力しました（${result.rows}行）。`, { label: 'フォルダーを開く', run: () => api.openParentFolder(result.path) });
    }).catch(error => showToast(errorMessage(error))));
    el('exportEvents').addEventListener('click', event => void withButton(event.currentTarget, async () => {
        const result = await api.exportEvents();
        showToast(`${result.path} を出力しました（${result.rows}行）。`, { label: 'フォルダーを開く', run: () => api.openParentFolder(result.path) });
    }).catch(error => showToast(errorMessage(error))));
    el('backupDb').addEventListener('click', event => void withButton(event.currentTarget, async () => {
        const result = await api.backupDatabase();
        showToast(`${result.path} を保存しました。`, { label: 'フォルダーを開く', run: () => api.openParentFolder(result.path) });
    }).catch(error => showToast(errorMessage(error))));
    el('databaseSwitch').addEventListener('click', async (event) => {
        const button = event.target.closest('button[data-db]');
        const next = button?.dataset.db;
        if (!button || !next || next === currentDatabaseEnvironment?.environment)
            return;
        const message = next === 'test'
            ? '検証DBへ切り替えますか？\n以降の記録・統計・出力は検証DBを使用します。'
            : '本番DBへ戻しますか？\n以降の記録は本番DBへ保存されます。';
        if (!window.confirm(message))
            return;
        try {
            button.disabled = true;
            currentDatabaseEnvironment = await api.switchDatabaseEnvironment(next);
            cachedBounds = null;
            realtimeEntries = [];
            realtimeSequence = 0;
            await loadDataManagement();
            showToast(`${currentDatabaseEnvironment.display_name}へ切り替えました。`);
        }
        catch (error) {
            showToast(errorMessage(error));
            await refreshDatabaseEnvironment().catch(() => undefined);
        }
        finally {
            button.disabled = false;
        }
    });
    el('generateSample').addEventListener('click', event => void withButton(event.currentTarget, async () => {
        const env = currentDatabaseEnvironment ?? await refreshDatabaseEnvironment();
        const source = await api.sampleSourceStatus();
        if (!env.is_test)
            return showToast('サンプルデータは検証DBでのみ生成できます。');
        if (!source.can_generate)
            return showToast('元データとなるアプリ／ショートカット実績がありません。');
        const yesterday = addDays(todayText(), -1);
        const start = addDays(yesterday, -59);
        if (!window.confirm(`サンプルデータを生成しますか？\n対象DB: ${env.display_name}\n予定件数: 約10,000件\n対象期間: ${start} ～ ${yesterday}\n生成元: 本番DBの実績アプリ ${source.application_count} / ショートカット ${source.shortcut_count}`))
            return;
        const result = await api.generateSampleData();
        cachedBounds = null;
        showToast(`検証DBへ詳細ショートカット履歴 ${formatNumber(result.events_generated)}件を生成しました（60日 / 利用時間 ${formatNumber(result.usage_rows_generated)}行 / リアルタイムログ ${formatNumber(result.realtime_log_rows)}件）。`);
        await loadDataManagement();
    }).catch(error => showToast(errorMessage(error))));
    el('deleteYesterdayBtn').addEventListener('click', event => void withButton(event.currentTarget, async () => {
        const env = currentDatabaseEnvironment ?? await refreshDatabaseEnvironment();
        const preview = await api.previewDeleteYesterdayAndEarlier();
        const total = preview.events + preview.daily_rows + preview.usage_rows;
        const details = `対象DB: ${env.display_name}\n削除基準日: ${preview.cutoff_date} 以下\n詳細ショートカット履歴: ${preview.events}件\n日次集計: ${preview.daily_rows}行\n利用時間: ${preview.usage_rows}行\n合計対象: ${total}\n\n実データも削除対象で、元に戻せません。`;
        const typed = window.prompt(`${details}\n\n実行するには「削除」と入力してください。`, '');
        if (typed !== '削除')
            return;
        const result = await api.deleteYesterdayAndEarlier();
        cachedBounds = null;
        showToast(`前日以前を削除しました（詳細 ${result.events} / 日次 ${result.daily_rows} / 利用時間 ${result.usage_rows}）。`);
        await loadDataManagement();
    }).catch(error => showToast(errorMessage(error))));
    const deleteModal = el('deleteModal');
    el('deleteBtn').addEventListener('click', () => deleteModal.classList.add('show'));
    el('cancelDelete').addEventListener('click', () => deleteModal.classList.remove('show'));
    el('confirmDelete').addEventListener('click', async () => {
        try {
            const result = await api.deleteStatistics();
            cachedBounds = null;
            deleteModal.classList.remove('show');
            showToast(`統計データを削除しました（詳細ショートカット履歴 ${result.events_deleted}件 / 日次 ${result.daily_deleted}件 / 利用時間 ${result.usage_deleted}行）。`);
            await loadDashboard();
        }
        catch (error) {
            showToast(errorMessage(error));
        }
    });
    el('themeControl').addEventListener('click', async (event) => {
        const button = event.target.closest('button[data-theme]');
        const theme = button?.dataset.theme;
        if (theme !== 'dark' && theme !== 'light')
            return;
        applyTheme(theme);
        try {
            await renderSettings(await api.setTheme(theme));
            showToast(theme === 'light' ? 'ライトモードに切り替えました。' : 'ダークモードに切り替えました。');
        }
        catch (error) {
            showToast(errorMessage(error));
            await renderSettings().catch(() => undefined);
        }
    });
    el('autoStartSwitch').addEventListener('click', async () => {
        const target = !el('autoStartSwitch').classList.contains('on');
        try {
            await renderSettings(await api.setAutoStart(target));
            showToast(`自動起動を${target ? '有効' : '無効'}にしました。`);
        }
        catch (error) {
            showToast(errorMessage(error));
        }
    });
    el('usageTrackingSwitch').addEventListener('click', async () => {
        const target = !el('usageTrackingSwitch').classList.contains('on');
        try {
            await renderSettings(await api.setUsageTracking(target));
            showToast(`アプリ利用時間の記録を${target ? '有効' : '無効'}にしました。`);
        }
        catch (error) {
            showToast(errorMessage(error));
        }
    });
    el('duplicateGuardSeconds').addEventListener('change', async (event) => {
        const input = event.target;
        const value = Math.round(Number(input.value) * 10) / 10;
        if (!Number.isFinite(value) || value < 0 || value > 10) {
            showToast('連続入力抑制は0.0〜10.0秒で指定してください。');
            return renderSettings();
        }
        try {
            await renderSettings(await api.setDuplicateGuardSeconds(value));
            showToast(`連続入力抑制を ${value.toFixed(1)} 秒に変更しました。`);
        }
        catch (error) {
            showToast(errorMessage(error));
            await renderSettings().catch(() => undefined);
        }
    });
    el('retentionDays').addEventListener('change', async (event) => {
        const input = event.target;
        const value = Math.round(Number(input.value));
        if (!Number.isFinite(value) || value < 1 || value > 365) {
            showToast('詳細ショートカット履歴の保持日数は1〜365日で指定してください。');
            return renderSettings();
        }
        try {
            await renderSettings(await api.setDetailEventRetentionDays(value));
            showToast(`詳細ショートカット履歴の保持を ${value} 日に変更しました。`);
        }
        catch (error) {
            showToast(errorMessage(error));
            await renderSettings().catch(() => undefined);
        }
    });
    el('previewOverlaySample').addEventListener('click', async () => { try {
        await api.previewOverlaySample();
    }
    catch (error) {
        showToast(errorMessage(error));
    } });
    el('overlaySwitch').addEventListener('click', async () => {
        const target = !el('overlaySwitch').classList.contains('on');
        try {
            await renderSettings(await api.setOverlayEnabled(target));
            showToast(`ショートカットオーバーレイを${target ? '有効' : '無効'}にしました。`);
        }
        catch (error) {
            showToast(errorMessage(error));
        }
    });
    el('overlayPositionMode').addEventListener('click', event => {
        const button = event.target.closest('button[data-mode]');
        if (!button || !currentOverlaySettings)
            return;
        const mode = button.dataset.mode;
        el('overlayPositionMode').querySelectorAll('button').forEach(node => node.classList.remove('active'));
        button.classList.add('active');
        if (mode === 'auto' && currentOverlaySettings.allowed_positions.length === 0)
            currentOverlaySettings.allowed_positions = [currentOverlaySettings.fixed_position];
        renderOverlaySettings({ ...currentOverlaySettings, position_mode: mode });
        void saveOverlaySettingsFromUi().catch(error => showToast(errorMessage(error)));
    });
    el('overlayPositionGrid').addEventListener('click', event => {
        const button = event.target.closest('button[data-position]');
        if (!button || !currentOverlaySettings)
            return;
        const position = button.dataset.position;
        const mode = document.querySelector('#overlayPositionMode button.active')?.dataset.mode;
        if (mode === 'fixed') {
            el('overlayPositionGrid').querySelectorAll('button').forEach(node => node.classList.remove('active'));
            button.classList.add('active');
            currentOverlaySettings.fixed_position = position;
        }
        else {
            const active = button.classList.contains('active');
            const activeCount = el('overlayPositionGrid').querySelectorAll('button.active').length;
            if (active && activeCount <= 1)
                return showToast('自動配置の許可位置は最低1つ必要です。');
            button.classList.toggle('active', !active);
            if (!active)
                currentOverlaySettings.preferred_position = position;
            else if (currentOverlaySettings.preferred_position === position) {
                const fallback = el('overlayPositionGrid').querySelector('button.active')?.dataset.position;
                if (fallback)
                    currentOverlaySettings.preferred_position = fallback;
            }
        }
        void saveOverlaySettingsFromUi().catch(error => showToast(errorMessage(error)));
    });
    el('overlayMonitorMode').addEventListener('change', () => {
        if (!currentOverlaySettings)
            return;
        const monitorMode = el('overlayMonitorMode').value;
        currentOverlaySettings.monitor_mode = monitorMode;
        if (monitorMode === 'fixed' && currentOverlaySettings.fixed_monitor_id && !currentOverlayMonitors.some(monitor => monitor.id === currentOverlaySettings?.fixed_monitor_id)) {
            currentOverlaySettings.fixed_monitor_id = '';
        }
        if (monitorMode === 'fixed' && !currentOverlaySettings.fixed_monitor_id && currentOverlayMonitors.length === 1 && currentOverlayMonitors[0]) {
            currentOverlaySettings.fixed_monitor_id = currentOverlayMonitors[0].id;
        }
        renderOverlayMonitorOptions(currentOverlaySettings);
        // fixed選択時はmonitor comboを先に操作可能にし、validation前の巻き戻りを防ぐ。
        if (monitorMode !== 'fixed' || currentOverlaySettings.fixed_monitor_id)
            void saveOverlaySettingsFromUi(false).catch(error => showToast(errorMessage(error)));
    });
    ['overlayDisplayMode', 'overlayMaxRows', 'overlayDuration', 'overlayEnterMs', 'overlayExitMs', 'overlayBackgroundColor'].forEach(id => {
        el(id).addEventListener('change', () => void saveOverlaySettingsFromUi().catch(error => showToast(errorMessage(error))));
    });
    el('overlayMonitorSelect').addEventListener('change', async (event) => {
        try {
            await saveOverlaySettingsFromUi();
            const select = event.target;
            const index = currentOverlayMonitors.findIndex(monitor => monitor.id === select.value);
            if (select.value && index >= 0)
                await api.previewOverlayMonitor(select.value, index + 1);
        }
        catch (error) {
            showToast(errorMessage(error));
        }
    });
    ['overlayEnterAnimation', 'overlayExitAnimation'].forEach(id => {
        el(id).addEventListener('change', () => void saveOverlaySettingsFromUi().catch(error => showToast(errorMessage(error))));
    });
    el('overlayBackgroundOpacity').addEventListener('input', event => {
        const value = Number(event.target.value);
        el('overlayOpacityValue').textContent = `${value}%`;
    });
    el('overlayBackgroundOpacity').addEventListener('change', () => void saveOverlaySettingsFromUi().catch(error => showToast(errorMessage(error))));
    el('overlayBackgroundColorPicker').addEventListener('input', event => {
        el('overlayBackgroundColor').value = event.target.value;
    });
    el('overlayBackgroundColorPicker').addEventListener('change', () => void saveOverlaySettingsFromUi().catch(error => showToast(errorMessage(error))));
    el('overlayBackgroundColor').addEventListener('input', event => {
        const value = event.target.value;
        if (/^#[0-9a-fA-F]{6}$/.test(value))
            el('overlayBackgroundColorPicker').value = value;
    });
    [
        ['repeatCounterBackground', 'repeatCounterBackgroundPicker'],
        ['repeatCounterText', 'repeatCounterTextPicker'],
        ['repeatCounterAccent', 'repeatCounterAccentPicker'],
    ].forEach(([textId, pickerId]) => {
        el(textId).addEventListener('input', event => { const value = event.target.value; if (/^#[0-9a-fA-F]{6}$/.test(value))
            el(pickerId).value = value; syncRepeatCounterDraftFromUi(); });
        el(pickerId).addEventListener('input', event => { el(textId).value = event.target.value; syncRepeatCounterDraftFromUi(); });
        el(pickerId).addEventListener('change', () => void saveOverlaySettingsFromUi(false).catch(error => showToast(errorMessage(error))));
        el(textId).addEventListener('change', () => void saveOverlaySettingsFromUi(false).catch(error => showToast(errorMessage(error))));
    });
    ['repeatCounterShape', 'repeatCounterRotation', 'repeatCounterFontSize'].forEach(id => {
        el(id).addEventListener('input', syncRepeatCounterDraftFromUi);
        el(id).addEventListener('change', () => void saveOverlaySettingsFromUi(false).catch(error => showToast(errorMessage(error))));
    });
    el('openOverlayLabels').addEventListener('click', async () => {
        try {
            await api.openOverlayLabels();
        }
        catch (error) {
            showToast(errorMessage(error));
        }
    });
    el('openOverlayPolicy').addEventListener('click', async () => {
        try {
            await api.openOverlayPolicy();
        }
        catch (error) {
            showToast(errorMessage(error));
        }
    });
    el('openOverlayTheme').addEventListener('click', async () => {
        try {
            await api.openOverlayTheme();
        }
        catch (error) {
            showToast(errorMessage(error));
        }
    });
    el('openOverlayDesignFolder').addEventListener('click', async () => {
        try {
            await api.openOverlayDesignFolder();
        }
        catch (error) {
            showToast(errorMessage(error));
        }
    });
    el('reloadOverlayAssets').addEventListener('click', async () => {
        try {
            await api.reloadOverlayAssets();
            showToast('オーバーレイのJSON/SVGを再読み込みしました。');
        }
        catch (error) {
            showToast(errorMessage(error));
        }
    });
    const elevatedModal = el('elevatedModal');
    const elevatedSwitch = el('elevatedSwitch');
    const setElevatedBusy = (busy) => { elevatedBusy = busy; elevatedSwitch.classList.toggle('busy', busy); elevatedSwitch.setAttribute('aria-busy', String(busy)); };
    elevatedSwitch.addEventListener('click', async () => {
        if (elevatedBusy)
            return;
        if (!elevatedSwitch.classList.contains('on')) {
            elevatedModal.classList.add('show');
            return;
        }
        setElevatedBusy(true);
        showToast('管理者権限Captureを終了しています...');
        try {
            await renderSettings(await api.setCaptureElevated(false));
            showToast('管理者権限モードを無効にしました。通常権限Captureは継続しています。');
        }
        catch (error) {
            showToast(errorMessage(error));
        }
        finally {
            setElevatedBusy(false);
        }
    });
    el('cancelElevated').addEventListener('click', () => { if (!elevatedBusy)
        elevatedModal.classList.remove('show'); });
    el('confirmElevated').addEventListener('click', async () => {
        if (elevatedBusy)
            return;
        setElevatedBusy(true);
        showToast('管理者権限Captureへ接続しています...');
        try {
            await renderSettings(await api.setCaptureElevated(true));
            elevatedModal.classList.remove('show');
            showToast('管理者権限モードを有効にしました。通常権限Captureも継続しています。');
        }
        catch (error) {
            elevatedModal.classList.remove('show');
            await renderSettings().catch(() => undefined);
            showToast(errorMessage(error));
        }
        finally {
            setElevatedBusy(false);
        }
    });
    el('openDataFolder').addEventListener('click', async () => {
        try {
            await api.openDataFolder();
        }
        catch (error) {
            showToast(errorMessage(error));
        }
    });
    el('clearRealtimeLog').addEventListener('click', async () => {
        await api.clearRealtimeLogs();
        realtimeEntries = [];
        realtimeSequence = 0;
        renderRealtimeLog();
        showToast('リアルタイムログ表示をクリアしました。');
    });
    el('keyPolicyMatrixBody').addEventListener('change', updateMappingSummary);
    el('keyPolicyMatrixHead').addEventListener('click', event => {
        const header = event.target.closest('[data-pattern]');
        const id = header?.dataset.pattern;
        if (!id)
            return;
        if (selectedModifierColumns.has(id))
            selectedModifierColumns.delete(id);
        else
            selectedModifierColumns.add(id);
        updateMappingSummary();
    });
    el('mappingCategoryFilter').addEventListener('click', event => {
        const button = event.target.closest('button[data-category]');
        const category = button?.dataset.category;
        if (!button || !category)
            return;
        if (activeMappingCategories.has(category))
            activeMappingCategories.delete(category);
        else
            activeMappingCategories.add(category);
        button.classList.toggle('active', activeMappingCategories.has(category));
        applyMappingFilters();
    });
    el('mappingKeySearch').addEventListener('click', () => void searchMappingPhysicalKey());
    el('keyboardLayout').addEventListener('change', event => {
        const select = event.currentTarget;
        const layout = select.value;
        const previous = currentOverlayLabels?.keyboard_layout ?? 'japanese-109';
        void (async () => {
            try {
                // Keep label edits made in the table before applying the new default table.
                await api.setOverlayKeyLabels(labelsFromUi());
                const labels = await api.setOverlayKeyboardLayout(layout);
                if (currentKeyMapping && currentKeyPolicy)
                    renderKeyMapping(currentKeyMapping, currentKeyPolicy, labels);
                showToast(`${layout === 'japanese-109' ? '日本語109キー' : '英語104キー'}の表示名既定値を適用しました。`);
            }
            catch (error) {
                select.value = previous;
                showToast(errorMessage(error));
            }
        })();
    });
    el('mappingEnableAll').addEventListener('click', () => bulkMappingChange('record_and_display_break'));
    el('mappingContinueDisplay').addEventListener('click', () => bulkMappingChange('continue_display'));
    el('mappingRecordDisplay').addEventListener('click', () => bulkMappingChange('record_and_display_break'));
    el('mappingDisableAll').addEventListener('click', () => bulkMappingChange('ignore'));
    el('mappingReset').addEventListener('click', () => bulkMappingChange('reset'));
    el('mappingSave').addEventListener('click', event => void withButton(event.currentTarget, async () => {
        const mapping = await api.setKeyMapping(mappingFromUi());
        const [policy, labels] = await Promise.all([api.setOverlayKeyPolicy(policyFromUi()), api.setOverlayKeyLabels(labelsFromUi())]);
        renderKeyMapping(mapping, policy, labels);
        showToast('キーマッピングとオーバーレイ表示名を保存し、Captureへ即時反映しました。');
    }).catch(error => showToast(errorMessage(error))));
    el('mappingExport').addEventListener('click', event => void withButton(event.currentTarget, async () => {
        const mapping = await api.setKeyMapping(mappingFromUi());
        const [policy, labels] = await Promise.all([api.setOverlayKeyPolicy(policyFromUi()), api.setOverlayKeyLabels(labelsFromUi())]);
        renderKeyMapping(mapping, policy, labels);
        const result = await api.exportKeyMapping();
        showToast(`${result.path} を出力しました。`, { label: 'フォルダーを開く', run: () => api.openParentFolder(result.path) });
    }).catch(error => showToast(errorMessage(error))));
    window.addEventListener('resize', () => { if (activePage === 'usage')
        drawUsageGraph(); });
    el('openGithub').addEventListener('click', async () => {
        try {
            await loadHelp();
            if (appInfo)
                await api.openExternalUrl(appInfo.github_url);
        }
        catch (error) {
            showToast(errorMessage(error));
        }
    });
}
async function poll() {
    if (polling)
        return;
    polling = true;
    try {
        await refreshCaptureStatus();
        if (['dashboard', 'ranking', 'usage', 'shortcut-detail'].includes(activePage))
            await loadPage(activePage);
    }
    finally {
        polling = false;
    }
}
async function boot() {
    bindEvents();
    await api.onCaptureStateChanged(() => { void refreshCaptureStatus(); if (activePage === 'settings')
        void renderSettings(); });
    await api.onOverlayEnabledChanged(enabled => {
        setSwitch(el('overlaySwitch'), enabled);
        if (currentOverlaySettings)
            currentOverlaySettings.enabled = enabled;
    });
    await api.onNavigatePage(page => goPage(page));
    await api.onRealtimeLogChanged(() => {
        if (activePage === 'realtime-log')
            void loadRealtimeLog().catch(error => showToast(errorMessage(error)));
    });
    const initialSettings = await api.settings().catch(() => null);
    if (initialSettings)
        applyTheme(initialSettings.theme);
    await refreshCaptureStatus();
    await refreshDatabaseEnvironment().catch(() => undefined);
    await loadHelp().catch(() => undefined);
    el('dailyRankingDate').value = todayText();
    const initialPage = window.__SHORTCUT_KEY_COUNTER_INITIAL_PAGE__;
    if (initialPage && initialPage in titles)
        goPage(initialPage);
    else
        await loadDashboard();
    window.setInterval(() => void poll(), 5000);
}
void boot().catch(error => showToast(errorMessage(error)));
