//! Intermediate representation shared by parsers, transforms and writers.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// A single typed cell value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Scalar {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
}

/// Render a float so that whole numbers keep a `.0` suffix and everything else
/// uses the shortest representation that round-trips.
pub fn format_float(f: f64) -> String {
    if f.is_finite() && f.fract() == 0.0 && f.abs() < 1e16 {
        format!("{:.1}", f)
    } else {
        format!("{}", f)
    }
}

impl Scalar {
    /// Infer a typed scalar from text. Inference is conservative: a value is
    /// only turned into a number when rendering it back produces exactly the
    /// same text, so `007`, `3.10`, `+5` and `1e5` stay strings.
    pub fn infer(raw: &str) -> Scalar {
        let t = raw.trim();
        if t.is_empty() {
            return Scalar::Null;
        }
        match t {
            "true" | "TRUE" | "True" => return Scalar::Bool(true),
            "false" | "FALSE" | "False" => return Scalar::Bool(false),
            "null" | "NULL" | "Null" => return Scalar::Null,
            _ => {}
        }
        if let Ok(i) = t.parse::<i64>() {
            if i.to_string() == t {
                return Scalar::Int(i);
            }
        }
        if let Ok(f) = t.parse::<f64>() {
            if f.is_finite() && format_float(f) == t {
                return Scalar::Float(f);
            }
        }
        Scalar::Str(raw.to_string())
    }

    /// Text form used for CSV / Markdown / plain text output. `Null` becomes "".
    pub fn as_text(&self) -> String {
        match self {
            Scalar::Null => String::new(),
            Scalar::Bool(b) => b.to_string(),
            Scalar::Int(i) => i.to_string(),
            Scalar::Float(f) => format_float(*f),
            Scalar::Str(s) => s.clone(),
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Scalar::Null)
    }

    pub fn is_numeric(&self) -> bool {
        matches!(self, Scalar::Int(_) | Scalar::Float(_))
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Scalar::Int(i) => Some(*i as f64),
            Scalar::Float(f) => Some(*f),
            Scalar::Str(s) => s.trim().replace(',', ".").parse::<f64>().ok(),
            Scalar::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            Scalar::Null => None,
        }
    }

    pub fn to_json(&self) -> Value {
        match self {
            Scalar::Null => Value::Null,
            Scalar::Bool(b) => Value::Bool(*b),
            Scalar::Int(i) => Value::from(*i),
            Scalar::Float(f) => serde_json::Number::from_f64(*f)
                .map(Value::Number)
                .unwrap_or(Value::Null),
            Scalar::Str(s) => Value::String(s.clone()),
        }
    }

    /// Convert a JSON value into a scalar; complex values become compact JSON
    /// strings and `Some(false)` is returned in the tuple's second slot to let
    /// callers know a structure was stringified.
    pub fn from_json(v: &Value) -> (Scalar, bool) {
        match v {
            Value::Null => (Scalar::Null, true),
            Value::Bool(b) => (Scalar::Bool(*b), true),
            Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    (Scalar::Int(i), true)
                } else {
                    (Scalar::Float(n.as_f64().unwrap_or(0.0)), true)
                }
            }
            Value::String(s) => (Scalar::Str(s.clone()), true),
            other => (Scalar::Str(other.to_string()), false),
        }
    }

    /// Name of the scalar type, used for column type analysis.
    pub fn type_name(&self) -> &'static str {
        match self {
            Scalar::Null => "null",
            Scalar::Bool(_) => "boolean",
            Scalar::Int(_) => "integer",
            Scalar::Float(_) => "float",
            Scalar::Str(_) => "string",
        }
    }
}

impl From<&str> for Scalar {
    fn from(s: &str) -> Self {
        Scalar::Str(s.to_string())
    }
}

impl From<String> for Scalar {
    fn from(s: String) -> Self {
        Scalar::Str(s)
    }
}

/// A rectangular table. `headers` always has one entry per column; when the
/// source had no header row the names are generated (`column_1`, ...) and
/// `has_header` is false.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Table {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<Scalar>>,
    pub has_header: bool,
}

pub fn generated_header(index: usize) -> String {
    format!("column_{}", index + 1)
}

impl Table {
    pub fn new(headers: Vec<String>, rows: Vec<Vec<Scalar>>) -> Table {
        Table {
            headers,
            rows,
            has_header: true,
        }
    }

    pub fn without_header(rows: Vec<Vec<Scalar>>) -> Table {
        let width = rows.iter().map(|r| r.len()).max().unwrap_or(0);
        Table {
            headers: (0..width).map(generated_header).collect(),
            rows,
            has_header: false,
        }
    }

    pub fn width(&self) -> usize {
        self.headers.len()
    }

    pub fn column_index(&self, name: &str) -> Option<usize> {
        if let Some(i) = self.headers.iter().position(|h| h == name) {
            return Some(i);
        }
        // Case-insensitive fallback, then 1-based positional (`column_2`, `2`).
        let lower = name.to_lowercase();
        if let Some(i) = self.headers.iter().position(|h| h.to_lowercase() == lower) {
            return Some(i);
        }
        if let Ok(n) = name.parse::<usize>() {
            if n >= 1 && n <= self.headers.len() {
                return Some(n - 1);
            }
        }
        None
    }

    pub fn column(&self, index: usize) -> Vec<&Scalar> {
        self.rows.iter().filter_map(|r| r.get(index)).collect()
    }

    /// Pad ragged rows, name empty headers and de-duplicate header names.
    /// Returns a list of human readable adjustments that were made.
    pub fn normalize(&mut self) -> Vec<String> {
        let mut notes = Vec::new();
        let width = self
            .headers
            .len()
            .max(self.rows.iter().map(|r| r.len()).max().unwrap_or(0));
        while self.headers.len() < width {
            let idx = self.headers.len();
            self.headers.push(generated_header(idx));
        }
        let mut ragged = 0usize;
        for row in &mut self.rows {
            if row.len() != width {
                ragged += 1;
                row.resize(width, Scalar::Null);
            }
        }
        if ragged > 0 {
            notes.push(crate::i18n::t("ragged_rows", &[&ragged]));
        }
        let mut seen: HashMap<String, usize> = HashMap::new();
        let mut renamed = Vec::new();
        for (i, h) in self.headers.iter_mut().enumerate() {
            let trimmed = h.trim().to_string();
            if trimmed.is_empty() {
                *h = generated_header(i);
                renamed.push(crate::i18n::t("empty_header_renamed", &[&(i + 1), h]));
            } else {
                *h = trimmed;
            }
            let count = seen.entry(h.clone()).or_insert(0);
            *count += 1;
            if *count > 1 {
                let new_name = format!("{}_{}", h, count);
                renamed.push(crate::i18n::t("duplicate_header_renamed", &[h, &new_name]));
                *h = new_name;
            }
        }
        if !renamed.is_empty() {
            notes.push(crate::i18n::t("headers_adjusted", &[&renamed.join(", ")]));
        }
        notes
    }

    /// Infer the dominant type of each column (ignoring nulls).
    pub fn column_types(&self) -> Vec<&'static str> {
        (0..self.width())
            .map(|c| {
                let mut counts: HashMap<&'static str, usize> = HashMap::new();
                for row in &self.rows {
                    if let Some(cell) = row.get(c) {
                        if !cell.is_null() {
                            *counts.entry(cell.type_name()).or_insert(0) += 1;
                        }
                    }
                }
                if counts.is_empty() {
                    return "null";
                }
                let has_float = counts.contains_key("float");
                let has_int = counts.contains_key("integer");
                let has_str = counts.contains_key("string");
                let has_bool = counts.contains_key("boolean");
                if has_str || (has_bool && (has_int || has_float)) {
                    "string"
                } else if has_float {
                    "float"
                } else if has_int {
                    "integer"
                } else if has_bool {
                    "boolean"
                } else {
                    "string"
                }
            })
            .collect()
    }

    /// Columns whose non-null cells mix several types (e.g. numbers and text).
    pub fn mixed_type_columns(&self) -> Vec<(String, Vec<&'static str>)> {
        let mut out = Vec::new();
        for c in 0..self.width() {
            let mut kinds: Vec<&'static str> = Vec::new();
            for row in &self.rows {
                if let Some(cell) = row.get(c) {
                    if !cell.is_null() && !kinds.contains(&cell.type_name()) {
                        kinds.push(cell.type_name());
                    }
                }
            }
            let numeric_only = kinds.iter().all(|k| *k == "integer" || *k == "float");
            if kinds.len() > 1 && !numeric_only {
                out.push((self.headers[c].clone(), kinds));
            }
        }
        out
    }
}

/// The shape of a parsed document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Document {
    Table(Table),
    Tree { value: Value },
    List { items: Vec<String> },
    Text { text: String },
}

impl Document {
    pub fn tree(value: Value) -> Document {
        Document::Tree { value }
    }
    pub fn list(items: Vec<String>) -> Document {
        Document::List { items }
    }
    pub fn text(text: impl Into<String>) -> Document {
        Document::Text { text: text.into() }
    }

    pub fn kind(&self) -> DocKind {
        match self {
            Document::Table(_) => DocKind::Table,
            Document::Tree { .. } => DocKind::Tree,
            Document::List { .. } => DocKind::List,
            Document::Text { .. } => DocKind::Text,
        }
    }

    pub fn summary(&self) -> DocSummary {
        match self {
            Document::Table(t) => DocSummary {
                kind: DocKind::Table,
                rows: Some(t.rows.len()),
                columns: Some(t.width()),
                headers: Some(t.headers.clone()),
                has_header: Some(t.has_header),
                items: None,
                tree_kind: None,
                column_types: Some(t.column_types().iter().map(|s| s.to_string()).collect()),
            },
            Document::Tree { value } => {
                let (tree_kind, items) = match value {
                    Value::Array(a) => ("array", Some(a.len())),
                    Value::Object(o) => ("object", Some(o.len())),
                    _ => ("scalar", None),
                };
                DocSummary {
                    kind: DocKind::Tree,
                    rows: None,
                    columns: None,
                    headers: None,
                    has_header: None,
                    items,
                    tree_kind: Some(tree_kind.to_string()),
                    column_types: None,
                }
            }
            Document::List { items } => DocSummary {
                kind: DocKind::List,
                rows: None,
                columns: None,
                headers: None,
                has_header: None,
                items: Some(items.len()),
                tree_kind: None,
                column_types: None,
            },
            Document::Text { text } => DocSummary {
                kind: DocKind::Text,
                rows: None,
                columns: None,
                headers: None,
                has_header: None,
                items: Some(text.lines().count()),
                tree_kind: None,
                column_types: None,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum DocKind {
    Table,
    Tree,
    List,
    Text,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocSummary {
    pub kind: DocKind,
    pub rows: Option<usize>,
    pub columns: Option<usize>,
    pub headers: Option<Vec<String>>,
    pub has_header: Option<bool>,
    pub items: Option<usize>,
    pub tree_kind: Option<String>,
    pub column_types: Option<Vec<String>>,
}

/// Input formats that can be detected and parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputFormat {
    Json,
    JsonLines,
    Csv,
    Tsv,
    MarkdownTable,
    HtmlTable,
    Yaml,
    Toml,
    Xml,
    KeyValue,
    List,
    Text,
}

impl InputFormat {
    pub fn all() -> &'static [InputFormat] {
        use InputFormat::*;
        &[
            Json,
            JsonLines,
            Csv,
            Tsv,
            MarkdownTable,
            HtmlTable,
            Yaml,
            Toml,
            Xml,
            KeyValue,
            List,
            Text,
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            InputFormat::Json => "JSON",
            InputFormat::JsonLines => "JSON Lines",
            InputFormat::Csv => "CSV",
            InputFormat::Tsv => "TSV (Excel)",
            InputFormat::MarkdownTable => "Markdown table",
            InputFormat::HtmlTable => "HTML table",
            InputFormat::Yaml => "YAML",
            InputFormat::Toml => "TOML",
            InputFormat::Xml => "XML",
            InputFormat::KeyValue => "Key-value pairs",
            InputFormat::List => "List",
            InputFormat::Text => "Plain text",
        }
    }

    pub fn label_tr(&self) -> &'static str {
        match self {
            InputFormat::Json => "JSON",
            InputFormat::JsonLines => "JSON Lines",
            InputFormat::Csv => "CSV",
            InputFormat::Tsv => "TSV (Excel)",
            InputFormat::MarkdownTable => "Markdown tablosu",
            InputFormat::HtmlTable => "HTML tablosu",
            InputFormat::Yaml => "YAML",
            InputFormat::Toml => "TOML",
            InputFormat::Xml => "XML",
            InputFormat::KeyValue => "Anahtar-değer çiftleri",
            InputFormat::List => "Liste",
            InputFormat::Text => "Düz metin",
        }
    }

    pub fn label_in(&self, lang: crate::i18n::Lang) -> &'static str {
        match lang {
            crate::i18n::Lang::En => self.label(),
            crate::i18n::Lang::Tr => self.label_tr(),
        }
    }

    /// Label in the current thread's language.
    pub fn label_now(&self) -> &'static str {
        self.label_in(crate::i18n::Lang::current())
    }

    pub fn id(&self) -> String {
        serde_json::to_value(self)
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_default()
    }

    pub fn parse_id(id: &str) -> Option<InputFormat> {
        serde_json::from_value(Value::String(id.trim().to_lowercase().replace('-', "_"))).ok()
    }
}

/// Output formats the writer can produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    Json,
    JsonCompact,
    JsonLines,
    Csv,
    Tsv,
    MarkdownTable,
    HtmlTable,
    Yaml,
    Toml,
    Xml,
    SqlInsert,
    MarkdownList,
    NumberedList,
    PlainLines,
    KeyValue,
    Template,
    Text,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputCategory {
    Tabular,
    Tree,
    List,
    Text,
}

impl OutputFormat {
    pub fn all() -> &'static [OutputFormat] {
        use OutputFormat::*;
        &[
            Json,
            JsonCompact,
            JsonLines,
            Csv,
            Tsv,
            MarkdownTable,
            HtmlTable,
            Yaml,
            Toml,
            Xml,
            SqlInsert,
            MarkdownList,
            NumberedList,
            PlainLines,
            KeyValue,
            Template,
            Text,
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            OutputFormat::Json => "JSON",
            OutputFormat::JsonCompact => "JSON (compact)",
            OutputFormat::JsonLines => "JSON Lines",
            OutputFormat::Csv => "CSV",
            OutputFormat::Tsv => "TSV (Excel)",
            OutputFormat::MarkdownTable => "Markdown table",
            OutputFormat::HtmlTable => "HTML table",
            OutputFormat::Yaml => "YAML",
            OutputFormat::Toml => "TOML",
            OutputFormat::Xml => "XML",
            OutputFormat::SqlInsert => "SQL INSERT",
            OutputFormat::MarkdownList => "Markdown list",
            OutputFormat::NumberedList => "Numbered list",
            OutputFormat::PlainLines => "Plain lines",
            OutputFormat::KeyValue => "Key-value pairs",
            OutputFormat::Template => "Custom template",
            OutputFormat::Text => "Plain text",
        }
    }

    pub fn label_tr(&self) -> &'static str {
        match self {
            OutputFormat::Json => "JSON",
            OutputFormat::JsonCompact => "JSON (sıkışık)",
            OutputFormat::JsonLines => "JSON Lines",
            OutputFormat::Csv => "CSV",
            OutputFormat::Tsv => "TSV (Excel)",
            OutputFormat::MarkdownTable => "Markdown tablosu",
            OutputFormat::HtmlTable => "HTML tablosu",
            OutputFormat::Yaml => "YAML",
            OutputFormat::Toml => "TOML",
            OutputFormat::Xml => "XML",
            OutputFormat::SqlInsert => "SQL INSERT",
            OutputFormat::MarkdownList => "Markdown listesi",
            OutputFormat::NumberedList => "Numaralı liste",
            OutputFormat::PlainLines => "Düz satırlar",
            OutputFormat::KeyValue => "Anahtar-değer çiftleri",
            OutputFormat::Template => "Özel şablon",
            OutputFormat::Text => "Düz metin",
        }
    }

    pub fn label_in(&self, lang: crate::i18n::Lang) -> &'static str {
        match lang {
            crate::i18n::Lang::En => self.label(),
            crate::i18n::Lang::Tr => self.label_tr(),
        }
    }

    /// Label in the current thread's language.
    pub fn label_now(&self) -> &'static str {
        self.label_in(crate::i18n::Lang::current())
    }

    pub fn category(&self) -> OutputCategory {
        use OutputFormat::*;
        match self {
            Csv | Tsv | MarkdownTable | HtmlTable | SqlInsert => OutputCategory::Tabular,
            Json | JsonCompact | JsonLines | Yaml | Toml | Xml | KeyValue => OutputCategory::Tree,
            MarkdownList | NumberedList | PlainLines => OutputCategory::List,
            Template | Text => OutputCategory::Text,
        }
    }

    pub fn extension(&self) -> &'static str {
        use OutputFormat::*;
        match self {
            Json | JsonCompact => "json",
            JsonLines => "jsonl",
            Csv => "csv",
            Tsv => "tsv",
            MarkdownTable | MarkdownList => "md",
            HtmlTable => "html",
            Yaml => "yaml",
            Toml => "toml",
            Xml => "xml",
            SqlInsert => "sql",
            NumberedList | PlainLines | KeyValue | Template | Text => "txt",
        }
    }

    pub fn id(&self) -> String {
        serde_json::to_value(self)
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_default()
    }

    pub fn parse_id(id: &str) -> Option<OutputFormat> {
        let norm = id.trim().to_lowercase().replace(['-', ' '], "_");
        let alias = match norm.as_str() {
            "excel" | "xlsx" => "tsv",
            "markdown" | "md" | "md_table" => "markdown_table",
            "html" => "html_table",
            "sql" | "insert" | "sql_inserts" => "sql_insert",
            "yml" => "yaml",
            "jsonl" | "ndjson" => "json_lines",
            "lines" | "plain" => "plain_lines",
            "list" | "bullet_list" | "bullets" => "markdown_list",
            "numbered" | "ordered_list" => "numbered_list",
            "kv" | "keyvalue" | "key_values" | "properties" | "env" => "key_value",
            "txt" | "string" => "text",
            other => other,
        };
        serde_json::from_value(Value::String(alias.to_string())).ok()
    }
}

/// Severity of a diagnostic produced during detection, conversion or validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Issue {
    pub severity: Severity,
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
}

/// Collects issues while a pipeline runs.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Diagnostics {
    pub issues: Vec<Issue>,
}

impl Diagnostics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, severity: Severity, code: &str, message: impl Into<String>) {
        let message = message.into();
        // Avoid flooding the report with identical issues.
        if self
            .issues
            .iter()
            .any(|i| i.code == code && i.message == message)
        {
            return;
        }
        self.issues.push(Issue {
            severity,
            code: code.to_string(),
            message,
            location: None,
        });
    }

    pub fn info(&mut self, code: &str, message: impl Into<String>) {
        self.push(Severity::Info, code, message)
    }

    /// Push a localized info message from the catalog (see [`crate::i18n`]).
    pub fn info_t(&mut self, code: &str, args: &[&dyn std::fmt::Display]) {
        self.push(Severity::Info, code, crate::i18n::t(code, args))
    }

    pub fn warn_t(&mut self, code: &str, args: &[&dyn std::fmt::Display]) {
        self.push(Severity::Warning, code, crate::i18n::t(code, args))
    }

    pub fn error_t(&mut self, code: &str, args: &[&dyn std::fmt::Display]) {
        self.push(Severity::Error, code, crate::i18n::t(code, args))
    }

    pub fn warn(&mut self, code: &str, message: impl Into<String>) {
        self.push(Severity::Warning, code, message)
    }

    pub fn error(&mut self, code: &str, message: impl Into<String>) {
        self.push(Severity::Error, code, message)
    }

    pub fn extend(&mut self, other: Diagnostics) {
        for issue in other.issues {
            if !self.issues.contains(&issue) {
                self.issues.push(issue);
            }
        }
    }

    pub fn has_errors(&self) -> bool {
        self.issues.iter().any(|i| i.severity == Severity::Error)
    }

    pub fn has_warnings(&self) -> bool {
        self.issues.iter().any(|i| i.severity == Severity::Warning)
    }

    pub fn count(&self, severity: Severity) -> usize {
        self.issues
            .iter()
            .filter(|i| i.severity == severity)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_conservatively() {
        assert_eq!(Scalar::infer("42"), Scalar::Int(42));
        assert_eq!(Scalar::infer("-7"), Scalar::Int(-7));
        assert_eq!(Scalar::infer("2.75"), Scalar::Float(2.75));
        assert_eq!(Scalar::infer("1.0"), Scalar::Float(1.0));
        assert_eq!(Scalar::infer("true"), Scalar::Bool(true));
        assert_eq!(Scalar::infer(""), Scalar::Null);
        assert_eq!(Scalar::infer("   "), Scalar::Null);
        // Values whose canonical form differs from the text stay strings.
        assert_eq!(Scalar::infer("007"), Scalar::Str("007".into()));
        assert_eq!(Scalar::infer("3.10"), Scalar::Str("3.10".into()));
        assert_eq!(Scalar::infer("+5"), Scalar::Str("+5".into()));
        assert_eq!(Scalar::infer("1e5"), Scalar::Str("1e5".into()));
        assert_eq!(Scalar::infer("-0"), Scalar::Str("-0".into()));
        assert_eq!(Scalar::infer("NaN"), Scalar::Str("NaN".into()));
        assert_eq!(Scalar::infer("hello"), Scalar::Str("hello".into()));
    }

    #[test]
    fn text_round_trips() {
        for s in ["42", "2.75", "1.0", "true", "hello", "007", "3.10"] {
            assert_eq!(Scalar::infer(s).as_text(), s);
        }
    }

    #[test]
    fn normalize_pads_and_renames() {
        let mut t = Table::new(
            vec!["a".into(), "".into(), "a".into()],
            vec![
                vec![Scalar::Int(1)],
                vec![
                    Scalar::Int(1),
                    Scalar::Int(2),
                    Scalar::Int(3),
                    Scalar::Int(4),
                ],
            ],
        );
        let notes = t.normalize();
        assert_eq!(t.headers, vec!["a", "column_2", "a_2", "column_4"]);
        assert!(t.rows.iter().all(|r| r.len() == 4));
        assert_eq!(notes.len(), 2);
    }

    #[test]
    fn column_index_is_lenient() {
        let t = Table::new(vec!["Name".into(), "Age".into()], vec![]);
        assert_eq!(t.column_index("Name"), Some(0));
        assert_eq!(t.column_index("name"), Some(0));
        assert_eq!(t.column_index("2"), Some(1));
        assert_eq!(t.column_index("missing"), None);
    }

    #[test]
    fn output_format_aliases() {
        assert_eq!(OutputFormat::parse_id("excel"), Some(OutputFormat::Tsv));
        assert_eq!(
            OutputFormat::parse_id("Markdown"),
            Some(OutputFormat::MarkdownTable)
        );
        assert_eq!(OutputFormat::parse_id("sql"), Some(OutputFormat::SqlInsert));
        assert_eq!(
            OutputFormat::parse_id("json_lines"),
            Some(OutputFormat::JsonLines)
        );
        assert_eq!(OutputFormat::parse_id("nope"), None);
        assert_eq!(OutputFormat::Json.id(), "json");
        assert_eq!(
            InputFormat::parse_id("markdown_table"),
            Some(InputFormat::MarkdownTable)
        );
    }

    #[test]
    fn diagnostics_dedupe() {
        let mut d = Diagnostics::new();
        d.warn("x", "same");
        d.warn("x", "same");
        d.info("y", "other");
        assert_eq!(d.issues.len(), 2);
        assert!(d.has_warnings());
        assert!(!d.has_errors());
    }
}
