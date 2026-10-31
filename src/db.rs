use std::path::Path;
use std::time::Duration;

use rusqlite::Connection;

// columns added after the first version of the table, older databases get them via ALTER TABLE
const COLUMNS: &[(&str, &str)] = &[
    ("format", "TEXT"),
    ("title", "TEXT"),
    ("author", "TEXT"),
    ("cover", "TEXT"), // zip entry of the epub cover image
    ("size", "INTEGER"),
    ("mtime", "INTEGER"),
    ("added_at", "INTEGER"),
    ("accessed_at", "INTEGER"), // last opened or downloaded through the api
];

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    // the watcher and request handlers write at the same time, wait rather than fail
    conn.busy_timeout(Duration::from_secs(5))?;
    Ok(conn)
}

pub fn init(path: &Path) -> rusqlite::Result<()> {
    let conn = open(path)?;

    conn.pragma_update_and_check(None, "journal_mode", "WAL", |_| Ok(()))?;

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS papers (
            id INTEGER PRIMARY KEY,
            filepath TEXT NOT NULL
        );
        -- enforce one row per file so INSERT ... ON CONFLICT can upsert
        CREATE UNIQUE INDEX IF NOT EXISTS papers_filepath ON papers (filepath);",
    )?;

    let existing = conn
        .prepare("SELECT name FROM pragma_table_info('papers')")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    for (name, kind) in COLUMNS {
        if !existing.iter().any(|e| e == name) {
            conn.execute(&format!("ALTER TABLE papers ADD COLUMN {name} {kind}"), [])?;
        }
    }

    Ok(())
}
