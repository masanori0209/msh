use std::fmt;

#[derive(Debug)]
pub enum MshError {
    Io(std::io::Error),
    DirNotFound(String),
    InvalidExport(String),
    CommandNotFound(String),
    ParseError(String),
    AliasError(String),
    UnsupportedSyntax { feature: String, workaround: String },
    ScriptError(String),
}

impl fmt::Display for MshError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "IO error: {e}"),
            Self::DirNotFound(path) => write!(f, "directory not found: {path}"),
            Self::InvalidExport(msg) => write!(f, "invalid export syntax: {msg}"),
            Self::CommandNotFound(cmd) => write!(f, "command not found: {cmd}"),
            Self::ParseError(msg) => write!(f, "parse error: {msg}"),
            Self::AliasError(msg) => write!(f, "alias error: {msg}"),
            Self::UnsupportedSyntax {
                feature,
                workaround,
            } => {
                write!(f, "unsupported syntax: {feature}\nworkaround: {workaround}")
            }
            Self::ScriptError(msg) => write!(f, "script error: {msg}"),
        }
    }
}

impl std::error::Error for MshError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl MshError {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Io(_) => "io",
            Self::DirNotFound(_) => "dir_not_found",
            Self::InvalidExport(_) => "invalid_export",
            Self::CommandNotFound(_) => "command_not_found",
            Self::ParseError(_) => "parse_error",
            Self::AliasError(_) => "alias_error",
            Self::UnsupportedSyntax { .. } => "unsupported_syntax",
            Self::ScriptError(_) => "script_error",
        }
    }

    pub fn workaround(&self) -> Option<&str> {
        match self {
            Self::UnsupportedSyntax { workaround, .. } => Some(workaround),
            _ => None,
        }
    }

    pub fn suggestion(&self) -> Option<String> {
        match self {
            Self::CommandNotFound(cmd) => crate::hints::suggest_command(cmd).map(str::to_string),
            _ => None,
        }
    }
}

impl From<std::io::Error> for MshError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

pub type Result<T> = std::result::Result<T, MshError>;
