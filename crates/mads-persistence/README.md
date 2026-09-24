# mads-persistence

Native persistence connector integration for MADS.rs. Connector features are
opt-in; the crate has no default database backend.

For PostgreSQL, depend explicitly on:

```toml
mads-persistence = { version = "0.9.1", features = ["sea-orm-postgres"] }
```

Import `mads_persistence::sea_orm::DatabaseModule` in the application root.
Its global provider exposes SeaORM's native `DatabaseConnection` to selected
services and repositories. Readiness is checked before the listener binds;
the connection closes on graceful shutdown. The connector does not generate
or apply migrations.

For manual construction, `DatabaseFactory::provide(SeaOrmPostgres::new(url))`
returns `PersistenceResult<DatabaseConnection>`: the native connection on
success or a typed `PersistenceError` on failure. `PersistenceError::kind()`
classifies failures while public formatting redacts connection details. MADS
does not map persistence failures automatically to HTTP responses. See the
[persistence design](../../docs/mads-persistence.md) for the connector model.
