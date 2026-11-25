use std::fmt::Write;

use serde_json::Value;

/// How to render JSON the way Python's `json.dumps` does.
#[derive(Clone, Copy, Debug, Default)]
pub struct Style {
    pub ascii: bool,
    pub sort_keys: bool,
    pub compact: bool,
}

/// `json.dumps(value)`: `", "` and `": "` separators with non-ASCII escaped.
#[must_use]
pub fn dumps(value: &Value) -> String {
    render(
        value,
        Style {
            ascii: true,
            ..Style::default()
        },
    )
}

/// `json.dumps(value, ensure_ascii=False)`.
#[must_use]
pub fn dumps_unicode(value: &Value) -> String {
    render(value, Style::default())
}

#[must_use]
pub fn render(value: &Value, style: Style) -> String {
    let mut out = String::new();
    write_value(&mut out, value, style);
    out
}

fn write_value(out: &mut String, value: &Value, style: Style) {
    let (item, pair) = if style.compact {
        (",", ":")
    } else {
        (", ", ": ")
    };
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(flag) => out.push_str(if *flag { "true" } else { "false" }),
        Value::Number(number) => out.push_str(&python_number(number)),
        Value::String(text) => write_string(out, text, style.ascii),
        Value::Array(items) => {
            out.push('[');
            for (index, element) in items.iter().enumerate() {
                if index > 0 {
                    out.push_str(item);
                }
                write_value(out, element, style);
            }
            out.push(']');
        }
        Value::Object(map) => {
            out.push('{');
            let mut entries: Vec<_> = map.iter().collect();
            if style.sort_keys {
                entries.sort_by(|left, right| left.0.cmp(right.0));
            }
            for (index, (key, element)) in entries.into_iter().enumerate() {
                if index > 0 {
                    out.push_str(item);
                }
                write_string(out, key, style.ascii);
                out.push_str(pair);
                write_value(out, element, style);
            }
            out.push('}');
        }
    }
}

fn python_number(number: &serde_json::Number) -> String {
    if number.is_f64() {
        let float = number.as_f64().unwrap_or_default();
        if float.is_finite() && float.fract() == 0.0 && float.abs() < 1e16 {
            return format!("{float:.1}");
        }
    }
    number.to_string()
}

fn write_string(out: &mut String, text: &str, ascii: bool) {
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if u32::from(c) < 0x20 || (ascii && !c.is_ascii()) => {
                let mut units = [0_u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    let _ = write!(out, "\\u{unit:04x}");
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn matches_python_defaults() {
        let value = json!({"b": [1, 2.0, "é😀"], "a": null, "c": {"x": true}});
        assert_eq!(
            dumps(&value),
            r#"{"b": [1, 2.0, "\u00e9\ud83d\ude00"], "a": null, "c": {"x": true}}"#
        );
        let sorted = render(
            &value,
            Style {
                ascii: false,
                sort_keys: true,
                compact: false,
            },
        );
        assert_eq!(
            sorted,
            r#"{"a": null, "b": [1, 2.0, "é😀"], "c": {"x": true}}"#
        );
    }
}
