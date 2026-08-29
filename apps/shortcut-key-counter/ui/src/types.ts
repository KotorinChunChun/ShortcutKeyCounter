export interface CommandError { code: string; message: string }
export interface CaptureStatus { enabled: boolean; dropped_queue_items: number }
export type CaptureSource = 'local' | 'elevated' | 'local_fallback'
export type HealthLevel = 'ok' | 'degraded' | 'error'
export interface CaptureHealth { source: CaptureSource; level: HealthLevel; message: string | null }
export type Theme = 'dark' | 'light'
export interface SettingsSnapshot { auto_start: boolean; capture_elevated: boolean; theme: Theme; overlay_enabled: boolean; usage_tracking_enabled: boolean; duplicate_guard_seconds: number; detail_event_retention_days: number }
export type OverlayPosition =
  | 'top-left' | 'top-center' | 'top-right'
  | 'middle-left' | 'center' | 'middle-right'
  | 'bottom-left' | 'bottom-center' | 'bottom-right'
export type OverlayPositionMode = 'fixed' | 'auto'
export type OverlayDisplayMode = 'desktop' | 'active-window'
export type OverlayMonitorMode = 'active-window' | 'fixed'
export type OverlayInAnimation = 'none' | 'fade' | 'slide-up' | 'slide-down' | 'slide-left' | 'slide-right' | 'pop'
export type OverlayOutAnimation = 'none' | 'fade' | 'slide-up' | 'slide-down' | 'slide-left' | 'slide-right' | 'shrink'
export type RepeatBadgeShape = 'pill' | 'rounded' | 'square'
export interface RepeatCounterTheme { background_color: string; text_color: string; accent_color: string; shape: RepeatBadgeShape; rotation_deg: number; font_size: number }
export type KeyAction = 'ignore' | 'continue_display' | 'display_break' | 'record_and_display_break'
export interface OverlayMonitor { id: string; label: string }
export interface OverlaySettings {
  enabled: boolean
  display_duration_ms: number
  display_mode: OverlayDisplayMode
  monitor_mode: OverlayMonitorMode
  fixed_monitor_id: string
  position_mode: OverlayPositionMode
  fixed_position: OverlayPosition
  allowed_positions: OverlayPosition[]
  preferred_position: OverlayPosition
  max_rows: number
  background_color: string
  background_opacity: number
  enter_animation: OverlayInAnimation
  exit_animation: OverlayOutAnimation
  enter_ms: number
  exit_ms: number
  repeat_counter: RepeatCounterTheme
  default_single_key_action: KeyAction
}
export type KeyboardLayout = 'japanese-109' | 'us-104'
export interface OverlayKeyLabels { version: number; keyboard_layout: KeyboardLayout; keys: Record<string, string> }
export interface KeyPolicy {
  version: number
  default_single_key: KeyAction
  default_shortcut: KeyAction
  overrides: Record<string, KeyAction>
}
export interface RankedShortcut { shortcut: string; count: number }
export interface ShortcutRankingRow extends RankedShortcut { application_count: number }
export interface RankedApplication { application: string; count: number }
export interface DailyPoint { date: string; count: number }
export interface DashboardSummary { total_count: number; unique_shortcuts: number; unique_applications: number; top_shortcut: RankedShortcut | null }
export interface ApplicationStats { application: string; count: number; unique_shortcuts: number; most_used_shortcut: string | null; active_seconds: number }
export interface ShortcutDetail { shortcut: string; total_count: number; applications: RankedApplication[]; daily: DailyPoint[] }
export interface ExcludedApp { id: number; executable_name: string; source: 'default' | 'user' | string; enabled: boolean }
export interface ExportResult { path: string; rows: number }
export interface DailyExportFilterSnapshot { applications: string[]; selected: string[] | null; first_date: string | null; last_date: string | null; name: string }
export interface BackupResult { path: string }
export interface ClearResult { events_deleted: number; daily_deleted: number; usage_deleted: number }
export interface DeleteBeforePreview { cutoff_date: string; events: number; daily_rows: number; usage_rows: number }
export interface DataBounds { first_date: string | null; last_date: string | null }
export interface AppPaths { data_directory: string; database_path: string; logs_directory: string }
export type RangeKey = 'today' | '7' | '30' | 'all'
export interface DateRange { start: string; end: string }

export interface KeyMappingEntry { virtual_key: number; name: string; enabled: boolean }
export interface MouseKeyMapping {
  left_click: boolean; right_click: boolean; middle_click: boolean; x1_click: boolean; x2_click: boolean;
  wheel_up: boolean; wheel_down: boolean; wheel_left: boolean; wheel_right: boolean
}
export interface KeyMapping { version: number; keyboard: KeyMappingEntry[]; mouse: MouseKeyMapping }
export interface KeyMappingExportResult { path: string }
export interface RealtimeLogEntry { sequence: number; occurred_at: string; shortcut: string; application: string; outcome: string }
export interface AppInfo { name: string; version: string; author: string; github_url: string }

export type DatabaseEnvironment = 'production' | 'test'
export interface DatabaseEnvironmentSnapshot { environment: DatabaseEnvironment; display_name: string; path: string; is_test: boolean }
export interface SampleSourceStatus { application_count: number; shortcut_count: number; can_generate: boolean }
export interface SampleGenerationResult { events_generated: number; usage_rows_generated: number; period_start: string; period_end: string; realtime_log_rows: number }

export type AnalyticsRangeKey = 'today' | '7d' | '30d' | 'all'
export interface KeyRankingItem { shortcut: string; count: number; average: number; ratio: number; comparison_rate: number | null }
export interface ApplicationRankingItem { application: string; count: number; key_type_count: number; active_seconds: number; ratio: number; comparison_rate: number | null }
export interface RankingsSnapshot { comparison_label: string | null; applications: string[]; key_rows: KeyRankingItem[]; application_rows: ApplicationRankingItem[] }
export interface UsageGraphPoint { bucket: string; count: number }
export interface UsageGraphSeries { identity: string; points: UsageGraphPoint[] }
export interface UsageGraphSnapshot { bucket_kind: string; buckets: string[]; series: UsageGraphSeries[] }
