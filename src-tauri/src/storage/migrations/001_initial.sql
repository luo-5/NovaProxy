-- 001_initial.sql: Initial SQLite schema for NovaProxy

CREATE TABLE IF NOT EXISTS subscriptions (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    url TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    user_agent TEXT,
    update_interval_mins INTEGER,
    use_proxy INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER,
    upload_bytes INTEGER,
    download_bytes INTEGER,
    total_bytes INTEGER,
    expire_at INTEGER,
    home_url TEXT,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

CREATE TABLE IF NOT EXISTS groups (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    group_type TEXT NOT NULL DEFAULT 'selector',
    node_ids_json TEXT NOT NULL DEFAULT '[]',
    test_url TEXT,
    interval_secs INTEGER,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

CREATE TABLE IF NOT EXISTS nodes (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    server TEXT NOT NULL,
    port INTEGER NOT NULL,
    protocol TEXT NOT NULL,
    subscription_id TEXT REFERENCES subscriptions(id) ON DELETE CASCADE,
    group_id TEXT REFERENCES groups(id) ON DELETE SET NULL,
    fingerprint TEXT NOT NULL,
    latency_ms INTEGER,
    sort_order INTEGER NOT NULL DEFAULT 0,
    node_json TEXT NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_nodes_subscription_id ON nodes(subscription_id);
CREATE INDEX IF NOT EXISTS idx_nodes_group_id ON nodes(group_id);
CREATE INDEX IF NOT EXISTS idx_nodes_protocol ON nodes(protocol);
CREATE INDEX IF NOT EXISTS idx_nodes_fingerprint ON nodes(fingerprint);

CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY NOT NULL,
    value_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

CREATE TABLE IF NOT EXISTS latency_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    node_id TEXT NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    tested_at INTEGER NOT NULL,
    latency_ms INTEGER,
    test_type TEXT NOT NULL DEFAULT 'tcp',
    error_msg TEXT
);

CREATE INDEX IF NOT EXISTS idx_latency_history_node_time ON latency_history(node_id, tested_at DESC);
