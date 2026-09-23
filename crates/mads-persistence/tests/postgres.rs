//! Real PostgreSQL acceptance tests for native SeaORM integration.
#![cfg(feature = "sea-orm-postgres")]
#![allow(missing_docs)]

use mads_core::{
    ApplicationContext, ConfigBuilder, Diagnostic, LifecycleFuture, LifecycleHook, MADS006,
    MADS011, Mads, MapSource,
};
use mads_persistence::{
    MADS140, PersistenceError, PersistenceErrorKind,
    sea_orm::{DatabaseConnection, DatabaseModule},
};
use sea_orm::entity::prelude::*;
use sea_orm::{ActiveValue::Set, IntoActiveModel, TransactionTrait};

static TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

struct ShutdownObserver(std::sync::Arc<std::sync::atomic::AtomicBool>);

impl LifecycleHook for ShutdownObserver {
    fn name(&self) -> &str {
        "shutdown-observer"
    }

    fn start<'a>(&'a self, _: &'a ApplicationContext) -> LifecycleFuture<'a> {
        Box::pin(async { Ok(()) })
    }

    fn stop<'a>(&'a self, context: &'a ApplicationContext) -> LifecycleFuture<'a> {
        Box::pin(async move {
            context
                .resolve::<DatabaseConnection>()?
                .ping()
                .await
                .map_err(|_| {
                    mads_core::Error::new(Diagnostic::new(
                        MADS011,
                        "observer failed",
                        "database was closed before observer",
                    ))
                })?;
            self.0.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        })
    }
}

struct FailingStop;

impl LifecycleHook for FailingStop {
    fn name(&self) -> &str {
        "failing-stop"
    }
    fn start<'a>(&'a self, _: &'a ApplicationContext) -> LifecycleFuture<'a> {
        Box::pin(async { Ok(()) })
    }
    fn stop<'a>(&'a self, _: &'a ApplicationContext) -> LifecycleFuture<'a> {
        Box::pin(async {
            Err(mads_core::Error::new(Diagnostic::new(
                MADS011,
                "deliberate stop failure",
                "deliberate stop failure",
            )))
        })
    }
}

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "mads_persistence_v090_items")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub name: String,
}

impl sea_orm::ActiveModelBehavior for ActiveModel {}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

#[mads_core::module(imports = [DatabaseModule])]
pub struct Root;

#[derive(Clone)]
pub struct ItemRepository {
    database: DatabaseConnection,
}

#[mads_core::provider]
pub fn item_repository(database: DatabaseConnection) -> ItemRepository {
    ItemRepository { database }
}

fn builder(url: &str) -> mads_core::MadsBuilder {
    let config = ConfigBuilder::new()
        .source(MapSource::new("test", [("persistence.seaorm.url", url)]))
        .build()
        .unwrap();
    let mut builder = Mads::builder_with_config(config);
    builder.root::<Root>().unwrap();
    builder
}

#[tokio::test]
#[ignore = "requires PostgreSQL through MADS_TEST_DATABASE_URL"]
async fn native_crud_transactions_and_shutdown() {
    let _guard = TEST_LOCK.lock().await;
    let url = std::env::var("MADS_TEST_DATABASE_URL").expect("MADS_TEST_DATABASE_URL is required");
    let mut app = builder(&url).build().await.unwrap();
    app.start().await.unwrap();
    let repo = app.context().resolve::<ItemRepository>().unwrap();
    let db = repo.database.clone();
    db.execute_unprepared(
        "CREATE TABLE IF NOT EXISTS mads_persistence_v090_items (id SERIAL PRIMARY KEY, name TEXT NOT NULL)")
        .await.unwrap();
    db.execute_unprepared("TRUNCATE TABLE mads_persistence_v090_items RESTART IDENTITY")
        .await
        .unwrap();

    let created = ActiveModel {
        name: Set("first".to_owned()),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();
    assert_eq!(
        Entity::find_by_id(created.id)
            .one(&db)
            .await
            .unwrap()
            .unwrap()
            .name,
        "first"
    );
    let mut changed = created.clone().into_active_model();
    changed.name = Set("updated".to_owned());
    assert_eq!(changed.update(&db).await.unwrap().name, "updated");

    let committed = db.begin().await.unwrap();
    let kept = ActiveModel {
        name: Set("kept".to_owned()),
        ..Default::default()
    }
    .insert(&committed)
    .await
    .unwrap();
    committed.commit().await.unwrap();
    assert!(
        Entity::find_by_id(kept.id)
            .one(&db)
            .await
            .unwrap()
            .is_some()
    );

    let rolled_back = db.begin().await.unwrap();
    let discarded = ActiveModel {
        name: Set("discarded".to_owned()),
        ..Default::default()
    }
    .insert(&rolled_back)
    .await
    .unwrap();
    rolled_back.rollback().await.unwrap();
    assert!(
        Entity::find_by_id(discarded.id)
            .one(&db)
            .await
            .unwrap()
            .is_none()
    );

    created.into_active_model().delete(&db).await.unwrap();
    assert_eq!(Entity::find().all(&db).await.unwrap().len(), 1);
    app.shutdown().await.unwrap();
    assert!(db.ping().await.is_err());
}

#[tokio::test]
#[ignore = "requires PostgreSQL through MADS_TEST_DATABASE_URL"]
async fn invalid_credentials_keep_connection_error_and_redact_source() {
    let _guard = TEST_LOCK.lock().await;
    let url = std::env::var("MADS_TEST_DATABASE_URL").expect("MADS_TEST_DATABASE_URL is required");
    let (scheme, authority) = url.split_once("://").unwrap();
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let bad = format!("{scheme}://mads-secret-user:mads-secret-password@{host}");
    let error = builder(&bad)
        .build()
        .await
        .err()
        .expect("invalid credentials must fail");
    assert_eq!(error.code(), MADS006);
    assert!(!format!("{error:?} {error}").contains("mads-secret"));
    let middle = std::error::Error::source(&error)
        .unwrap()
        .downcast_ref::<mads_core::Error>()
        .unwrap();
    assert_eq!(middle.code(), MADS140);
    let persistence = std::error::Error::source(middle)
        .unwrap()
        .downcast_ref::<PersistenceError>()
        .unwrap();
    assert_eq!(persistence.kind(), PersistenceErrorKind::Connection);
}

#[tokio::test]
#[ignore = "requires PostgreSQL through MADS_TEST_DATABASE_URL"]
async fn query_failure_still_closes_pool_after_application_stop_failure() {
    let _guard = TEST_LOCK.lock().await;
    let url = std::env::var("MADS_TEST_DATABASE_URL").expect("MADS_TEST_DATABASE_URL is required");
    let seen = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut builder = builder(&url);
    builder.lifecycle_hook(ShutdownObserver(seen.clone()));
    builder.lifecycle_hook(FailingStop);
    let mut app = builder.build().await.unwrap();
    app.start().await.unwrap();
    let db = app.context().resolve::<DatabaseConnection>().unwrap();
    assert!(
        db.execute_unprepared("SELECT * FROM mads_persistence_v090_missing_table")
            .await
            .is_err()
    );
    let error = app.shutdown().await.unwrap_err();
    assert_eq!(error.code(), MADS011);
    assert_eq!(error.diagnostic().subject(), Some("failing-stop"));
    assert!(seen.load(std::sync::atomic::Ordering::SeqCst));
    assert!(db.ping().await.is_err());
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires PostgreSQL through MADS_TEST_DATABASE_URL"]
fn conventional_startup_checks_database_before_bind_and_handles_sigterm() {
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        process::{Command, Stdio},
        time::{Duration, Instant},
    };

    let url = std::env::var("MADS_TEST_DATABASE_URL").expect("MADS_TEST_DATABASE_URL is required");
    let build = Command::new("cargo")
        .args([
            "build",
            "-p",
            "mads-persistence",
            "--features",
            "sea-orm-postgres",
            "--example",
            "standard_run",
        ])
        .status()
        .unwrap();
    assert!(build.success());
    let binary = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("examples/standard_run");

    let occupied = TcpListener::bind("127.0.0.1:0").unwrap();
    let occupied_port = occupied.local_addr().unwrap().port();
    let (scheme, authority) = url.split_once("://").unwrap();
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let bad_url = format!("{scheme}://mads-secret-user:mads-secret-password@{host}");
    let failure = Command::new(&binary)
        .current_dir(std::env::temp_dir())
        .env("MADS_PERSISTENCE__SEAORM__URL", bad_url)
        .env("MADS_SERVER__HOST", "127.0.0.1")
        .env("MADS_SERVER__PORT", occupied_port.to_string())
        .output()
        .unwrap();
    assert!(!failure.status.success());
    let stderr = String::from_utf8_lossy(&failure.stderr);
    assert!(
        stderr.contains("MADS006") || stderr.contains("MADS140"),
        "{stderr}"
    );
    assert!(!stderr.contains("mads-secret"));
    assert!(
        !stderr.contains("bind"),
        "database failure must precede occupied listener"
    );
    drop(occupied);

    let available = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = available.local_addr().unwrap();
    drop(available);
    let mut child = Command::new(binary)
        .current_dir(std::env::temp_dir())
        .env("MADS_PERSISTENCE__SEAORM__URL", url)
        .env("MADS_SERVER__HOST", "127.0.0.1")
        .env("MADS_SERVER__PORT", address.port().to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut healthy = false;
    while Instant::now() < deadline {
        if let Ok(mut stream) = TcpStream::connect_timeout(&address, Duration::from_millis(100)) {
            let _ = stream.set_read_timeout(Some(Duration::from_millis(200)));
            let _ = stream
                .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
            let mut response = String::new();
            healthy = stream.read_to_string(&mut response).is_ok() && response.contains("healthy");
            if healthy {
                break;
            }
        }
        if child.try_wait().unwrap().is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    if !healthy {
        let _ = child.kill();
        let output = child.wait_with_output().unwrap();
        panic!(
            "health endpoint did not become ready: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert!(
        Command::new("kill")
            .args(["-TERM", &child.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
