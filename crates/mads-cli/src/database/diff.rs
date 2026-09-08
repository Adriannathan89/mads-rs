use std::collections::{BTreeMap, BTreeSet};

use super::{
    catalog::{ForeignKeyDependency, LiveSchema, UnsupportedKind, UnsupportedObject},
    schema::{ColumnSchema, DesiredSchema, PgType, QualifiedTableName, TableSchema},
};
use crate::diagnostic::{CliError, MADS212};

/// A supported, reversible schema-shape operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Operation {
    /// Creates a complete table declaration including its primary key.
    CreateTable(TableSchema),
    /// Drops a complete table declaration retained for the inverse operation.
    DropTable(TableSchema),
    /// Adds one column to an existing table.
    AddColumn {
        /// The table receiving the column.
        table: QualifiedTableName,
        /// The complete added column shape.
        column: ColumnSchema,
    },
    /// Drops one column while retaining its complete shape for the inverse.
    DropColumn {
        /// The table losing the column.
        table: QualifiedTableName,
        /// The complete dropped column shape.
        column: ColumnSchema,
    },
    /// Changes one column's supported PostgreSQL type.
    AlterType {
        /// The table containing the column.
        table: QualifiedTableName,
        /// The column name.
        column: String,
        /// The captured live type.
        from: PgType,
        /// The declared desired type.
        to: PgType,
    },
    /// Makes one existing column non-nullable.
    SetNotNull {
        /// The table containing the column.
        table: QualifiedTableName,
        /// The column name.
        column: String,
    },
    /// Makes one existing column nullable.
    DropNotNull {
        /// The table containing the column.
        table: QualifiedTableName,
        /// The column name.
        column: String,
    },
}

impl Operation {
    fn inverse(&self) -> Self {
        match self {
            Self::CreateTable(table) => Self::DropTable(table.clone()),
            Self::DropTable(table) => Self::CreateTable(table.clone()),
            Self::AddColumn { table, column } => Self::DropColumn {
                table: table.clone(),
                column: column.clone(),
            },
            Self::DropColumn { table, column } => Self::AddColumn {
                table: table.clone(),
                column: column.clone(),
            },
            Self::AlterType {
                table,
                column,
                from,
                to,
            } => Self::AlterType {
                table: table.clone(),
                column: column.clone(),
                from: to.clone(),
                to: from.clone(),
            },
            Self::SetNotNull { table, column } => Self::DropNotNull {
                table: table.clone(),
                column: column.clone(),
            },
            Self::DropNotNull { table, column } => Self::SetNotNull {
                table: table.clone(),
                column: column.clone(),
            },
        }
    }
}

/// A manual-review boundary encountered while planning a migration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MigrationWarning {
    /// The affected table, column, or named unsupported object.
    pub(crate) subject: String,
    /// The review action or risk that requires attention.
    pub(crate) message: String,
}

impl MigrationWarning {
    /// Converts this review boundary into one public MADS212 warning record.
    pub(crate) fn diagnostic(&self) -> crate::output::model::CliDiagnostic {
        crate::output::model::CliDiagnostic::warning(
            MADS212,
            "Migration requires review",
            &self.message,
        )
        .with_subject(&self.subject)
        .with_suggestion("review up.sql and down.sql before applying")
    }
}

/// A deterministic, reversible migration shape plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MigrationPlan {
    up: Vec<Operation>,
    down: Vec<Operation>,
    warnings: Vec<MigrationWarning>,
}

impl MigrationPlan {
    /// Returns supported operations in the order they must be applied.
    pub(crate) fn up(&self) -> &[Operation] {
        &self.up
    }

    /// Returns exact inverse operations in reverse application order.
    pub(crate) fn down(&self) -> &[Operation] {
        &self.down
    }

    /// Returns deterministic manual-review warnings.
    pub(crate) fn warnings(&self) -> &[MigrationWarning] {
        &self.warnings
    }

    /// Returns whether the supported schema shapes already match.
    pub(crate) fn is_empty(&self) -> bool {
        self.up.is_empty()
    }
}

/// Returns deterministic, review-safe warnings for public CLI output and SQL comments.
pub(crate) fn review_warnings(warnings: &[MigrationWarning]) -> Vec<MigrationWarning> {
    let mut normalized = warnings
        .iter()
        .map(|warning| MigrationWarning {
            subject: escape_warning_line_breaks(&warning.subject),
            message: escape_warning_line_breaks(&warning.message),
        })
        .collect::<Vec<_>>();
    normalized.sort_by(|left, right| {
        left.subject
            .cmp(&right.subject)
            .then_with(|| left.message.cmp(&right.message))
    });
    normalized.dedup();
    normalized
}

fn escape_warning_line_breaks(content: &str) -> String {
    content.replace('\r', "\\r").replace('\n', "\\n")
}

/// Plans the supported shape changes from a live PostgreSQL snapshot to a desired schema.
pub(crate) fn plan_diff(
    desired: &DesiredSchema,
    live: &LiveSchema,
) -> Result<MigrationPlan, CliError> {
    plan_tables_ref(
        live.tables(),
        live.unsupported(),
        live.foreign_keys(),
        desired.tables(),
    )
}

fn plan_tables_ref(
    live: &BTreeMap<QualifiedTableName, TableSchema>,
    unsupported: &[UnsupportedObject],
    foreign_keys: &[ForeignKeyDependency],
    desired: &BTreeMap<QualifiedTableName, TableSchema>,
) -> Result<MigrationPlan, CliError> {
    reject_primary_key_changes(live, desired)?;

    let dropped_tables = live
        .keys()
        .filter(|name| !desired.contains_key(*name))
        .cloned()
        .collect::<BTreeSet<_>>();
    reject_retained_foreign_key_references(&dropped_tables, foreign_keys)?;
    let dropped_table_order = order_dropped_tables(&dropped_tables, foreign_keys)?;

    let mut up = Vec::new();
    let mut affected = AffectedShape::default();
    let mut warnings = Vec::new();

    for (name, table) in desired {
        if !live.contains_key(name) {
            up.push(Operation::CreateTable(table.clone()));
        }
    }

    for (name, desired_table) in desired {
        let Some(live_table) = live.get(name) else {
            continue;
        };
        let live_columns = columns_by_name(live_table);
        let mut additions = desired_table
            .columns
            .iter()
            .filter(|column| !live_columns.contains_key(column.name.as_str()))
            .collect::<Vec<_>>();
        additions
            .sort_by(|left, right| (left.ordinal, &left.name).cmp(&(right.ordinal, &right.name)));
        for column in additions {
            up.push(Operation::AddColumn {
                table: name.clone(),
                column: column.clone(),
            });
            affected.mark_column(name, &column.name);
            if !column.nullable {
                warnings.push(warning(
                    column_subject(name, &column.name),
                    "adding a non-null column requires a manual backfill or data-safety review",
                ));
            }
        }
    }

    for (name, live_table) in live {
        let Some(desired_table) = desired.get(name) else {
            continue;
        };
        let desired_columns = columns_by_name(desired_table);
        let mut common = live_table
            .columns
            .iter()
            .filter_map(|live_column| {
                desired_columns
                    .get(live_column.name.as_str())
                    .map(|desired_column| (live_column, *desired_column))
            })
            .collect::<Vec<_>>();
        common.sort_by(|(left, _), (right, _)| {
            (left.ordinal, &left.name).cmp(&(right.ordinal, &right.name))
        });
        for (live_column, desired_column) in common {
            if live_column.sql_type != desired_column.sql_type {
                up.push(Operation::AlterType {
                    table: name.clone(),
                    column: live_column.name.clone(),
                    from: live_column.sql_type.clone(),
                    to: desired_column.sql_type.clone(),
                });
                affected.mark_column(name, &live_column.name);
                warnings.push(warning(
                    column_subject(name, &live_column.name),
                    "changing a column type may require a risky cast; review the generated SQL",
                ));
            }
        }
    }

    for (name, live_table) in live {
        let Some(desired_table) = desired.get(name) else {
            continue;
        };
        let desired_columns = columns_by_name(desired_table);
        let mut common = live_table
            .columns
            .iter()
            .filter_map(|live_column| {
                desired_columns
                    .get(live_column.name.as_str())
                    .map(|desired_column| (live_column, *desired_column))
            })
            .collect::<Vec<_>>();
        common.sort_by(|(left, _), (right, _)| {
            (left.ordinal, &left.name).cmp(&(right.ordinal, &right.name))
        });
        for (live_column, desired_column) in common {
            if live_column.nullable == desired_column.nullable {
                continue;
            }
            let operation = if desired_column.nullable {
                Operation::DropNotNull {
                    table: name.clone(),
                    column: live_column.name.clone(),
                }
            } else {
                Operation::SetNotNull {
                    table: name.clone(),
                    column: live_column.name.clone(),
                }
            };
            up.push(operation);
            affected.mark_column(name, &live_column.name);
        }
    }

    for (name, live_table) in live {
        let Some(desired_table) = desired.get(name) else {
            continue;
        };
        let desired_columns = columns_by_name(desired_table);
        let mut removals = live_table
            .columns
            .iter()
            .filter(|column| !desired_columns.contains_key(column.name.as_str()))
            .collect::<Vec<_>>();
        removals
            .sort_by(|left, right| (right.ordinal, &right.name).cmp(&(left.ordinal, &left.name)));
        for column in removals {
            up.push(Operation::DropColumn {
                table: name.clone(),
                column: column.clone(),
            });
            affected.mark_column(name, &column.name);
            warnings.push(warning(
                column_subject(name, &column.name),
                "dropping a column may discard data; down.sql restores shape, not data",
            ));
        }
    }

    for name in dropped_table_order {
        let table = live
            .get(&name)
            .expect("dropped table names are derived from the live snapshot");
        up.push(Operation::DropTable(table.clone()));
        affected.mark_table(&name);
        warnings.push(warning(
            table_subject(&name),
            "dropping a table may discard data; down.sql restores shape, not data",
        ));
    }

    warnings.extend(unsupported_warnings(unsupported, foreign_keys, &affected));
    warnings.sort_by(|left, right| {
        (&left.subject, &left.message).cmp(&(&right.subject, &right.message))
    });
    warnings.dedup();

    let down = up.iter().rev().map(Operation::inverse).collect();
    Ok(MigrationPlan { up, down, warnings })
}

fn columns_by_name(table: &TableSchema) -> BTreeMap<&str, &ColumnSchema> {
    table
        .columns
        .iter()
        .map(|column| (column.name.as_str(), column))
        .collect()
}

fn reject_primary_key_changes(
    live: &BTreeMap<QualifiedTableName, TableSchema>,
    desired: &BTreeMap<QualifiedTableName, TableSchema>,
) -> Result<(), CliError> {
    for (name, live_table) in live {
        let Some(desired_table) = desired.get(name) else {
            continue;
        };
        if live_table.primary_key != desired_table.primary_key {
            return Err(diff_error(
                "changing an existing table primary key is unsupported",
                table_subject(name),
            ));
        }
        let desired_columns = columns_by_name(desired_table);
        if let Some(primary_key_column) = live_table
            .primary_key
            .iter()
            .find(|column| !desired_columns.contains_key(column.as_str()))
        {
            return Err(diff_error(
                "dropping a primary-key column from a retained table is unsupported",
                column_subject(name, primary_key_column),
            ));
        }
    }
    Ok(())
}

fn reject_retained_foreign_key_references(
    dropped_tables: &BTreeSet<QualifiedTableName>,
    foreign_keys: &[ForeignKeyDependency],
) -> Result<(), CliError> {
    if let Some(dependency) = foreign_keys.iter().find(|dependency| {
        dropped_tables.contains(&dependency.parent) && !dropped_tables.contains(&dependency.child)
    }) {
        return Err(diff_error(
            "a retained table still references a table selected for drop",
            format!("{}.{}", dependency.child.schema, dependency.name),
        ));
    }
    Ok(())
}

fn order_dropped_tables(
    dropped_tables: &BTreeSet<QualifiedTableName>,
    foreign_keys: &[ForeignKeyDependency],
) -> Result<Vec<QualifiedTableName>, CliError> {
    let mut incoming = dropped_tables
        .iter()
        .cloned()
        .map(|name| (name, 0usize))
        .collect::<BTreeMap<_, _>>();
    let edges = foreign_keys
        .iter()
        .filter(|dependency| {
            dropped_tables.contains(&dependency.child)
                && dropped_tables.contains(&dependency.parent)
        })
        .map(|dependency| (dependency.child.clone(), dependency.parent.clone()))
        .collect::<BTreeSet<_>>();
    let mut outgoing = BTreeMap::<QualifiedTableName, Vec<QualifiedTableName>>::new();
    for (child, parent) in edges {
        outgoing.entry(child).or_default().push(parent.clone());
        *incoming
            .get_mut(&parent)
            .expect("dropped parent must be present") += 1;
    }

    let mut ready = incoming
        .iter()
        .filter_map(|(name, count)| (*count == 0).then_some(name.clone()))
        .collect::<BTreeSet<_>>();
    let mut ordered = Vec::with_capacity(dropped_tables.len());
    while let Some(next) = ready.pop_first() {
        if let Some(parents) = outgoing.get(&next) {
            for parent in parents {
                let count = incoming
                    .get_mut(parent)
                    .expect("foreign-key parent must be present");
                *count -= 1;
                if *count == 0 {
                    ready.insert(parent.clone());
                }
            }
        }
        ordered.push(next);
    }
    if ordered.len() != dropped_tables.len() {
        return Err(diff_error(
            "foreign-key cycle among dropped tables is unsupported",
            "dropped tables",
        ));
    }
    Ok(ordered)
}

#[derive(Default)]
struct AffectedShape {
    tables: BTreeSet<QualifiedTableName>,
    columns: BTreeMap<QualifiedTableName, BTreeSet<String>>,
}

impl AffectedShape {
    fn mark_table(&mut self, table: &QualifiedTableName) {
        self.tables.insert(table.clone());
    }

    fn mark_column(&mut self, table: &QualifiedTableName, column: &str) {
        self.columns
            .entry(table.clone())
            .or_default()
            .insert(column.to_owned());
    }

    fn touches(&self, object: &UnsupportedObject) -> bool {
        self.tables.contains(&object.table)
            || (object.columns.is_empty() && self.columns.contains_key(&object.table))
            || self
                .columns
                .get(&object.table)
                .is_some_and(|columns| object.columns.iter().any(|column| columns.contains(column)))
    }

    fn touches_table_or_column(&self, table: &QualifiedTableName) -> bool {
        self.tables.contains(table) || self.columns.contains_key(table)
    }
}

fn unsupported_warnings(
    unsupported: &[UnsupportedObject],
    foreign_keys: &[ForeignKeyDependency],
    affected: &AffectedShape,
) -> Vec<MigrationWarning> {
    let mut objects = unsupported
        .iter()
        .filter(|object| affected.touches(object))
        .collect::<Vec<_>>();
    objects.sort_by(|left, right| {
        (&left.table, &left.kind, &left.name).cmp(&(&right.table, &right.kind, &right.name))
    });
    let mut warnings = objects
        .into_iter()
        .map(|object| {
            warning(
                format!(
                    "{}.{}.{}",
                    object.table.schema, object.table.table, object.name
                ),
                format!(
                    "unsupported {} is affected and will not be synthesized; review it manually",
                    unsupported_kind_name(&object.kind)
                ),
            )
        })
        .collect::<Vec<_>>();
    warnings.extend(
        foreign_keys
            .iter()
            .filter(|dependency| affected.touches_table_or_column(&dependency.parent))
            .map(|dependency| {
                warning(
                    format!(
                        "{}.{}.{}",
                        dependency.child.schema, dependency.child.table, dependency.name
                    ),
                    "unsupported foreign key is affected and will not be synthesized; review it manually",
                )
            }),
    );
    warnings
}

fn unsupported_kind_name(kind: &UnsupportedKind) -> &'static str {
    match kind {
        UnsupportedKind::Default => "default",
        UnsupportedKind::Index => "index",
        UnsupportedKind::Check => "check constraint",
        UnsupportedKind::Trigger => "trigger",
        UnsupportedKind::ForeignKey => "foreign key",
    }
}

fn table_subject(table: &QualifiedTableName) -> String {
    format!("{}.{}", table.schema, table.table)
}

fn column_subject(table: &QualifiedTableName, column: &str) -> String {
    format!("{}.{}.{}", table.schema, table.table, column)
}

fn warning(subject: String, message: impl Into<String>) -> MigrationWarning {
    MigrationWarning {
        subject,
        message: message.into(),
    }
}

fn diff_error(message: impl Into<String>, subject: impl Into<String>) -> CliError {
    CliError::new(
        MADS212,
        "Schema difference is unsafe or unsupported",
        message,
    )
    .with_subject(subject)
    .with_suggestion("adjust the schema manually before generating a migration")
}

#[cfg(test)]
fn plan_tables(
    live: BTreeMap<QualifiedTableName, TableSchema>,
    unsupported: Vec<UnsupportedObject>,
    foreign_keys: Vec<ForeignKeyDependency>,
    desired: BTreeMap<QualifiedTableName, TableSchema>,
) -> Result<MigrationPlan, CliError> {
    plan_tables_ref(&live, &unsupported, &foreign_keys, &desired)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{Operation, plan_tables};
    use crate::{
        database::{
            catalog::{ForeignKeyDependency, UnsupportedKind, UnsupportedObject},
            schema::{ColumnSchema, PgType, QualifiedTableName, TableSchema},
        },
        diagnostic::MADS212,
    };

    #[test]
    fn adding_a_column_has_a_drop_inverse() {
        let plan = plan_tables(
            tables([table("users", [("id", PgType::BigInt, false)])]),
            vec![],
            vec![],
            tables([table(
                "users",
                [
                    ("id", PgType::BigInt, false),
                    ("nickname", PgType::Text, true),
                ],
            )]),
        )
        .expect("column addition should plan");

        assert_eq!(
            plan.up(),
            &[Operation::AddColumn {
                table: name("users"),
                column: column("nickname", PgType::Text, true, 1),
            }]
        );
        assert_eq!(
            plan.down(),
            &[Operation::DropColumn {
                table: name("users"),
                column: column("nickname", PgType::Text, true, 1),
            }]
        );
    }

    #[test]
    fn create_table_has_a_drop_inverse_and_keeps_composite_primary_key() {
        let memberships = table_with_key(
            "memberships",
            [
                ("user_id", PgType::BigInt, false),
                ("team_id", PgType::BigInt, false),
            ],
            ["user_id", "team_id"],
        );
        let plan = plan_tables(
            BTreeMap::new(),
            vec![],
            vec![],
            tables([memberships.clone()]),
        )
        .expect("table creation should plan");

        assert_eq!(plan.up(), &[Operation::CreateTable(memberships.clone())]);
        assert_eq!(plan.down(), &[Operation::DropTable(memberships)]);
    }

    #[test]
    fn dropping_a_table_has_a_create_inverse_and_keeps_composite_primary_key() {
        let memberships = table_with_key(
            "memberships",
            [
                ("user_id", PgType::BigInt, false),
                ("team_id", PgType::BigInt, false),
            ],
            ["user_id", "team_id"],
        );
        let plan = plan_tables(
            tables([memberships.clone()]),
            vec![],
            vec![],
            BTreeMap::new(),
        )
        .expect("table drop should plan");

        assert_eq!(plan.up(), &[Operation::DropTable(memberships.clone())]);
        assert_eq!(plan.down(), &[Operation::CreateTable(memberships)]);
        assert_eq!(plan.warnings()[0].subject, "public.memberships");
    }

    #[test]
    fn dropping_a_column_has_an_add_inverse() {
        let obsolete = column("obsolete", PgType::Text, true, 1);
        let plan = plan_tables(
            tables([table(
                "users",
                [
                    ("id", PgType::BigInt, false),
                    ("obsolete", PgType::Text, true),
                ],
            )]),
            vec![],
            vec![],
            tables([table("users", [("id", PgType::BigInt, false)])]),
        )
        .expect("column drop should plan");

        assert_eq!(
            plan.up(),
            &[Operation::DropColumn {
                table: name("users"),
                column: obsolete.clone(),
            }]
        );
        assert_eq!(
            plan.down(),
            &[Operation::AddColumn {
                table: name("users"),
                column: obsolete,
            }]
        );
    }

    #[test]
    fn altering_a_type_has_an_exact_reverse() {
        let plan = plan_tables(
            tables([table("users", [("id", PgType::Integer, false)])]),
            vec![],
            vec![],
            tables([table("users", [("id", PgType::BigInt, false)])]),
        )
        .expect("type change should plan");

        assert_eq!(
            plan.up(),
            &[Operation::AlterType {
                table: name("users"),
                column: "id".into(),
                from: PgType::Integer,
                to: PgType::BigInt,
            }]
        );
        assert_eq!(
            plan.down(),
            &[Operation::AlterType {
                table: name("users"),
                column: "id".into(),
                from: PgType::BigInt,
                to: PgType::Integer,
            }]
        );
    }

    #[test]
    fn setting_not_null_has_a_drop_not_null_inverse() {
        let plan = plan_tables(
            tables([table("users", [("id", PgType::BigInt, true)])]),
            vec![],
            vec![],
            tables([table("users", [("id", PgType::BigInt, false)])]),
        )
        .expect("not-null change should plan");

        assert_eq!(
            plan.up(),
            &[Operation::SetNotNull {
                table: name("users"),
                column: "id".into(),
            }]
        );
        assert_eq!(
            plan.down(),
            &[Operation::DropNotNull {
                table: name("users"),
                column: "id".into(),
            }]
        );
    }

    #[test]
    fn dropping_not_null_has_a_set_not_null_inverse() {
        let plan = plan_tables(
            tables([table("users", [("id", PgType::BigInt, false)])]),
            vec![],
            vec![],
            tables([table("users", [("id", PgType::BigInt, true)])]),
        )
        .expect("nullable change should plan");

        assert_eq!(
            plan.up(),
            &[Operation::DropNotNull {
                table: name("users"),
                column: "id".into(),
            }]
        );
        assert_eq!(
            plan.down(),
            &[Operation::SetNotNull {
                table: name("users"),
                column: "id".into(),
            }]
        );
    }

    #[test]
    fn no_diff_is_empty() {
        let users = table("users", [("id", PgType::BigInt, false)]);
        let plan = plan_tables(tables([users.clone()]), vec![], vec![], tables([users]))
            .expect("matching schemas should plan");
        assert!(plan.is_empty());
        assert!(plan.up().is_empty());
        assert!(plan.down().is_empty());
        assert!(plan.warnings().is_empty());
    }

    #[test]
    fn operations_follow_the_documented_deterministic_order() {
        let plan = plan_tables(
            tables([
                table(
                    "users",
                    [
                        ("id", PgType::Integer, true),
                        ("obsolete", PgType::Text, true),
                    ],
                ),
                table("zombies", [("id", PgType::BigInt, false)]),
            ]),
            vec![],
            vec![],
            tables([
                table("accounts", [("id", PgType::BigInt, false)]),
                table(
                    "users",
                    [
                        ("id", PgType::BigInt, false),
                        ("email", PgType::Text, false),
                    ],
                ),
            ]),
        )
        .expect("mixed operations should plan");

        assert!(matches!(plan.up()[0], Operation::CreateTable(_)));
        assert!(matches!(plan.up()[1], Operation::AddColumn { .. }));
        assert!(matches!(plan.up()[2], Operation::AlterType { .. }));
        assert!(matches!(plan.up()[3], Operation::SetNotNull { .. }));
        assert!(matches!(plan.up()[4], Operation::DropColumn { .. }));
        assert!(matches!(plan.up()[5], Operation::DropTable(_)));
        assert_eq!(plan.down().len(), plan.up().len());
    }

    #[test]
    fn changing_an_existing_primary_key_is_a_hard_mads212_error() {
        let live = table_with_key(
            "users",
            [
                ("id", PgType::BigInt, false),
                ("email", PgType::Text, false),
            ],
            ["id"],
        );
        let desired = table_with_key(
            "users",
            [
                ("id", PgType::BigInt, false),
                ("email", PgType::Text, false),
            ],
            ["email"],
        );
        let error = plan_tables(tables([live]), vec![], vec![], tables([desired]))
            .expect_err("existing primary key changes are unsafe");
        assert_eq!(error.code(), MADS212);
    }

    #[test]
    fn dropping_a_primary_key_column_on_a_retained_table_is_a_hard_mads212_error() {
        let error = plan_tables(
            tables([table(
                "users",
                [("id", PgType::BigInt, false), ("name", PgType::Text, true)],
            )]),
            vec![],
            vec![],
            tables([table("users", [("name", PgType::Text, true)])]),
        )
        .expect_err("primary key column drops are unsafe");
        assert_eq!(error.code(), MADS212);
    }

    #[test]
    fn retained_child_foreign_key_blocks_parent_drop() {
        let error = plan_tables(
            tables([
                table("parents", [("id", PgType::BigInt, false)]),
                table("children", [("id", PgType::BigInt, false)]),
            ]),
            vec![],
            vec![foreign_key("children", "parents")],
            tables([table("children", [("id", PgType::BigInt, false)])]),
        )
        .expect_err("retained child cannot reference a dropped parent");
        assert_eq!(error.code(), MADS212);
    }

    #[test]
    fn parent_type_change_warns_for_each_retained_child_foreign_key() {
        let plan = plan_tables(
            tables([
                table("parents", [("id", PgType::Integer, false)]),
                table(
                    "children",
                    [
                        ("id", PgType::BigInt, false),
                        ("parent_id", PgType::Integer, false),
                    ],
                ),
            ]),
            vec![UnsupportedObject {
                kind: UnsupportedKind::ForeignKey,
                table: name("children"),
                name: "children_parent_id_fkey".into(),
                columns: vec!["parent_id".into()],
            }],
            vec![ForeignKeyDependency {
                name: "children_parent_id_fkey".into(),
                child: name("children"),
                parent: name("parents"),
            }],
            tables([
                table("parents", [("id", PgType::BigInt, false)]),
                table(
                    "children",
                    [
                        ("id", PgType::BigInt, false),
                        ("parent_id", PgType::Integer, false),
                    ],
                ),
            ]),
        )
        .expect("parent type change should plan with a foreign-key warning");

        assert_eq!(
            plan.warnings(),
            &[
                super::MigrationWarning {
                    subject: "public.children.children_parent_id_fkey".into(),
                    message: "unsupported foreign key is affected and will not be synthesized; review it manually".into(),
                },
                super::MigrationWarning {
                    subject: "public.parents.id".into(),
                    message: "changing a column type may require a risky cast; review the generated SQL".into(),
                },
            ]
        );
    }

    #[test]
    fn dropped_foreign_key_children_precede_parents_and_down_recreates_parents_first() {
        let parents = table("parents", [("id", PgType::BigInt, false)]);
        let children = table("children", [("id", PgType::BigInt, false)]);
        let plan = plan_tables(
            tables([parents.clone(), children.clone()]),
            vec![],
            vec![foreign_key("children", "parents")],
            BTreeMap::new(),
        )
        .expect("dependent tables can be dropped in a safe order");

        assert_eq!(
            plan.up(),
            &[
                Operation::DropTable(children),
                Operation::DropTable(parents)
            ]
        );
        assert!(matches!(
            plan.down(),
            [Operation::CreateTable(_), Operation::CreateTable(_)]
        ));
        assert_eq!(
            plan.down()
                .iter()
                .map(|operation| match operation {
                    Operation::CreateTable(table) => table.name.table.as_str(),
                    _ => unreachable!("only create operations are expected"),
                })
                .collect::<Vec<_>>(),
            ["parents", "children"]
        );
    }

    #[test]
    fn multiple_foreign_keys_between_the_same_dropped_tables_do_not_form_a_cycle() {
        let plan = plan_tables(
            tables([
                table("parents", [("id", PgType::BigInt, false)]),
                table("children", [("id", PgType::BigInt, false)]),
            ]),
            vec![],
            vec![
                foreign_key("children", "parents"),
                ForeignKeyDependency {
                    name: "children_parents_second_fk".into(),
                    child: name("children"),
                    parent: name("parents"),
                },
            ],
            BTreeMap::new(),
        )
        .expect("parallel foreign keys do not make a dependency cycle");

        assert_eq!(
            plan.up()
                .iter()
                .map(|operation| match operation {
                    Operation::DropTable(table) => table.name.table.as_str(),
                    _ => unreachable!("only table drops are expected"),
                })
                .collect::<Vec<_>>(),
            ["children", "parents"]
        );
    }

    #[test]
    fn foreign_key_cycles_among_dropped_tables_are_a_hard_mads212_error() {
        let error = plan_tables(
            tables([
                table("accounts", [("id", PgType::BigInt, false)]),
                table("users", [("id", PgType::BigInt, false)]),
            ]),
            vec![],
            vec![
                foreign_key("accounts", "users"),
                foreign_key("users", "accounts"),
            ],
            BTreeMap::new(),
        )
        .expect_err("cyclic foreign-key drops are unsupported");

        assert_eq!(error.code(), MADS212);
    }

    #[test]
    fn dropped_data_bearing_shape_warns_deterministically() {
        let plan = plan_tables(
            tables([table(
                "users",
                [
                    ("id", PgType::BigInt, false),
                    ("obsolete", PgType::Text, true),
                ],
            )]),
            vec![],
            vec![],
            tables([table("users", [("id", PgType::BigInt, false)])]),
        )
        .expect("drop should plan with warning");
        assert!(
            plan.warnings()
                .iter()
                .any(|warning| warning.subject == "public.users.obsolete")
        );
    }

    #[test]
    fn review_warning_becomes_one_structured_mads212_diagnostic() {
        let warning = super::warning(
            "public.users.email".into(),
            "changing a column type may require a risky cast; review the generated SQL",
        );

        assert_eq!(
            serde_json::to_string(&warning.diagnostic())
                .expect("review warning diagnostic should serialize"),
            "{\"severity\":\"warning\",\"code\":\"MADS212\",\"title\":\"Migration requires review\",\"message\":\"changing a column type may require a risky cast; review the generated SQL\",\"subject\":\"public.users.email\",\"location\":null,\"suggestions\":[\"review up.sql and down.sql before applying\"]}"
        );
    }

    #[test]
    fn type_changes_and_added_not_null_columns_warn() {
        let plan = plan_tables(
            tables([table("users", [("id", PgType::Integer, false)])]),
            vec![],
            vec![],
            tables([table(
                "users",
                [
                    ("id", PgType::BigInt, false),
                    ("email", PgType::Text, false),
                ],
            )]),
        )
        .expect("changes should plan with warnings");
        let subjects = plan
            .warnings()
            .iter()
            .map(|warning| warning.subject.as_str())
            .collect::<Vec<_>>();
        assert_eq!(subjects, ["public.users.email", "public.users.id"]);
    }

    #[test]
    fn affected_unsupported_objects_get_named_warnings_only_when_touched() {
        let unsupported = UnsupportedObject {
            kind: UnsupportedKind::Check,
            table: name("users"),
            name: "users_email_check".into(),
            columns: vec!["email".into()],
        };
        let untouched = UnsupportedObject {
            kind: UnsupportedKind::Index,
            table: name("users"),
            name: "users_name_idx".into(),
            columns: vec!["name".into()],
        };
        let plan = plan_tables(
            tables([table(
                "users",
                [
                    ("id", PgType::BigInt, false),
                    ("email", PgType::Text, true),
                    ("name", PgType::Text, true),
                ],
            )]),
            vec![unsupported, untouched],
            vec![],
            tables([table(
                "users",
                [
                    ("id", PgType::BigInt, false),
                    ("email", PgType::Text, false),
                    ("name", PgType::Text, true),
                ],
            )]),
        )
        .expect("shape change should plan");
        assert!(
            plan.warnings()
                .iter()
                .any(|warning| warning.subject == "public.users.users_email_check")
        );
        assert!(
            !plan
                .warnings()
                .iter()
                .any(|warning| warning.subject == "public.users.users_name_idx")
        );
    }

    fn name(table: &str) -> QualifiedTableName {
        QualifiedTableName::new("public", table)
    }

    fn column(name: &str, sql_type: PgType, nullable: bool, ordinal: usize) -> ColumnSchema {
        ColumnSchema {
            name: name.into(),
            sql_type,
            nullable,
            ordinal,
        }
    }

    fn table<const N: usize>(table: &str, columns: [(&str, PgType, bool); N]) -> TableSchema {
        table_with_key(table, columns, ["id"])
    }

    fn table_with_key<const N: usize, const K: usize>(
        table: &str,
        columns: [(&str, PgType, bool); N],
        primary_key: [&str; K],
    ) -> TableSchema {
        TableSchema {
            name: name(table),
            columns: columns
                .into_iter()
                .enumerate()
                .map(|(ordinal, (column_name, sql_type, nullable))| {
                    column(column_name, sql_type, nullable, ordinal)
                })
                .collect(),
            primary_key: primary_key.into_iter().map(str::to_owned).collect(),
        }
    }

    fn tables<const N: usize>(
        tables: [TableSchema; N],
    ) -> BTreeMap<QualifiedTableName, TableSchema> {
        tables
            .into_iter()
            .map(|table| (table.name.clone(), table))
            .collect()
    }

    fn foreign_key(child: &str, parent: &str) -> ForeignKeyDependency {
        ForeignKeyDependency {
            name: format!("{child}_{parent}_fk"),
            child: name(child),
            parent: name(parent),
        }
    }
}
