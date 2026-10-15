//! The JSON of the game data tables: the text `mod asset` writes them as ([`json_text`], laid out
//! for a person to edit), and, for the tables a mod gives back, members of the shape they should
//! have, each wrong shape an error naming the member. People edit these files by hand, so
//! `"groups": {...}` where a list belongs must say so rather than change nothing.

use std::fmt::Display;

use serde_json::{Map, Value};

use crate::{bail, Result};

/// A value that should be an object: its members, or an error saying what it should be.
pub fn object(value: &Value, what: impl Display) -> Result<&Map<String, Value>> {
    match value.as_object() {
        Some(object) => Ok(object),
        None => bail!("{what} is an object, not {}", shape(value)),
    }
}

/// A member that may be left out and should otherwise be a list: its items (none when absent),
/// or an error naming it.
pub fn list<'a>(object: &'a Map<String, Value>, key: &str, what: impl Display) -> Result<&'a [Value]> {
    match object.get(key) {
        None => Ok(&[]),
        Some(Value::Array(items)) => Ok(items),
        Some(other) => bail!("{what}: \"{key}\" is a list, not {}", shape(other)),
    }
}

/// An object's member, null when absent (as indexing a Value gives; a Map's index would panic).
pub fn member<'a>(object: &'a Map<String, Value>, key: &str) -> &'a Value {
    object.get(key).unwrap_or(&Value::Null)
}

/// What a JSON value is, for messages: `a string ("x")`, `an object`.
pub fn shape(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(flag) => format!("{flag}"),
        Value::Number(number) => format!("the number {number}"),
        Value::String(text) => format!("a string ({text:?})"),
        Value::Array(_) => "a list".to_string(),
        Value::Object(_) => "an object".to_string(),
    }
}

/// The table as text: objects a field a line, lists of numbers on one line, as a person edits it.
pub fn json_text(value: &Value) -> String {
    let mut out = String::new();
    write_value(value, 0, &mut out);
    out.push('\n');
    out
}

fn write_value(value: &Value, indent: usize, out: &mut String) {
    let pad = |depth: usize| "  ".repeat(depth);
    match value {
        // a small entry of a list ({"item": 210, "count": 5}) on one line
        Value::Object(map)
            if indent > 2 && map.len() <= 3 && map.values().all(|item| !item.is_object() && !item.is_array()) =>
        {
            let members: Vec<String> = map
                .iter()
                .map(|(key, item)| {
                    format!("{}: {}", serde_json::to_string(key).unwrap(), serde_json::to_string(item).unwrap())
                })
                .collect();
            out.push_str(&format!("{{{}}}", members.join(", ")));
        }
        Value::Object(map) if !map.is_empty() => {
            out.push_str("{\n");
            for (number, (key, item)) in map.iter().enumerate() {
                out.push_str(&pad(indent + 1));
                out.push_str(&serde_json::to_string(key).unwrap());
                out.push_str(": ");
                write_value(item, indent + 1, out);
                if number + 1 < map.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push_str(&pad(indent));
            out.push('}');
        }
        // rows of text, one a line (a dungeon layout's cells, dungeon_layout)
        Value::Array(list)
            if list.iter().any(|item| item.is_object() || item.is_array())
                || (list.len() > 1 && list.iter().all(|item| item.as_str().is_some_and(|text| text.contains(' ')))) =>
        {
            out.push_str("[\n");
            for (number, item) in list.iter().enumerate() {
                out.push_str(&pad(indent + 1));
                write_value(item, indent + 1, out);
                if number + 1 < list.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push_str(&pad(indent));
            out.push(']');
        }
        Value::Array(list) => {
            let items: Vec<String> = list.iter().map(|item| serde_json::to_string(item).unwrap()).collect();
            out.push('[');
            out.push_str(&items.join(", "));
            out.push(']');
        }
        other => out.push_str(&serde_json::to_string(other).unwrap()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_member_of_the_wrong_shape_is_an_error_naming_it() {
        let record = json!({"groups": {"group": 1}, "floors": [1, 2]});
        let record = object(&record, "a dungeon").unwrap();
        assert_eq!(list(record, "floors", "dungeon 0").unwrap().len(), 2);
        assert!(list(record, "absent", "dungeon 0").unwrap().is_empty());
        let error = list(record, "groups", "dungeon 0").unwrap_err().to_string();
        assert_eq!(error, "dungeon 0: \"groups\" is a list, not an object");
        assert_eq!(object(&json!(3), "a dungeon").unwrap_err().to_string(), "a dungeon is an object, not the number 3");
    }
}
