use axum::extract::Path;

pub async fn book(Path(id): Path<u32>) -> Result<Response, StatusCode> {}

pub async fn books() {}
