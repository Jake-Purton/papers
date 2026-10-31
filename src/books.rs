use std::path::PathBuf;

use axum::Json;
use axum::body::Body;
use axum::extract::{Path, Query, Request, State};
use axum::http::header::{
    CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_SECURITY_POLICY, CONTENT_TYPE,
    X_CONTENT_TYPE_OPTIONS,
};
use axum::http::{HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use rusqlite::{Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use tower::ServiceExt;
use tower_http::services::ServeFile;

use crate::db;
use crate::meta;

#[derive(Clone)]
pub struct AppState {
    pub db_file: PathBuf,
    // canonical, every served file must live under it
    pub books_dir: PathBuf,
}

#[derive(Serialize)]
pub struct Book {
    id: i64,
    title: String,
    author: Option<String>,
    format: String,
    filename: String,
    size: i64,
    added_at: Option<i64>,
    accessed_at: Option<i64>,
    cover_url: Option<String>,
}

const SELECT_BOOK: &str = "SELECT id, filepath, format, title, author, size, mtime, added_at, accessed_at, cover IS NOT NULL FROM papers";

fn book_from_row(row: &Row) -> rusqlite::Result<Book> {
    let id: i64 = row.get(0)?;
    let filepath: String = row.get(1)?;
    let path = std::path::Path::new(&filepath);
    let mtime: Option<i64> = row.get(6)?;

    Ok(Book {
        id,
        title: row
            .get::<_, Option<String>>(3)?
            .unwrap_or_else(|| meta::title_from_filename(path)),
        author: row.get(4)?,
        format: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
        filename: path
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default(),
        size: row.get::<_, Option<i64>>(5)?.unwrap_or(0),
        added_at: row.get(7)?,
        accessed_at: row.get(8)?,
        // versioned by mtime so the cover can be cached forever
        cover_url: row
            .get::<_, bool>(9)?
            .then(|| format!("/api/books/{id}/cover?v={}", mtime.unwrap_or(0))),
    })
}

// run blocking sqlite work off the async workers
async fn query<T, F>(state: &AppState, f: F) -> Result<T, StatusCode>
where
    T: Send + 'static,
    F: FnOnce(&Connection) -> rusqlite::Result<T> + Send + 'static,
{
    let db_file = state.db_file.clone();

    tokio::task::spawn_blocking(move || f(&db::open(&db_file)?))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map_err(|e| {
            eprintln!("database error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

fn touch(conn: &Connection, id: i64) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE papers SET accessed_at = unixepoch() WHERE id = ?1",
        [id],
    )
}

fn inside_books_dir(state: &AppState, filepath: &str) -> Result<PathBuf, StatusCode> {
    let path = std::fs::canonicalize(filepath).map_err(|_| StatusCode::NOT_FOUND)?;

    if !path.starts_with(&state.books_dir) {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(path)
}

// access times change constantly, and browsers reuse cached responses on back/forward
const NO_STORE: [(HeaderName, &str); 1] = [(CACHE_CONTROL, "no-store")];

pub async fn books(State(state): State<AppState>) -> Result<impl IntoResponse, StatusCode> {
    query(&state, |conn| {
        conn.prepare(&format!(
            "{SELECT_BOOK} ORDER BY accessed_at IS NULL, accessed_at DESC, added_at DESC, id DESC"
        ))?
        .query_map([], book_from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()
    })
    .await
    .map(|books| (NO_STORE, Json(books)))
}

// opening a book in the reader fetches this, so it counts as an access
pub async fn book(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, StatusCode> {
    query(&state, move |conn| {
        touch(conn, id)?;
        conn.query_row(&format!("{SELECT_BOOK} WHERE id = ?1"), [id], book_from_row)
            .optional()
    })
    .await?
    .map(|book| (NO_STORE, Json(book)))
    .ok_or(StatusCode::NOT_FOUND)
}

#[derive(Deserialize)]
pub struct FileQuery {
    download: Option<String>,
}

pub async fn book_file(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(params): Query<FileQuery>,
    req: Request,
) -> Result<Response, StatusCode> {
    let filepath = query(&state, move |conn| {
        touch(conn, id)?;
        conn.query_row("SELECT filepath FROM papers WHERE id = ?1", [id], |row| {
            row.get::<_, String>(0)
        })
        .optional()
    })
    .await?
    .ok_or(StatusCode::NOT_FOUND)?;

    let path = inside_books_dir(&state, &filepath)?;

    let filename = path
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_default();

    // ServeFile handles Range, If-Modified-Since and HEAD, and sets the mime type from the extension
    let mut res = ServeFile::new(&path)
        .oneshot(req)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map(Body::new);

    let disposition = if params.download.is_some() {
        "attachment"
    } else {
        "inline"
    };

    if let Ok(value) = HeaderValue::from_str(&format!(
        "{disposition}; filename*=UTF-8''{}",
        utf8_percent_encode(&filename, NON_ALPHANUMERIC)
    )) {
        res.headers_mut().insert(CONTENT_DISPOSITION, value);
    }

    Ok(res)
}

pub async fn book_cover(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Response, StatusCode> {
    let (filepath, cover) = query(&state, move |conn| {
        conn.query_row(
            "SELECT filepath, cover FROM papers WHERE id = ?1 AND cover IS NOT NULL",
            [id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
    })
    .await?
    .ok_or(StatusCode::NOT_FOUND)?;

    let path = inside_books_dir(&state, &filepath)?;

    let mime = match cover.rsplit('.').next().unwrap_or("").to_lowercase().as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    };

    let bytes = tokio::task::spawn_blocking(move || meta::read_zip_entry(&path, &cover))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok((
        [
            (CONTENT_TYPE, mime),
            (CACHE_CONTROL, "public, max-age=31536000, immutable"),
            // covers come from untrusted files, an svg opened directly must not run scripts
            (CONTENT_SECURITY_POLICY, "default-src 'none'; style-src 'unsafe-inline'; sandbox"),
            (X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        bytes,
    )
        .into_response())
}
