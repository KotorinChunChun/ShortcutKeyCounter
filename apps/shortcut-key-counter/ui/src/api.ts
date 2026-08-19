import type {
  AppPaths, ApplicationStats, BackupResult, CaptureHealth, CaptureStatus, ClearResult,
  DailyPoint, DashboardSummary, DataBounds, ExcludedApp, ExportResult, RankedApplication,
  RankedShortcut, SettingsSnapshot, ShortcutDetail, ShortcutRankingRow, KeyMapping,
  KeyMappingExportResult, RealtimeLogEntry, AppInfo, Theme, OverlaySettings, KeyPolicy,
  OverlayMonitor, OverlayKeyLabels, DatabaseEnvironment, DatabaseEnvironmentSnapshot, SampleSourceStatus,
  SampleGenerationResult, DeleteBeforePreview, RankingsSnapshot, UsageGraphSnapshot, AnalyticsRangeKey
} from './types.js'

declare global {
  interface Window {
    __TAURI__?: {
      core: { invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> },
      event?: { listen<T>(event: string, handler: (event: { payload: T }) => void): Promise<() => void> }
    }
    __SHORTCUT_KEY_COUNTER_INITIAL_PAGE__?: string
  }
}

function invoke<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  const api = window.__TAURI__?.core
  if (!api) return Promise.reject({ code: 'TAURI_UNAVAILABLE', message: 'Tauri APIを利用できません。' })
  return api.invoke<T>(command, args)
}

function listen<T>(event: string, handler: (payload: T) => void): Promise<() => void> {
  const eventApi = window.__TAURI__?.event
  if (!eventApi) return Promise.resolve(() => undefined)
  return eventApi.listen<T>(event, event => handler(event.payload))
}

export const api = {
  captureStatus: () => invoke<CaptureStatus>('get_capture_status'),
  setCaptureEnabled: (enabled: boolean) => invoke<CaptureStatus>('set_capture_enabled', { enabled }),
  captureHealth: () => invoke<CaptureHealth>('get_capture_health'),
  dashboard: (start: string, end: string) => invoke<DashboardSummary>('get_dashboard_summary', { start, end }),
  shortcutRanking: (start: string, end: string, application: string | null, limit: number) => invoke<RankedShortcut[]>('get_shortcut_ranking', { start, end, application, limit }),
  shortcutRankingExtended: (start: string, end: string, application: string | null, limit: number) => invoke<ShortcutRankingRow[]>('get_shortcut_ranking_extended', { start, end, application, limit }),
  applicationRanking: (start: string, end: string, limit: number) => invoke<RankedApplication[]>('get_application_ranking', { start, end, limit }),
  applicationStatistics: (start: string, end: string) => invoke<ApplicationStats[]>('get_application_statistics', { start, end }),
  dailyTrend: (start: string, end: string, shortcut: string | null = null, application: string | null = null) => invoke<DailyPoint[]>('get_daily_trend', { start, end, shortcut, application }),
  shortcutDetail: (shortcut: string, start: string, end: string) => invoke<ShortcutDetail>('get_shortcut_detail', { shortcut, start, end }),
  listShortcuts: (start: string, end: string) => invoke<RankedShortcut[]>('list_shortcuts', { start, end }),
  dataBounds: () => invoke<DataBounds>('get_data_bounds'),
  rankingsV014: (range: AnalyticsRangeKey, application: string | null) => invoke<RankingsSnapshot>('get_rankings_v014', { range, application }),
  usageGraphV014: (mode: 'keys' | 'apps', range: AnalyticsRangeKey, identities: string[]) => invoke<UsageGraphSnapshot>('get_usage_graph_v014', { mode, range, identities }),
  usageGraphColors: () => invoke<Record<string, string>>('get_usage_graph_colors'),
  setUsageGraphColors: (colors: Record<string, string>) => invoke<Record<string, string>>('set_usage_graph_colors', { colors }),
  excludedApps: () => invoke<ExcludedApp[]>('get_excluded_apps'),
  addExcludedApp: (executableName: string) => invoke<ExcludedApp[]>('add_excluded_app', { executableName }),
  setExcludedEnabled: (executableName: string, enabled: boolean) => invoke<ExcludedApp[]>('set_excluded_app_enabled', { executableName, enabled }),
  removeExcludedApp: (executableName: string) => invoke<ExcludedApp[]>('remove_user_excluded_app', { executableName }),
  settings: () => invoke<SettingsSnapshot>('get_settings'),
  setAutoStart: (enabled: boolean) => invoke<SettingsSnapshot>('set_auto_start', { enabled }),
  setTheme: (theme: Theme) => invoke<SettingsSnapshot>('set_theme', { theme }),
  setUsageTracking: (enabled: boolean) => invoke<SettingsSnapshot>('set_usage_tracking', { enabled }),
  setDuplicateGuardSeconds: (seconds: number) => invoke<SettingsSnapshot>('set_duplicate_guard_seconds', { seconds }),
  setDetailEventRetentionDays: (days: number) => invoke<SettingsSnapshot>('set_detail_event_retention_days', { days }),
  setOverlayEnabled: (enabled: boolean) => invoke<SettingsSnapshot>('set_overlay_enabled', { enabled }),
  overlaySettings: () => invoke<OverlaySettings>('get_overlay_settings'),
  overlayMonitors: () => invoke<OverlayMonitor[]>('get_overlay_monitors'),
  previewOverlayMonitor: (monitorId: string, number: number) => invoke<void>('preview_overlay_monitor', { monitorId, number }),
  setOverlaySettings: (settings: OverlaySettings) => invoke<OverlaySettings>('set_overlay_settings', { settings }),
  overlayKeyLabels: () => invoke<OverlayKeyLabels>('get_overlay_key_labels'),
  setOverlayKeyLabels: (labels: OverlayKeyLabels) => invoke<OverlayKeyLabels>('set_overlay_key_labels', { labels }),
  overlayKeyPolicy: () => invoke<KeyPolicy>('get_overlay_key_policy'),
  setOverlayKeyPolicy: (policy: KeyPolicy) => invoke<KeyPolicy>('set_overlay_key_policy', { policy }),
  reloadOverlayAssets: () => invoke<void>('reload_overlay_assets'),
  previewOverlaySample: () => invoke<void>('preview_overlay_sample'),
  beginKeySearch: () => invoke<void>('begin_key_search'),
  pollKeySearch: () => invoke<string | null>('poll_key_search'),
  cancelKeySearch: () => invoke<void>('cancel_key_search'),
  setCaptureElevated: (enabled: boolean) => invoke<SettingsSnapshot>('set_capture_elevated', { enabled }),
  exportDaily: () => invoke<ExportResult>('export_daily_csv'),
  exportEvents: () => invoke<ExportResult>('export_events_csv'),
  databaseEnvironment: () => invoke<DatabaseEnvironmentSnapshot>('get_database_environment'),
  switchDatabaseEnvironment: (environment: DatabaseEnvironment) => invoke<DatabaseEnvironmentSnapshot>('switch_database_environment', { environment }),
  sampleSourceStatus: () => invoke<SampleSourceStatus>('get_sample_source_status'),
  generateSampleData: () => invoke<SampleGenerationResult>('generate_sample_data'),
  previewDeleteYesterdayAndEarlier: () => invoke<DeleteBeforePreview>('preview_delete_yesterday_and_earlier'),
  deleteYesterdayAndEarlier: () => invoke<DeleteBeforePreview>('delete_yesterday_and_earlier'),
  backupDatabase: () => invoke<BackupResult>('backup_database'),
  deleteStatistics: () => invoke<ClearResult>('delete_all_statistics'),
  appPaths: () => invoke<AppPaths>('get_app_paths'),
  openDataFolder: () => invoke<void>('open_data_folder'),
  openOverlayLabels: () => invoke<void>('open_overlay_labels'),
  openOverlayTheme: () => invoke<void>('open_overlay_theme'),
  openOverlayPolicy: () => invoke<void>('open_overlay_policy'),
  openOverlayDesignFolder: () => invoke<void>('open_overlay_design_folder'),
  openParentFolder: (path: string) => invoke<void>('open_parent_folder', { path }),
  keyMapping: () => invoke<KeyMapping>('get_key_mapping'),
  setKeyMapping: (mapping: KeyMapping) => invoke<KeyMapping>('set_key_mapping', { mapping }),
  exportKeyMapping: () => invoke<KeyMappingExportResult>('export_key_mapping_json'),
  realtimeLogs: (afterSequence: number, limit = 200) => invoke<RealtimeLogEntry[]>('get_realtime_logs', { afterSequence, limit }),
  clearRealtimeLogs: () => invoke<void>('clear_realtime_logs'),
  appInfo: () => invoke<AppInfo>('get_app_info'),
  openExternalUrl: (url: string) => invoke<void>('open_external_url', { url }),
  onCaptureStateChanged: (handler: (enabled: boolean) => void) => listen<boolean>('capture-state-changed', handler),
  onOverlayEnabledChanged: (handler: (enabled: boolean) => void) => listen<boolean>('overlay-enabled-changed', handler),
  onNavigatePage: (handler: (page: string) => void) => listen<string>('navigate-page', handler),
  onRealtimeLogChanged: (handler: (sequence: number) => void) => listen<number>('realtime-log-changed', handler),
}
