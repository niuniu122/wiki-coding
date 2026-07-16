use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PermissionName {
    Confirm,
    FullAccess,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommandIntent {
    Interrupt,
    NewSession,
    ListSessions,
    Resume(String),
    Compact,
    ApiSetup,
    Provider(Option<String>),
    AgentContinue,
    AgentSubmit(String),
    ChatSubmit(String),
    ListModels,
    SwitchModel(String),
    Capabilities(Option<String>),
    Permissions(Option<PermissionName>),
    ToggleTrace,
    RetryInitialization,
    Vault(String),
    Exit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandAvailability {
    Available,
    NotAvailable { owning_phase: u8 },
}

impl CommandIntent {
    #[must_use]
    pub const fn availability(&self) -> CommandAvailability {
        CommandAvailability::Available
    }

    #[must_use]
    pub const fn canonical_name(&self) -> &'static str {
        match self {
            Self::Interrupt => "/interrupt",
            Self::NewSession => "/new",
            Self::ListSessions => "/threads",
            Self::Resume(_) => "/resume",
            Self::Compact => "/compact",
            Self::ApiSetup => "/api",
            Self::Provider(_) => "/provider",
            Self::AgentContinue => "/continue",
            Self::AgentSubmit(_) => "/agent",
            Self::ChatSubmit(_) => "/chat",
            Self::ListModels => "/models",
            Self::SwitchModel(_) => "/model",
            Self::Capabilities(_) => "/capabilities",
            Self::Permissions(_) => "/permissions",
            Self::ToggleTrace => "/trace",
            Self::RetryInitialization => "/retry",
            Self::Vault(_) => "/vault",
            Self::Exit => "/exit",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParsedInput {
    Prompt(String),
    Command(CommandIntent),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandParseError {
    Empty,
    MissingArgument(&'static str),
    UnexpectedArgument(&'static str),
    InvalidPermissionMode,
    InvalidCapabilitiesSyntax,
    InvalidVaultSyntax,
    UnknownCommand,
}

impl fmt::Display for CommandParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("input is empty"),
            Self::MissingArgument(command) => write!(formatter, "{command} requires an argument"),
            Self::UnexpectedArgument(command) => {
                write!(formatter, "{command} does not accept an argument")
            }
            Self::InvalidPermissionMode => {
                formatter.write_str("/permissions accepts only confirm or full-access")
            }
            Self::InvalidCapabilitiesSyntax => {
                formatter.write_str("use /capabilities or /capabilities search <query>")
            }
            Self::InvalidVaultSyntax => formatter.write_str(
                "use /vault status|lint|repair|rebuild|import|gc|forget followed by its arguments",
            ),
            Self::UnknownCommand => formatter.write_str("unknown slash command"),
        }
    }
}

impl std::error::Error for CommandParseError {}

pub fn parse_input(raw: &str) -> Result<ParsedInput, CommandParseError> {
    let input = raw.trim();
    if input.is_empty() {
        return Err(CommandParseError::Empty);
    }
    if !input.starts_with('/') {
        return Ok(ParsedInput::Prompt(input.to_owned()));
    }
    let mut pieces = input.splitn(2, char::is_whitespace);
    let command = pieces.next().ok_or(CommandParseError::Empty)?;
    let argument = pieces
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let intent = match command {
        "/interrupt" => CommandIntent::Interrupt.no_argument(argument)?,
        "/new" => CommandIntent::NewSession.no_argument(argument)?,
        "/threads" => CommandIntent::ListSessions.no_argument(argument)?,
        "/resume" => CommandIntent::Resume(required("/resume", argument)?),
        "/compact" => CommandIntent::Compact.no_argument(argument)?,
        "/api" => CommandIntent::ApiSetup.no_argument(argument)?,
        "/provider" => CommandIntent::Provider(argument.map(str::to_owned)),
        "/continue" => CommandIntent::AgentContinue.no_argument(argument)?,
        "/agent" => CommandIntent::AgentSubmit(required("/agent", argument)?),
        "/chat" => CommandIntent::ChatSubmit(required("/chat", argument)?),
        "/models" => CommandIntent::ListModels.no_argument(argument)?,
        "/model" => CommandIntent::SwitchModel(required("/model", argument)?),
        "/capabilities" => CommandIntent::Capabilities(parse_capabilities(argument)?),
        "/permissions" => CommandIntent::Permissions(parse_permission(argument)?),
        "/trace" => CommandIntent::ToggleTrace.no_argument(argument)?,
        "/retry" => CommandIntent::RetryInitialization.no_argument(argument)?,
        "/vault" => CommandIntent::Vault(parse_vault(argument)?),
        "/exit" | "/quit" => CommandIntent::Exit.no_argument(argument)?,
        _ => return Err(CommandParseError::UnknownCommand),
    };
    Ok(ParsedInput::Command(intent))
}

fn parse_vault(argument: Option<&str>) -> Result<String, CommandParseError> {
    let value = argument.ok_or(CommandParseError::InvalidVaultSyntax)?;
    let action = value
        .split_whitespace()
        .next()
        .ok_or(CommandParseError::InvalidVaultSyntax)?;
    if matches!(
        action,
        "bootstrap" | "status" | "lint" | "repair" | "rebuild" | "import" | "gc" | "forget"
    ) {
        Ok(value.to_owned())
    } else {
        Err(CommandParseError::InvalidVaultSyntax)
    }
}

impl CommandIntent {
    fn no_argument(self, argument: Option<&str>) -> Result<Self, CommandParseError> {
        if argument.is_some() {
            Err(CommandParseError::UnexpectedArgument(self.canonical_name()))
        } else {
            Ok(self)
        }
    }
}

fn required(command: &'static str, argument: Option<&str>) -> Result<String, CommandParseError> {
    argument
        .map(str::to_owned)
        .ok_or(CommandParseError::MissingArgument(command))
}

fn parse_permission(argument: Option<&str>) -> Result<Option<PermissionName>, CommandParseError> {
    argument
        .map(|value| match value {
            "confirm" => Ok(PermissionName::Confirm),
            "full-access" => Ok(PermissionName::FullAccess),
            _ => Err(CommandParseError::InvalidPermissionMode),
        })
        .transpose()
}

fn parse_capabilities(argument: Option<&str>) -> Result<Option<String>, CommandParseError> {
    match argument {
        None => Ok(None),
        Some(value) => value
            .strip_prefix("search ")
            .map(str::trim)
            .filter(|query| !query.is_empty())
            .map(|query| Some(query.to_owned()))
            .ok_or(CommandParseError::InvalidCapabilitiesSyntax),
    }
}
