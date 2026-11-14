//! Errors: a stable code, a readable message and optional details.

/// Why an operation failed. Each serializes as its stable name (`invalid-argument`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, twip_wire::Encode, twip_wire::Decode)]
#[wire(rename_all = "kebab-case")]
pub enum ErrorCode {
    /// The session was closed.
    Destroyed,
    /// An argument is out of its range, or names something that doesn't exist.
    InvalidArgument,
    /// The operation can't run here now (nothing to undo, no table at the selection).
    Unavailable,
    /// The operation changes the document, and the session is read only.
    ReadOnly,
    /// The bytes aren't a document the engine reads. The open document stays.
    ImportFailed,
    /// The document couldn't be written in the requested format.
    ExportFailed,
    /// The operation was cancelled.
    Aborted,
    /// The platform refused (a browser denying clipboard access).
    NotAllowed,
    /// The engine doesn't do this yet.
    Unsupported,
    /// A fault in the engine. The session rejects later operations.
    Internal,
}

/// A failed operation: its code, a message for developers, and details where the code needs them.
#[derive(Clone, Debug, PartialEq, twip_wire::Encode, twip_wire::Decode)]
#[wire(rename_all = "camelCase", deny_unknown_fields)]
pub struct Error {
    /// Why it failed.
    pub code: ErrorCode,
    /// A readable message for developers, not for end users.
    pub message: String,
    /// More about the failure, where the code needs it (the argument that was invalid).
    #[wire(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<twip_wire::RawJson>,
}

impl Error {
    /// An error with `code` and `message` and no details.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Error {
        Error { code, message: message.into(), details: None }
    }

    /// `what` isn't something the engine does yet.
    pub fn unsupported(what: &str) -> Error {
        Error::new(ErrorCode::Unsupported, format!("{what} is not supported yet"))
    }

    /// An argument is invalid: `what` says which and why.
    pub fn invalid(what: impl Into<String>) -> Error {
        Error::new(ErrorCode::InvalidArgument, what)
    }

    /// The operation can't run now: `why` says what's missing.
    pub fn unavailable(why: impl Into<String>) -> Error {
        Error::new(ErrorCode::Unavailable, why)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", twip_wire::Named::name(&self.code), self.message)
    }
}

impl std::error::Error for Error {}
