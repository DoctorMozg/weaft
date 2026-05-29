//! weaft-core: parse, compile, lint, and token-count a weaft project.
//!
//! A weaft *project* is a directory with a `weaft.yaml` info file, a `skills/` folder
//! of skill `.md` files, an `agents/` folder of subagent `.md` files, and an optional
//! `fragments/` folder of shared Jinja partials. Each skill/agent is YAML frontmatter
//! plus a Jinja2 body that is rendered against a host [`capability`] matrix, so a single
//! source compiles to meaningfully different output per target.

pub mod capability;
pub mod compile;
pub mod diag;
pub mod fs;
pub mod ir;
pub mod lint;
pub mod params;
pub mod parse;
pub mod tokens;

pub use diag::{Diagnostic, Severity, WeftError};
pub use ir::{Agent, AgentMeta, Project, ProjectInfo, Skill, SkillMeta, Targets};
pub use params::{ParamValue, ParamValues};
