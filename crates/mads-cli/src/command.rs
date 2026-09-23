//! Value-preserving parsing for supported MADS.rs CLI commands.

use std::ffi::{OsStr, OsString};

use mads_common::__private::InspectionKind;

use crate::scaffold::{ProjectName, ProjectNameError};

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

/// A command that publishes the bundled minimal MADS project.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NewCommand {
    pub(crate) name: ProjectName,
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
    /// Creates a minimal MADS application in a new child directory.
    New(NewCommand),
    /// Inspects an application through its standard MADS entry point.
    Inspect(InspectionCommand),
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
    /// `mads new`.
    New,
    /// `mads routes`.
    Routes,
    /// `mads graph`.
    Graph,
    /// `mads doctor`.
    Doctor,
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
    /// `new` was not followed by a project name.
    MissingProjectName,
    /// The project name violates the fixed scaffolding policy.
    InvalidProjectName(ProjectNameError),
    /// Application arguments were supplied to an inspection command.
    ApplicationArgumentsNotAccepted,
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
        Some("new") => parse_new_command(remaining, format, global_format),
        Some("routes") => {
            parse_finite_inspection(InspectionKind::Routes, remaining, format, global_format)
        }
        Some("graph") => {
            parse_finite_inspection(InspectionKind::Graph, remaining, format, global_format)
        }
        Some("doctor") => {
            parse_finite_inspection(InspectionKind::Doctor, remaining, format, global_format)
        }
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
    let (format, arguments, _) = select_local_format(arguments, format, global_format)?;
    parse_inspection_command(kind, &arguments)
        .map(|command| (command, format))
        .map_err(|error| (error, Some(format)))
}

fn parse_new_command(
    arguments: &[OsString],
    format: OutputFormat,
    global_format: bool,
) -> CommandParseResult<(Command, OutputFormat)> {
    let (format, arguments, _) = select_local_format(arguments, format, global_format)?;
    let Some((name, remaining)) = arguments.split_first() else {
        return Err((ParseError::MissingProjectName, Some(format)));
    };
    let name = name
        .to_str()
        .ok_or(ParseError::NonUnicodeValue("project name"))
        .map_err(|error| (error, Some(format)))?;
    let name = ProjectName::parse(name)
        .map_err(ParseError::InvalidProjectName)
        .map_err(|error| (error, Some(format)))?;
    if let Some(argument) = remaining.first() {
        return Err((ParseError::UnknownArgument(argument.clone()), Some(format)));
    }

    Ok((Command::New(NewCommand { name }), format))
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

fn select_local_format(
    arguments: &[OsString],
    mut format: OutputFormat,
    mut format_seen: bool,
) -> CommandParseResult<(OutputFormat, Vec<OsString>, bool)> {
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

    Ok((format, retained, format_seen))
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
    let (command, _) = arguments.split_first()?;
    match command.to_str()? {
        "new" => Some(CanonicalCommand::New),
        "routes" => Some(CanonicalCommand::Routes),
        "graph" => Some(CanonicalCommand::Graph),
        "doctor" => Some(CanonicalCommand::Doctor),
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
            Self::MissingProjectName => write!(formatter, "missing project name"),
            Self::InvalidProjectName(error) => error.fmt(formatter),
            Self::ApplicationArgumentsNotAccepted => {
                write!(
                    formatter,
                    "inspection command does not accept application arguments"
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use mads_common::__private::InspectionKind;

    use crate::scaffold::ProjectName;

    use super::{
        ApplicationCommand, CanonicalCommand, Command, InspectionCommand, Invocation, NewCommand,
        OutputFormat, ParseError, TargetSelection, parse as parse_invocation,
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
                args(&["new", "my-app"]),
                Command::New(NewCommand {
                    name: ProjectName::parse("my-app").expect("fixture name should be valid"),
                }),
            ),
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

        let failure = parse_invocation(&args(&["--format", "json", "db", "status"])).unwrap_err();
        assert!(matches!(failure.error, ParseError::UnknownCommand(command) if command == "db"));
        assert_eq!(failure.format, Some(OutputFormat::Json));
        assert_eq!(failure.command, None);
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
    fn rejects_removed_top_level_commands() {
        assert!(parse(&args(&["foundation"])).is_err());
        assert!(
            matches!(parse(&args(&["db"])), Err(ParseError::UnknownCommand(command)) if command == "db")
        );
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
        let mut arguments = args(&["run", "--package"]);
        arguments.push(non_unicode_argument());
        assert!(matches!(
            parse(&arguments),
            Err(ParseError::NonUnicodeValue("--package"))
        ));
    }

    #[test]
    fn rejects_unknown_top_level_arguments_precisely() {
        assert!(matches!(
            parse(&args(&["unknown"])),
            Err(ParseError::UnknownCommand(command)) if command == "unknown"
        ));
        assert!(matches!(
            parse(&args(&["run", "extra"])),
            Err(ParseError::UnknownArgument(argument)) if argument == "extra"
        ));
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
