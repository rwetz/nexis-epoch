//! Epoch's own settings, stored as `settings.toml` in the app config folder.
//!
//! The file is read field by field: a missing or invalid value falls back to
//! its default with a warning rather than discarding the whole file, and
//! saving goes through `toml_edit`, so comments a user wrote in it survive.
//! Credentials never belong here (see docs/ARCHITECTURE.md, "Secrets").

use std::path::Path;

use serde::{Deserialize, Serialize};
use toml_edit::{DocumentMut, Item, Table};

use epoch_core::fs::atomic_write;
use epoch_core::{CoreError, Result};

pub const SETTINGS_FILE: &str = "settings.toml";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub python: PythonSettings,
    pub data: DataSettings,
    pub explorer: ExplorerSettings,
    pub experiments: ExperimentSettings,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PythonSettings {
    /// An interpreter to prefer over detected environments. Empty = detect.
    pub interpreter: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataSettings {
    /// Rows fetched per page in the dataset grid.
    pub page_size: u32,
    /// Most rows an ad-hoc SQL query returns.
    pub query_row_limit: u32,
    /// Queries are interrupted after this many seconds.
    pub query_timeout_secs: u32,
    /// DuckDB memory limit, in MiB.
    pub memory_limit_mb: u32,
    /// DuckDB worker threads; 0 lets DuckDB decide.
    pub threads: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplorerSettings {
    pub show_hidden: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentSettings {
    /// Index experiment stores when a project is opened.
    pub index_on_open: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            python: PythonSettings {
                interpreter: String::new(),
            },
            data: DataSettings {
                page_size: 200,
                query_row_limit: 10_000,
                query_timeout_secs: 60,
                memory_limit_mb: 2_048,
                threads: 0,
            },
            explorer: ExplorerSettings { show_hidden: false },
            experiments: ExperimentSettings {
                index_on_open: true,
            },
        }
    }
}

/// Inclusive bounds for each numeric setting.
pub mod limits {
    pub const PAGE_SIZE: (u32, u32) = (25, 2_000);
    pub const QUERY_ROW_LIMIT: (u32, u32) = (100, 200_000);
    pub const QUERY_TIMEOUT_SECS: (u32, u32) = (1, 3_600);
    pub const MEMORY_LIMIT_MB: (u32, u32) = (256, 262_144);
    pub const THREADS: (u32, u32) = (0, 256);
}

impl AppSettings {
    /// Bring every value inside its bounds, naming what was changed.
    pub fn validate(&mut self) -> Vec<String> {
        let mut warnings = Vec::new();
        let mut clamp = |name: &str, v: &mut u32, (lo, hi): (u32, u32)| {
            if *v < lo || *v > hi {
                let fixed = (*v).clamp(lo, hi);
                warnings.push(format!(
                    "{name} = {v} is outside {lo}..={hi}; using {fixed}"
                ));
                *v = fixed;
            }
        };
        let d = &mut self.data;
        clamp("data.page_size", &mut d.page_size, limits::PAGE_SIZE);
        clamp(
            "data.query_row_limit",
            &mut d.query_row_limit,
            limits::QUERY_ROW_LIMIT,
        );
        clamp(
            "data.query_timeout_secs",
            &mut d.query_timeout_secs,
            limits::QUERY_TIMEOUT_SECS,
        );
        clamp(
            "data.memory_limit_mb",
            &mut d.memory_limit_mb,
            limits::MEMORY_LIMIT_MB,
        );
        clamp("data.threads", &mut d.threads, limits::THREADS);
        self.python.interpreter = self.python.interpreter.trim().to_string();
        warnings
    }
}

/// Settings plus anything that was wrong with the file.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadedSettings {
    pub settings: AppSettings,
    pub warnings: Vec<String>,
    pub path: String,
}

struct Reader<'a> {
    doc: &'a DocumentMut,
    warnings: Vec<String>,
}

impl Reader<'_> {
    fn item(&self, section: &str, key: &str) -> Option<&Item> {
        self.doc.get(section)?.get(key)
    }

    fn u32(&mut self, section: &str, key: &str, default: u32) -> u32 {
        match self.item(section, key) {
            None => default,
            Some(item) => match item.as_integer().and_then(|i| u32::try_from(i).ok()) {
                Some(v) => v,
                None => {
                    self.warnings
                        .push(format!("{section}.{key} must be a whole number ≥ 0"));
                    default
                }
            },
        }
    }

    fn bool(&mut self, section: &str, key: &str, default: bool) -> bool {
        match self.item(section, key) {
            None => default,
            Some(item) => item.as_bool().unwrap_or_else(|| {
                self.warnings
                    .push(format!("{section}.{key} must be true or false"));
                default
            }),
        }
    }

    fn string(&mut self, section: &str, key: &str, default: &str) -> String {
        match self.item(section, key) {
            None => default.to_string(),
            Some(item) => item.as_str().map(str::to_string).unwrap_or_else(|| {
                self.warnings
                    .push(format!("{section}.{key} must be a string"));
                default.to_string()
            }),
        }
    }
}

/// Read settings from `dir/settings.toml`. A missing file is not an error.
pub fn load(dir: &Path) -> LoadedSettings {
    let path = dir.join(SETTINGS_FILE);
    let d = AppSettings::default();
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(_) => {
            return LoadedSettings {
                settings: d,
                warnings: Vec::new(),
                path: path.display().to_string(),
            }
        }
    };
    let doc: DocumentMut = match text.parse() {
        Ok(doc) => doc,
        Err(e) => {
            let first = e.message().lines().next().unwrap_or_default().to_string();
            return LoadedSettings {
                settings: d,
                warnings: vec![format!(
                    "{SETTINGS_FILE} could not be parsed ({first}); using defaults"
                )],
                path: path.display().to_string(),
            };
        }
    };
    let mut r = Reader {
        doc: &doc,
        warnings: Vec::new(),
    };
    let mut settings = AppSettings {
        python: PythonSettings {
            interpreter: r.string("python", "interpreter", &d.python.interpreter),
        },
        data: DataSettings {
            page_size: r.u32("data", "page_size", d.data.page_size),
            query_row_limit: r.u32("data", "query_row_limit", d.data.query_row_limit),
            query_timeout_secs: r.u32("data", "query_timeout_secs", d.data.query_timeout_secs),
            memory_limit_mb: r.u32("data", "memory_limit_mb", d.data.memory_limit_mb),
            threads: r.u32("data", "threads", d.data.threads),
        },
        explorer: ExplorerSettings {
            show_hidden: r.bool("explorer", "show_hidden", d.explorer.show_hidden),
        },
        experiments: ExperimentSettings {
            index_on_open: r.bool("experiments", "index_on_open", d.experiments.index_on_open),
        },
    };
    let mut warnings = r.warnings;
    warnings.extend(settings.validate());
    LoadedSettings {
        settings,
        warnings,
        path: path.display().to_string(),
    }
}

const HEADER: &str = "\
# Nexis Epoch settings. Edited from the Settings page; comments you add here
# are kept. Never put credentials in this file.
";

/// Write settings to `dir/settings.toml`, keeping the existing file's
/// comments and layout where it parses.
pub fn save(dir: &Path, settings: &AppSettings) -> Result<AppSettings> {
    let mut settings = settings.clone();
    let warnings = settings.validate();
    if !warnings.is_empty() {
        return Err(CoreError::Invalid(warnings.join("; ")));
    }
    let path = dir.join(SETTINGS_FILE);
    let mut doc: DocumentMut = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| t.parse().ok())
        .unwrap_or_else(|| HEADER.parse().expect("header is valid TOML"));

    fn set(doc: &mut DocumentMut, section: &str, key: &str, new: toml_edit::Value) {
        if !doc.contains_table(section) {
            doc.insert(section, Item::Table(Table::new()));
        }
        let table = doc[section].as_table_mut().expect("section is a table");
        match table.get_mut(key).and_then(Item::as_value_mut) {
            Some(old) => {
                let decor = old.decor().clone();
                *old = new;
                *old.decor_mut() = decor;
            }
            None => {
                table.insert(key, Item::Value(new));
            }
        }
    }

    let s = &settings;
    set(
        &mut doc,
        "python",
        "interpreter",
        toml_edit::Value::from(s.python.interpreter.as_str()),
    );
    let ints = [
        ("page_size", s.data.page_size),
        ("query_row_limit", s.data.query_row_limit),
        ("query_timeout_secs", s.data.query_timeout_secs),
        ("memory_limit_mb", s.data.memory_limit_mb),
        ("threads", s.data.threads),
    ];
    for (key, v) in ints {
        set(&mut doc, "data", key, toml_edit::Value::from(i64::from(v)));
    }
    set(
        &mut doc,
        "explorer",
        "show_hidden",
        toml_edit::Value::from(s.explorer.show_hidden),
    );
    set(
        &mut doc,
        "experiments",
        "index_on_open",
        toml_edit::Value::from(s.experiments.index_on_open),
    );

    atomic_write(&path, doc.to_string().as_bytes())?;
    settings.validate();
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = load(dir.path());
        assert_eq!(loaded.settings, AppSettings::default());
        assert!(loaded.warnings.is_empty());
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = AppSettings::default();
        s.data.page_size = 500;
        s.explorer.show_hidden = true;
        s.python.interpreter = "C:/py/python.exe".into();
        save(dir.path(), &s).unwrap();
        assert_eq!(load(dir.path()).settings, s);
    }

    #[test]
    fn bad_values_fall_back_one_field_at_a_time() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(SETTINGS_FILE),
            "[data]\npage_size = \"lots\"\nquery_timeout_secs = 99999\nthreads = 4\n",
        )
        .unwrap();
        let loaded = load(dir.path());
        assert_eq!(loaded.settings.data.page_size, 200);
        assert_eq!(loaded.settings.data.query_timeout_secs, 3_600);
        assert_eq!(loaded.settings.data.threads, 4);
        assert_eq!(loaded.warnings.len(), 2, "{:?}", loaded.warnings);
    }

    #[test]
    fn save_keeps_user_comments() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(SETTINGS_FILE);
        std::fs::write(&file, "# mine\n[data]\npage_size = 100  # small screen\n").unwrap();
        let mut s = load(dir.path()).settings;
        s.data.page_size = 300;
        save(dir.path(), &s).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.starts_with("# mine\n"), "{text}");
        assert!(text.contains("page_size = 300  # small screen"), "{text}");
    }

    #[test]
    fn save_rejects_out_of_range_values() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = AppSettings::default();
        s.data.query_row_limit = 5;
        assert!(save(dir.path(), &s).is_err());
    }
}
