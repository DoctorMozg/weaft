//! Error and diagnostic types.
//!
//! Hard failures use [`WeftError`] (a `thiserror` + `miette::Diagnostic` enum). Lint
//! findings, which are collected rather than thrown, use the lighter [`Diagnostic`]
//! struct with a [`Severity`].

use miette::SourceSpan;
use std::path::PathBuf;
use thiserror::Error;

/// A hard error that aborts the current operation.
#[derive(Error, Debug, miette::Diagnostic)]
pub enum WeftError {
    #[error("failed to read {path}")]
    #[diagnostic(code(weaft::io::read))]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to write {path}")]
    #[diagnostic(code(weaft::io::write))]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("could not find weaft.yaml at {0}")]
    #[diagnostic(
        code(weaft::project::missing_manifest),
        help("run `weaft init <name>` to scaffold a new project, or pass --manifest-path")
    )]
    MissingManifest(PathBuf),

    #[error("invalid weaft.yaml")]
    #[diagnostic(code(weaft::project::manifest))]
    Manifest {
        #[source_code]
        src: String,
        #[label("{label}")]
        span: SourceSpan,
        label: String,
    },

    #[error("invalid frontmatter in {path}")]
    #[diagnostic(code(weaft::parse::frontmatter))]
    Frontmatter {
        path: PathBuf,
        #[source_code]
        src: String,
        #[label("{label}")]
        span: SourceSpan,
        label: String,
    },

    #[error("{path} is missing its `+++`/`---` frontmatter block")]
    #[diagnostic(
        code(weaft::parse::no_frontmatter),
        help("skill and agent files must start with a YAML frontmatter block delimited by `---`")
    )]
    NoFrontmatter { path: PathBuf },

    #[error("template render failed for {artifact}")]
    #[diagnostic(code(weaft::compile::render))]
    Render {
        artifact: String,
        #[source]
        source: Box<minijinja::Error>,
    },

    #[error("unknown target: {0}")]
    #[diagnostic(
        code(weaft::target::unknown),
        help("known targets: claude-code, cursor, agents-md")
    )]
    UnknownTarget(String),

    #[error("invalid --param `{raw}`")]
    #[diagnostic(code(weaft::param::invalid), help("expected key=value"))]
    BadParam { raw: String },

    #[error("parameter `{name}` expects a {expected} value, got `{got}`")]
    #[diagnostic(code(weaft::param::type_mismatch))]
    ParamType {
        name: String,
        expected: &'static str,
        got: String,
    },
}

/// Severity of a collected lint finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Warning,
    Error,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Severity::Warning => "warning",
            Severity::Error => "error",
        }
    }
}

/// A single lint finding. Collected into a `Vec` and reported together.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub code: &'static str,
    pub severity: Severity,
    pub message: String,
    pub help: Option<String>,
    /// Which artifact (skill/agent name or file) the finding concerns.
    pub artifact: Option<String>,
}

impl Diagnostic {
    pub fn error(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            severity: Severity::Error,
            message: message.into(),
            help: None,
            artifact: None,
        }
    }

    pub fn warning(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            severity: Severity::Warning,
            message: message.into(),
            help: None,
            artifact: None,
        }
    }

    #[must_use]
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    #[must_use]
    pub fn with_artifact(mut self, artifact: impl Into<String>) -> Self {
        self.artifact = Some(artifact.into());
        self
    }

    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }
}
