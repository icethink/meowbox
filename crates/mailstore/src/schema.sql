-- Meowbox schema v2
-- 変更するときは migrations の version を上げること（Store::open が適用する）

CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS accounts (
    id            INTEGER PRIMARY KEY,
    name          TEXT NOT NULL,
    kind          TEXT NOT NULL CHECK (kind IN ('imap','gmail','m365')),
    email         TEXT NOT NULL UNIQUE,
    project_tag   TEXT,
    settings_json TEXT NOT NULL DEFAULT '{}',
    created_at    TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS folders (
    id           INTEGER PRIMARY KEY,
    account_id   INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    path         TEXT NOT NULL,
    role         TEXT NOT NULL DEFAULT 'other',
    uidvalidity  INTEGER,
    last_uid     INTEGER NOT NULL DEFAULT 0,
    UNIQUE (account_id, path)
);

CREATE TABLE IF NOT EXISTS messages (
    id              INTEGER PRIMARY KEY,
    account_id      INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    folder_id       INTEGER NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
    uid             INTEGER NOT NULL,
    message_id      TEXT,
    thread_key      TEXT NOT NULL,
    from_addr       TEXT NOT NULL,
    from_name       TEXT,
    to_json         TEXT NOT NULL DEFAULT '[]',
    cc_json         TEXT NOT NULL DEFAULT '[]',
    subject         TEXT NOT NULL DEFAULT '',
    date            TEXT NOT NULL,
    snippet         TEXT NOT NULL DEFAULT '',
    body_text       TEXT NOT NULL DEFAULT '',
    body_html       TEXT,
    has_attachments INTEGER NOT NULL DEFAULT 0,
    is_read         INTEGER NOT NULL DEFAULT 0,
    is_flagged      INTEGER NOT NULL DEFAULT 0,
    is_archived     INTEGER NOT NULL DEFAULT 0,
    raw_path        TEXT,
    UNIQUE (folder_id, uid)
);
CREATE INDEX IF NOT EXISTS idx_messages_thread ON messages(thread_key, date);
CREATE INDEX IF NOT EXISTS idx_messages_account_date ON messages(account_id, date DESC);
CREATE INDEX IF NOT EXISTS idx_messages_message_id ON messages(message_id);

CREATE TABLE IF NOT EXISTS attachments (
    id         INTEGER PRIMARY KEY,
    message_id INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    filename   TEXT NOT NULL,
    mime       TEXT NOT NULL,
    size       INTEGER NOT NULL,
    path       TEXT
);

CREATE TABLE IF NOT EXISTS ai_summaries (
    id         INTEGER PRIMARY KEY,
    target     TEXT NOT NULL,   -- "message:<id>" | "thread:<key>" | "daily:<yyyy-mm-dd>"
    model      TEXT NOT NULL,
    summary    TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_ai_summaries_target ON ai_summaries(target);

CREATE TABLE IF NOT EXISTS tasks (
    id                INTEGER PRIMARY KEY,
    account_id        INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    source_message_id INTEGER REFERENCES messages(id) ON DELETE SET NULL,
    title             TEXT NOT NULL,
    due               TEXT,
    status            TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open','done','dismissed')),
    confidence        REAL NOT NULL DEFAULT 1.0,
    created_by        TEXT NOT NULL DEFAULT 'user',
    created_at        TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS drafts (
    id          INTEGER PRIMARY KEY,
    account_id  INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    in_reply_to INTEGER REFERENCES messages(id) ON DELETE SET NULL,
    to_json     TEXT NOT NULL DEFAULT '[]',
    subject     TEXT NOT NULL DEFAULT '',
    body        TEXT NOT NULL DEFAULT '',
    status      TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft','approved','sent')),
    created_at  TEXT NOT NULL
);

-- 全文検索。trigram tokenizer で日本語も部分一致できる（SQLite 3.34+）。
CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
    subject, body_text, from_name, from_addr,
    content='messages', content_rowid='id',
    tokenize='trigram'
);

CREATE TRIGGER IF NOT EXISTS messages_ai AFTER INSERT ON messages BEGIN
    INSERT INTO messages_fts(rowid, subject, body_text, from_name, from_addr)
    VALUES (new.id, new.subject, new.body_text, new.from_name, new.from_addr);
END;
CREATE TRIGGER IF NOT EXISTS messages_ad AFTER DELETE ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, subject, body_text, from_name, from_addr)
    VALUES ('delete', old.id, old.subject, old.body_text, old.from_name, old.from_addr);
END;
CREATE TRIGGER IF NOT EXISTS messages_au AFTER UPDATE ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, subject, body_text, from_name, from_addr)
    VALUES ('delete', old.id, old.subject, old.body_text, old.from_name, old.from_addr);
    INSERT INTO messages_fts(rowid, subject, body_text, from_name, from_addr)
    VALUES (new.id, new.subject, new.body_text, new.from_name, new.from_addr);
END;
