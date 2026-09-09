use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write,
};

use mads_common::__private::{
    DependencyReport, DiagnosticReport, DoctorCheck, DoctorStatus, InspectionReport,
    ModuleImportReport, ModuleReport, ProviderReport, RouteReport,
};

pub(crate) fn render_routes(report: &InspectionReport) -> String {
    let routes = ordered_routes(report);

    let mut output =
        String::from("METHOD  PATH        ROUTE                    CONTROLLER       GUARD  SOURCE");
    if routes.is_empty() {
        output.push_str("\n(none)");
        return output;
    }

    for route in routes {
        let route_name = format!("{}::{}", route.route_trait, route.handler);
        let guard = if route.guard_active { "yes" } else { "no" };
        write!(
            output,
            "\n{:<7} {:<10} {:<24} {:<16} {:<6} {}:{}:{}",
            route.method,
            route.path,
            route_name,
            route.controller,
            guard,
            route.location.file,
            route.location.line,
            route.location.column
        )
        .expect("writing to a String cannot fail");
    }
    output
}

pub(crate) fn render_graph(report: &InspectionReport) -> String {
    let mut output = String::from("Modules\n");
    render_modules(report, &mut output);

    output.push_str("\n\nProviders\n");
    let providers = ordered_providers(report);
    if providers.is_empty() {
        output.push_str("(none)");
    } else {
        let type_width = providers
            .iter()
            .map(|provider| provider.type_name.len())
            .max()
            .unwrap_or(0);
        for (index, provider) in providers.into_iter().enumerate() {
            if index > 0 {
                output.push('\n');
            }
            let owner = provider.owner.as_deref().unwrap_or("(none)");
            write!(
                output,
                "{:<type_width$}  owner={} origin={} visibility={} state={}",
                provider.type_name, owner, provider.origin, provider.visibility, provider.state
            )
            .expect("writing to a String cannot fail");
        }
    }

    output.push_str("\n\nDependencies\n");
    let dependencies = ordered_dependencies(report);
    if dependencies.is_empty() {
        output.push_str("(none)");
    } else {
        for (index, dependency) in dependencies.into_iter().enumerate() {
            if index > 0 {
                output.push('\n');
            }
            write!(
                output,
                "{} -> {}",
                dependency.provider, dependency.dependency
            )
            .expect("writing to a String cannot fail");
        }
    }

    output.push_str("\n\nConstruction order\n");
    match &report.graph.construction_order {
        Some(order) if !order.is_empty() => {
            for (index, provider) in order.iter().enumerate() {
                if index > 0 {
                    output.push('\n');
                }
                write!(output, "{}. {provider}", index + 1)
                    .expect("writing to a String cannot fail");
            }
        }
        _ => output.push_str("(none)"),
    }
    output
}

fn render_modules(report: &InspectionReport, output: &mut String) {
    let Some(root) = report.graph.root_module.as_deref() else {
        let mut modules = report
            .graph
            .modules
            .iter()
            .map(|module| module.type_name.as_str())
            .collect::<Vec<_>>();
        modules.sort_unstable();
        modules.dedup();
        if modules.is_empty() {
            output.push_str("(none)");
        } else {
            output.push_str(&modules.join("\n"));
        }
        return;
    };

    output.push_str(root);
    let mut imports = BTreeMap::<&str, Vec<&str>>::new();
    for ModuleImportReport { importer, imported } in &report.graph.imports {
        imports.entry(importer).or_default().push(imported);
    }
    for children in imports.values_mut() {
        children.sort_unstable();
        children.dedup();
    }
    let mut visited = BTreeSet::from([root]);
    render_imports(root, "", &imports, &mut visited, output);

    let mut remaining = report
        .graph
        .modules
        .iter()
        .map(|module| module.type_name.as_str())
        .filter(|module| !visited.contains(module))
        .collect::<Vec<_>>();
    remaining.sort_unstable();
    remaining.dedup();
    for module in remaining {
        write!(output, "\n{module}").expect("writing to a String cannot fail");
    }
}

fn render_imports<'a>(
    module: &'a str,
    prefix: &str,
    imports: &BTreeMap<&'a str, Vec<&'a str>>,
    visited: &mut BTreeSet<&'a str>,
    output: &mut String,
) {
    let Some(children) = imports.get(module) else {
        return;
    };
    for (index, child) in children.iter().enumerate() {
        let last = index + 1 == children.len();
        let connector = if last { "└──" } else { "├──" };
        write!(output, "\n{prefix}{connector} {child}").expect("writing to a String cannot fail");
        if visited.insert(child) {
            let child_prefix = format!("{prefix}{}", if last { "    " } else { "│   " });
            render_imports(child, &child_prefix, imports, visited, output);
        }
    }
}

pub(crate) fn render_doctor(report: &InspectionReport) -> String {
    let checks = ordered_checks(report);
    if checks.is_empty() {
        return "(none)".into();
    }

    checks
        .into_iter()
        .map(|check| {
            if check.group == "auto-configuration" {
                format!(
                    "{:<11} {} {}",
                    doctor_status(check.status),
                    check.group,
                    check.summary
                )
            } else {
                format!(
                    "{:<11} {:<19} {}",
                    doctor_status(check.status),
                    check.group,
                    check.summary
                )
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn doctor_group_rank(group: &str) -> u8 {
    match group {
        "configuration" => 0,
        "module graph" => 1,
        "providers" => 2,
        "routes" => 3,
        "guards/strategies" => 4,
        "server/CORS" => 5,
        "auto-configuration" => 6,
        _ => 7,
    }
}

const fn doctor_status_rank(status: DoctorStatus) -> u8 {
    match status {
        DoctorStatus::Pass => 0,
        DoctorStatus::Skipped => 1,
        DoctorStatus::Overridden => 2,
        DoctorStatus::Failed => 3,
    }
}

const fn doctor_status(status: DoctorStatus) -> &'static str {
    match status {
        DoctorStatus::Pass => "PASS",
        DoctorStatus::Skipped => "SKIPPED",
        DoctorStatus::Overridden => "OVERRIDDEN",
        DoctorStatus::Failed => "FAILED",
    }
}

pub(crate) fn render_diagnostics(report: &InspectionReport) -> String {
    let diagnostics = ordered_diagnostics(report);
    diagnostics
        .into_iter()
        .map(render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub(crate) fn ordered_routes(report: &InspectionReport) -> Vec<&RouteReport> {
    let mut routes = report.routes.iter().collect::<Vec<_>>();
    routes.sort_by(|left, right| {
        left.method
            .cmp(&right.method)
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.controller.cmp(&right.controller))
            .then_with(|| left.route_trait.cmp(&right.route_trait))
            .then_with(|| left.handler.cmp(&right.handler))
    });
    routes
}

pub(crate) fn ordered_modules(report: &InspectionReport) -> Vec<&ModuleReport> {
    let mut modules = report.graph.modules.iter().collect::<Vec<_>>();
    modules.sort_by(|left, right| {
        left.type_name
            .cmp(&right.type_name)
            .then_with(|| left.namespace.cmp(&right.namespace))
            .then_with(|| left.location.file.cmp(&right.location.file))
            .then_with(|| left.location.line.cmp(&right.location.line))
            .then_with(|| left.location.column.cmp(&right.location.column))
    });
    modules
}

pub(crate) fn ordered_imports(report: &InspectionReport) -> Vec<&ModuleImportReport> {
    let mut imports = report.graph.imports.iter().collect::<Vec<_>>();
    imports.sort_by(|left, right| {
        left.importer
            .cmp(&right.importer)
            .then_with(|| left.imported.cmp(&right.imported))
    });
    imports
}

pub(crate) fn ordered_providers(report: &InspectionReport) -> Vec<&ProviderReport> {
    let mut providers = report.graph.providers.iter().collect::<Vec<_>>();
    providers.sort_by(|left, right| {
        left.type_name
            .cmp(&right.type_name)
            .then_with(|| left.owner.cmp(&right.owner))
            .then_with(|| left.origin.cmp(&right.origin))
            .then_with(|| left.visibility.cmp(&right.visibility))
            .then_with(|| left.state.cmp(&right.state))
            .then_with(|| provider_location(left).cmp(&provider_location(right)))
    });
    providers
}

pub(crate) fn ordered_dependencies(report: &InspectionReport) -> Vec<&DependencyReport> {
    let mut dependencies = report.graph.dependencies.iter().collect::<Vec<_>>();
    dependencies.sort_by(|left, right| {
        left.provider
            .cmp(&right.provider)
            .then_with(|| left.dependency.cmp(&right.dependency))
    });
    dependencies
}

pub(crate) fn ordered_checks(report: &InspectionReport) -> Vec<&DoctorCheck> {
    let mut checks = report.checks.iter().collect::<Vec<_>>();
    checks.sort_by(|left, right| {
        doctor_group_rank(&left.group)
            .cmp(&doctor_group_rank(&right.group))
            .then_with(|| left.summary.cmp(&right.summary))
            .then_with(|| doctor_status_rank(left.status).cmp(&doctor_status_rank(right.status)))
    });
    checks
}

pub(crate) fn ordered_diagnostics(report: &InspectionReport) -> Vec<&DiagnosticReport> {
    let mut diagnostics = report.diagnostics.iter().collect::<Vec<_>>();
    diagnostics.sort_by(|left, right| {
        left.code
            .cmp(&right.code)
            .then_with(|| diagnostic_location(left).cmp(&diagnostic_location(right)))
            .then_with(|| left.subject.cmp(&right.subject))
            .then_with(|| left.title.cmp(&right.title))
            .then_with(|| left.message.cmp(&right.message))
            .then_with(|| left.suggestions.cmp(&right.suggestions))
    });
    diagnostics
}

fn provider_location(provider: &ProviderReport) -> Option<(&str, u32, u32)> {
    provider
        .location
        .as_ref()
        .map(|location| (location.file.as_str(), location.line, location.column))
}

fn diagnostic_location(diagnostic: &DiagnosticReport) -> Option<(&str, u32, u32)> {
    diagnostic
        .location
        .as_ref()
        .map(|location| (location.file.as_str(), location.line, location.column))
}

pub(crate) fn render_diagnostic(diagnostic: &DiagnosticReport) -> String {
    let mut output = format!("error[{}]: {}", diagnostic.code, diagnostic.title);
    if let Some(location) = &diagnostic.location {
        write!(
            output,
            "\n  --> {}:{}:{}",
            location.file, location.line, location.column
        )
        .expect("writing to a String cannot fail");
    }
    if let Some(subject) = &diagnostic.subject {
        write!(output, "\n  = subject: {subject}").expect("writing to a String cannot fail");
    }
    write!(output, "\n  = {}", diagnostic.message).expect("writing to a String cannot fail");
    for suggestion in &diagnostic.suggestions {
        write!(output, "\n  help: {suggestion}").expect("writing to a String cannot fail");
    }
    output
}

#[cfg(test)]
mod tests {
    use mads_common::__private::{
        DependencyReport, DiagnosticReport, DoctorCheck, GraphReport, InspectionKind,
        InspectionReport, ModuleImportReport, ModuleReport, ProviderReport, RouteReport,
        SourceReport,
    };

    use super::{render_diagnostics, render_doctor, render_graph, render_routes};

    fn report() -> InspectionReport {
        InspectionReport {
            kind: InspectionKind::Doctor,
            graph: GraphReport {
                root_module: Some("AppModule".into()),
                modules: vec![
                    ModuleReport {
                        type_name: "UserModule".into(),
                        namespace: "user".into(),
                        location: SourceReport {
                            file: "src/user.rs".into(),
                            line: 1,
                            column: 1,
                        },
                    },
                    ModuleReport {
                        type_name: "AppModule".into(),
                        namespace: "app".into(),
                        location: SourceReport {
                            file: "src/main.rs".into(),
                            line: 1,
                            column: 1,
                        },
                    },
                ],
                imports: vec![ModuleImportReport {
                    importer: "AppModule".into(),
                    imported: "UserModule".into(),
                }],
                providers: vec![
                    ProviderReport {
                        type_name: "UserService".into(),
                        owner: Some("UserModule".into()),
                        origin: "service".into(),
                        visibility: "public".into(),
                        state: "planned".into(),
                        location: None,
                    },
                    ProviderReport {
                        type_name: "UserRepository".into(),
                        owner: Some("UserModule".into()),
                        origin: "repository".into(),
                        visibility: "public".into(),
                        state: "planned".into(),
                        location: None,
                    },
                ],
                dependencies: vec![DependencyReport {
                    provider: "UserService".into(),
                    dependency: "UserRepository".into(),
                }],
                construction_order: Some(vec!["UserRepository".into(), "UserService".into()]),
                auto_configurations: Vec::new(),
            },
            routes: vec![
                RouteReport {
                    method: "POST".into(),
                    path: "/users".into(),
                    route_trait: "UserRoutes".into(),
                    handler: "create_user".into(),
                    controller: "UserController".into(),
                    location: SourceReport {
                        file: "src/user.rs".into(),
                        line: 18,
                        column: 5,
                    },
                    guard_active: false,
                },
                RouteReport {
                    method: "GET".into(),
                    path: "/users/:id".into(),
                    route_trait: "UserRoutes".into(),
                    handler: "get_user".into(),
                    controller: "UserController".into(),
                    location: SourceReport {
                        file: "src/user.rs".into(),
                        line: 12,
                        column: 5,
                    },
                    guard_active: true,
                },
            ],
            checks: vec![
                DoctorCheck::overridden(
                    "auto-configuration",
                    "application provider overrides default",
                ),
                DoctorCheck::pass("server/CORS", "automatic server configuration is valid"),
                DoctorCheck::skipped("guards/strategies", "JWT feature is not enabled"),
                DoctorCheck::pass("routes", "2 valid routes"),
                DoctorCheck::pass("providers", "2 selected providers"),
                DoctorCheck::pass("module graph", "2 reachable modules"),
                DoctorCheck::pass("configuration", "sources loaded"),
            ],
            diagnostics: vec![
                DiagnosticReport {
                    code: "MADS003".into(),
                    title: "missing provider".into(),
                    message: "UserService needs UserRepository".into(),
                    subject: Some("UserRepository".into()),
                    location: Some(SourceReport {
                        file: "src/user.rs".into(),
                        line: 12,
                        column: 5,
                    }),
                    suggestions: vec!["import UserModule".into(), "register UserRepository".into()],
                },
                DiagnosticReport {
                    code: "MADS001".into(),
                    title: "duplicate provider".into(),
                    message: "UserRepository is registered twice".into(),
                    subject: None,
                    location: None,
                    suggestions: Vec::new(),
                },
            ],
            failed: true,
        }
    }

    #[test]
    fn renders_routes_in_stable_order() {
        assert_eq!(
            render_routes(&report()),
            "METHOD  PATH        ROUTE                    CONTROLLER       GUARD  SOURCE\nGET     /users/:id UserRoutes::get_user     UserController   yes    src/user.rs:12:5\nPOST    /users     UserRoutes::create_user  UserController   no     src/user.rs:18:5"
        );
    }

    #[test]
    fn renders_graph_sections_in_stable_order() {
        assert_eq!(
            render_graph(&report()),
            "Modules\nAppModule\n└── UserModule\n\nProviders\nUserRepository  owner=UserModule origin=repository visibility=public state=planned\nUserService     owner=UserModule origin=service visibility=public state=planned\n\nDependencies\nUserService -> UserRepository\n\nConstruction order\n1. UserRepository\n2. UserService"
        );
    }

    #[test]
    fn renders_doctor_groups_in_fixed_order() {
        assert_eq!(
            render_doctor(&report()),
            "PASS        configuration       sources loaded\nPASS        module graph        2 reachable modules\nPASS        providers           2 selected providers\nPASS        routes              2 valid routes\nSKIPPED     guards/strategies   JWT feature is not enabled\nPASS        server/CORS         automatic server configuration is valid\nOVERRIDDEN  auto-configuration application provider overrides default"
        );
    }

    #[test]
    fn renders_diagnostics_in_core_style_and_stable_order() {
        assert_eq!(
            render_diagnostics(&report()),
            "error[MADS001]: duplicate provider\n  = UserRepository is registered twice\n\nerror[MADS003]: missing provider\n  --> src/user.rs:12:5\n  = subject: UserRepository\n  = UserService needs UserRepository\n  help: import UserModule\n  help: register UserRepository"
        );
    }

    #[test]
    fn failed_reports_keep_empty_partial_sections_visible() {
        let report = InspectionReport {
            kind: InspectionKind::Graph,
            graph: GraphReport::default(),
            routes: Vec::new(),
            checks: Vec::new(),
            diagnostics: Vec::new(),
            failed: true,
        };

        assert_eq!(
            render_routes(&report),
            "METHOD  PATH        ROUTE                    CONTROLLER       GUARD  SOURCE\n(none)"
        );
        assert_eq!(
            render_graph(&report),
            "Modules\n(none)\n\nProviders\n(none)\n\nDependencies\n(none)\n\nConstruction order\n(none)"
        );
        assert_eq!(render_doctor(&report), "(none)");
    }
}
