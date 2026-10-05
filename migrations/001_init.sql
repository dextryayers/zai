-- AICLI sqlite schema v1, applied in Phase 3.
-- Kept here in Phase 0 for review, not yet executed.
CREATE TABLE IF NOT EXISTS sessions(
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  model TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'active'
);
CREATE TABLE IF NOT EXISTS turns(
  id TEXT PRIMARY KEY,
  session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
  role TEXT NOT NULL,
  content TEXT NOT NULL,
  tokens_in INTEGER NOT NULL DEFAULT 0,
  tokens_out INTEGER NOT NULL DEFAULT 0,
  status TEXT NOT NULL DEFAULT 'done',
  created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_turns_session ON turns(session_id, created_at);
CREATE TABLE IF NOT EXISTS tasks(
  id TEXT PRIMARY KEY,
  date TEXT NOT NULL,
  text TEXT NOT NULL,
  done INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_tasks_date ON tasks(date, done);
CREATE TABLE IF NOT EXISTS notes(
  id TEXT PRIMARY KEY,
  date TEXT NOT NULL,
  text TEXT NOT NULL,
  created_at TEXT NOT NULL
);
