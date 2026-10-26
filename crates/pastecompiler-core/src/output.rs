//! Writers that serialize a [`Document`] into each supported output format.

use crate::coerce::{flatten_value, to_list, to_table, to_text, to_tree, value_to_line};
use crate::error::{Error, Result};
use crate::model::{Diagnostics, Document, OutputFormat, Scalar, Table};
use crate::template::{self, html_escape, Vars};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Options accepted by the writers. All fields are optional; unknown fields are ignored.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OutputOptions {
    /// Indentation width for JSON / XML (default 2).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indent: Option<usize>,
    /// Field delimiter for CSV (default `,`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delimiter: Option<char>,
    /// Quote every CSV field (default false).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quote_all: Option<bool>,
    /// Emit a header row for CSV/TSV (default: whether the table has one).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<bool>,
    /// Use CRLF line endings for CSV (default false).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crlf: Option<bool>,
    /// Text used for empty cells in CSV / Markdown / HTML (default "").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub null_as: Option<String>,
    /// SQL table name (default `data`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub table_name: Option<String>,
    /// SQL dialect: `postgres` (default), `mysql`, `sqlite`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dialect: Option<String>,
    /// Emit a CREATE TABLE statement before the inserts (default false).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_table: Option<bool>,
    /// One multi-row INSERT (true, default) or one INSERT per row (false).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch: Option<bool>,
    /// XML root element name (default `root`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_name: Option<String>,
    /// XML / TOML array item name (default `item`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_name: Option<String>,
    /// Emit the `<?xml ...?>` declaration (default true).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub declaration: Option<bool>,
    /// Template for the `template` output, e.g. `{{name}} <{{email}}>`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    /// Separator between rendered template rows (default newline) or between
    /// key and value in `key_value` output (default `: `).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub separator: Option<String>,
    /// Text placed before / after the rendered template rows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,
    /// Bullet for Markdown lists (default `-`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bullet: Option<String>,
    /// Pad Markdown table cells so columns line up (default true).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub align: Option<bool>,
}

impl OutputOptions {
    fn indent(&self) -> usize {
        self.indent.unwrap_or(2).min(8)
    }
    fn null_as(&self) -> String {
        self.null_as.clone().unwrap_or_default()
    }
}

fn trim_trailing_newlines(mut s: String) -> String {
    while s.ends_with('\n') || s.ends_with('\r') {
        s.pop();
    }
    s
}

/// Serialize `doc` as `format`.
pub fn render(
    doc: &Document,
    format: OutputFormat,
    opts: &OutputOptions,
    diag: &mut Diagnostics,
) -> Result<String> {
    let out = match format {
        OutputFormat::Json => render_json(&to_tree(doc, diag), opts.indent())?,
        OutputFormat::JsonCompact => serde_json::to_string(&to_tree(doc, diag))?,
        OutputFormat::JsonLines => render_json_lines(&to_tree(doc, diag))?,
        OutputFormat::Csv => render_csv(&to_table(doc, diag), opts.delimiter.unwrap_or(','), opts)?,
        OutputFormat::Tsv => render_csv(&to_table(doc, diag), '\t', opts)?,
        OutputFormat::MarkdownTable => render_markdown_table(&to_table(doc, diag), opts, diag),
        OutputFormat::HtmlTable => render_html_table(&to_table(doc, diag), opts),
        OutputFormat::Yaml => render_yaml(&to_tree(doc, diag))?,
        OutputFormat::Toml => render_toml(&to_tree(doc, diag), opts, diag)?,
        OutputFormat::Xml => render_xml(&to_tree(doc, diag), opts, diag),
        OutputFormat::SqlInsert => render_sql(&to_table(doc, diag), opts, diag)?,
        OutputFormat::MarkdownList => {
            let bullet = opts.bullet.clone().unwrap_or_else(|| "-".to_string());
            to_list(doc, diag)
                .iter()
                .map(|i| format!("{bullet} {i}"))
                .collect::<Vec<_>>()
                .join("\n")
        }
        OutputFormat::NumberedList => to_list(doc, diag)
            .iter()
            .enumerate()
            .map(|(i, item)| format!("{}. {item}", i + 1))
            .collect::<Vec<_>>()
            .join("\n"),
        OutputFormat::PlainLines => to_list(doc, diag).join("\n"),
        OutputFormat::KeyValue => render_key_value(doc, opts, diag),
        OutputFormat::Template => render_template(doc, opts, diag)?,
        OutputFormat::Text => to_text(doc),
    };
    Ok(trim_trailing_newlines(out))
}

// ---------------------------------------------------------------------------
// JSON / YAML
// ---------------------------------------------------------------------------

pub fn render_json(value: &Value, indent: usize) -> Result<String> {
    let indent_str = " ".repeat(indent.max(1));
    let formatter = serde_json::ser::PrettyFormatter::with_indent(indent_str.as_bytes());
    let mut buf = Vec::new();
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, formatter);
    serde::Serialize::serialize(value, &mut ser)?;
    String::from_utf8(buf).map_err(|e| Error::Parse(e.to_string()))
}

fn render_json_lines(value: &Value) -> Result<String> {
    match value {
        Value::Array(items) => Ok(items
            .iter()
            .map(serde_json::to_string)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .join("\n")),
        other => Ok(serde_json::to_string(other)?),
    }
}

fn render_yaml(value: &Value) -> Result<String> {
    serde_yaml::to_string(value)
        .map_err(|e| Error::Parse(crate::i18n::t("err_yaml_serialize", &[&e])))
}

// ---------------------------------------------------------------------------
// CSV
// ---------------------------------------------------------------------------

fn render_csv(table: &Table, delimiter: char, opts: &OutputOptions) -> Result<String> {
    let mut buf = [0u8; 4];
    let encoded = delimiter.encode_utf8(&mut buf);
    if encoded.len() != 1 {
        return Err(Error::Unsupported(crate::i18n::t(
            "err_delimiter_ascii",
            &[&format!("{delimiter:?}")],
        )));
    }
    let mut writer = csv::WriterBuilder::new()
        .delimiter(encoded.as_bytes()[0])
        .quote_style(if opts.quote_all.unwrap_or(false) {
            csv::QuoteStyle::Always
        } else {
            csv::QuoteStyle::Necessary
        })
        .terminator(if opts.crlf.unwrap_or(false) {
            csv::Terminator::CRLF
        } else {
            csv::Terminator::Any(b'\n')
        })
        .flexible(true)
        .from_writer(Vec::new());
    let null_as = opts.null_as();
    if opts.header.unwrap_or(table.has_header) {
        writer
            .write_record(&table.headers)
            .map_err(|e| Error::Parse(e.to_string()))?;
    }
    for row in &table.rows {
        let cells: Vec<String> = row
            .iter()
            .map(|c| {
                if c.is_null() {
                    null_as.clone()
                } else {
                    c.as_text()
                }
            })
            .collect();
        writer
            .write_record(&cells)
            .map_err(|e| Error::Parse(e.to_string()))?;
    }
    let bytes = writer
        .into_inner()
        .map_err(|e| Error::Parse(e.to_string()))?;
    String::from_utf8(bytes).map_err(|e| Error::Parse(e.to_string()))
}

// ---------------------------------------------------------------------------
// Markdown / HTML tables
// ---------------------------------------------------------------------------

fn md_cell(s: &str) -> String {
    s.replace('|', "\\|")
        .replace("\r\n", "<br>")
        .replace('\n', "<br>")
}

fn render_markdown_table(table: &Table, opts: &OutputOptions, diag: &mut Diagnostics) -> String {
    let null_as = opts.null_as();
    let align = opts.align.unwrap_or(true);
    let header: Vec<String> = table.headers.iter().map(|h| md_cell(h)).collect();
    let rows: Vec<Vec<String>> = table
        .rows
        .iter()
        .map(|r| {
            r.iter()
                .map(|c| {
                    if c.is_null() {
                        null_as.clone()
                    } else {
                        md_cell(&c.as_text())
                    }
                })
                .collect()
        })
        .collect();
    if table
        .rows
        .iter()
        .flatten()
        .any(|c| matches!(c, Scalar::Str(s) if s.contains('\n')))
    {
        diag.info_t("md_newlines", &[]);
    }
    let types = table.column_types();
    let width = table.width();
    let mut widths: Vec<usize> = header.iter().map(|h| h.chars().count().max(3)).collect();
    if align {
        for r in &rows {
            for (i, c) in r.iter().enumerate() {
                if i < widths.len() {
                    widths[i] = widths[i].max(c.chars().count());
                }
            }
        }
    }
    let fmt_row = |cells: &[String]| -> String {
        let mut parts = Vec::with_capacity(width);
        for (i, col_width) in widths.iter().enumerate() {
            let cell = cells.get(i).cloned().unwrap_or_default();
            let numeric = matches!(types.get(i), Some(&"integer") | Some(&"float"));
            let padded = if align {
                let pad = col_width.saturating_sub(cell.chars().count());
                if numeric {
                    format!("{}{}", " ".repeat(pad), cell)
                } else {
                    format!("{}{}", cell, " ".repeat(pad))
                }
            } else {
                cell
            };
            parts.push(padded);
        }
        format!("| {} |", parts.join(" | "))
    };
    let mut out = Vec::with_capacity(rows.len() + 2);
    out.push(fmt_row(&header));
    let sep: Vec<String> = (0..width)
        .map(|i| {
            let numeric = matches!(types.get(i), Some(&"integer") | Some(&"float"));
            let w = if align { widths[i] } else { 3 };
            if numeric {
                format!("{}:", "-".repeat(w.saturating_sub(1).max(2)))
            } else {
                "-".repeat(w)
            }
        })
        .collect();
    out.push(format!("| {} |", sep.join(" | ")));
    for r in &rows {
        out.push(fmt_row(r));
    }
    out.join("\n")
}

fn render_html_table(table: &Table, opts: &OutputOptions) -> String {
    let null_as = opts.null_as();
    let pad = " ".repeat(opts.indent());
    let mut out = String::from("<table>\n");
    if table.has_header || opts.header.unwrap_or(false) {
        out.push_str(&format!("{pad}<thead>\n{pad}{pad}<tr>"));
        for h in &table.headers {
            out.push_str(&format!("<th>{}</th>", html_escape(h)));
        }
        out.push_str(&format!("</tr>\n{pad}</thead>\n"));
    }
    out.push_str(&format!("{pad}<tbody>\n"));
    for row in &table.rows {
        out.push_str(&format!("{pad}{pad}<tr>"));
        for c in row {
            let text = if c.is_null() {
                null_as.clone()
            } else {
                c.as_text()
            };
            out.push_str(&format!(
                "<td>{}</td>",
                html_escape(&text).replace('\n', "<br>")
            ));
        }
        out.push_str("</tr>\n");
    }
    out.push_str(&format!("{pad}</tbody>\n</table>"));
    out
}

// ---------------------------------------------------------------------------
// TOML
// ---------------------------------------------------------------------------

fn json_to_toml(v: &Value, path: &str, diag: &mut Diagnostics) -> Option<toml::Value> {
    match v {
        Value::Null => {
            diag.warn_t(
                "toml_null_dropped",
                &[&(if path.is_empty() { "value" } else { path })],
            );
            None
        }
        Value::Bool(b) => Some(toml::Value::Boolean(*b)),
        Value::Number(n) => Some(if let Some(i) = n.as_i64() {
            toml::Value::Integer(i)
        } else {
            toml::Value::Float(n.as_f64().unwrap_or(0.0))
        }),
        Value::String(s) => Some(toml::Value::String(s.clone())),
        Value::Array(items) => Some(toml::Value::Array(
            items
                .iter()
                .enumerate()
                .filter_map(|(i, item)| json_to_toml(item, &format!("{path}[{i}]"), diag))
                .collect(),
        )),
        Value::Object(obj) => {
            let mut table = toml::Table::new();
            for (k, val) in obj {
                let child_path = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                if let Some(t) = json_to_toml(val, &child_path, diag) {
                    table.insert(k.clone(), t);
                }
            }
            Some(toml::Value::Table(table))
        }
    }
}

fn render_toml(value: &Value, opts: &OutputOptions, diag: &mut Diagnostics) -> Result<String> {
    let item_name = opts.item_name.clone().unwrap_or_else(|| "item".to_string());
    let top = match value {
        Value::Object(_) => {
            json_to_toml(value, "", diag).unwrap_or(toml::Value::Table(toml::Table::new()))
        }
        Value::Array(_) => {
            diag.info_t("toml_array_wrapped", &[&item_name]);
            let mut table = toml::Table::new();
            if let Some(arr) = json_to_toml(value, "", diag) {
                table.insert(item_name, arr);
            }
            toml::Value::Table(table)
        }
        scalar => {
            let mut table = toml::Table::new();
            if let Some(v) = json_to_toml(scalar, "value", diag) {
                table.insert("value".to_string(), v);
            }
            toml::Value::Table(table)
        }
    };
    toml::to_string_pretty(&top)
        .map_err(|e| Error::Unsupported(crate::i18n::t("err_toml_unrepresentable", &[&e])))
}

// ---------------------------------------------------------------------------
// XML
// ---------------------------------------------------------------------------

fn sanitize_xml_name(name: &str, changed: &mut bool) -> String {
    let mut out = String::with_capacity(name.len());
    for (i, c) in name.chars().enumerate() {
        let ok = c.is_alphanumeric() || c == '_' || c == '-' || (c == '.' && i > 0);
        if ok {
            out.push(c);
        } else {
            out.push('_');
            *changed = true;
        }
    }
    if out.is_empty() {
        *changed = true;
        out.push_str("field");
    }
    if out.starts_with(|c: char| c.is_ascii_digit() || c == '-' || c == '.') {
        out.insert(0, '_');
        *changed = true;
    }
    if out.to_lowercase().starts_with("xml") {
        out.insert(0, '_');
        *changed = true;
    }
    out
}

fn xml_escape(s: &str) -> String {
    html_escape(s)
}

fn write_xml(
    out: &mut String,
    name: &str,
    value: &Value,
    depth: usize,
    step: usize,
    changed: &mut bool,
) {
    let pad = " ".repeat(depth * step);
    match value {
        Value::Array(items) => {
            for item in items {
                write_xml(out, name, item, depth, step, changed);
            }
        }
        Value::Object(obj) => {
            let mut attrs = String::new();
            let mut text: Option<String> = None;
            let mut children: Vec<(&String, &Value)> = Vec::new();
            for (k, v) in obj {
                if let Some(attr) = k.strip_prefix('@') {
                    if !v.is_object() && !v.is_array() {
                        attrs.push_str(&format!(
                            " {}=\"{}\"",
                            sanitize_xml_name(attr, changed),
                            xml_escape(&value_to_line(v))
                        ));
                        continue;
                    }
                }
                if k == "#text" {
                    text = Some(value_to_line(v));
                    continue;
                }
                children.push((k, v));
            }
            if children.is_empty() {
                match text {
                    Some(t) => out.push_str(&format!(
                        "{pad}<{name}{attrs}>{}</{name}>\n",
                        xml_escape(&t)
                    )),
                    None => out.push_str(&format!("{pad}<{name}{attrs}/>\n")),
                }
            } else {
                out.push_str(&format!("{pad}<{name}{attrs}>\n"));
                if let Some(t) = text {
                    out.push_str(&format!(
                        "{}{}\n",
                        " ".repeat((depth + 1) * step),
                        xml_escape(&t)
                    ));
                }
                for (k, v) in children {
                    let child_name = sanitize_xml_name(k, changed);
                    write_xml(out, &child_name, v, depth + 1, step, changed);
                }
                out.push_str(&format!("{pad}</{name}>\n"));
            }
        }
        Value::Null => out.push_str(&format!("{pad}<{name}/>\n")),
        scalar => out.push_str(&format!(
            "{pad}<{name}>{}</{name}>\n",
            xml_escape(&value_to_line(scalar))
        )),
    }
}

fn render_xml(value: &Value, opts: &OutputOptions, diag: &mut Diagnostics) -> String {
    let step = opts.indent();
    let item_name = opts.item_name.clone().unwrap_or_else(|| "item".to_string());
    let mut changed = false;
    let mut out = String::new();
    if opts.declaration.unwrap_or(true) {
        out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    }
    // A single-key object whose value is a container is used as the root.
    let single_root = match (value, &opts.root_name) {
        (Value::Object(obj), None) if obj.len() == 1 => obj
            .iter()
            .next()
            .filter(|(_, v)| v.is_object() || v.is_array())
            .map(|(k, v)| (k.clone(), v)),
        _ => None,
    };
    match single_root {
        Some((name, inner)) if !inner.is_array() => {
            let root = sanitize_xml_name(&name, &mut changed);
            write_xml(&mut out, &root, inner, 0, step, &mut changed);
        }
        _ => {
            let root = sanitize_xml_name(
                &opts.root_name.clone().unwrap_or_else(|| "root".to_string()),
                &mut changed,
            );
            match value {
                Value::Array(items) => {
                    out.push_str(&format!("<{root}>\n"));
                    for item in items {
                        write_xml(&mut out, &item_name, item, 1, step, &mut changed);
                    }
                    out.push_str(&format!("</{root}>\n"));
                }
                Value::Object(_) => write_xml(&mut out, &root, value, 0, step, &mut changed),
                scalar => write_xml(&mut out, &root, scalar, 0, step, &mut changed),
            }
        }
    }
    if changed {
        diag.warn_t("xml_names_sanitized", &[]);
    }
    if value.as_array().map(|a| a.len() == 1).unwrap_or(false) {
        diag.info_t("xml_single_item", &[]);
    }
    out
}

// ---------------------------------------------------------------------------
// SQL
// ---------------------------------------------------------------------------

fn sql_quote_ident(name: &str, dialect: &str) -> String {
    if dialect == "mysql" {
        format!("`{}`", name.replace('`', "``"))
    } else {
        format!("\"{}\"", name.replace('"', "\"\""))
    }
}

fn sql_literal(cell: &Scalar, dialect: &str) -> String {
    match cell {
        Scalar::Null => "NULL".to_string(),
        Scalar::Bool(b) => {
            if dialect == "sqlite" {
                if *b { "1" } else { "0" }.to_string()
            } else if *b {
                "TRUE".to_string()
            } else {
                "FALSE".to_string()
            }
        }
        Scalar::Int(i) => i.to_string(),
        Scalar::Float(f) => crate::model::format_float(*f),
        Scalar::Str(s) => {
            let mut escaped = s.replace('\'', "''");
            if dialect == "mysql" {
                escaped = escaped.replace('\\', "\\\\");
            }
            format!("'{escaped}'")
        }
    }
}

fn sql_type(kind: &str, dialect: &str) -> &'static str {
    match (kind, dialect) {
        ("integer", "mysql") => "INT",
        ("integer", _) => "INTEGER",
        ("float", "sqlite") => "REAL",
        ("float", "mysql") => "DOUBLE",
        ("float", _) => "DOUBLE PRECISION",
        ("boolean", "sqlite") => "INTEGER",
        ("boolean", _) => "BOOLEAN",
        _ => "TEXT",
    }
}

fn render_sql(table: &Table, opts: &OutputOptions, diag: &mut Diagnostics) -> Result<String> {
    let dialect = opts
        .dialect
        .clone()
        .unwrap_or_else(|| "postgres".to_string())
        .to_lowercase();
    if !["postgres", "mysql", "sqlite", "ansi"].contains(&dialect.as_str()) {
        return Err(Error::Unsupported(crate::i18n::t(
            "err_sql_dialect",
            &[&dialect],
        )));
    }
    let table_name = opts
        .table_name
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "data".to_string());
    if !table.has_header {
        diag.info_t("sql_generated_columns", &[]);
    }
    let quoted_table = sql_quote_ident(&table_name, &dialect);
    let columns: Vec<String> = table
        .headers
        .iter()
        .map(|h| sql_quote_ident(h, &dialect))
        .collect();
    let mut out = String::new();
    if opts.create_table.unwrap_or(false) {
        let types = table.column_types();
        let defs: Vec<String> = columns
            .iter()
            .zip(types.iter())
            .map(|(c, t)| format!("  {c} {}", sql_type(t, &dialect)))
            .collect();
        out.push_str(&format!(
            "CREATE TABLE {quoted_table} (\n{}\n);\n\n",
            defs.join(",\n")
        ));
    }
    if table.rows.is_empty() {
        diag.warn_t("sql_no_rows", &[]);
        return Ok(out);
    }
    let row_literal = |row: &Vec<Scalar>| -> String {
        format!(
            "({})",
            row.iter()
                .map(|c| sql_literal(c, &dialect))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    if opts.batch.unwrap_or(true) {
        out.push_str(&format!(
            "INSERT INTO {quoted_table} ({}) VALUES\n",
            columns.join(", ")
        ));
        let values: Vec<String> = table
            .rows
            .iter()
            .map(|r| format!("  {}", row_literal(r)))
            .collect();
        out.push_str(&values.join(",\n"));
        out.push_str(";\n");
    } else {
        for row in &table.rows {
            out.push_str(&format!(
                "INSERT INTO {quoted_table} ({}) VALUES {};\n",
                columns.join(", "),
                row_literal(row)
            ));
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Key/value and templates
// ---------------------------------------------------------------------------

fn render_key_value(doc: &Document, opts: &OutputOptions, diag: &mut Diagnostics) -> String {
    let sep = opts.separator.clone().unwrap_or_else(|| ": ".to_string());
    match doc {
        Document::Table(t) if t.has_header && t.width() == 2 => t
            .rows
            .iter()
            .map(|r| format!("{}{sep}{}", r[0].as_text(), r[1].as_text()))
            .collect::<Vec<_>>()
            .join("\n"),
        Document::Table(t) => {
            diag.info_t("kv_blocks", &[]);
            t.rows
                .iter()
                .map(|r| {
                    t.headers
                        .iter()
                        .zip(r.iter())
                        .map(|(h, c)| format!("{h}{sep}{}", c.as_text()))
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .collect::<Vec<_>>()
                .join("\n\n")
        }
        other => {
            let value = to_tree(other, diag);
            match &value {
                Value::Object(_) => {
                    let flat = flatten_value(&value);
                    if flat.len() != value.as_object().map(|o| o.len()).unwrap_or(0) {
                        diag.info_t("flattened_kv", &[]);
                    }
                    flat.iter()
                        .map(|(k, v)| format!("{k}{sep}{}", value_to_line(v)))
                        .collect::<Vec<_>>()
                        .join("\n")
                }
                Value::Array(items) => items
                    .iter()
                    .enumerate()
                    .map(|(i, v)| format!("{}{sep}{}", i + 1, value_to_line(v)))
                    .collect::<Vec<_>>()
                    .join("\n"),
                scalar => format!("value{sep}{}", value_to_line(scalar)),
            }
        }
    }
}

/// Build the variable sets (one per row/item) a template can see.
pub fn template_rows(doc: &Document, diag: &mut Diagnostics) -> Vec<Vars> {
    let mut rows: Vec<Vars> = match doc {
        Document::List { items } => items
            .iter()
            .map(|i| {
                let mut v = Vars::new();
                v.insert("item".into(), i.clone());
                v.insert("value".into(), i.clone());
                v
            })
            .collect(),
        Document::Text { text } => text
            .lines()
            .map(|l| {
                let mut v = Vars::new();
                v.insert("line".into(), l.to_string());
                v.insert("item".into(), l.to_string());
                v.insert("value".into(), l.to_string());
                v
            })
            .collect(),
        other => {
            let table = to_table(other, diag);
            table
                .rows
                .iter()
                .map(|r| {
                    let mut v = Vars::new();
                    for (i, h) in table.headers.iter().enumerate() {
                        let text = r.get(i).map(|c| c.as_text()).unwrap_or_default();
                        v.insert(h.clone(), text.clone());
                        v.insert(format!("col{}", i + 1), text.clone());
                        v.insert(format!("{}", i + 1), text);
                    }
                    if let Some(first) = r.first() {
                        v.entry("value".into()).or_insert_with(|| first.as_text());
                    }
                    v
                })
                .collect()
        }
    };
    let count = rows.len();
    for (i, v) in rows.iter_mut().enumerate() {
        v.insert("_index".into(), (i + 1).to_string());
        v.insert("_index0".into(), i.to_string());
        v.insert("_count".into(), count.to_string());
    }
    rows
}

fn render_template(doc: &Document, opts: &OutputOptions, diag: &mut Diagnostics) -> Result<String> {
    let tpl = opts
        .template
        .clone()
        .ok_or_else(|| Error::Recipe(crate::i18n::t("err_template_missing", &[])))?;
    let rows = template_rows(doc, diag);
    if let Some(first) = rows.first() {
        for var in template::referenced_vars(&tpl) {
            if !first.contains_key(&var) {
                diag.warn_t("template_unknown_var", &[&var]);
            }
        }
    }
    let rendered: Vec<String> = rows
        .iter()
        .map(|vars| template::render(&tpl, vars, false))
        .collect::<Result<Vec<_>>>()?;
    let sep = opts
        .separator
        .clone()
        .map(|s| s.replace("\\n", "\n").replace("\\t", "\t"))
        .unwrap_or_else(|| "\n".to_string());
    let mut out = String::new();
    if let Some(p) = &opts.prefix {
        out.push_str(&p.replace("\\n", "\n"));
        out.push('\n');
    }
    out.push_str(&rendered.join(&sep));
    if let Some(s) = &opts.suffix {
        out.push('\n');
        out.push_str(&s.replace("\\n", "\n"));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_table() -> Document {
        Document::Table(Table::new(
            vec!["name".into(), "age".into(), "note".into()],
            vec![
                vec![
                    Scalar::Str("Ada".into()),
                    Scalar::Int(36),
                    Scalar::Str("likes | pipes".into()),
                ],
                vec![
                    Scalar::Str("O'Brien".into()),
                    Scalar::Float(54.5),
                    Scalar::Null,
                ],
            ],
        ))
    }

    fn render_ok(doc: &Document, f: OutputFormat, opts: OutputOptions) -> String {
        render(doc, f, &opts, &mut Diagnostics::new()).unwrap()
    }

    #[test]
    fn json_outputs() {
        let doc = sample_table();
        let json = render_ok(&doc, OutputFormat::Json, OutputOptions::default());
        assert!(json.starts_with("[\n  {\n    \"name\": \"Ada\""));
        let compact = render_ok(&doc, OutputFormat::JsonCompact, OutputOptions::default());
        assert_eq!(
            compact,
            r#"[{"name":"Ada","age":36,"note":"likes | pipes"},{"name":"O'Brien","age":54.5,"note":null}]"#
        );
        let lines = render_ok(&doc, OutputFormat::JsonLines, OutputOptions::default());
        assert_eq!(lines.lines().count(), 2);
        let four = render_ok(
            &Document::tree(json!({"a": 1})),
            OutputFormat::Json,
            OutputOptions {
                indent: Some(4),
                ..Default::default()
            },
        );
        assert_eq!(four, "{\n    \"a\": 1\n}");
    }

    #[test]
    fn csv_outputs() {
        let doc = sample_table();
        let csv = render_ok(&doc, OutputFormat::Csv, OutputOptions::default());
        assert_eq!(csv, "name,age,note\nAda,36,likes | pipes\nO'Brien,54.5,");
        let tsv = render_ok(&doc, OutputFormat::Tsv, OutputOptions::default());
        assert!(tsv.starts_with("name\tage\tnote\n"));
        let semi = render_ok(
            &doc,
            OutputFormat::Csv,
            OutputOptions {
                delimiter: Some(';'),
                quote_all: Some(true),
                ..Default::default()
            },
        );
        assert!(semi.starts_with("\"name\";\"age\";\"note\""));
        let no_header = render_ok(
            &doc,
            OutputFormat::Csv,
            OutputOptions {
                header: Some(false),
                null_as: Some("NULL".into()),
                ..Default::default()
            },
        );
        assert_eq!(no_header, "Ada,36,likes | pipes\nO'Brien,54.5,NULL");
    }

    #[test]
    fn markdown_and_html() {
        let doc = sample_table();
        let md = render_ok(&doc, OutputFormat::MarkdownTable, OutputOptions::default());
        let lines: Vec<&str> = md.lines().collect();
        assert_eq!(lines.len(), 4);
        assert!(lines[0].starts_with("| name"));
        assert!(lines[1].contains("---"));
        assert!(lines[2].contains("likes \\| pipes"));
        // numeric column is right aligned
        assert!(lines[1].contains("-:"));
        let html = render_ok(&doc, OutputFormat::HtmlTable, OutputOptions::default());
        assert!(html.contains("<th>name</th>"));
        assert!(html.contains("<td>O&#39;Brien</td>"));
    }

    #[test]
    fn yaml_toml_xml() {
        let doc = Document::tree(
            json!({"server": {"host": "localhost", "port": 8080, "tags": ["a", "b"], "none": null}}),
        );
        let yaml = render_ok(&doc, OutputFormat::Yaml, OutputOptions::default());
        assert!(yaml.contains("host: localhost"));
        let mut d = Diagnostics::new();
        let toml_out = render(&doc, OutputFormat::Toml, &OutputOptions::default(), &mut d).unwrap();
        assert!(toml_out.contains("[server]"));
        assert!(toml_out.contains("port = 8080"));
        assert!(d.issues.iter().any(|i| i.code == "toml_null_dropped"));
        let xml = render_ok(&doc, OutputFormat::Xml, OutputOptions::default());
        assert!(xml.starts_with("<?xml"));
        assert!(xml.contains("<server>"));
        assert!(xml.contains("<tags>a</tags>"));
        assert!(xml.contains("<none/>"));
    }

    #[test]
    fn xml_arrays_and_attributes() {
        let doc = Document::tree(json!([{"@id": 1, "name": "A & B"}, {"@id": 2, "name": "C"}]));
        let xml = render_ok(
            &doc,
            OutputFormat::Xml,
            OutputOptions {
                declaration: Some(false),
                root_name: Some("users".into()),
                item_name: Some("user".into()),
                ..Default::default()
            },
        );
        assert_eq!(xml, "<users>\n  <user id=\"1\">\n    <name>A &amp; B</name>\n  </user>\n  <user id=\"2\">\n    <name>C</name>\n  </user>\n</users>");
        let mut d = Diagnostics::new();
        let bad = render(
            &Document::tree(json!({"1 bad key": 1})),
            OutputFormat::Xml,
            &OutputOptions::default(),
            &mut d,
        )
        .unwrap();
        assert!(bad.contains("<_1_bad_key>1</_1_bad_key>"));
        assert!(d.issues.iter().any(|i| i.code == "xml_names_sanitized"));
    }

    #[test]
    fn sql_inserts() {
        let doc = sample_table();
        let sql = render_ok(
            &doc,
            OutputFormat::SqlInsert,
            OutputOptions {
                table_name: Some("people".into()),
                ..Default::default()
            },
        );
        assert_eq!(sql, "INSERT INTO \"people\" (\"name\", \"age\", \"note\") VALUES\n  ('Ada', 36, 'likes | pipes'),\n  ('O''Brien', 54.5, NULL);");
        let mysql = render_ok(
            &doc,
            OutputFormat::SqlInsert,
            OutputOptions {
                dialect: Some("mysql".into()),
                batch: Some(false),
                create_table: Some(true),
                ..Default::default()
            },
        );
        assert!(mysql.starts_with(
            "CREATE TABLE `data` (\n  `name` TEXT,\n  `age` DOUBLE,\n  `note` TEXT\n);"
        ));
        assert_eq!(mysql.matches("INSERT INTO").count(), 2);
        assert!(render(
            &doc,
            OutputFormat::SqlInsert,
            &OutputOptions {
                dialect: Some("oracle".into()),
                ..Default::default()
            },
            &mut Diagnostics::new()
        )
        .is_err());
    }

    #[test]
    fn lists_and_key_values() {
        let doc = Document::list(vec!["a".into(), "b".into()]);
        assert_eq!(
            render_ok(&doc, OutputFormat::MarkdownList, OutputOptions::default()),
            "- a\n- b"
        );
        assert_eq!(
            render_ok(&doc, OutputFormat::NumberedList, OutputOptions::default()),
            "1. a\n2. b"
        );
        assert_eq!(
            render_ok(&doc, OutputFormat::PlainLines, OutputOptions::default()),
            "a\nb"
        );
        let kv = render_ok(
            &Document::tree(json!({"a": 1, "b": {"c": "x"}})),
            OutputFormat::KeyValue,
            OutputOptions::default(),
        );
        assert_eq!(kv, "a: 1\nb.c: x");
        let env = render_ok(
            &Document::tree(json!({"HOST": "x"})),
            OutputFormat::KeyValue,
            OutputOptions {
                separator: Some("=".into()),
                ..Default::default()
            },
        );
        assert_eq!(env, "HOST=x");
    }

    #[test]
    fn templates() {
        let doc = sample_table();
        let out = render_ok(
            &doc,
            OutputFormat::Template,
            OutputOptions {
                template: Some("{{_index}}. {{name | upper}} ({{age}})".into()),
                ..Default::default()
            },
        );
        assert_eq!(out, "1. ADA (36)\n2. O'BRIEN (54.5)");
        let out = render_ok(
            &doc,
            OutputFormat::Template,
            OutputOptions {
                template: Some("{{name}}".into()),
                separator: Some(", ".into()),
                prefix: Some("[".into()),
                suffix: Some("]".into()),
                ..Default::default()
            },
        );
        assert_eq!(out, "[\nAda, O'Brien\n]");
        assert!(render(
            &doc,
            OutputFormat::Template,
            &OutputOptions::default(),
            &mut Diagnostics::new()
        )
        .is_err());
        let mut d = Diagnostics::new();
        render(
            &doc,
            OutputFormat::Template,
            &OutputOptions {
                template: Some("{{nope}}".into()),
                ..Default::default()
            },
            &mut d,
        )
        .unwrap();
        assert!(d.issues.iter().any(|i| i.code == "template_unknown_var"));
    }

    #[test]
    fn text_output_passthrough() {
        assert_eq!(
            render_ok(
                &Document::text("hello\n"),
                OutputFormat::Text,
                OutputOptions::default()
            ),
            "hello"
        );
        assert_eq!(
            render_ok(
                &sample_table(),
                OutputFormat::Text,
                OutputOptions::default()
            )
            .lines()
            .count(),
            3
        );
    }
}
