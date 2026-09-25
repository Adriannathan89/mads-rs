use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::extract::{Path, State, rejection::JsonRejection};
use axum::http::{
    HeaderMap, HeaderValue, StatusCode,
    header::{AUTHORIZATION, WWW_AUTHENTICATE},
};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};

#[derive(Deserialize)]
struct PostInput {
    title: String,
    body: String,
}

#[derive(Serialize)]
struct Post {
    id: i32,
    title: String,
    body: String,
}

#[derive(Deserialize)]
struct LoginInput {
    username: String,
    password: String,
}

#[derive(Serialize, Deserialize)]
struct Claims {
    sub: String,
    user_id: u64,
    exp: usize,
}

#[derive(Clone)]
struct AuthState {
    username: String,
    password: String,
    secret: String,
}

fn error(status: StatusCode, code: &str) -> (StatusCode, Json<Value>) {
    (status, Json(json!({"error": {"code": code}})))
}

fn passport_unauthorized() -> axum::response::Response {
    let mut response = (
        StatusCode::UNAUTHORIZED,
        Json(json!({"error": {"code": "unauthorized", "message": "authentication was rejected"}})),
    )
        .into_response();
    response
        .headers_mut()
        .insert(WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
    response
}

fn json_rejection(rejection: JsonRejection) -> (StatusCode, Json<Value>) {
    if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE {
        error(StatusCode::PAYLOAD_TOO_LARGE, "payload_too_large")
    } else if rejection.status() == StatusCode::UNSUPPORTED_MEDIA_TYPE {
        error(StatusCode::UNSUPPORTED_MEDIA_TYPE, "unsupported_media_type")
    } else {
        error(StatusCode::UNPROCESSABLE_ENTITY, "validation_error")
    }
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let mut values = headers.get_all(AUTHORIZATION).iter();
    let value = values.next()?.to_str().ok()?;
    if values.next().is_some() {
        return None;
    }
    let mut parts = value.split_whitespace();
    let scheme = parts.next()?;
    let token = parts.next()?;
    if !scheme.eq_ignore_ascii_case("Bearer") || parts.next().is_some() {
        return None;
    }
    Some(token)
}

fn valid_post(
    input: Result<Json<PostInput>, JsonRejection>,
) -> Result<PostInput, (StatusCode, Json<Value>)> {
    let Json(input) = input.map_err(json_rejection)?;
    if input.title.is_empty() || input.title.chars().count() > 120 || input.body.is_empty() {
        return Err(error(StatusCode::UNPROCESSABLE_ENTITY, "validation_error"));
    }
    Ok(input)
}

async fn hello() -> &'static str {
    "Hello, world!"
}

async fn create(
    State(pool): State<PgPool>,
    input: Result<Json<PostInput>, JsonRejection>,
) -> impl IntoResponse {
    let input = match valid_post(input) {
        Ok(input) => input,
        Err(error) => return error.into_response(),
    };
    match sqlx::query("INSERT INTO posts (title, body) VALUES ($1, $2) RETURNING id")
        .bind(&input.title)
        .bind(&input.body)
        .fetch_one(&pool)
        .await
    {
        Ok(row) => (
            StatusCode::CREATED,
            Json(json!(Post {
                id: row.get("id"),
                title: input.title,
                body: input.body
            })),
        )
            .into_response(),
        Err(_) => error(StatusCode::INTERNAL_SERVER_ERROR, "internal").into_response(),
    }
}

async fn list(State(pool): State<PgPool>) -> impl IntoResponse {
    match sqlx::query("SELECT id, title, body FROM posts ORDER BY id")
        .fetch_all(&pool)
        .await
    {
        Ok(rows) => Json(
            rows.into_iter()
                .map(|row| Post {
                    id: row.get("id"),
                    title: row.get("title"),
                    body: row.get("body"),
                })
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(_) => error(StatusCode::INTERNAL_SERVER_ERROR, "internal").into_response(),
    }
}

async fn find(State(pool): State<PgPool>, Path(id): Path<i32>) -> impl IntoResponse {
    match sqlx::query("SELECT id, title, body FROM posts WHERE id = $1")
        .bind(id)
        .fetch_optional(&pool)
        .await
    {
        Ok(Some(row)) => Json(Post {
            id: row.get("id"),
            title: row.get("title"),
            body: row.get("body"),
        })
        .into_response(),
        Ok(None) => error(StatusCode::NOT_FOUND, "not_found").into_response(),
        Err(_) => error(StatusCode::INTERNAL_SERVER_ERROR, "internal").into_response(),
    }
}

async fn update(
    State(pool): State<PgPool>,
    Path(id): Path<i32>,
    input: Result<Json<PostInput>, JsonRejection>,
) -> impl IntoResponse {
    let input = match valid_post(input) {
        Ok(input) => input,
        Err(error) => return error.into_response(),
    };
    match sqlx::query("UPDATE posts SET title = $1, body = $2 WHERE id = $3 RETURNING id")
        .bind(&input.title)
        .bind(&input.body)
        .bind(id)
        .fetch_optional(&pool)
        .await
    {
        Ok(Some(row)) => Json(Post {
            id: row.get("id"),
            title: input.title,
            body: input.body,
        })
        .into_response(),
        Ok(None) => error(StatusCode::NOT_FOUND, "not_found").into_response(),
        Err(_) => error(StatusCode::INTERNAL_SERVER_ERROR, "internal").into_response(),
    }
}

async fn delete(State(pool): State<PgPool>, Path(id): Path<i32>) -> impl IntoResponse {
    match sqlx::query("DELETE FROM posts WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
    {
        Ok(result) if result.rows_affected() > 0 => StatusCode::NO_CONTENT.into_response(),
        Ok(_) => error(StatusCode::NOT_FOUND, "not_found").into_response(),
        Err(_) => error(StatusCode::INTERNAL_SERVER_ERROR, "internal").into_response(),
    }
}

async fn login(
    State(state): State<AuthState>,
    input: Result<Json<LoginInput>, JsonRejection>,
) -> impl IntoResponse {
    let Json(input) = match input {
        Ok(input) => input,
        Err(rejection) => return json_rejection(rejection).into_response(),
    };
    if input.username.is_empty() || input.password.chars().count() < 8 {
        return error(StatusCode::UNPROCESSABLE_ENTITY, "validation_error").into_response();
    }
    if input.username != state.username || input.password != state.password {
        eprintln!("login rejected");
        return error(StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    let exp = (SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        + Duration::from_secs(900))
    .as_secs() as usize;
    let claims = Claims {
        sub: "1".into(),
        user_id: 1,
        exp,
    };
    match jsonwebtoken::encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(state.secret.as_bytes()),
    ) {
        Ok(token) => {
            eprintln!("login succeeded");
            Json(json!({"access_token": token})).into_response()
        }
        Err(_) => error(StatusCode::INTERNAL_SERVER_ERROR, "internal").into_response(),
    }
}

async fn me(State(state): State<AuthState>, headers: HeaderMap) -> impl IntoResponse {
    let Some(token) = bearer_token(&headers) else {
        return passport_unauthorized();
    };
    let decoded = jsonwebtoken::decode::<Claims>(
        token,
        &DecodingKey::from_secret(state.secret.as_bytes()),
        &Validation::new(Algorithm::HS256),
    );
    match decoded {
        Ok(data) if data.claims.user_id == 1 && data.claims.sub == "1" => {
            eprintln!("protected profile read");
            Json(json!({"id": 1, "username": state.username})).into_response()
        }
        _ => passport_unauthorized(),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = std::env::var("BENCH_MODE")?;
    let port: u16 = std::env::var("PORT")?.parse()?;
    let app = match mode.as_str() {
        "hello" => Router::new().route("/", get(hello)),
        "posts" => {
            let pool = PgPoolOptions::new()
                .max_connections(10)
                .connect(&std::env::var("DATABASE_URL")?)
                .await?;
            Router::new()
                .route("/posts", get(list).post(create))
                .route("/posts/{id}", get(find).put(update).delete(delete))
                .with_state(pool)
        }
        "auth" => {
            let state = AuthState {
                username: std::env::var("DEMO_USERNAME")?,
                password: std::env::var("DEMO_PASSWORD")?,
                secret: std::env::var("JWT_SECRET")?,
            };
            Router::new()
                .route("/auth/login", axum::routing::post(login))
                .route("/auth/me", get(me))
                .with_state(state)
        }
        _ => return Err("BENCH_MODE must be hello, posts, or auth".into()),
    };
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
