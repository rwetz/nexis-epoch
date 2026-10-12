//! TOML for Epoch: the Configuration Studio's parser and editor, and the
//! app's own `settings.toml`.

pub mod document;
pub mod edit;
pub mod settings;

pub use document::{parse, Diagnostic, NodeKind, ParseReport, PathSeg, ScalarValue, TomlNode};
pub use edit::{set_value, EditError};
pub use settings::{AppSettings, LoadedSettings};
