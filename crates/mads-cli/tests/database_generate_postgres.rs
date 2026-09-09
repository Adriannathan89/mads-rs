//! PostgreSQL round-trip acceptance coverage for schema-diff generation.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Output,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use assert_cmd::Command;
use diesel::{
    Connection, PgConnection, QueryableByName, RunQueryDsl,
    connection::SimpleConnection,
    sql_query,
    sql_types::{Bool, Integer, Nullable, Text},
};
use mads::diesel;
use serde_json::{Value, json};
use tempfile::{TempDir, tempdir};

static TEST_LOCK: Mutex<()> = Mutex::new(());
static SCHEMA_SEQUENCE: AtomicU64 = AtomicU64::new(0);

const SCHEMA_PLACEHOLDER: &str = "__MADS_TEST_SCHEMA__";

#[test]
#[ignore = "requires PostgreSQL through MADS_TEST_DATABASE_URL"]
fn creates_a_table_with_a_single_primary_key() {
    assert_round_trip(GenerationCase::new(
        "",
        schema_files(&[schema("users (id)", "id -> Int8,\nname -> Text,")]),
        shape(&[table(
            "users",
            &[column("id", "bigint", false), column("name", "text", false)],
            &["id"],
        )]),
    ));
}

#[test]
#[ignore = "requires PostgreSQL through MADS_TEST_DATABASE_URL"]
fn creates_a_table_with_a_composite_primary_key() {
    assert_round_trip(GenerationCase::new(
        "",
        schema_files(&[schema(
            "memberships (user_id, team_id)",
            "user_id -> Int8,\nteam_id -> Int8,\nrole -> Text,",
        )]),
        shape(&[table(
            "memberships",
            &[
                column("user_id", "bigint", false),
                column("team_id", "bigint", false),
                column("role", "text", false),
            ],
            &["user_id", "team_id"],
        )]),
    ));
}

#[test]
#[ignore = "requires PostgreSQL through MADS_TEST_DATABASE_URL"]
fn drops_a_table_and_recreates_its_shape_on_down() {
    assert_round_trip(
        GenerationCase::new(
            "CREATE TABLE __MADS_TEST_SCHEMA__.anchor (id bigint PRIMARY KEY);\nCREATE TABLE __MADS_TEST_SCHEMA__.users (id bigint PRIMARY KEY, name text NOT NULL);",
            schema_files(&[schema("anchor (id)", "id -> Int8,")]),
            shape(&[table(
                "anchor",
                &[column("id", "bigint", false)],
                &["id"],
            )]),
        )
        .with_warnings(&["down.sql restores shape, not data"]),
    );
}

#[test]
#[ignore = "requires PostgreSQL through MADS_TEST_DATABASE_URL"]
fn adds_a_nullable_column() {
    assert_round_trip(GenerationCase::new(
        "CREATE TABLE __MADS_TEST_SCHEMA__.users (id bigint PRIMARY KEY);",
        schema_files(&[schema(
            "users (id)",
            "id -> Int8,\nnickname -> Nullable<Text>,",
        )]),
        shape(&[table(
            "users",
            &[
                column("id", "bigint", false),
                column("nickname", "text", true),
            ],
            &["id"],
        )]),
    ));
}

#[test]
#[ignore = "requires PostgreSQL through MADS_TEST_DATABASE_URL"]
fn adds_a_non_null_column_to_an_empty_table_with_a_warning() {
    assert_round_trip(
        GenerationCase::new(
            "CREATE TABLE __MADS_TEST_SCHEMA__.users (id bigint PRIMARY KEY);",
            schema_files(&[schema("users (id)", "id -> Int8,\nemail -> Text,")]),
            shape(&[table(
                "users",
                &[
                    column("id", "bigint", false),
                    column("email", "text", false),
                ],
                &["id"],
            )]),
        )
        .with_warnings(&["manual backfill or data-safety review"]),
    );
}

#[test]
#[ignore = "requires PostgreSQL through MADS_TEST_DATABASE_URL"]
fn drops_an_ordinary_column_and_recreates_its_shape_on_down() {
    assert_round_trip(
        GenerationCase::new(
            "CREATE TABLE __MADS_TEST_SCHEMA__.users (id bigint PRIMARY KEY, nickname text);",
            schema_files(&[schema("users (id)", "id -> Int8,")]),
            shape(&[table("users", &[column("id", "bigint", false)], &["id"])]),
        )
        .with_warnings(&["down.sql restores shape, not data"]),
    );
}

#[test]
#[ignore = "requires PostgreSQL through MADS_TEST_DATABASE_URL"]
fn changes_integer_to_bigint_with_castable_data() {
    assert_round_trip(
        GenerationCase::new(
            "CREATE TABLE __MADS_TEST_SCHEMA__.scores (id bigint PRIMARY KEY, score integer NOT NULL);\nINSERT INTO __MADS_TEST_SCHEMA__.scores VALUES (1, 7);",
            schema_files(&[schema("scores (id)", "id -> Int8,\nscore -> Int8,")]),
            shape(&[table(
                "scores",
                &[
                    column("id", "bigint", false),
                    column("score", "bigint", false),
                ],
                &["id"],
            )]),
        )
        .with_warnings(&["risky cast"]),
    );
}

#[test]
#[ignore = "requires PostgreSQL through MADS_TEST_DATABASE_URL"]
fn generate_json_emits_one_top_level_mads212_warning_without_data_duplication() {
    with_project(
        GenerationCase::new(
            "CREATE TABLE __MADS_TEST_SCHEMA__.scores (id bigint PRIMARY KEY, score integer NOT NULL);",
            schema_files(&[schema("scores (id)", "id -> Int8,\nscore -> Int8,")]),
            shape(&[table(
                "scores",
                &[
                    column("id", "bigint", false),
                    column("score", "bigint", false),
                ],
                &["id"],
            )]),
        ),
        |mut project| {
            project.apply_live_start_sql();
            project.write_schema_files();

            let output = project.run_generate_json_output();
            assert_success(&output);
            assert!(output.stderr.is_empty(), "stderr was not empty: {output:?}");
            let document: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
                panic!(
                    "JSON stdout should be exactly one document: {error}; stdout={:?}",
                    String::from_utf8_lossy(&output.stdout)
                )
            });

            assert_eq!(document["command"], "db generate");
            assert_eq!(document["ok"], true);
            let data = document["data"]
                .as_object()
                .expect("data should be an object");
            assert_eq!(
                data.keys().map(String::as_str).collect::<Vec<_>>(),
                ["migration_path", "review_required", "status"]
            );
            assert_eq!(data["status"], "generated");
            assert_eq!(data["review_required"], true);
            assert!(
                data["migration_path"].as_str().is_some_and(
                    |path| path.starts_with("migrations/") && path.ends_with("_schema_diff")
                ),
                "migration path was not package-relative: {document}"
            );
            assert!(
                !document["data"].to_string().contains("risky cast"),
                "review warning leaked into data: {}",
                document["data"]
            );

            assert_eq!(
                document["diagnostics"],
                json!([{
                    "severity": "warning",
                    "code": "MADS212",
                    "title": "Migration requires review",
                    "message": "changing a column type may require a risky cast; review the generated SQL",
                    "subject": format!("{}.scores.score", project.schema_name),
                    "location": null,
                    "suggestions": ["review up.sql and down.sql before applying"]
                }])
            );

            let migration = only_migration_directory(project.root());
            let up_sql = fs::read_to_string(migration.join("up.sql"))
                .expect("generated up.sql should be readable");
            let down_sql = fs::read_to_string(migration.join("down.sql"))
                .expect("generated down.sql should be readable");
            assert_warning_comment(&up_sql, "risky cast");
            assert_warning_comment(&down_sql, "risky cast");
        },
    );
}

#[test]
#[ignore = "requires PostgreSQL through MADS_TEST_DATABASE_URL"]
fn changes_nullable_to_not_null_with_non_null_data() {
    assert_round_trip(GenerationCase::new(
        "CREATE TABLE __MADS_TEST_SCHEMA__.users (id bigint PRIMARY KEY, nickname text);\nINSERT INTO __MADS_TEST_SCHEMA__.users VALUES (1, 'mads');",
        schema_files(&[schema("users (id)", "id -> Int8,\nnickname -> Text,")]),
        shape(&[table(
            "users",
            &[
                column("id", "bigint", false),
                column("nickname", "text", false),
            ],
            &["id"],
        )]),
    ));
}

#[test]
#[ignore = "requires PostgreSQL through MADS_TEST_DATABASE_URL"]
fn changes_not_null_to_nullable() {
    assert_round_trip(GenerationCase::new(
        "CREATE TABLE __MADS_TEST_SCHEMA__.users (id bigint PRIMARY KEY, nickname text NOT NULL);",
        schema_files(&[schema(
            "users (id)",
            "id -> Int8,\nnickname -> Nullable<Text>,",
        )]),
        shape(&[table(
            "users",
            &[
                column("id", "bigint", false),
                column("nickname", "text", true),
            ],
            &["id"],
        )]),
    ));
}

#[test]
#[ignore = "requires PostgreSQL through MADS_TEST_DATABASE_URL"]
fn merges_tables_from_split_schema_files() {
    let files = BTreeMap::from([
        (
            PathBuf::from("src/schema/user.rs"),
            schema("users (id)", "id -> Int8,\nname -> Text,"),
        ),
        (
            PathBuf::from("src/schema/comment.rs"),
            schema("comments (id)", "id -> Int8,\nbody -> Text,"),
        ),
    ]);
    assert_round_trip(GenerationCase::new(
        "",
        files,
        shape(&[
            table(
                "comments",
                &[column("id", "bigint", false), column("body", "text", false)],
                &["id"],
            ),
            table(
                "users",
                &[column("id", "bigint", false), column("name", "text", false)],
                &["id"],
            ),
        ]),
    ));
}

#[test]
#[ignore = "requires PostgreSQL through MADS_TEST_DATABASE_URL"]
fn mixed_operations_are_rendered_in_deterministic_order() {
    assert_round_trip(
        GenerationCase::new(
            "CREATE TABLE __MADS_TEST_SCHEMA__.alpha (id bigint PRIMARY KEY, score integer NOT NULL, label text NOT NULL, obsolete text);\nCREATE TABLE __MADS_TEST_SCHEMA__.zeta (id bigint PRIMARY KEY);",
            schema_files(&[
                schema(
                    "alpha (id)",
                    "id -> Int8,\nadded -> Nullable<Text>,\nscore -> Int8,\nlabel -> Nullable<Text>,",
                ),
                schema("beta (id)", "id -> Int8,"),
            ]),
            shape(&[
                table(
                    "alpha",
                    &[
                        column("id", "bigint", false),
                        column("score", "bigint", false),
                        column("label", "text", true),
                        column("added", "text", true),
                    ],
                    &["id"],
                ),
                table(
                    "beta",
                    &[column("id", "bigint", false)],
                    &["id"],
                ),
            ]),
        )
        .with_warnings(&["risky cast", "down.sql restores shape, not data"])
        .with_up_order(&[
            "CREATE TABLE \"__MADS_TEST_SCHEMA__\".\"beta\"",
            "ALTER TABLE \"__MADS_TEST_SCHEMA__\".\"alpha\" ADD COLUMN \"added\"",
            "ALTER TABLE \"__MADS_TEST_SCHEMA__\".\"alpha\" ALTER COLUMN \"score\" TYPE bigint",
            "ALTER TABLE \"__MADS_TEST_SCHEMA__\".\"alpha\" ALTER COLUMN \"label\" DROP NOT NULL",
            "ALTER TABLE \"__MADS_TEST_SCHEMA__\".\"alpha\" DROP COLUMN \"obsolete\"",
            "DROP TABLE \"__MADS_TEST_SCHEMA__\".\"zeta\"",
        ]),
    );
}

#[test]
#[ignore = "requires PostgreSQL through MADS_TEST_DATABASE_URL"]
fn exact_no_diff_creates_no_migration_files() {
    with_project(
        GenerationCase::new(
            "CREATE TABLE __MADS_TEST_SCHEMA__.users (id bigint PRIMARY KEY, nickname text);",
            schema_files(&[schema(
                "users (id)",
                "id -> Int8,\nnickname -> Nullable<Text>,",
            )]),
            shape(&[table(
                "users",
                &[
                    column("id", "bigint", false),
                    column("nickname", "text", true),
                ],
                &["id"],
            )]),
        ),
        |mut project| {
            project.apply_live_start_sql();
            project.write_schema_files();
            assert_eq!(
                project.capture_supported_shape(),
                project.case.desired_shape()
            );
            let output = project.run_generate_output();
            assert_success(&output);
            assert!(String::from_utf8_lossy(&output.stdout).contains("schema is up to date"));
            assert!(!project.root().join("migrations").exists());
        },
    );
}

#[test]
#[ignore = "requires PostgreSQL through MADS_TEST_DATABASE_URL"]
fn affected_unsupported_objects_produce_named_warnings_and_comments() {
    with_project(unsupported_objects_case(), |mut project| {
        project.apply_live_start_sql();
        project.write_schema_files();
        let generated = project.run_generate();
        for fragment in [
            "users.note",
            "users.users_note_idx",
            "users.users_note_check",
            "users.users_touch_trigger",
            "children.children_parent_id_fkey",
        ] {
            assert!(
                generated.stdout.contains(fragment),
                "missing terminal warning {fragment}"
            );
            assert_warning_comment(generated.up_sql(), fragment);
            assert_warning_comment(generated.down_sql(), fragment);
        }
    });
}

#[test]
#[ignore = "requires PostgreSQL through MADS_TEST_DATABASE_URL"]
fn retained_child_foreign_key_blocks_parent_drop_before_files() {
    assert_generation_fails_before_files(
        GenerationCase::new(
            "CREATE TABLE __MADS_TEST_SCHEMA__.parents (id bigint PRIMARY KEY);\nCREATE TABLE __MADS_TEST_SCHEMA__.children (id bigint PRIMARY KEY, parent_id bigint NOT NULL, CONSTRAINT children_parent_id_fkey FOREIGN KEY (parent_id) REFERENCES __MADS_TEST_SCHEMA__.parents(id));",
            schema_files(&[schema("children (id)", "id -> Int8,\nparent_id -> Int8,")]),
            shape(&[]),
        ),
        "MADS212",
    );
}

#[test]
#[ignore = "requires PostgreSQL through MADS_TEST_DATABASE_URL"]
fn primary_key_changes_fail_before_files() {
    assert_generation_fails_before_files(
        GenerationCase::new(
            "CREATE TABLE __MADS_TEST_SCHEMA__.memberships (user_id bigint NOT NULL, team_id bigint NOT NULL, PRIMARY KEY (user_id));",
            schema_files(&[schema(
                "memberships (user_id, team_id)",
                "user_id -> Int8,\nteam_id -> Int8,",
            )]),
            shape(&[]),
        ),
        "MADS212",
    );
}

#[test]
#[ignore = "requires PostgreSQL through MADS_TEST_DATABASE_URL"]
fn custom_types_fail_before_files() {
    assert_generation_fails_before_files(
        GenerationCase::new(
            "",
            schema_files(&[format!(
                "diesel::table! {{ {SCHEMA_PLACEHOLDER}.events (id) {{ id -> Int8, payload -> SecretExtensionType, }} }}\n"
            )]),
            shape(&[]),
        ),
        "MADS210",
    );
}

fn assert_round_trip(case: GenerationCase) {
    with_project(case, |mut project| {
        project.apply_live_start_sql();
        project.write_schema_files();
        let initial = project.capture_supported_shape();
        let generated = project.run_generate();
        project.apply_sql(generated.up_sql());
        assert_eq!(
            project.capture_supported_shape(),
            project.case.desired_shape()
        );
        project.apply_sql(generated.down_sql());
        assert_eq!(project.capture_supported_shape(), initial);
    });
}

fn assert_generation_fails_before_files(case: GenerationCase, diagnostic: &str) {
    with_project(case, |mut project| {
        project.apply_live_start_sql();
        project.write_schema_files();
        let output = project.run_generate_output();
        assert!(
            !output.status.success(),
            "generation unexpectedly succeeded"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(diagnostic),
            "stderr did not contain {diagnostic}: {stderr}"
        );
        assert!(!project.root().join("migrations").exists());
    });
}

fn with_project(case: GenerationCase, test: impl FnOnce(RoundTripProject)) {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    test(RoundTripProject::new(&case));
}

struct GenerationCase {
    live_start_sql: String,
    schema_files: BTreeMap<PathBuf, String>,
    desired_shape: SupportedShape,
    expected_warning_fragments: Vec<String>,
    expected_up_order: Vec<String>,
}

impl GenerationCase {
    fn new(
        live_start_sql: impl Into<String>,
        schema_files: BTreeMap<PathBuf, String>,
        desired_shape: SupportedShape,
    ) -> Self {
        Self {
            live_start_sql: live_start_sql.into(),
            schema_files,
            desired_shape,
            expected_warning_fragments: Vec::new(),
            expected_up_order: Vec::new(),
        }
    }

    fn with_warnings(mut self, fragments: &[&str]) -> Self {
        self.expected_warning_fragments = fragments.iter().map(|value| (*value).into()).collect();
        self
    }

    fn with_up_order(mut self, fragments: &[&str]) -> Self {
        self.expected_up_order = fragments.iter().map(|value| (*value).into()).collect();
        self
    }

    fn desired_shape(&self) -> SupportedShape {
        self.desired_shape.clone()
    }
}

struct RoundTripProject {
    case: GenerationCase,
    project: TempDir,
    database_url: String,
    schema_name: String,
    connection: PgConnection,
    _cleanup: SchemaCleanup,
}

impl RoundTripProject {
    fn new(case: &GenerationCase) -> Self {
        let database_url = std::env::var("MADS_TEST_DATABASE_URL")
            .expect("MADS_TEST_DATABASE_URL is required for ignored PostgreSQL tests");
        let schema_name = unique_schema_name();
        let connection = PgConnection::establish(&database_url)
            .expect("PostgreSQL test connection should be established");
        let project = tempdir().expect("temporary MADS project should be created");
        fs::write(
            project.path().join("Cargo.toml"),
            "[package]\nname = \"database-generate-postgres-test\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .expect("test Cargo manifest should be written");
        fs::create_dir(project.path().join("src")).expect("source directory should be created");
        fs::write(project.path().join("src/lib.rs"), "").expect("library target should be written");
        fs::write(
            project.path().join("mads.toml"),
            "[database]\nurl = \"${MADS_TEST_DATABASE_URL}\"\npool_size = 1\nmigrate = false\n",
        )
        .expect("MADS database configuration should be written");

        Self {
            case: GenerationCase {
                live_start_sql: substitute_schema(&case.live_start_sql, &schema_name),
                schema_files: case
                    .schema_files
                    .iter()
                    .map(|(path, source)| (path.clone(), substitute_schema(source, &schema_name)))
                    .collect(),
                desired_shape: case.desired_shape.clone(),
                expected_warning_fragments: case.expected_warning_fragments.clone(),
                expected_up_order: case
                    .expected_up_order
                    .iter()
                    .map(|fragment| substitute_schema(fragment, &schema_name))
                    .collect(),
            },
            project,
            database_url: database_url.clone(),
            schema_name: schema_name.clone(),
            connection,
            _cleanup: SchemaCleanup {
                database_url,
                schema_name,
            },
        }
    }

    fn root(&self) -> &Path {
        self.project.path()
    }

    fn apply_live_start_sql(&mut self) {
        self.connection
            .batch_execute(&format!(
                "CREATE SCHEMA {};",
                quote_identifier(&self.schema_name)
            ))
            .expect("unique PostgreSQL schema should be created");
        if !self.case.live_start_sql.trim().is_empty() {
            self.connection
                .batch_execute(&self.case.live_start_sql)
                .expect("live starting SQL should apply");
        }
    }

    fn write_schema_files(&self) {
        for (relative_path, source) in &self.case.schema_files {
            let path = self.root().join(relative_path);
            fs::create_dir_all(path.parent().expect("schema path should have a parent"))
                .expect("schema parent directory should be created");
            fs::write(path, source).expect("schema source should be written");
        }
    }

    fn capture_supported_shape(&mut self) -> SupportedShape {
        let rows = sql_query(
            r#"
                SELECT c.relname AS table_name,
                       a.attnum::integer AS ordinal,
                       a.attname AS column_name,
                       pg_catalog.format_type(a.atttypid, a.atttypmod) AS sql_type,
                       NOT a.attnotnull AS nullable,
                       (
                           SELECT key.ordinality::integer
                           FROM pg_catalog.pg_index AS primary_index
                           CROSS JOIN LATERAL unnest(primary_index.indkey)
                               WITH ORDINALITY AS key(attnum, ordinality)
                           WHERE primary_index.indrelid = c.oid
                             AND primary_index.indisprimary
                             AND key.attnum = a.attnum
                           LIMIT 1
                       ) AS primary_key_position
                FROM pg_catalog.pg_attribute AS a
                JOIN pg_catalog.pg_class AS c ON c.oid = a.attrelid
                JOIN pg_catalog.pg_namespace AS n ON n.oid = c.relnamespace
                WHERE n.nspname = $1
                  AND c.relkind IN ('r', 'p')
                  AND a.attnum > 0
                  AND NOT a.attisdropped
                ORDER BY c.relname, a.attnum
            "#,
        )
        .bind::<Text, _>(&self.schema_name)
        .load::<ShapeRow>(&mut self.connection)
        .expect("supported PostgreSQL shape should be captured");

        let mut tables = BTreeMap::<String, (Vec<ColumnShape>, Vec<(i32, String)>)>::new();
        for row in rows {
            assert!(row.ordinal > 0, "catalog column ordinal must be positive");
            let entry = tables.entry(row.table_name).or_default();
            entry.0.push(ColumnShape {
                name: row.column_name.clone(),
                sql_type: row.sql_type,
                nullable: row.nullable,
            });
            if let Some(position) = row.primary_key_position {
                entry.1.push((position, row.column_name));
            }
        }
        SupportedShape(
            tables
                .into_iter()
                .map(|(name, (columns, mut primary_key))| {
                    primary_key.sort_by_key(|(position, _)| *position);
                    TableShape {
                        name,
                        columns,
                        primary_key: primary_key.into_iter().map(|(_, column)| column).collect(),
                    }
                })
                .collect(),
        )
    }

    fn run_generate(&self) -> GeneratedSql {
        let output = self.run_generate_output();
        assert_success(&output);
        let stdout = String::from_utf8(output.stdout).expect("CLI stdout should be UTF-8");
        for warning in &self.case.expected_warning_fragments {
            assert!(
                stdout.contains(warning),
                "missing warning `{warning}` in {stdout}"
            );
        }
        let migration = only_migration_directory(self.root());
        let up_sql = fs::read_to_string(migration.join("up.sql"))
            .expect("generated up.sql should be readable");
        let down_sql = fs::read_to_string(migration.join("down.sql"))
            .expect("generated down.sql should be readable");
        for warning in &self.case.expected_warning_fragments {
            assert_warning_comment(&up_sql, warning);
            assert_warning_comment(&down_sql, warning);
        }
        assert_in_order(&up_sql, &self.case.expected_up_order);
        GeneratedSql {
            up_sql,
            down_sql,
            stdout,
        }
    }

    fn run_generate_output(&self) -> Output {
        let mut command = Command::cargo_bin("mads").expect("mads test binary should build");
        command
            .current_dir(self.root())
            .env_remove("DATABASE_URL")
            .env_remove("MADS_DATABASE__URL")
            .env("MADS_TEST_DATABASE_URL", &self.database_url)
            .args(["db", "generate"])
            .output()
            .expect("mads db generate should run")
    }

    fn run_generate_json_output(&self) -> Output {
        let mut command = Command::cargo_bin("mads").expect("mads test binary should build");
        command
            .current_dir(self.root())
            .env_remove("DATABASE_URL")
            .env_remove("MADS_DATABASE__URL")
            .env("MADS_TEST_DATABASE_URL", &self.database_url)
            .args(["db", "generate", "--format", "json"])
            .output()
            .expect("mads db generate JSON should run")
    }

    fn apply_sql(&mut self, sql: &str) {
        self.connection
            .batch_execute(sql)
            .expect("generated SQL should apply to PostgreSQL");
    }
}

struct GeneratedSql {
    up_sql: String,
    down_sql: String,
    stdout: String,
}

impl GeneratedSql {
    fn up_sql(&self) -> &str {
        &self.up_sql
    }

    fn down_sql(&self) -> &str {
        &self.down_sql
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SupportedShape(Vec<TableShape>);

#[derive(Clone, Debug, Eq, PartialEq)]
struct TableShape {
    name: String,
    columns: Vec<ColumnShape>,
    primary_key: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ColumnShape {
    name: String,
    sql_type: String,
    nullable: bool,
}

#[derive(QueryableByName)]
struct ShapeRow {
    #[diesel(sql_type = Text)]
    table_name: String,
    #[diesel(sql_type = Integer)]
    ordinal: i32,
    #[diesel(sql_type = Text)]
    column_name: String,
    #[diesel(sql_type = Text)]
    sql_type: String,
    #[diesel(sql_type = Bool)]
    nullable: bool,
    #[diesel(sql_type = Nullable<Integer>)]
    primary_key_position: Option<i32>,
}

struct SchemaCleanup {
    database_url: String,
    schema_name: String,
}

impl Drop for SchemaCleanup {
    fn drop(&mut self) {
        if let Ok(mut connection) = PgConnection::establish(&self.database_url) {
            let _ = connection.batch_execute(&format!(
                "DROP SCHEMA IF EXISTS {} CASCADE;",
                quote_identifier(&self.schema_name)
            ));
        }
    }
}

fn unsupported_objects_case() -> GenerationCase {
    GenerationCase::new(
        r#"
            CREATE TABLE __MADS_TEST_SCHEMA__.users (
                id bigint PRIMARY KEY,
                note text CONSTRAINT users_note_default DEFAULT 'draft',
                CONSTRAINT users_note_check CHECK (length(note) > 0)
            );
            CREATE INDEX users_note_idx ON __MADS_TEST_SCHEMA__.users(note);
            CREATE FUNCTION __MADS_TEST_SCHEMA__.touch_users() RETURNS trigger LANGUAGE plpgsql AS $$
            BEGIN RETURN NEW; END
            $$;
            CREATE TRIGGER users_touch_trigger BEFORE UPDATE ON __MADS_TEST_SCHEMA__.users
                FOR EACH ROW EXECUTE FUNCTION __MADS_TEST_SCHEMA__.touch_users();
            CREATE TABLE __MADS_TEST_SCHEMA__.parents (id integer PRIMARY KEY);
            CREATE TABLE __MADS_TEST_SCHEMA__.children (
                id bigint PRIMARY KEY,
                parent_id integer NOT NULL,
                CONSTRAINT children_parent_id_fkey FOREIGN KEY (parent_id)
                    REFERENCES __MADS_TEST_SCHEMA__.parents(id)
            );
        "#,
        schema_files(&[
            schema("users (id)", "id -> Int8,"),
            schema("parents (id)", "id -> Int8,"),
            schema("children (id)", "id -> Int8,\nparent_id -> Integer,"),
        ]),
        shape(&[]),
    )
}

fn schema(table_declaration: &str, columns: &str) -> String {
    format!(
        "diesel::table! {{\n    {SCHEMA_PLACEHOLDER}.{table_declaration} {{\n        {}\n    }}\n}}\n",
        columns.replace('\n', "\n        ")
    )
}

fn schema_files(sources: &[String]) -> BTreeMap<PathBuf, String> {
    BTreeMap::from([(PathBuf::from("src/schema.rs"), sources.join("\n"))])
}

fn shape(tables: &[TableShape]) -> SupportedShape {
    let mut tables = tables.to_vec();
    tables.sort_by(|left, right| left.name.cmp(&right.name));
    SupportedShape(tables)
}

fn table(name: &str, columns: &[ColumnShape], primary_key: &[&str]) -> TableShape {
    TableShape {
        name: name.into(),
        columns: columns.to_vec(),
        primary_key: primary_key.iter().map(|column| (*column).into()).collect(),
    }
}

fn column(name: &str, sql_type: &str, nullable: bool) -> ColumnShape {
    ColumnShape {
        name: name.into(),
        sql_type: sql_type.into(),
        nullable,
    }
}

fn substitute_schema(value: &str, schema_name: &str) -> String {
    value.replace(SCHEMA_PLACEHOLDER, schema_name)
}

fn unique_schema_name() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after UNIX epoch")
        .as_nanos();
    let sequence = SCHEMA_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("mads_rt_{}_{}_{}", std::process::id(), nanos, sequence)
}

fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn only_migration_directory(root: &Path) -> PathBuf {
    let entries = fs::read_dir(root.join("migrations"))
        .expect("migrations directory should exist")
        .collect::<Result<Vec<_>, _>>()
        .expect("migration entries should be readable");
    assert_eq!(
        entries.len(),
        1,
        "exactly one migration should be generated"
    );
    entries[0].path()
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "command failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_in_order(haystack: &str, fragments: &[String]) {
    let mut offset = 0;
    for fragment in fragments {
        let relative = haystack[offset..]
            .find(fragment)
            .unwrap_or_else(|| panic!("missing ordered SQL fragment `{fragment}` in:\n{haystack}"));
        offset += relative + fragment.len();
    }
}

fn assert_warning_comment(sql: &str, fragment: &str) {
    assert!(
        sql.lines()
            .any(|line| line.starts_with("-- WARNING: ") && line.contains(fragment)),
        "missing warning comment containing `{fragment}` in:\n{sql}"
    );
}
