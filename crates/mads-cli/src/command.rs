//! Value-preserving parsing for supported MADS.rs CLI commands.

use std::ffi::{OsStr, OsString};

use mads_common::__private::InspectionKind;

/// Cargo package and binary selectors for an application command.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct TargetSelection {
    pub(crate) package: Option<String>,
    pub(crate) binary: Option<String>,
}

/// A command that selects and invokes an application binary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ApplicationCommand {
    pub(crate) target: TargetSelection,
    pub(crate) arguments: Vec<OsString>,
}

/// A command that requests a private application-inspection report.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InspectionCommand {
    pub(crate) kind: InspectionKind,
    pub(crate) target: TargetSelection,
}

/// A database command and its selected Cargo package.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DatabaseInvocation {
    pub(crate) command: DatabaseCommand,
    pub(crate) package: Option<String>,
}

/// A supported top-level command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Command {
    /// Prints general help.
    Help,
    /// Prints the CLI version.
    Version,
    /// Builds and runs an application.
    Run(ApplicationCommand),
    /// Watches, rebuilds, and restarts an application.
    Dev(ApplicationCommand),
    /// Inspects an application through its standard MADS entry point.
    Inspect(InspectionCommand),
    /// Runs or describes a database command.
    Database(DatabaseInvocation),
}

/// A supported database subcommand.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DatabaseCommand {
    /// Generates one complete, review-required schema diff migration.
    Generate,
    /// Applies pending migrations.
    Migrate,
    /// Reverts the most recently applied migration.
    Rollback,
    /// Prints migration status.
    Status,
    /// Prints database command help.
    Help,
}

/// The finite output formats supported by MADS-owned commands.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum OutputFormat {
    /// Preserve the established text output.
    #[default]
    Human,
    /// Select the versioned machine-readable output.
    Json,
}

/// A parsed command together with its selected output format.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Invocation {
    pub(crate) command: Command,
    pub(crate) format: OutputFormat,
}

impl Invocation {
    #[cfg(test)]
    fn human(command: Command) -> Self {
        Self {
            command,
            format: OutputFormat::Human,
        }
    }

    #[cfg(test)]
    fn json(command: Command) -> Self {
        Self {
            command,
            format: OutputFormat::Json,
        }
    }
}

/// The canonical spelling of a finite command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CanonicalCommand {
    /// `mads routes`.
    Routes,
    /// `mads graph`.
    Graph,
    /// `mads doctor`.
    Doctor,
    /// `mads db generate`.
    DatabaseGenerate,
    /// `mads db migrate`.
    DatabaseMigrate,
    /// `mads db rollback`.
    DatabaseRollback,
    /// `mads db status`.
    DatabaseStatus,
    /// `mads db --help`.
    DatabaseHelp,
}

/// A syntax failure together with the output and command context parsed so far.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ParseFailure {
    pub(crate) error: ParseError,
    pub(crate) format: Option<OutputFormat>,
    pub(crate) command: Option<CanonicalCommand>,
}

type CommandParseResult<T> = Result<T, (ParseError, Option<OutputFormat>)>;
type LeadingFormatResult<'arguments> =
    Result<(OutputFormat, bool, &'arguments [OsString]), (ParseError, &'arguments [OsString])>;

/// An unsupported CLI argument form.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ParseError {
    /// The top-level command was not recognized.
    UnknownCommand(OsString),
    /// An option or positional argument was not recognized.
    UnknownArgument(OsString),
    /// An option was not followed by its required value.
    MissingValue(&'static str),
    /// An option value could not be represented as Unicode.
    NonUnicodeValue(&'static str),
    /// An option was supplied more than once.
    DuplicateOption(&'static str),
    /// An output-format value was not one of the supported finite formats.
    InvalidOutputFormat(OsString),
    /// Output selection was supplied to a command that does not own finite output.
    OutputFormatNotSupported,
    /// Output selection was supplied without a command to render.
    MissingCommand,
    /// Application arguments were supplied to an inspection command.
    ApplicationArgumentsNotAccepted,
    /// `db` was not followed by a database command.
    MissingDatabaseCommand,
    /// A database subcommand was not recognized.
    UnknownDatabaseCommand(OsString),
    /// A syntax error arose while parsing database command options.
    DatabaseSyntax(Box<ParseError>),
}

/// Parses process arguments after the executable name.
pub(crate) fn parse(arguments: &[OsString]) -> Result<Invocation, ParseFailure> {
    let (format, global_format, arguments) = match parse_leading_format(arguments) {
        Ok(parsed) => parsed,
        Err((error, remaining)) => {
            return Err(ParseFailure {
                error,
                format: None,
                command: canonical_command(remaining),
            });
        }
    };

    if global_format
        && arguments
            .first()
            .is_some_and(|argument| argument == OsStr::new("--format"))
    {
        let remaining = arguments.get(2..).unwrap_or_default();
        return Err(ParseFailure {
            error: ParseError::DuplicateOption("--format"),
            format: Some(format),
            command: canonical_command(remaining),
        });
    }

    let Some((command, remaining)) = arguments.split_first() else {
        return if global_format {
            Err(ParseFailure {
                error: ParseError::MissingCommand,
                format: Some(format),
                command: None,
            })
        } else {
            Ok(Invocation {
                command: Command::Help,
                format,
            })
        };
    };

    let canonical = canonical_command(arguments);
    let result = match command.to_str() {
        Some("--help" | "-h") => parse_non_finite_command(Command::Help, remaining, global_format),
        Some("--version" | "-V") => {
            parse_non_finite_command(Command::Version, remaining, global_format)
        }
        Some("run") => parse_streaming_command(remaining, global_format)
            .map(|command| (Command::Run(command), OutputFormat::Human)),
        Some("dev") => parse_streaming_command(remaining, global_format)
            .map(|command| (Command::Dev(command), OutputFormat::Human)),
        Some("routes") => {
            parse_finite_inspection(InspectionKind::Routes, remaining, format, global_format)
        }
        Some("graph") => {
            parse_finite_inspection(InspectionKind::Graph, remaining, format, global_format)
        }
        Some("doctor") => {
            parse_finite_inspection(InspectionKind::Doctor, remaining, format, global_format)
        }
        Some("db") => parse_database_command(remaining, format, global_format),
        _ => Err((ParseError::UnknownCommand(command.clone()), Some(format))),
    };

    result
        .map(|(command, format)| Invocation { command, format })
        .map_err(|(error, format)| ParseFailure {
            error,
            format,
            command: canonical,
        })
}

fn parse_leading_format(arguments: &[OsString]) -> LeadingFormatResult<'_> {
    if arguments
        .first()
        .is_none_or(|argument| argument != OsStr::new("--format"))
    {
        return Ok((OutputFormat::Human, false, arguments));
    }

    let Some(value) = arguments.get(1) else {
        return Err((ParseError::MissingValue("--format"), &[]));
    };
    if value.to_str().is_some_and(|value| value.starts_with('-')) {
        return Err((ParseError::MissingValue("--format"), &arguments[1..]));
    }

    let format = parse_format_value(value).map_err(|error| (error, &arguments[2..]))?;
    Ok((format, true, &arguments[2..]))
}

fn parse_non_finite_command(
    command: Command,
    arguments: &[OsString],
    global_format: bool,
) -> CommandParseResult<(Command, OutputFormat)> {
    if global_format || contains_format(arguments) {
        return Err((
            ParseError::OutputFormatNotSupported,
            Some(OutputFormat::Human),
        ));
    }
    if arguments.is_empty() {
        Ok((command, OutputFormat::Human))
    } else {
        Err((
            ParseError::UnknownArgument(arguments[0].clone()),
            Some(OutputFormat::Human),
        ))
    }
}

fn parse_streaming_command(
    arguments: &[OsString],
    global_format: bool,
) -> CommandParseResult<ApplicationCommand> {
    if global_format || contains_format_before_separator(arguments) {
        return Err((
            ParseError::OutputFormatNotSupported,
            Some(OutputFormat::Human),
        ));
    }
    parse_application_command(arguments).map_err(|error| (error, Some(OutputFormat::Human)))
}

fn parse_finite_inspection(
    kind: InspectionKind,
    arguments: &[OsString],
    format: OutputFormat,
    global_format: bool,
) -> CommandParseResult<(Command, OutputFormat)> {
    let (format, arguments) = select_local_format(arguments, format, global_format)?;
    parse_inspection_command(kind, &arguments)
        .map(|command| (command, format))
        .map_err(|error| (error, Some(format)))
}

fn parse_inspection_command(
    kind: InspectionKind,
    arguments: &[OsString],
) -> Result<Command, ParseError> {
    let mut target = TargetSelection::default();
    let mut index = 0;

    while let Some(argument) = arguments.get(index) {
        if argument == OsStr::new("--") {
            return Err(ParseError::ApplicationArgumentsNotAccepted);
        }

        match argument.to_str() {
            Some("--package" | "-p") => {
                parse_selector(arguments, &mut index, "--package", &mut target.package)?;
            }
            Some("--bin") => {
                parse_selector(arguments, &mut index, "--bin", &mut target.binary)?;
            }
            _ => return Err(ParseError::UnknownArgument(argument.clone())),
        }
        index += 1;
    }

    Ok(Command::Inspect(InspectionCommand { kind, target }))
}

fn parse_application_command(arguments: &[OsString]) -> Result<ApplicationCommand, ParseError> {
    let mut target = TargetSelection::default();
    let mut index = 0;

    while let Some(argument) = arguments.get(index) {
        if argument == OsStr::new("--") {
            return Ok(ApplicationCommand {
                target,
                arguments: arguments[index + 1..].to_vec(),
            });
        }

        match argument.to_str() {
            Some("--package" | "-p") => {
                parse_selector(arguments, &mut index, "--package", &mut target.package)?;
            }
            Some("--bin") => {
                parse_selector(arguments, &mut index, "--bin", &mut target.binary)?;
            }
            _ => return Err(ParseError::UnknownArgument(argument.clone())),
        }
        index += 1;
    }

    Ok(ApplicationCommand {
        target,
        arguments: Vec::new(),
    })
}

fn parse_database_command(
    arguments: &[OsString],
    format: OutputFormat,
    global_format: bool,
) -> CommandParseResult<(Command, OutputFormat)> {
    let Some((command, options)) = arguments.split_first() else {
        return Err((
            ParseError::MissingDatabaseCommand,
            Some(if global_format {
                format
            } else {
                OutputFormat::Human
            }),
        ));
    };

    let command = match command.to_str() {
        Some("generate") => DatabaseCommand::Generate,
        Some("migrate") => DatabaseCommand::Migrate,
        Some("rollback") => DatabaseCommand::Rollback,
        Some("status") => DatabaseCommand::Status,
        Some("--help" | "-h") => DatabaseCommand::Help,
        _ => {
            return Err((
                ParseError::UnknownDatabaseCommand(command.clone()),
                Some(if global_format {
                    format
                } else {
                    OutputFormat::Human
                }),
            ));
        }
    };

    let (format, options) = select_local_format(options, format, global_format)?;
    if command == DatabaseCommand::Help && (global_format || format == OutputFormat::Json) {
        return Err((
            ParseError::OutputFormatNotSupported,
            Some(OutputFormat::Human),
        ));
    }

    let mut package = None;
    let mut index = 0;
    while let Some(argument) = options.get(index) {
        match argument.to_str() {
            Some("--package" | "-p") => {
                parse_selector(&options, &mut index, "--package", &mut package)
                    .map_err(|error| ParseError::DatabaseSyntax(Box::new(error)))
                    .map_err(|error| (error, Some(format)))?;
            }
            _ => {
                return Err((
                    ParseError::DatabaseSyntax(Box::new(ParseError::UnknownArgument(
                        argument.clone(),
                    ))),
                    Some(format),
                ));
            }
        }
        index += 1;
    }

    Ok((
        Command::Database(DatabaseInvocation { command, package }),
        format,
    ))
}

fn select_local_format(
    arguments: &[OsString],
    mut format: OutputFormat,
    mut format_seen: bool,
) -> CommandParseResult<(OutputFormat, Vec<OsString>)> {
    let mut retained = Vec::with_capacity(arguments.len());
    let mut index = 0;

    while let Some(argument) = arguments.get(index) {
        match argument.to_str() {
            Some("--package" | "-p" | "--bin") => {
                retained.push(argument.clone());
                index += 1;
                if let Some(value) = arguments.get(index) {
                    retained.push(value.clone());
                }
            }
            Some("--format") => {
                if format_seen {
                    return Err((ParseError::DuplicateOption("--format"), Some(format)));
                }
                format_seen = true;
                index += 1;
                let Some(value) = arguments.get(index) else {
                    return Err((ParseError::MissingValue("--format"), None));
                };
                if value.to_str().is_some_and(|value| value.starts_with('-')) {
                    return Err((ParseError::MissingValue("--format"), None));
                }
                format = parse_format_value(value).map_err(|error| (error, None))?;
            }
            _ => retained.push(argument.clone()),
        }
        index += 1;
    }

    Ok((format, retained))
}

fn parse_format_value(value: &OsString) -> Result<OutputFormat, ParseError> {
    match value.to_str() {
        Some("human") => Ok(OutputFormat::Human),
        Some("json") => Ok(OutputFormat::Json),
        _ => Err(ParseError::InvalidOutputFormat(value.clone())),
    }
}

fn contains_format(arguments: &[OsString]) -> bool {
    arguments
        .iter()
        .any(|argument| argument == OsStr::new("--format"))
}

fn contains_format_before_separator(arguments: &[OsString]) -> bool {
    let mut index = 0;

    while let Some(argument) = arguments.get(index) {
        if argument == OsStr::new("--") {
            return false;
        }
        if matches!(argument.to_str(), Some("--package" | "-p" | "--bin")) {
            index += 2;
            continue;
        }
        if argument == OsStr::new("--format") {
            return true;
        }
        index += 1;
    }

    false
}

fn canonical_command(arguments: &[OsString]) -> Option<CanonicalCommand> {
    let (command, remaining) = arguments.split_first()?;
    match command.to_str()? {
        "routes" => Some(CanonicalCommand::Routes),
        "graph" => Some(CanonicalCommand::Graph),
        "doctor" => Some(CanonicalCommand::Doctor),
        "db" => match remaining.first()?.to_str()? {
            "generate" => Some(CanonicalCommand::DatabaseGenerate),
            "migrate" => Some(CanonicalCommand::DatabaseMigrate),
            "rollback" => Some(CanonicalCommand::DatabaseRollback),
            "status" => Some(CanonicalCommand::DatabaseStatus),
            "--help" | "-h" => Some(CanonicalCommand::DatabaseHelp),
            _ => None,
        },
        _ => None,
    }
}

fn parse_selector(
    arguments: &[OsString],
    index: &mut usize,
    option: &'static str,
    destination: &mut Option<String>,
) -> Result<(), ParseError> {
    if destination.is_some() {
        return Err(ParseError::DuplicateOption(option));
    }

    *index += 1;
    let value = arguments
        .get(*index)
        .ok_or(ParseError::MissingValue(option))?;
    let value = value.to_str().ok_or(ParseError::NonUnicodeValue(option))?;
    if value.starts_with('-') {
        return Err(ParseError::MissingValue(option));
    }
    *destination = Some(value.to_owned());
    Ok(())
}

impl ParseError {
    /// Returns whether the error arose while parsing a database command.
    pub(crate) fn is_database_command(&self) -> bool {
        match self {
            Self::MissingDatabaseCommand
            | Self::UnknownDatabaseCommand(_)
            | Self::DatabaseSyntax(_) => true,
            Self::UnknownCommand(_)
            | Self::UnknownArgument(_)
            | Self::MissingValue(_)
            | Self::NonUnicodeValue(_)
            | Self::DuplicateOption(_)
            | Self::InvalidOutputFormat(_)
            | Self::OutputFormatNotSupported
            | Self::MissingCommand
            | Self::ApplicationArgumentsNotAccepted => false,
        }
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownCommand(command) => {
                write!(formatter, "unknown command: {}", command.to_string_lossy())
            }
            Self::UnknownArgument(argument) => {
                write!(
                    formatter,
                    "unknown argument: {}",
                    argument.to_string_lossy()
                )
            }
            Self::MissingValue(option) => write!(formatter, "missing value for {option}"),
            Self::NonUnicodeValue(option) => {
                write!(formatter, "value for {option} is not valid Unicode")
            }
            Self::DuplicateOption(option) => write!(formatter, "duplicate option: {option}"),
            Self::InvalidOutputFormat(value) => write!(
                formatter,
                "invalid output format: {} (expected human or json)",
                value.to_string_lossy()
            ),
            Self::OutputFormatNotSupported => {
                write!(formatter, "output format is not supported for this command")
            }
            Self::MissingCommand => write!(formatter, "missing command for --format"),
            Self::ApplicationArgumentsNotAccepted => {
                write!(
                    formatter,
                    "inspection command does not accept application arguments"
                )
            }
            Self::MissingDatabaseCommand => write!(formatter, "missing database command"),
            Self::UnknownDatabaseCommand(command) => {
                write!(
                    formatter,
                    "unknown database command: {}",
                    command.to_string_lossy()
                )
            }
            Self::DatabaseSyntax(error) => error.fmt(formatter),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use mads_common::__private::InspectionKind;

    use super::{
        ApplicationCommand, CanonicalCommand, Command, DatabaseCommand, DatabaseInvocation,
        InspectionCommand, Invocation, OutputFormat, ParseError, TargetSelection,
        parse as parse_invocation,
    };

    fn args(arguments: &[&str]) -> Vec<OsString> {
        arguments.iter().map(OsString::from).collect()
    }

    fn parse(arguments: &[OsString]) -> Result<Command, ParseError> {
        parse_invocation(arguments)
            .map(|invocation| invocation.command)
            .map_err(|failure| failure.error)
    }

    #[test]
    fn format_is_accepted_before_or_after_every_finite_command() {
        let cases = [
            (
                args(&["routes"]),
                Command::Inspect(InspectionCommand {
                    kind: InspectionKind::Routes,
                    target: TargetSelection::default(),
                }),
            ),
            (
                args(&["graph"]),
                Command::Inspect(InspectionCommand {
                    kind: InspectionKind::Graph,
                    target: TargetSelection::default(),
                }),
            ),
            (
                args(&["doctor"]),
                Command::Inspect(InspectionCommand {
                    kind: InspectionKind::Doctor,
                    target: TargetSelection::default(),
                }),
            ),
            (
                args(&["db", "generate"]),
                Command::Database(DatabaseInvocation {
                    command: DatabaseCommand::Generate,
                    package: None,
                }),
            ),
            (
                args(&["db", "migrate"]),
                Command::Database(DatabaseInvocation {
                    command: DatabaseCommand::Migrate,
                    package: None,
                }),
            ),
            (
                args(&["db", "rollback"]),
                Command::Database(DatabaseInvocation {
                    command: DatabaseCommand::Rollback,
                    package: None,
                }),
            ),
            (
                args(&["db", "status"]),
                Command::Database(DatabaseInvocation {
                    command: DatabaseCommand::Status,
                    package: None,
                }),
            ),
        ];

        for (spelling, command) in cases {
            let mut before = args(&["--format", "json"]);
            before.extend(spelling.clone());
            let mut after = spelling;
            after.extend(args(&["--format", "json"]));

            assert_eq!(
                parse_invocation(&before),
                Ok(Invocation::json(command.clone()))
            );
            assert_eq!(parse_invocation(&after), Ok(Invocation::json(command)));
        }
    }

    #[test]
    fn format_failures_keep_valid_json_and_canonical_command_context() {
        let duplicate =
            parse_invocation(&args(&["routes", "--format", "json", "--format", "human"]));
        assert!(matches!(
            duplicate,
            Err(failure)
                if failure.error == ParseError::DuplicateOption("--format")
                    && failure.format == Some(OutputFormat::Json)
                    && failure.command == Some(CanonicalCommand::Routes)
        ));

        let duplicate =
            parse_invocation(&args(&["--format", "json", "--format", "human", "routes"]));
        assert!(matches!(
            duplicate,
            Err(failure)
                if failure.error == ParseError::DuplicateOption("--format")
                    && failure.format == Some(OutputFormat::Json)
                    && failure.command == Some(CanonicalCommand::Routes)
        ));

        for arguments in [
            args(&["routes", "--format"]),
            args(&["routes", "--format", "yaml"]),
        ] {
            let failure = parse_invocation(&arguments).unwrap_err();
            assert!(matches!(
                failure.error,
                ParseError::MissingValue("--format") | ParseError::InvalidOutputFormat(_)
            ));
            assert_eq!(failure.format, None);
            assert_eq!(failure.command, Some(CanonicalCommand::Routes));
        }

        let failure = parse_invocation(&args(&["--format", "json", "db", "status", "--unknown"]))
            .unwrap_err();
        assert!(matches!(failure.error, ParseError::DatabaseSyntax(_)));
        assert_eq!(failure.format, Some(OutputFormat::Json));
        assert_eq!(failure.command, Some(CanonicalCommand::DatabaseStatus));
    }

    #[test]
    fn format_is_rejected_by_streaming_and_help_commands_but_not_after_separator() {
        for arguments in [
            args(&["--format", "json", "run"]),
            args(&["run", "--format", "json"]),
            args(&["--format", "json", "dev"]),
            args(&["dev", "--format", "json"]),
            args(&["--format", "json", "--help"]),
            args(&["--version", "--format", "json"]),
            args(&["--format", "json", "db", "--help"]),
            args(&["db", "--help", "--format", "json"]),
        ] {
            assert!(matches!(
                parse_invocation(&arguments),
                Err(failure) if failure.error == ParseError::OutputFormatNotSupported
            ));
        }

        let command = parse_invocation(&args(&["run", "--", "--format", "json"])).unwrap();
        assert_eq!(
            command,
            Invocation::human(Command::Run(ApplicationCommand {
                target: TargetSelection::default(),
                arguments: args(&["--format", "json"]),
            }))
        );
    }

    #[test]
    fn streaming_selector_values_are_not_mistaken_for_format_options() {
        let failure =
            parse_invocation(&args(&["run", "--package", "--format", "json"])).unwrap_err();

        assert_eq!(failure.error, ParseError::MissingValue("--package"));
        assert_eq!(failure.format, Some(OutputFormat::Human));
    }

    #[test]
    fn parses_run_selectors_and_preserves_forwarded_arguments() {
        let command = parse(&args(&[
            "run",
            "-p",
            "api",
            "--bin",
            "server",
            "--",
            "--port",
            "4100",
            "two words",
        ]))
        .unwrap();

        assert_eq!(
            command,
            Command::Run(ApplicationCommand {
                target: TargetSelection {
                    package: Some("api".into()),
                    binary: Some("server".into()),
                },
                arguments: args(&["--port", "4100", "two words"]),
            })
        );
    }

    #[test]
    fn parses_dev_selectors_and_preserves_forwarded_arguments() {
        let command = parse(&args(&[
            "dev",
            "-p",
            "api",
            "--bin",
            "server",
            "--",
            "--seed",
            "42",
            "two words",
        ]))
        .unwrap();

        assert_eq!(
            command,
            Command::Dev(ApplicationCommand {
                target: TargetSelection {
                    package: Some("api".into()),
                    binary: Some("server".into()),
                },
                arguments: args(&["--seed", "42", "two words"]),
            })
        );
    }

    #[test]
    fn dev_rejects_duplicate_and_missing_selectors_like_run() {
        for arguments in [
            args(&["--package", "api", "-p", "web"]),
            args(&["--bin"]),
            args(&["--package", "--bin", "server"]),
        ] {
            assert_eq!(
                parse(&[vec![OsString::from("dev")], arguments.clone()].concat()),
                parse(&[vec![OsString::from("run")], arguments].concat()),
            );
        }
    }

    #[test]
    fn parses_inspection_selectors_and_rejects_application_arguments() {
        assert_eq!(
            parse(&args(&["routes", "-p", "api", "--bin", "server"])).unwrap(),
            Command::Inspect(InspectionCommand {
                kind: InspectionKind::Routes,
                target: TargetSelection {
                    package: Some("api".into()),
                    binary: Some("server".into()),
                },
            })
        );
        assert!(parse(&args(&["doctor", "--", "argument"])).is_err());
    }

    #[test]
    fn database_commands_accept_package_but_reject_binary_and_application_arguments() {
        assert!(matches!(
            parse(&args(&["db", "status", "--package", "api"])),
            Ok(Command::Database(DatabaseInvocation { .. }))
        ));
        assert!(parse(&args(&["db", "status", "--bin", "server"])).is_err());
        assert!(parse(&args(&["db", "status", "--", "extra"])).is_err());
    }

    #[test]
    fn generate_accepts_only_an_optional_package_selector() {
        assert!(matches!(
            parse(&args(&["db", "generate"])),
            Ok(Command::Database(DatabaseInvocation {
                command: DatabaseCommand::Generate,
                package: None,
            }))
        ));
        assert!(matches!(
            parse(&args(&["db", "generate", "-p", "api"])),
            Ok(Command::Database(DatabaseInvocation {
                command: DatabaseCommand::Generate,
                package: Some(package),
            })) if package == "api"
        ));

        for arguments in [
            ["db", "generate", "users"].as_slice(),
            ["db", "generate", "--diff-schema"].as_slice(),
            ["db", "generate", "--bin", "server"].as_slice(),
            ["db", "generate", "--", "extra"].as_slice(),
            ["db", "generate", "-p", "api", "--package", "web"].as_slice(),
        ] {
            assert!(
                parse(&args(arguments)).is_err(),
                "{arguments:?} should fail"
            );
        }
    }

    #[test]
    fn rejects_foundation_and_named_generation_forms() {
        assert!(parse(&args(&["foundation"])).is_err());
        assert!(parse(&args(&["db", "generate", "named"])).is_err());
        assert!(parse(&args(&["db", "generate", "--diff-schema"])).is_err());
    }

    #[test]
    fn rejects_duplicate_missing_and_non_unicode_selector_values() {
        assert!(matches!(
            parse(&args(&["run", "-p", "api", "--package", "web"])),
            Err(ParseError::DuplicateOption("--package"))
        ));
        assert!(matches!(
            parse(&args(&["run", "--bin"])),
            Err(ParseError::MissingValue("--bin"))
        ));
        assert!(matches!(
            parse(&args(&["run", "--package", "--bin", "server"])),
            Err(ParseError::MissingValue("--package"))
        ));
        assert!(matches!(
            parse(&args(&["db", "status", "--package", "--bin"])),
            Err(ParseError::DatabaseSyntax(error))
                if matches!(*error, ParseError::MissingValue("--package"))
        ));

        let mut arguments = args(&["run", "--package"]);
        arguments.push(non_unicode_argument());
        assert!(matches!(
            parse(&arguments),
            Err(ParseError::NonUnicodeValue("--package"))
        ));
    }

    #[test]
    fn rejects_unknown_top_level_and_database_arguments_precisely() {
        assert!(matches!(
            parse(&args(&["unknown"])),
            Err(ParseError::UnknownCommand(command)) if command == "unknown"
        ));
        assert!(matches!(
            parse(&args(&["run", "extra"])),
            Err(ParseError::UnknownArgument(argument)) if argument == "extra"
        ));
        assert!(matches!(
            parse(&args(&["db"])),
            Err(ParseError::MissingDatabaseCommand)
        ));
    }

    #[test]
    fn every_database_option_error_keeps_database_help_scope() {
        let cases = [
            args(&["db", "status", "--bin", "server"]),
            args(&["db", "status", "--package", "api", "-p", "web"]),
            args(&["db", "status", "--package"]),
        ];

        for arguments in cases {
            let error = parse(&arguments).unwrap_err();
            assert!(error.is_database_command(), "error lost DB scope: {error}");
        }

        let mut non_unicode = args(&["db", "status", "--package"]);
        non_unicode.push(non_unicode_argument());
        let error = parse(&non_unicode).unwrap_err();
        assert!(error.is_database_command(), "error lost DB scope: {error}");
    }

    #[test]
    fn preserves_non_unicode_application_arguments_after_separator() {
        let non_unicode = non_unicode_argument();
        let mut arguments = args(&["run", "--"]);
        arguments.push(non_unicode.clone());

        let Command::Run(command) = parse(&arguments).unwrap() else {
            panic!("run should parse as an application command");
        };

        assert_eq!(command.arguments, vec![non_unicode]);
    }

    #[cfg(unix)]
    fn non_unicode_argument() -> OsString {
        use std::os::unix::ffi::OsStringExt;

        OsString::from_vec(vec![0xff])
    }

    #[cfg(windows)]
    fn non_unicode_argument() -> OsString {
        use std::os::windows::ffi::OsStringExt;

        OsString::from_wide(&[0xd800])
    }
}
