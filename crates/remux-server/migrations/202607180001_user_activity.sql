-- Track when a user last authenticated and was last active (device touch).
-- Both columns are RFC3339 text timestamps (UTC), nullable for legacy rows.
ALTER TABLE users ADD COLUMN last_login_at   TEXT;
ALTER TABLE users ADD COLUMN last_activity_at TEXT;

-- Admin-facing activity log. Mirrors the subset of Jellyfin's
-- ActivityLogDto shape that we actually populate (Type/Name/Overview/
-- UserId/Date/Severity/RowId/ShortOverview). Custom remux-only columns
-- (ip_address, device_id, client) live alongside rather than under a
-- `remux` namespace because this table is never serialized 1:1 to JSON —
-- the API handler projects it into the Jellyfin-shaped DTO.
CREATE TABLE IF NOT EXISTS activity_log (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    log_type        TEXT    NOT NULL,
    name            TEXT    NOT NULL,
    short_overview  TEXT,
    user_id         TEXT    REFERENCES users(id) ON DELETE SET NULL,
    date            TEXT    NOT NULL,
    severity        TEXT    NOT NULL DEFAULT 'Info',
    ip_address      TEXT,
    device_id       TEXT,
    client          TEXT
);

CREATE INDEX IF NOT EXISTS idx_activity_log_date ON activity_log(date DESC);
CREATE INDEX IF NOT EXISTS idx_activity_log_user ON activity_log(user_id, date DESC);
