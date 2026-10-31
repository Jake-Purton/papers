use std::io::Read;
use std::path::Path;
use std::println;

use rusqlite::Connection;
use std::fs::{self, File};
use std::path::PathBuf;
use zip::ZipArchive;

pub fn rescan(books_dir: &Path, db_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let conn = Connection::open(db_dir)?;

    // Scan files and update database
    println!("Rescanning, checking for new files:");
    let mut files = Vec::new();

    scan_directory(books_dir, 0, &mut files)?;

    for path in files {
        let valid = match path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase()
            .as_str()
        {
            "pdf" => is_valid_pdf(&path),
            "epub" => is_valid_epub(&path),
            _ => false,
        };

        if !valid {
            println!("Invalid file: {}", path.display());
            continue;
        }

        let filepath = path.to_string_lossy();

        let inserted = conn.execute(
            "INSERT OR IGNORE INTO papers (filepath) VALUES (?1)",
            [&*filepath],
        )?;

        if inserted > 0 {
            println!("Added: {}", filepath);
        }
    }

    println!("Rescanning, checking for orphans:");

    let mut stmt = conn.prepare("SELECT id, filepath FROM papers")?;

    let papers = stmt.query_map([], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;

    let mut orphans = Vec::new();

    for paper in papers {
        let (id, filepath) = paper?;

        if !Path::new(&filepath).exists() {
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
