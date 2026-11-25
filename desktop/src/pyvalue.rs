use serde_json::Value;

/// Python truthiness of a decoded JSON value.
#[must_use]
pub fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(flag)) => *flag,
        Some(Value::Number(number)) => number.as_f64().is_some_and(|number| number != 0.0),
        Some(Value::String(text)) => !text.is_empty(),
        Some(Value::Array(items)) => !items.is_empty(),
        Some(Value::Object(map)) => !map.is_empty(),
    }
}

/// Python `==` between decoded JSON values, where `1 == 1.0` and a missing key is `None`.
#[must_use]
pub fn same(left: Option<&Value>, right: Option<&Value>) -> bool {
    let left = left.unwrap_or(&Value::Null);
    let right = right.unwrap_or(&Value::Null);
    match (left, right) {
        (Value::Number(a), Value::Number(b)) => a.as_f64() == b.as_f64(),
        (Value::Bool(a), Value::Number(b)) | (Value::Number(b), Value::Bool(a)) => {
            b.as_f64() == Some(f64::from(u8::from(*a)))
        }
        _ => left == right,
    }
}

/// A key of a nested object, treating a non-object as having no keys.
#[must_use]
pub fn field<'a>(value: Option<&'a Value>, key: &str) -> Option<&'a Value> {
    value.and_then(|value| value.get(key))
}

/// Python `str()` of a decoded JSON value.
#[must_use]
pub fn py_str(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => "None".to_owned(),
        Some(Value::Bool(true)) => "True".to_owned(),
        Some(Value::Bool(false)) => "False".to_owned(),
        Some(Value::String(text)) => text.clone(),
        Some(other) => other.to_string(),
    }
}
