mod watch;

use std::path::Path;
use std::time::Duration;

use tokio::sync::mpsc::channel;

use axum::{Router, routing::get};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use rusqlite::Connection;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let books_path: String = std::env::var("BOOKS_DIR").unwrap_or("./data/books".into());
    let db_path: String = std::env::var("DB_DIR").unwrap_or("./data/db/database.db".into());

    let books_dir = Path::new(&books_path).to_path_buf();
    let db_dir = Path::new(&db_path).to_path_buf();

    let conn = Connection::open(db_path).unwrap();

    conn.execute(
        "CREATE TABLE IF NOT EXISTS papers (
            id INTEGER PRIMARY KEY,
            filepath TEXT NOT NULL
        )",
        [],
    )
    .unwrap();

    let (tx, mut rx) = channel(100);

    let mut watcher = RecommendedWatcher::new(
        move |event| {
            let _ = tx.blocking_send(event);
        },
        Config::default(),
    )
    .unwrap();

    watcher
        .watch(
            std::path::Path::new("./data/books"),
            RecursiveMode::Recursive,
        )
        .unwrap();

    // rescan on init, blocking so we don't overlap
    watch::rescan(&books_dir, &db_dir);

    {
        let books_dir = books_dir.clone();
        let db_dir = db_dir.clone();

        tokio::spawn(async move {
            // wait for an event
            while let Some(_) = rx.recv().await {
                // if one happens wait 100ms then drain the queue of events
                tokio::time::sleep(Duration::from_millis(100)).await;
                while let Ok(_) = rx.try_recv() {}
                let _ = watch::rescan(&books_dir, &db_dir);
            }
        });
    }

    let port: String = std::env::var("PORT").unwrap_or("8080".into());

    let pages: Router = Router::new();

    let app = Router::new().merge(pages);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port))
        .await
        .unwrap();

    println!("listening on 0.0.0.0:{}", port);

    axum::serve(listener, app).await.unwrap();
}
