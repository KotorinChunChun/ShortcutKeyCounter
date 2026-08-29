CREATE TABLE IF NOT EXISTS shortcut_hourly_summary (
    date TEXT NOT NULL,
    hour INTEGER NOT NULL CHECK (hour BETWEEN 0 AND 23),
    count INTEGER NOT NULL CHECK (count >= 0),
    PRIMARY KEY (date, hour)
);

CREATE INDEX IF NOT EXISTS idx_hourly_summary_date
    ON shortcut_hourly_summary (date);

-- 詳細イベントに実時刻がある期間だけを正確に再構築する。
-- shortcut_dailyだけが残る過去データは、架空の時間帯へ配分しない。
INSERT INTO shortcut_hourly_summary (date, hour, count)
SELECT substr(occurred_at, 1, 10),
       CAST(substr(occurred_at, 12, 2) AS INTEGER),
       COUNT(*)
FROM shortcut_events
GROUP BY substr(occurred_at, 1, 10), CAST(substr(occurred_at, 12, 2) AS INTEGER)
ON CONFLICT(date, hour) DO UPDATE SET count = excluded.count;
