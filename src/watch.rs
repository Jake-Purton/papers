use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::Path;
use std::println;
use std::time::UNIX_EPOCH;

use rusqlite::params;
use std::fs::{self, File};
use std::path::PathBuf;
use zip::ZipArchive;

use crate::db;
use crate::meta::{self, Meta};

pub fn rescan(books_dir: &Path, db_file: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let conn = db::open(db_file)?;

    // Scan files and update database
    println!("Rescanning, checking for new files:");
    let mut files = Vec::new();

    scan_directory(books_dir, 0, &mut files)?;

    let found: HashSet<String> = files
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();

    let known: HashMap<String, Option<i64>> = conn
        .prepare("SELECT filepath, mtime FROM papers")?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;

    for path in files {
        let filepath = path.to_string_lossy();

        let metadata = fs::metadata(&path)?;
        let size = metadata.len() as i64;
        let mtime = metadata.modified()?.duration_since(UNIX_EPOCH)?.as_secs() as i64;

        // unchanged since the last scan, nothing to re-read
        let previous = known.get(&*filepath);
        if previous == Some(&Some(mtime)) {
            continue;
        }

        let format = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let valid = match format.as_str() {
            "pdf" => is_valid_pdf(&path),
            "epub" => is_valid_epub(&path),
            _ => false,
        };

        if !valid {
            println!("Invalid file: {}", path.display());
            continue;
        }

        let meta = match format.as_str() {
            "epub" => meta::epub(&path),
            _ => Meta::default(),
        };
        let title = meta.title.unwrap_or_else(|| meta::title_from_filename(&path));

        conn.execute(
            "INSERT INTO papers (filepath, format, title, author, cover, size, mtime, added_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, unixepoch())
            ON CONFLICT (filepath) DO UPDATE SET
                format = excluded.format,
                title = excluded.title,
                author = excluded.author,
                cover = excluded.cover,
                size = excluded.size,
                mtime = excluded.mtime,
                added_at = coalesce(papers.added_at, excluded.added_at)",
            params![&*filepath, format, title, meta.author, meta.cover, size, mtime],
        )?;

        if previous.is_some() {
            println!("Updated: {}", filepath);
        } else {
            println!("Added: {}", filepath);
        }
    }

    println!("Rescanning, checking for orphans:");

    let mut stmt = conn.prepare("SELECT id, filepath FROM papers")?;

    let papers = stmt.query_map([], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;

    let mut orphans = Vec::new();

    // anything the scan didn't find: deleted, moved out of books_dir, or stored under an old path
    for paper in papers {
        let (id, filepath) = paper?;

        if !found.contains(&filepath) {
            orphans.push((id, filepath));
        }
    }

    drop(stmt);

    for (id, filepath) in orphans {
        conn.execute("DELETE FROM papers WHERE id = ?1", [id])?;
        println!("Removed orphan: {}", filepath);
    }

    println!("Rescan done");
    Ok(())
}

fn is_valid_pdf(path: &Path) -> bool {
    let Ok(mut file) = File::open(path) else {
        return false;
    };

    // PDF header should appear within the first 1024 bytes.
    let mut buffer = [0u8; 1024];

    let Ok(n) = file.read(&mut buffer) else {
        return false;
    };

    buffer[..n].windows(5).any(|window| window == b"%PDF-")
}

fn is_valid_epub(path: &Path) -> bool {
    let Ok(file) = File::open(path) else {
        return false;
    };

    let Ok(mut archive) = ZipArchive::new(file) else {
        return false;
    };

    let Ok(mut mimetype) = archive.by_name("mimetype") else {
        return false;
    };

    let mut content = String::new();

    if mimetype.read_to_string(&mut content).is_err() {
        return false;
    }

    if content != "application/epub+zip" {
        return false;
    }

    // Release the mutable borrow of archive.
    drop(mimetype);

    archive.by_name("META-INF/container.xml").is_ok()
}

fn scan_directory(dir: &Path, depth: usize, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    if depth > 4 {
        return Ok(());
    }

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            scan_directory(&path, depth + 1, files)?;
        } else if file_type.is_file() {
            let extension = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();

            if extension == "pdf" || extension == "epub" {
                files.push(path);
            }
        }
    }

    Ok(())
}
