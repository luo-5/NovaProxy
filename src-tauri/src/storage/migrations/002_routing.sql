-- 002_routing.sql: Routing rules table

CREATE TABLE IF NOT EXISTS routing_rules (
    id TEXT PRIMARY KEY NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    rule_type TEXT NOT NULL,
    pattern TEXT NOT NULL,
    action TEXT NOT NULL,
    action_target TEXT,
    description TEXT,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_routing_rules_sort ON routing_rules(sort_order ASC, created_at ASC);
