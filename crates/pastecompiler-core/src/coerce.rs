//! Shape coercions between tables, trees, lists and text, plus flatten /
//! unflatten helpers for nested data.

use crate::model::{Diagnostics, Document, Scalar, Table};
use serde_json::{Map, Value};

/// Flatten a JSON value into a single-level map with dotted keys
/// (`address.city`, `tags.0`).
pub fn flatten_value(value: &Value) -> Map<String, Value> {
    let mut out = Map::new();
    fn walk(prefix: &str, v: &Value, out: &mut Map<String, Value>) {
        match v {
            Value::Object(o) if !o.is_empty() => {
                for (k, child) in o {
                    let key = if prefix.is_empty() {
                        k.clone()
                    } else {
                        format!("{prefix}.{k}")
                    };
                    walk(&key, child, out);
                }
            }
            Value::Array(a) if !a.is_empty() => {
                for (i, child) in a.iter().enumerate() {
                    let key = if prefix.is_empty() {
                        i.to_string()
                    } else {
                        format!("{prefix}.{i}")
                    };
                    walk(&key, child, out);
                }
            }
            other => {
                out.insert(prefix.to_string(), other.clone());
            }
        }
    }
    walk("", value, &mut out);
    out
}

/// Reverse of [`flatten_value`]: dotted keys become nested objects and
/// numeric path segments that form a dense 0..n range become arrays.
pub fn unflatten_map(map: &Map<String, Value>) -> Value {
    let mut root = Value::Object(Map::new());
    for (key, v) in map {
        let parts: Vec<&str> = key.split('.').collect();
        insert_path(&mut root, &parts, v.clone());
    }
    arrayify(root)
}

fn insert_path(target: &mut Value, parts: &[&str], value: Value) {
    if parts.is_empty() {
        *target = value;
        return;
    }
    if !target.is_object() {
        *target = Value::Object(Map::new());
    }
    let obj = target.as_object_mut().expect("object");
    let entry = obj.entry(parts[0].to_string()).or_insert(Value::Null);
    if parts.len() == 1 {
        *entry = value;
    } else {
        insert_path(entry, &parts[1..], value);
    }
}

fn arrayify(v: Value) -> Value {
    match v {
        Value::Object(o) => {
            let converted: Map<String, Value> =
                o.into_iter().map(|(k, v)| (k, arrayify(v))).collect();
            let is_dense_array = !converted.is_empty()
                && converted
                    .keys()
                    .enumerate()
                    .all(|(i, k)| k.parse::<usize>().map(|n| n == i).unwrap_or(false));
            if is_dense_array {
                Value::Array(converted.into_iter().map(|(_, v)| v).collect())
            } else {
                Value::Object(converted)
            }
        }
        other => other,
    }
}

fn row_from_object(
    obj: &Map<String, Value>,
    headers: &[String],
    diag: &mut Diagnostics,
) -> Vec<Scalar> {
    let flat = flatten_value(&Value::Object(obj.clone()));
    headers
        .iter()
        .map(|h| match flat.get(h) {
            Some(v) => {
                let (s, ok) = Scalar::from_json(v);
                if !ok {
                    diag.warn_t("nested_stringified", &[&h]);
                }
                s
            }
            None => Scalar::Null,
        })
        .collect()
}

fn union_headers(items: &[&Map<String, Value>]) -> Vec<String> {
    let mut headers: Vec<String> = Vec::new();
    for obj in items {
        for key in flatten_value(&Value::Object((*obj).clone())).keys() {
            if !headers.contains(key) {
                headers.push(key.clone());
            }
        }
    }
    headers
}

fn array_to_table(items: &[Value], diag: &mut Diagnostics) -> Table {
    if items.is_empty() {
        return Table::new(vec![], vec![]);
    }
    let objects: Vec<&Map<String, Value>> = items.iter().filter_map(|v| v.as_object()).collect();
    if objects.len() == items.len() {
        let headers = union_headers(&objects);
        let nested = objects
            .iter()
            .any(|o| o.values().any(|v| v.is_object() || v.is_array()));
        if nested {
            diag.warn_t("flattened", &[]);
        }
        let key_counts: Vec<usize> = objects
            .iter()
            .map(|o| flatten_value(&Value::Object((*o).clone())).len())
            .collect();
        if key_counts.iter().any(|c| *c != headers.len()) {
            diag.info_t("sparse_keys", &[]);
        }
        let rows = objects
            .iter()
            .map(|o| row_from_object(o, &headers, diag))
            .collect();
        return Table::new(headers, rows);
    }
    let arrays: Vec<&Vec<Value>> = items.iter().filter_map(|v| v.as_array()).collect();
    if arrays.len() == items.len() {
        let rows = arrays
            .iter()
            .map(|a| {
                a.iter()
                    .map(|v| {
                        let (s, ok) = Scalar::from_json(v);
                        if !ok {
                            diag.warn_t("nested_stringified_rows", &[]);
                        }
                        s
                    })
                    .collect()
            })
            .collect();
        let mut t = Table::without_header(rows);
        for note in t.normalize() {
            diag.warn("table_normalized", note);
        }
        return t;
    }
    if items.iter().all(|v| !v.is_object() && !v.is_array()) {
        let rows = items.iter().map(|v| vec![Scalar::from_json(v).0]).collect();
        return Table::new(vec!["value".to_string()], rows);
    }
    diag.warn_t("mixed_array", &[]);
    let mut normalized: Vec<Map<String, Value>> = Vec::new();
    for item in items {
        match item {
            Value::Object(o) => normalized.push(o.clone()),
            other => {
                let mut m = Map::new();
                m.insert("value".to_string(), other.clone());
                normalized.push(m);
            }
        }
    }
    let refs: Vec<&Map<String, Value>> = normalized.iter().collect();
    let headers = union_headers(&refs);
    let rows = refs
        .iter()
        .map(|o| row_from_object(o, &headers, diag))
        .collect();
    Table::new(headers, rows)
}

fn object_to_table(obj: &Map<String, Value>, diag: &mut Diagnostics) -> Table {
    if obj.is_empty() {
        return Table::new(vec![], vec![]);
    }
    // API envelope: a single key holding an array of records.
    if obj.len() == 1 {
        if let Some((key, Value::Array(items))) = obj.iter().next() {
            if !items.is_empty() && items.iter().all(|v| v.is_object()) {
                diag.info_t("envelope_unwrapped", &[&key]);
                return array_to_table(items, diag);
            }
        }
    }
    // Map of records: {"id1": {...}, "id2": {...}}
    if obj.values().all(|v| v.is_object()) {
        let mut items: Vec<Value> = Vec::new();
        for (k, v) in obj {
            let mut m = Map::new();
            m.insert("key".to_string(), Value::String(k.clone()));
            if let Value::Object(inner) = v {
                for (ik, iv) in inner {
                    m.insert(ik.clone(), iv.clone());
                }
            }
            items.push(Value::Object(m));
        }
        diag.info_t("map_of_records", &[]);
        return array_to_table(&items, diag);
    }
    // Plain key/value object -> two columns.
    let flat = flatten_value(&Value::Object(obj.clone()));
    if flat.len() != obj.len() {
        diag.warn_t("flattened_keys", &[]);
    }
    let rows = flat
        .iter()
        .map(|(k, v)| vec![Scalar::Str(k.clone()), Scalar::from_json(v).0])
        .collect();
    Table::new(vec!["key".to_string(), "value".to_string()], rows)
}

/// Convert any document into a table.
pub fn to_table(doc: &Document, diag: &mut Diagnostics) -> Table {
    match doc {
        Document::Table(t) => t.clone(),
        Document::Tree { value } => match value {
            Value::Array(items) => array_to_table(items, diag),
            Value::Object(obj) => object_to_table(obj, diag),
            scalar => Table::new(
                vec!["value".to_string()],
                vec![vec![Scalar::from_json(scalar).0]],
            ),
        },
        Document::List { items } => Table::new(
            vec!["item".to_string()],
            items.iter().map(|i| vec![Scalar::Str(i.clone())]).collect(),
        ),
        Document::Text { text } => {
            diag.info_t("text_as_rows", &[]);
            Table::new(
                vec!["line".to_string()],
                text.lines()
                    .map(|l| vec![Scalar::Str(l.to_string())])
                    .collect(),
            )
        }
    }
}

/// Convert a table into a JSON value: an array of objects when it has a header,
/// otherwise an array of arrays.
pub fn table_to_value(table: &Table) -> Value {
    if table.has_header {
        Value::Array(
            table
                .rows
                .iter()
                .map(|row| {
                    let mut obj = Map::new();
                    for (i, h) in table.headers.iter().enumerate() {
                        obj.insert(
                            h.clone(),
                            row.get(i).map(|c| c.to_json()).unwrap_or(Value::Null),
                        );
                    }
                    Value::Object(obj)
                })
                .collect(),
        )
    } else {
        Value::Array(
            table
                .rows
                .iter()
                .map(|row| Value::Array(row.iter().map(|c| c.to_json()).collect()))
                .collect(),
        )
    }
}

/// Convert any document into a JSON tree.
pub fn to_tree(doc: &Document, diag: &mut Diagnostics) -> Value {
    match doc {
        Document::Table(t) => {
            if t.headers.iter().any(|h| h.contains('.')) {
                diag.info_t("dotted_headers", &[]);
            }
            table_to_value(t)
        }
        Document::Tree { value } => value.clone(),
        Document::List { items } => {
            Value::Array(items.iter().map(|i| Value::String(i.clone())).collect())
        }
        Document::Text { text } => {
            if text.contains('\n') {
                diag.info_t("text_as_items", &[]);
                Value::Array(text.lines().map(|l| Value::String(l.to_string())).collect())
            } else {
                Value::String(text.clone())
            }
        }
    }
}

/// Convert any document into a list of strings.
pub fn to_list(doc: &Document, diag: &mut Diagnostics) -> Vec<String> {
    match doc {
        Document::Table(t) => {
            if t.width() > 1 {
                diag.info_t("rows_joined", &[]);
            }
            t.rows
                .iter()
                .map(|r| {
                    r.iter()
                        .map(|c| c.as_text())
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .collect()
        }
        Document::Tree { value } => match value {
            Value::Array(items) => items.iter().map(value_to_line).collect(),
            Value::Object(obj) => obj
                .iter()
                .map(|(k, v)| format!("{k}: {}", value_to_line(v)))
                .collect(),
            scalar => vec![value_to_line(scalar)],
        },
        Document::List { items } => items.clone(),
        Document::Text { text } => text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.to_string())
            .collect(),
    }
}

pub fn value_to_line(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        Value::Number(_) | Value::Bool(_) => v.to_string(),
        other => other.to_string(),
    }
}

/// Convert any document into plain text.
pub fn to_text(doc: &Document) -> String {
    match doc {
        Document::Table(t) => {
            let mut lines = Vec::new();
            if t.has_header {
                lines.push(t.headers.join("\t"));
            }
            for row in &t.rows {
                lines.push(
                    row.iter()
                        .map(|c| c.as_text())
                        .collect::<Vec<_>>()
                        .join("\t"),
                );
            }
            lines.join("\n")
        }
        Document::Tree { value } => match value {
            Value::String(s) => s.clone(),
            other => serde_json::to_string_pretty(other).unwrap_or_default(),
        },
        Document::List { items } => items.join("\n"),
        Document::Text { text } => text.clone(),
    }
}

/// Transpose a table (rows become columns). The header becomes the first column.
pub fn transpose(table: &Table) -> Table {
    let mut rows: Vec<Vec<Scalar>> = Vec::new();
    for (c, header) in table.headers.iter().enumerate() {
        let mut row = Vec::with_capacity(table.rows.len() + 1);
        if table.has_header {
            row.push(Scalar::Str(header.clone()));
        }
        for r in &table.rows {
            row.push(r.get(c).cloned().unwrap_or(Scalar::Null));
        }
        rows.push(row);
    }
    let mut t = Table::without_header(rows);
    if table.has_header {
        t.headers[0] = "field".to_string();
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn flatten_and_unflatten_round_trip() {
        let v = json!({"a": 1, "b": {"c": [1, 2], "d": {"e": "x"}}, "f": []});
        let flat = flatten_value(&v);
        assert_eq!(flat.get("b.c.0"), Some(&json!(1)));
        assert_eq!(flat.get("b.d.e"), Some(&json!("x")));
        assert_eq!(flat.get("f"), Some(&json!([])));
        let back = unflatten_map(&flat);
        assert_eq!(back, v);
    }

    #[test]
    fn array_of_objects_to_table_with_union_headers() {
        let mut d = Diagnostics::new();
        let doc = Document::tree(json!([{"a": 1, "b": "x"}, {"a": 2, "c": true}]));
        let t = to_table(&doc, &mut d);
        assert_eq!(t.headers, vec!["a", "b", "c"]);
        assert_eq!(
            t.rows[1],
            vec![Scalar::Int(2), Scalar::Null, Scalar::Bool(true)]
        );
        assert!(d.issues.iter().any(|i| i.code == "sparse_keys"));
    }

    #[test]
    fn nested_objects_flatten() {
        let mut d = Diagnostics::new();
        let doc = Document::tree(
            json!([{"name": "Ada", "address": {"city": "London"}, "tags": ["a", "b"]}]),
        );
        let t = to_table(&doc, &mut d);
        assert_eq!(t.headers, vec!["name", "address.city", "tags.0", "tags.1"]);
        assert!(d.issues.iter().any(|i| i.code == "flattened"));
    }

    #[test]
    fn object_shapes() {
        let mut d = Diagnostics::new();
        let t = to_table(&Document::tree(json!({"a": 1, "b": "x"})), &mut d);
        assert_eq!(t.headers, vec!["key", "value"]);
        assert_eq!(t.rows.len(), 2);

        let t = to_table(
            &Document::tree(json!({"data": [{"id": 1}, {"id": 2}]})),
            &mut d,
        );
        assert_eq!(t.headers, vec!["id"]);
        assert_eq!(t.rows.len(), 2);

        let t = to_table(
            &Document::tree(json!({"u1": {"n": 1}, "u2": {"n": 2}})),
            &mut d,
        );
        assert_eq!(t.headers, vec!["key", "n"]);
        assert_eq!(t.rows[0][0], Scalar::Str("u1".into()));
    }

    #[test]
    fn scalars_and_arrays_of_arrays() {
        let mut d = Diagnostics::new();
        let t = to_table(&Document::tree(json!([1, 2, 3])), &mut d);
        assert_eq!(t.headers, vec!["value"]);
        let t = to_table(&Document::tree(json!([[1, 2], [3, 4]])), &mut d);
        assert!(!t.has_header);
        assert_eq!(t.rows.len(), 2);
    }

    #[test]
    fn table_to_tree_and_back() {
        let table = Table::new(
            vec!["a".into(), "b".into()],
            vec![vec![Scalar::Int(1), Scalar::Str("x".into())]],
        );
        let v = to_tree(&Document::Table(table.clone()), &mut Diagnostics::new());
        assert_eq!(v, json!([{"a": 1, "b": "x"}]));
        let t2 = to_table(&Document::tree(v), &mut Diagnostics::new());
        assert_eq!(t2, table);
    }

    #[test]
    fn lists_and_text() {
        let mut d = Diagnostics::new();
        assert_eq!(to_list(&Document::text("a\n\nb"), &mut d), vec!["a", "b"]);
        assert_eq!(to_tree(&Document::text("a\nb"), &mut d), json!(["a", "b"]));
        assert_eq!(to_tree(&Document::text("solo"), &mut d), json!("solo"));
        assert_eq!(
            to_list(&Document::tree(json!({"k": 1})), &mut d),
            vec!["k: 1"]
        );
        let t = to_table(&Document::list(vec!["x".into()]), &mut d);
        assert_eq!(t.headers, vec!["item"]);
    }

    #[test]
    fn transpose_table() {
        let table = Table::new(
            vec!["a".into(), "b".into()],
            vec![
                vec![Scalar::Int(1), Scalar::Int(2)],
                vec![Scalar::Int(3), Scalar::Int(4)],
            ],
        );
        let t = transpose(&table);
        assert_eq!(t.rows.len(), 2);
        assert_eq!(
            t.rows[0],
            vec![Scalar::Str("a".into()), Scalar::Int(1), Scalar::Int(3)]
        );
        assert_eq!(t.headers[0], "field");
    }
}
