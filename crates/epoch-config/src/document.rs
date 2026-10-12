//! Parsing a TOML document into diagnostics and an inspectable tree.

use serde::{Deserialize, Serialize};
use toml_edit::{Document, Item, Table, Value};

/// One step in a path through a document: a key, or an array index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PathSeg {
    Index(usize),
    Key(String),
}

/// A scalar TOML value. Datetimes keep their TOML spelling.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum ScalarValue {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Datetime(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Table,
    InlineTable,
    Array,
    ArrayOfTables,
    Scalar,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TomlNode {
    /// The key, or `[i]` for an array element.
    pub label: String,
    pub path: Vec<PathSeg>,
    pub kind: NodeKind,
    pub value: Option<ScalarValue>,
    /// 1-based line where the key (or element) starts.
    pub line: Option<u32>,
    pub children: Vec<TomlNode>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub message: String,
    /// 1-based line and UTF-16 column (what an editor in a webview counts).
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParseReport {
    pub ok: bool,
    pub diagnostics: Vec<Diagnostic>,
    /// The document's top-level entries; empty when parsing failed.
    pub tree: Vec<TomlNode>,
}

/// Byte offsets → (line, UTF-16 column), both 1-based.
struct LineIndex<'a> {
    text: &'a str,
    starts: Vec<usize>,
}

impl<'a> LineIndex<'a> {
    fn new(text: &'a str) -> Self {
        let mut starts = vec![0];
        starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
        Self { text, starts }
    }

    fn position(&self, offset: usize) -> (u32, u32) {
        let offset = offset.min(self.text.len());
        let line = self.starts.partition_point(|&s| s <= offset) - 1;
        let start = self.starts[line];
        // Clamp to a char boundary before slicing.
        let mut end = offset;
        while !self.text.is_char_boundary(end) {
            end -= 1;
        }
        let column = self.text[start..end].encode_utf16().count();
        (line as u32 + 1, column as u32 + 1)
    }
}

pub fn parse(text: &str) -> ParseReport {
    let index = LineIndex::new(text);
    match Document::parse(text) {
        Ok(doc) => ParseReport {
            ok: true,
            diagnostics: Vec::new(),
            tree: table_children(doc.as_table(), &[], &index),
        },
        Err(err) => {
            let span = err.span().unwrap_or(0..0);
            let (line, column) = index.position(span.start);
            let (end_line, end_column) = index.position(span.end.max(span.start + 1));
            // The message's first line is the summary; the rest re-quotes the
            // source, which the editor already shows.
            let message = err
                .message()
                .lines()
                .next()
                .unwrap_or("invalid TOML")
                .to_string();
            ParseReport {
                ok: false,
                diagnostics: vec![Diagnostic {
                    message,
                    line,
                    column,
                    end_line,
                    end_column,
                }],
                tree: Vec::new(),
            }
        }
    }
}

fn child_path(parent: &[PathSeg], seg: PathSeg) -> Vec<PathSeg> {
    let mut path = parent.to_vec();
    path.push(seg);
    path
}

fn table_children(table: &Table, parent: &[PathSeg], index: &LineIndex) -> Vec<TomlNode> {
    let mut out = Vec::new();
    for (key, item) in table.iter() {
        let path = child_path(parent, PathSeg::Key(key.to_string()));
        let key_span = table.key(key).and_then(|k| k.span());
        let line = key_span
            .or_else(|| item.span())
            .map(|s| index.position(s.start).0);
        if let Some(node) = item_node(key.to_string(), path, item, line, index) {
            out.push(node);
        }
    }
    out
}

fn item_node(
    label: String,
    path: Vec<PathSeg>,
    item: &Item,
    line: Option<u32>,
    index: &LineIndex,
) -> Option<TomlNode> {
    Some(match item {
        Item::None => return None,
        Item::Value(v) => value_node(label, path, v, line, index),
        Item::Table(t) => TomlNode {
            label,
            kind: NodeKind::Table,
            value: None,
            line: t.span().map(|s| index.position(s.start).0).or(line),
            children: table_children(t, &path, index),
            path,
        },
        Item::ArrayOfTables(a) => TomlNode {
            label,
            kind: NodeKind::ArrayOfTables,
            value: None,
            line: a.span().map(|s| index.position(s.start).0).or(line),
            children: a
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    let p = child_path(&path, PathSeg::Index(i));
                    TomlNode {
                        label: format!("[{i}]"),
                        kind: NodeKind::Table,
                        value: None,
                        line: t.span().map(|s| index.position(s.start).0),
                        children: table_children(t, &p, index),
                        path: p,
                    }
                })
                .collect(),
            path,
        },
    })
}

fn value_node(
    label: String,
    path: Vec<PathSeg>,
    value: &Value,
    line: Option<u32>,
    index: &LineIndex,
) -> TomlNode {
    let line = line.or_else(|| value.span().map(|s| index.position(s.start).0));
    match value {
        Value::Array(a) => TomlNode {
            label,
            kind: NodeKind::Array,
            value: None,
            line,
            children: a
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    let p = child_path(&path, PathSeg::Index(i));
                    let l = v.span().map(|s| index.position(s.start).0);
                    value_node(format!("[{i}]"), p, v, l, index)
                })
                .collect(),
            path,
        },
        Value::InlineTable(t) => TomlNode {
            label,
            kind: NodeKind::InlineTable,
            value: None,
            line,
            children: t
                .iter()
                .map(|(k, v)| {
                    let p = child_path(&path, PathSeg::Key(k.to_string()));
                    let l = t
                        .key(k)
                        .and_then(|k| k.span())
                        .map(|s| index.position(s.start).0);
                    value_node(k.to_string(), p, v, l, index)
                })
                .collect(),
            path,
        },
        scalar => TomlNode {
            label,
            kind: NodeKind::Scalar,
            value: scalar_of(scalar),
            line,
            children: Vec::new(),
            path,
        },
    }
}

pub(crate) fn scalar_of(value: &Value) -> Option<ScalarValue> {
    Some(match value {
        Value::String(s) => ScalarValue::String(s.value().clone()),
        Value::Integer(i) => ScalarValue::Integer(*i.value()),
        Value::Float(f) => ScalarValue::Float(*f.value()),
        Value::Boolean(b) => ScalarValue::Boolean(*b.value()),
        Value::Datetime(d) => ScalarValue::Datetime(d.value().to_string()),
        Value::Array(_) | Value::InlineTable(_) => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRAIN: &str = r#"# nexis-ml-rs — an MLP over tabular data.
[train]
epochs = 30
lr = 0.2
device = "auto"  # auto | cpu | gpu

[model]
hidden = [16, 8]
opts = { dropout = 0.1 }

[[stage]]
name = "warmup"
[[stage]]
name = "main"
"#;

    #[test]
    fn builds_a_tree_with_lines() {
        let report = parse(TRAIN);
        assert!(report.ok, "{:?}", report.diagnostics);
        let train = &report.tree[0];
        assert_eq!(train.label, "train");
        assert_eq!(train.kind, NodeKind::Table);
        assert_eq!(train.line, Some(2));
        let epochs = &train.children[0];
        assert_eq!(epochs.value, Some(ScalarValue::Integer(30)));
        assert_eq!(epochs.line, Some(3));
        assert_eq!(
            epochs.path,
            vec![PathSeg::Key("train".into()), PathSeg::Key("epochs".into())]
        );
        let hidden = &report.tree[1].children[0];
        assert_eq!(hidden.kind, NodeKind::Array);
        assert_eq!(hidden.children[1].path[2], PathSeg::Index(1));
        assert_eq!(report.tree[1].children[1].kind, NodeKind::InlineTable);
        let stages = &report.tree[2];
        assert_eq!(stages.kind, NodeKind::ArrayOfTables);
        assert_eq!(stages.children.len(), 2);
        assert_eq!(
            stages.children[1].children[0].value,
            Some(ScalarValue::String("main".into()))
        );
    }

    #[test]
    fn reports_errors_with_positions() {
        let report = parse("[train]\nepochs = \nlr = 1\n");
        assert!(!report.ok);
        let d = &report.diagnostics[0];
        assert_eq!(d.line, 2);
        assert!(d.column >= 9, "{d:?}");
        let dup = parse("a = 1\na = 2\n");
        assert!(!dup.ok);
        assert_eq!(dup.diagnostics[0].line, 2);
    }

    #[test]
    fn columns_count_utf16_units() {
        let index = LineIndex::new("é = \"x\"\n");
        // 'é' is two bytes but one UTF-16 unit.
        assert_eq!(index.position(2), (1, 2));
    }

    #[test]
    fn path_segments_round_trip_through_json() {
        let path = vec![PathSeg::Key("model".into()), PathSeg::Index(0)];
        let json = serde_json::to_string(&path).unwrap();
        assert_eq!(json, r#"["model",0]"#);
        assert_eq!(serde_json::from_str::<Vec<PathSeg>>(&json).unwrap(), path);
    }
}
