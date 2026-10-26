//! Parsers that turn text in a known format into a [`Document`].

use crate::detect::{bullet_re, key_value_re, md_separator_re, non_empty_lines, strip_bom};
use crate::error::{Error, Result};
use crate::model::{Diagnostics, Document, InputFormat, Scalar, Table};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ParseOptions {
    /// Delimiter for CSV-like input. Defaults to the detected one (or `,`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delimiter: Option<char>,
    /// Treat the first row as a header. Defaults to a heuristic guess.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_header: Option<bool>,
    /// Infer numbers/booleans from text cells (default true).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub infer_types: Option<bool>,
    /// Trim whitespace around cells (default true).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trim: Option<bool>,
}

impl ParseOptions {
    pub fn infer_types(&self) -> bool {
        self.infer_types.unwrap_or(true)
    }
    pub fn trim(&self) -> bool {
        self.trim.unwrap_or(true)
    }
}

fn cell(text: &str, infer: bool) -> Scalar {
    if infer {
        Scalar::infer(text)
    } else if text.is_empty() {
        Scalar::Null
    } else {
        Scalar::Str(text.to_string())
    }
}

/// Parse `input` as `format`.
pub fn parse(
    input: &str,
    format: InputFormat,
    opts: &ParseOptions,
    diag: &mut Diagnostics,
) -> Result<Document> {
    let input = strip_bom(input);
    if input.contains("\r\n") {
        diag.info_t("crlf_line_endings", &[]);
    }
    match format {
        InputFormat::Json => parse_json(input),
        InputFormat::JsonLines => parse_json_lines(input, diag),
        InputFormat::Csv => parse_delimited(input, opts.delimiter.unwrap_or(','), opts, diag),
        InputFormat::Tsv => parse_delimited(input, opts.delimiter.unwrap_or('\t'), opts, diag),
        InputFormat::MarkdownTable => parse_markdown_table(input, opts, diag),
        InputFormat::HtmlTable => parse_html_table(input, opts, diag),
        InputFormat::Yaml => parse_yaml(input),
        InputFormat::Toml => parse_toml(input),
        InputFormat::Xml => parse_xml(input, opts, diag),
        InputFormat::KeyValue => parse_key_value(input, opts, diag),
        InputFormat::List => Ok(parse_list(input)),
        InputFormat::Text => Ok(Document::text(input.trim_end_matches(['\n', '\r']))),
    }
}

// ---------------------------------------------------------------------------
// JSON
// ---------------------------------------------------------------------------

pub fn parse_json(input: &str) -> Result<Document> {
    let trimmed = input.trim();
    let value: Value = serde_json::from_str(trimmed)
        .map_err(|e| Error::Parse(crate::i18n::t("err_invalid_json", &[&e])))?;
    Ok(Document::tree(value))
}

pub fn parse_json_lines(input: &str, diag: &mut Diagnostics) -> Result<Document> {
    let mut items = Vec::new();
    let mut skipped = 0;
    for (i, line) in input.lines().enumerate() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        match serde_json::from_str::<Value>(t) {
            Ok(v) => items.push(v),
            Err(e) => {
                skipped += 1;
                if skipped <= 3 {
                    diag.warn_t("jsonl_bad_line", &[&(i + 1), &e]);
                }
            }
        }
    }
    if items.is_empty() {
        return Err(Error::Parse(crate::i18n::t("err_no_json_lines", &[])));
    }
    if skipped > 3 {
        diag.warn_t("jsonl_bad_lines", &[&skipped]);
    }
    Ok(Document::tree(Value::Array(items)))
}

// ---------------------------------------------------------------------------
// Delimited (CSV / TSV / ; / |)
// ---------------------------------------------------------------------------

/// Split delimited text into raw string rows. Blank records are dropped.
pub fn delimited_rows(input: &str, delimiter: char, trim: bool) -> Result<Vec<Vec<String>>> {
    let mut delim_buf = [0u8; 4];
    let encoded = delimiter.encode_utf8(&mut delim_buf);
    if encoded.len() != 1 {
        return Err(Error::Unsupported(crate::i18n::t(
            "err_delimiter_ascii",
            &[&format!("{delimiter:?}")],
        )));
    }
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(encoded.as_bytes()[0])
        .has_headers(false)
        .flexible(true)
        .trim(if trim {
            csv::Trim::All
        } else {
            csv::Trim::None
        })
        .from_reader(strip_bom(input).as_bytes());
    let mut rows = Vec::new();
    for record in reader.records() {
        let record =
            record.map_err(|e| Error::Parse(crate::i18n::t("err_invalid_delimited", &[&e])))?;
        let row: Vec<String> = record.iter().map(|s| s.to_string()).collect();
        if row.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        rows.push(row);
    }
    Ok(rows)
}

/// Heuristic: does the first row look like a header?
pub fn guess_has_header(rows: &[Vec<String>]) -> bool {
    if rows.len() < 2 {
        return false;
    }
    let first = &rows[0];
    if first.iter().any(|c| c.trim().is_empty()) {
        return false;
    }
    let mut unique = first
        .iter()
        .map(|c| c.trim().to_lowercase())
        .collect::<Vec<_>>();
    unique.sort();
    unique.dedup();
    if unique.len() != first.len() {
        return false;
    }
    if first
        .iter()
        .any(|c| !matches!(Scalar::infer(c), Scalar::Str(_)))
    {
        return false;
    }
    let others_have_numeric = rows[1..]
        .iter()
        .any(|r| r.iter().any(|c| Scalar::infer(c).is_numeric()));
    if others_have_numeric {
        return true;
    }
    // Data rows share a shape (emails, dates, ids) that the header lacks.
    for (col, head) in first.iter().enumerate() {
        let values: Vec<&str> = rows[1..]
            .iter()
            .filter_map(|r| r.get(col))
            .map(|s| s.as_str())
            .collect();
        if values.is_empty() {
            continue;
        }
        let all = |f: &dyn Fn(&str) -> bool| values.iter().all(|v| f(v)) && !f(head);
        if all(&|v| v.contains('@'))
            || all(&|v| v.chars().any(|c| c.is_ascii_digit()))
            || all(&|v| v.contains("://"))
        {
            return true;
        }
    }
    // All-text tables (e.g. copied from a spreadsheet) usually carry a header.
    true
}

pub fn parse_delimited(
    input: &str,
    delimiter: char,
    opts: &ParseOptions,
    diag: &mut Diagnostics,
) -> Result<Document> {
    let rows = delimited_rows(input, delimiter, opts.trim())?;
    if rows.is_empty() {
        return Err(Error::Parse(crate::i18n::t("err_no_rows", &[])));
    }
    let has_header = opts.has_header.unwrap_or_else(|| guess_has_header(&rows));
    let infer = opts.infer_types();
    let mut table = if has_header {
        let mut iter = rows.into_iter();
        let headers = iter.next().unwrap_or_default();
        let data: Vec<Vec<Scalar>> = iter
            .map(|r| r.iter().map(|c| cell(c, infer)).collect())
            .collect();
        Table::new(headers, data)
    } else {
        Table::without_header(
            rows.iter()
                .map(|r| r.iter().map(|c| cell(c, infer)).collect())
                .collect(),
        )
    };
    finish_table(&mut table, diag);
    Ok(Document::Table(table))
}

fn finish_table(table: &mut Table, diag: &mut Diagnostics) {
    for note in table.normalize() {
        diag.warn("table_normalized", note);
    }
    if !table.has_header {
        diag.info_t("no_header", &[]);
    }
    for (name, kinds) in table.mixed_type_columns() {
        diag.info_t(
            "mixed_types",
            &[&name, &kinds.join(&crate::i18n::t("word_and", &[]))],
        );
    }
    if table.rows.is_empty() {
        diag.warn_t("empty_table", &[]);
    }
    // Decimal-comma numbers are common in European locales; flag them.
    let decimal_comma = table
        .rows
        .iter()
        .flatten()
        .filter(|c| {
            if let Scalar::Str(s) = c {
                let t = s.trim();
                t.len() >= 3
                    && t.chars().filter(|c| *c == ',').count() == 1
                    && t.chars()
                        .all(|c| c.is_ascii_digit() || c == ',' || c == '-')
            } else {
                false
            }
        })
        .count();
    if decimal_comma >= 2 {
        diag.info_t("decimal_comma", &[&decimal_comma]);
    }
}

// ---------------------------------------------------------------------------
// Markdown table
// ---------------------------------------------------------------------------

pub fn split_markdown_row(line: &str) -> Vec<String> {
    let mut t = line.trim();
    if let Some(s) = t.strip_prefix('|') {
        t = s;
    }
    if let Some(s) = t.strip_suffix('|') {
        t = s;
    }
    let mut cells = Vec::new();
    let mut current = String::new();
    let mut chars = t.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(&'|') = chars.peek() {
                    current.push('|');
                    chars.next();
                } else {
                    current.push('\\');
                }
            }
            '|' => {
                cells.push(std::mem::take(&mut current));
            }
            _ => current.push(c),
        }
    }
    cells.push(current);
    cells
        .into_iter()
        .map(|c| {
            c.trim()
                .replace("<br>", "\n")
                .replace("<br/>", "\n")
                .replace("<br />", "\n")
        })
        .collect()
}

pub fn parse_markdown_table(
    input: &str,
    opts: &ParseOptions,
    diag: &mut Diagnostics,
) -> Result<Document> {
    let lines = non_empty_lines(input);
    if lines.len() < 2 {
        return Err(Error::Parse(crate::i18n::t("err_md_needs_rows", &[])));
    }
    let sep_index = lines
        .iter()
        .position(|l| l.contains('-') && md_separator_re().is_match(l))
        .ok_or_else(|| Error::Parse(crate::i18n::t("err_md_no_separator", &[])))?;
    if sep_index == 0 {
        return Err(Error::Parse(crate::i18n::t("err_md_separator_first", &[])));
    }
    let headers = split_markdown_row(lines[sep_index - 1]);
    let infer = opts.infer_types();
    let rows: Vec<Vec<Scalar>> = lines[sep_index + 1..]
        .iter()
        .filter(|l| l.contains('|'))
        .map(|l| {
            split_markdown_row(l)
                .iter()
                .map(|c| cell(c, infer))
                .collect()
        })
        .collect();
    let mut table = Table::new(headers, rows);
    if sep_index > 1 {
        diag.info_t("markdown_prefix_ignored", &[&(sep_index - 1)]);
    }
    finish_table(&mut table, diag);
    Ok(Document::Table(table))
}

// ---------------------------------------------------------------------------
// HTML table
// ---------------------------------------------------------------------------

fn collapse_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_space = true;
    for c in s.chars() {
        if c.is_whitespace() {
            if !last_space {
                out.push(' ');
            }
            last_space = true;
        } else {
            out.push(c);
            last_space = false;
        }
    }
    out.trim().to_string()
}

pub fn parse_html_table(
    input: &str,
    opts: &ParseOptions,
    diag: &mut Diagnostics,
) -> Result<Document> {
    use scraper::{Html, Selector};
    let doc = Html::parse_fragment(input);
    let table_sel = Selector::parse("table").expect("selector");
    let tr_sel = Selector::parse("tr").expect("selector");
    let cell_sel = Selector::parse("th, td").expect("selector");
    let tables: Vec<_> = doc.select(&table_sel).collect();
    let table_el = tables
        .first()
        .ok_or_else(|| Error::Parse(crate::i18n::t("err_html_no_table", &[])))?;
    if tables.len() > 1 {
        diag.warn_t("multiple_tables", &[&tables.len()]);
    }
    let mut raw_rows: Vec<Vec<String>> = Vec::new();
    let mut first_row_is_th = false;
    let mut spans = false;
    for (ri, tr) in table_el.select(&tr_sel).enumerate() {
        let mut row = Vec::new();
        let mut all_th = true;
        let mut any = false;
        for c in tr.select(&cell_sel) {
            any = true;
            if c.value().name() != "th" {
                all_th = false;
            }
            if c.value().attr("colspan").is_some() || c.value().attr("rowspan").is_some() {
                spans = true;
            }
            let text: String = c.text().collect::<Vec<_>>().join(" ");
            row.push(collapse_ws(&text));
        }
        if !any {
            continue;
        }
        if ri == 0 && all_th {
            first_row_is_th = true;
        }
        raw_rows.push(row);
    }
    if raw_rows.is_empty() {
        return Err(Error::Parse(crate::i18n::t("err_html_no_rows", &[])));
    }
    if spans {
        diag.warn_t("html_spans_ignored", &[]);
    }
    let has_header = opts
        .has_header
        .unwrap_or(first_row_is_th || guess_has_header(&raw_rows));
    let infer = opts.infer_types();
    let mut table = if has_header {
        let mut iter = raw_rows.into_iter();
        let headers = iter.next().unwrap_or_default();
        Table::new(
            headers,
            iter.map(|r| r.iter().map(|c| cell(c, infer)).collect())
                .collect(),
        )
    } else {
        Table::without_header(
            raw_rows
                .iter()
                .map(|r| r.iter().map(|c| cell(c, infer)).collect())
                .collect(),
        )
    };
    finish_table(&mut table, diag);
    Ok(Document::Table(table))
}

// ---------------------------------------------------------------------------
// YAML
// ---------------------------------------------------------------------------

fn yaml_to_json(v: serde_yaml::Value) -> Value {
    match v {
        serde_yaml::Value::Null => Value::Null,
        serde_yaml::Value::Bool(b) => Value::Bool(b),
        serde_yaml::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::from(i)
            } else if let Some(u) = n.as_u64() {
                Value::from(u)
            } else {
                n.as_f64()
                    .and_then(serde_json::Number::from_f64)
                    .map(Value::Number)
                    .unwrap_or(Value::Null)
            }
        }
        serde_yaml::Value::String(s) => Value::String(s),
        serde_yaml::Value::Sequence(seq) => {
            Value::Array(seq.into_iter().map(yaml_to_json).collect())
        }
        serde_yaml::Value::Mapping(map) => {
            let mut out = Map::new();
            for (k, v) in map {
                let key = match k {
                    serde_yaml::Value::String(s) => s,
                    other => match yaml_to_json(other) {
                        Value::String(s) => s,
                        Value::Null => "null".to_string(),
                        o => o.to_string(),
                    },
                };
                out.insert(key, yaml_to_json(v));
            }
            Value::Object(out)
        }
        serde_yaml::Value::Tagged(t) => yaml_to_json(t.value),
    }
}

/// Parse all YAML documents in `input` into JSON values.
pub fn yaml_documents(input: &str) -> Result<Vec<Value>> {
    let mut docs = Vec::new();
    for de in serde_yaml::Deserializer::from_str(input) {
        let v: serde_yaml::Value = serde::Deserialize::deserialize(de)
            .map_err(|e| Error::Parse(crate::i18n::t("err_invalid_yaml", &[&e])))?;
        docs.push(yaml_to_json(v));
    }
    Ok(docs)
}

pub fn parse_yaml(input: &str) -> Result<Document> {
    let docs = yaml_documents(input)?;
    match docs.len() {
        0 => Err(Error::Parse(crate::i18n::t("err_empty_yaml", &[]))),
        1 => Ok(Document::tree(docs.into_iter().next().unwrap())),
        _ => Ok(Document::tree(Value::Array(docs))),
    }
}

// ---------------------------------------------------------------------------
// TOML
// ---------------------------------------------------------------------------

fn toml_to_json(v: toml::Value) -> Value {
    match v {
        toml::Value::String(s) => Value::String(s),
        toml::Value::Integer(i) => Value::from(i),
        toml::Value::Float(f) => serde_json::Number::from_f64(f)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        toml::Value::Boolean(b) => Value::Bool(b),
        toml::Value::Datetime(d) => Value::String(d.to_string()),
        toml::Value::Array(a) => Value::Array(a.into_iter().map(toml_to_json).collect()),
        toml::Value::Table(t) => {
            Value::Object(t.into_iter().map(|(k, v)| (k, toml_to_json(v))).collect())
        }
    }
}

pub fn parse_toml(input: &str) -> Result<Document> {
    let table: toml::Table = toml::from_str(input)
        .map_err(|e| Error::Parse(crate::i18n::t("err_invalid_toml", &[&e])))?;
    Ok(Document::tree(toml_to_json(toml::Value::Table(table))))
}

// ---------------------------------------------------------------------------
// XML
// ---------------------------------------------------------------------------

fn xml_element_to_json(node: roxmltree::Node, infer: bool) -> Value {
    let mut obj = Map::new();
    for attr in node.attributes() {
        obj.insert(
            format!("@{}", attr.name()),
            cell(attr.value(), infer).to_json(),
        );
    }
    let mut text = String::new();
    for child in node.children() {
        if child.is_text() {
            text.push_str(child.text().unwrap_or(""));
        } else if child.is_element() {
            let name = child.tag_name().name().to_string();
            let value = xml_element_to_json(child, infer);
            match obj.get_mut(&name) {
                Some(Value::Array(arr)) => arr.push(value),
                Some(existing) => {
                    let prev = existing.take();
                    *existing = Value::Array(vec![prev, value]);
                }
                None => {
                    obj.insert(name, value);
                }
            }
        }
    }
    let text = text.trim();
    if obj.is_empty() {
        return cell(text, infer).to_json();
    }
    if !text.is_empty() {
        obj.insert("#text".to_string(), cell(text, infer).to_json());
    }
    Value::Object(obj)
}

pub fn parse_xml(input: &str, opts: &ParseOptions, diag: &mut Diagnostics) -> Result<Document> {
    let doc = roxmltree::Document::parse(input.trim())
        .map_err(|e| Error::Parse(crate::i18n::t("err_invalid_xml", &[&e])))?;
    let root = doc.root_element();
    let value = xml_element_to_json(root, opts.infer_types());
    let mut wrapper = Map::new();
    wrapper.insert(root.tag_name().name().to_string(), value);
    diag.info_t("xml_attributes", &[]);
    Ok(Document::tree(Value::Object(wrapper)))
}

// ---------------------------------------------------------------------------
// Key/value and lists
// ---------------------------------------------------------------------------

fn unquote(s: &str) -> &str {
    let t = s.trim();
    if t.len() >= 2 {
        let bytes = t.as_bytes();
        if (bytes[0] == b'"' && bytes[t.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[t.len() - 1] == b'\'')
        {
            return &t[1..t.len() - 1];
        }
    }
    t
}

pub fn parse_key_value(
    input: &str,
    opts: &ParseOptions,
    diag: &mut Diagnostics,
) -> Result<Document> {
    let mut obj = Map::new();
    let mut ignored = 0;
    let mut duplicates = 0;
    let infer = opts.infer_types();
    for line in non_empty_lines(input) {
        let t = line.trim();
        if t.starts_with('#') || t.starts_with("//") {
            continue;
        }
        match key_value_re().captures(t) {
            Some(caps) => {
                let key = caps.get(1).map(|m| m.as_str().trim()).unwrap_or("");
                let raw_value = caps.get(2).map(|m| m.as_str()).unwrap_or("");
                let value = unquote(raw_value);
                let scalar = if raw_value.trim() != value {
                    Scalar::Str(value.to_string())
                } else {
                    cell(value, infer)
                };
                if obj
                    .insert(unquote(key).to_string(), scalar.to_json())
                    .is_some()
                {
                    duplicates += 1;
                }
            }
            None => ignored += 1,
        }
    }
    if obj.is_empty() {
        return Err(Error::Parse(crate::i18n::t("err_no_key_values", &[])));
    }
    if ignored > 0 {
        diag.warn_t("kv_lines_ignored", &[&ignored]);
    }
    if duplicates > 0 {
        diag.warn_t("kv_duplicate_keys", &[&duplicates]);
    }
    Ok(Document::tree(Value::Object(obj)))
}

pub fn strip_bullet(line: &str) -> String {
    let t = line.trim();
    if bullet_re().is_match(t) {
        let idx = t.find(char::is_whitespace).unwrap_or(t.len());
        let rest = t[idx..].trim_start();
        // Task-list checkboxes.
        let rest = rest
            .strip_prefix("[ ] ")
            .or_else(|| rest.strip_prefix("[x] "))
            .or_else(|| rest.strip_prefix("[X] "))
            .unwrap_or(rest);
        return rest.to_string();
    }
    t.to_string()
}

pub fn parse_list(input: &str) -> Document {
    let items = non_empty_lines(input)
        .into_iter()
        .map(strip_bullet)
        .collect();
    Document::list(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> ParseOptions {
        ParseOptions::default()
    }

    fn table(doc: Document) -> Table {
        match doc {
            Document::Table(t) => t,
            other => panic!("expected table, got {other:?}"),
        }
    }

    fn tree(doc: Document) -> Value {
        match doc {
            Document::Tree { value } => value,
            other => panic!("expected tree, got {other:?}"),
        }
    }

    #[test]
    fn csv_with_header_and_types() {
        let mut d = Diagnostics::new();
        let t = table(
            parse(
                "name,age,score\nAda,36,9.5\nLinus,54,8.0",
                InputFormat::Csv,
                &opts(),
                &mut d,
            )
            .unwrap(),
        );
        assert_eq!(t.headers, vec!["name", "age", "score"]);
        assert_eq!(t.rows.len(), 2);
        assert_eq!(t.rows[0][1], Scalar::Int(36));
        assert_eq!(t.rows[0][2], Scalar::Float(9.5));
        assert!(t.has_header);
    }

    #[test]
    fn csv_quoted_fields_and_newlines() {
        let mut d = Diagnostics::new();
        let t = table(
            parse(
                "a,b\n\"hello, world\",\"multi\nline\"\n",
                InputFormat::Csv,
                &opts(),
                &mut d,
            )
            .unwrap(),
        );
        assert_eq!(t.rows[0][0], Scalar::Str("hello, world".into()));
        assert_eq!(t.rows[0][1], Scalar::Str("multi\nline".into()));
    }

    #[test]
    fn csv_without_header() {
        let mut d = Diagnostics::new();
        let t = table(parse("1,2,3\n4,5,6", InputFormat::Csv, &opts(), &mut d).unwrap());
        assert!(!t.has_header);
        assert_eq!(t.headers, vec!["column_1", "column_2", "column_3"]);
        assert_eq!(t.rows.len(), 2);
    }

    #[test]
    fn csv_ragged_rows_are_padded() {
        let mut d = Diagnostics::new();
        let t = table(parse("a,b,c\n1,2\n4,5,6,7", InputFormat::Csv, &opts(), &mut d).unwrap());
        assert_eq!(t.headers.len(), 4);
        assert!(t.rows.iter().all(|r| r.len() == 4));
        assert!(d.issues.iter().any(|i| i.code == "table_normalized"));
    }

    #[test]
    fn tsv_and_semicolon() {
        let mut d = Diagnostics::new();
        let t = table(parse("a\tb\n1\t2", InputFormat::Tsv, &opts(), &mut d).unwrap());
        assert_eq!(t.headers, vec!["a", "b"]);
        let o = ParseOptions {
            delimiter: Some(';'),
            ..Default::default()
        };
        let t = table(parse("a;b\n1;2", InputFormat::Csv, &o, &mut d).unwrap());
        assert_eq!(t.rows[0], vec![Scalar::Int(1), Scalar::Int(2)]);
    }

    #[test]
    fn markdown_table_parses_with_escapes() {
        let mut d = Diagnostics::new();
        let md = "| Name | Note |\n|---|---|\n| Ada | a \\| b |\n| Linus | x<br>y |";
        let t = table(parse(md, InputFormat::MarkdownTable, &opts(), &mut d).unwrap());
        assert_eq!(t.headers, vec!["Name", "Note"]);
        assert_eq!(t.rows[0][1], Scalar::Str("a | b".into()));
        assert_eq!(t.rows[1][1], Scalar::Str("x\ny".into()));
    }

    #[test]
    fn html_table_parses() {
        let mut d = Diagnostics::new();
        let html = "<table><thead><tr><th>Name</th><th>Age</th></tr></thead><tbody><tr><td>Ada</td><td>36</td></tr><tr><td> Linus </td><td>54</td></tr></tbody></table>";
        let t = table(parse(html, InputFormat::HtmlTable, &opts(), &mut d).unwrap());
        assert_eq!(t.headers, vec!["Name", "Age"]);
        assert_eq!(t.rows[1][0], Scalar::Str("Linus".into()));
        assert_eq!(t.rows[1][1], Scalar::Int(54));
    }

    #[test]
    fn html_table_excel_style_without_th() {
        let mut d = Diagnostics::new();
        let html =
            "<table><tr><td>Name</td><td>Age</td></tr><tr><td>Ada</td><td>36</td></tr></table>";
        let t = table(parse(html, InputFormat::HtmlTable, &opts(), &mut d).unwrap());
        assert!(t.has_header);
        assert_eq!(t.headers, vec!["Name", "Age"]);
    }

    #[test]
    fn yaml_and_toml_to_tree() {
        let v = tree(
            parse(
                "name: Ada\nage: 36\ntags: [a, b]\n",
                InputFormat::Yaml,
                &opts(),
                &mut Diagnostics::new(),
            )
            .unwrap(),
        );
        assert_eq!(v["name"], "Ada");
        assert_eq!(v["age"], 36);
        assert_eq!(v["tags"][1], "b");
        let v = tree(
            parse(
                "[server]\nhost = \"localhost\"\nport = 8080\n",
                InputFormat::Toml,
                &opts(),
                &mut Diagnostics::new(),
            )
            .unwrap(),
        );
        assert_eq!(v["server"]["port"], 8080);
    }

    #[test]
    fn yaml_multi_document_becomes_array() {
        let v = tree(
            parse(
                "a: 1\n---\na: 2\n",
                InputFormat::Yaml,
                &opts(),
                &mut Diagnostics::new(),
            )
            .unwrap(),
        );
        assert_eq!(v.as_array().unwrap().len(), 2);
    }

    #[test]
    fn yaml_non_string_keys() {
        let v = tree(
            parse(
                "1: one\n2: two\n",
                InputFormat::Yaml,
                &opts(),
                &mut Diagnostics::new(),
            )
            .unwrap(),
        );
        assert_eq!(v["1"], "one");
    }

    #[test]
    fn xml_to_tree() {
        let xml = r#"<users><user id="1"><name>Ada</name><tag>a</tag><tag>b</tag></user><user id="2"><name>Linus</name></user></users>"#;
        let v = tree(parse(xml, InputFormat::Xml, &opts(), &mut Diagnostics::new()).unwrap());
        let users = &v["users"]["user"];
        assert_eq!(users[0]["@id"], 1);
        assert_eq!(users[0]["name"], "Ada");
        assert_eq!(users[0]["tag"][1], "b");
        assert_eq!(users[1]["name"], "Linus");
    }

    #[test]
    fn key_value_pairs() {
        let mut d = Diagnostics::new();
        let v = tree(
            parse(
                "HOST=localhost\nPORT = 8080\n# comment\nNAME=\"My App\"\nEMPTY=\ngarbage line",
                InputFormat::KeyValue,
                &opts(),
                &mut d,
            )
            .unwrap(),
        );
        assert_eq!(v["HOST"], "localhost");
        assert_eq!(v["PORT"], 8080);
        assert_eq!(v["NAME"], "My App");
        assert!(v.get("EMPTY").is_none());
        assert!(d.issues.iter().any(|i| i.code == "kv_lines_ignored"));
    }

    #[test]
    fn list_strips_bullets() {
        match parse(
            "- one\n* two\n3. three\n- [ ] four",
            InputFormat::List,
            &opts(),
            &mut Diagnostics::new(),
        )
        .unwrap()
        {
            Document::List { items } => assert_eq!(items, vec!["one", "two", "three", "four"]),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn header_guess() {
        let rows = |s: &str| delimited_rows(s, ',', true).unwrap();
        assert!(guess_has_header(&rows("name,age\nAda,36")));
        assert!(!guess_has_header(&rows("1,2\n3,4")));
        assert!(!guess_has_header(&rows("a,b")));
        assert!(!guess_has_header(&rows("a,a\n1,2")));
        assert!(guess_has_header(&rows(
            "email,name\nx@y.com,Ada\nz@w.com,Linus"
        )));
    }

    #[test]
    fn json_lines_skips_bad_lines() {
        let mut d = Diagnostics::new();
        let v = tree(
            parse(
                "{\"a\":1}\nnot json\n{\"a\":2}",
                InputFormat::JsonLines,
                &opts(),
                &mut d,
            )
            .unwrap(),
        );
        assert_eq!(v.as_array().unwrap().len(), 2);
        assert!(d.has_warnings());
    }
}
