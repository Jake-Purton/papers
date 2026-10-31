mod books;
mod db;
mod meta;
mod watch;

use std::fs;
use std::path::Path;
use std::time::Duration;

use tokio::sync::mpsc::channel;

use axum::{Router, routing::get};
use books::{AppState, book, book_cover, book_file, books};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use tower_http::services::ServeDir;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let books_path: String = std::env::var("BOOKS_DIR").unwrap_or("./data/books".into());
    let db_path: String = std::env::var("DB_PATH").unwrap_or("./data/db/database.db".into());
    let static_path: String = std::env::var("STATIC_DIR").unwrap_or("./static".into());

    let db_file = Path::new(&db_path).to_path_buf();

    // data/db is gitignored, so it won't exist on a fresh checkout
    fs::create_dir_all(&books_path).unwrap();
    if let Some(parent) = db_file.parent() {
        fs::create_dir_all(parent).unwrap();
    }

    // absolute, so a file's stored path doesn't depend on how BOOKS_DIR was spelled
    let books_dir = fs::canonicalize(&books_path).unwrap();

    db::init(&db_file).unwrap();

    let (tx, mut rx) = channel(100);

    let mut watcher = RecommendedWatcher::new(
        move |event: notify::Result<notify::Event>| {
            // opening/reading a file is an Access event; our own rescan causes those,
            // so reacting to them would loop forever
            match &event {
                Ok(e) if e.kind.is_access() => {}
                _ => {
                    let _ = tx.blocking_send(event);
                }
            }
        },
        Config::default(),
    )
    .unwrap();

    watcher
        .watch(
            &books_dir,
            RecursiveMode::Recursive,
        )
        .unwrap();

    // rescan on init, blocking so we don't overlap
    if let Err(e) = watch::rescan(&books_dir, &db_file) {
        eprintln!("rescan failed: {e}");
    }

    {
        let books_dir = books_dir.clone();
        let db_file = db_file.clone();

        tokio::spawn(async move {
            // wait for an event
            while let Some(_) = rx.recv().await {
                // if one happens wait 100ms then drain the queue of events
                tokio::time::sleep(Duration::from_millis(100)).await;
                while let Ok(_) = rx.try_recv() {}
                // rescan does blocking fs and sqlite work, keep it off the async workers
                let books_dir = books_dir.clone();
                let db_file = db_file.clone();
                let result = tokio::task::spawn_blocking(move || {
                    watch::rescan(&books_dir, &db_file).map_err(|e| e.to_string())
                })
                .await;
                if let Ok(Err(e)) = result {
                    eprintln!("rescan failed: {e}");
                }
            }
        });
    }

    let port: String = std::env::var("PORT").unwrap_or("8080".into());

    let state = AppState {
        db_file: db_file.clone(),
        books_dir: books_dir.clone(),
    };

    let api: Router = Router::new()
        .route("/api/books", get(books))
        .route("/api/books/{id}", get(book))
        .route("/api/books/{id}/file", get(book_file))
        .route("/api/books/{id}/cover", get(book_cover))
        .with_state(state);

    // everything that isn't the api is the frontend
    let app = api.fallback_service(ServeDir::new(static_path));

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port))
        .await
        .unwrap();

    println!("listening on 0.0.0.0:{}", port);

    axum::serve(listener, app).await.unwrap();
}
