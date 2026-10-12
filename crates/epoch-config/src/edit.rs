//! Format-preserving edits.
//!
//! `toml_edit` keeps each value's surrounding whitespace and comments
//! ("decor") and re-emits untouched parts of the document as they were
//! written. Replacing a scalar here keeps its decor too, so `lr = 0.2  # fast`
//! becomes `lr = 0.1  # fast`. What is *not* guaranteed: the replaced value's
//! own spelling (`1_000` becomes `1000`, `'single'` quotes become `"double"`).

use toml_edit::{DocumentMut, Item, Value};

use crate::document::{scalar_of, PathSeg, ScalarValue};

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum EditError {
    #[error("the document has syntax errors; fix them before editing values")]
    Parse,
    #[error("no value at {0}")]
    NotFound(String),
    #[error("{0} is not a single value")]
    NotScalar(String),
    #[error("{path} holds {found}; refusing to replace it with {wanted}")]
    TypeMismatch {
        path: String,
        found: &'static str,
        wanted: &'static str,
    },
}

fn describe(path: &[PathSeg]) -> String {
    let mut out = String::new();
    for seg in path {
        match seg {
            PathSeg::Key(k) if out.is_empty() => out.push_str(k),
            PathSeg::Key(k) => {
                out.push('.');
                out.push_str(k);
            }
            PathSeg::Index(i) => out.push_str(&format!("[{i}]")),
        }
    }
    out
}

fn type_name(v: &ScalarValue) -> &'static str {
    match v {
        ScalarValue::String(_) => "a string",
        ScalarValue::Integer(_) => "an integer",
        ScalarValue::Float(_) => "a float",
        ScalarValue::Boolean(_) => "a boolean",
        ScalarValue::Datetime(_) => "a datetime",
    }
}

/// Replace the scalar at `path` and return the new document text.
///
/// The new value must have the same type as the old one, except that an
/// integer may become a float (`lr = 1` → `lr = 0.5`). A datetime must be a
/// valid TOML datetime.
pub fn set_value(text: &str, path: &[PathSeg], value: &ScalarValue) -> Result<String, EditError> {
    let mut doc: DocumentMut = text.parse().map_err(|_| EditError::Parse)?;
    let where_ = describe(path);
    let mut item: &mut Item = doc.as_item_mut();
    for seg in path {
        let next = match seg {
            PathSeg::Key(k) => item.get_mut(k.as_str()),
            PathSeg::Index(i) => item.get_mut(*i),
        };
        // A missing key comes back as `Some(Item::None)`, not `None`.
        item = next
            .filter(|i| !i.is_none())
            .ok_or_else(|| EditError::NotFound(where_.clone()))?;
    }
    let target = item
        .as_value_mut()
        .ok_or_else(|| EditError::NotScalar(where_.clone()))?;
    let current = scalar_of(target).ok_or_else(|| EditError::NotScalar(where_.clone()))?;
    let compatible = std::mem::discriminant(&current) == std::mem::discriminant(value)
        || matches!(
            (&current, value),
            (ScalarValue::Integer(_), ScalarValue::Float(_))
        );
    if !compatible {
        return Err(EditError::TypeMismatch {
            path: where_,
            found: type_name(&current),
            wanted: type_name(value),
        });
    }
    let mut replacement = match value {
        ScalarValue::String(s) => Value::from(s.as_str()),
        ScalarValue::Integer(i) => Value::from(*i),
        ScalarValue::Float(f) => Value::from(*f),
        ScalarValue::Boolean(b) => Value::from(*b),
        ScalarValue::Datetime(d) => Value::from(d.parse::<toml_edit::Datetime>().map_err(
            |_| EditError::TypeMismatch {
                path: where_.clone(),
                found: "a datetime",
                wanted: "an invalid datetime",
            },
        )?),
    };
    *replacement.decor_mut() = target.decor().clone();
    *target = replacement;
    Ok(doc.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "# Training knobs\n[train]\nepochs = 30   # passes\nlr = 1\ndevice = \"auto\"  # auto | cpu | gpu\n\n[model]\nhidden = [16, 8]  # widths\n";

    fn key(k: &str) -> PathSeg {
        PathSeg::Key(k.into())
    }

    #[test]
    fn untouched_documents_round_trip_byte_for_byte() {
        let doc: DocumentMut = SRC.parse().unwrap();
        assert_eq!(doc.to_string(), SRC);
    }

    #[test]
    fn replaces_a_value_and_keeps_comments() {
        let out = set_value(
            SRC,
            &[key("train"), key("epochs")],
            &ScalarValue::Integer(50),
        )
        .unwrap();
        assert_eq!(
            out,
            SRC.replace("epochs = 30   # passes", "epochs = 50   # passes")
        );
    }

    #[test]
    fn edits_array_elements_and_widens_integers() {
        let out = set_value(
            SRC,
            &[key("model"), key("hidden"), PathSeg::Index(1)],
            &ScalarValue::Integer(4),
        )
        .unwrap();
        assert!(out.contains("hidden = [16, 4]  # widths"), "{out}");
        let out = set_value(SRC, &[key("train"), key("lr")], &ScalarValue::Float(0.5)).unwrap();
        assert!(out.contains("lr = 0.5\n"), "{out}");
    }

    #[test]
    fn refuses_type_changes_and_missing_paths() {
        let err = set_value(
            SRC,
            &[key("train"), key("device")],
            &ScalarValue::Integer(1),
        );
        assert!(matches!(err, Err(EditError::TypeMismatch { .. })));
        let err = set_value(SRC, &[key("train"), key("nope")], &ScalarValue::Integer(1));
        assert_eq!(err, Err(EditError::NotFound("train.nope".into())));
        let err = set_value(
            SRC,
            &[key("model"), key("hidden")],
            &ScalarValue::Integer(1),
        );
        assert!(matches!(err, Err(EditError::NotScalar(_))));
        assert_eq!(
            set_value("x = ", &[key("x")], &ScalarValue::Integer(1)),
            Err(EditError::Parse)
        );
    }
}
