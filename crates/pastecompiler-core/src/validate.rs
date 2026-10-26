//! Validation of a conversion: parses the output back, compares it with the
//! source document and reports data loss, format problems and inconsistencies.

use crate::coerce::{flatten_value, to_list, to_table, to_tree, value_to_line};
use crate::model::{Diagnostics, Document, InputFormat, Issue, OutputFormat, Severity, Table};
use crate::output::OutputOptions;
use crate::parse::{self, ParseOptions};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoundTrip {
    /// The output was parsed back and matched the source document exactly.
    Verified,
    /// The output was parsed back but differs from the source document.
    Differences,
    /// The output could not be parsed back at all.
    Failed,
    /// No parser exists for this output format (SQL, templates, plain text).
    Unavailable,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Stats {
    pub input_bytes: usize,
    pub output_bytes: usize,
    pub input_rows: Option<usize>,
    pub output_rows: Option<usize>,
    pub input_columns: Option<usize>,
    pub output_columns: Option<usize>,
    pub elapsed_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidationReport {
    /// `Some(true)` when the round trip matched and nothing lossy happened,
    /// `Some(false)` when data changed, `None` when it cannot be verified.
    pub lossless: Option<bool>,
    pub round_trip: RoundTrip,
    pub issues: Vec<Issue>,
    pub differences: Vec<String>,
    pub stats: Stats,
}

impl ValidationReport {
    pub fn warnings(&self) -> usize {
        self.issues
            .iter()
            .filter(|i| i.severity == Severity::Warning)
            .count()
    }
    pub fn errors(&self) -> usize {
        self.issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .count()
    }
}

const MAX_DIFFS: usize = 8;

fn round_trip_format(format: OutputFormat) -> Option<InputFormat> {
    use OutputFormat::*;
    Some(match format {
        Json | JsonCompact => InputFormat::Json,
        JsonLines => InputFormat::JsonLines,
        Csv => InputFormat::Csv,
        Tsv => InputFormat::Tsv,
        MarkdownTable => InputFormat::MarkdownTable,
        HtmlTable => InputFormat::HtmlTable,
        Yaml => InputFormat::Yaml,
        Toml => InputFormat::Toml,
        Xml => InputFormat::Xml,
        KeyValue => InputFormat::KeyValue,
        MarkdownList | NumberedList => InputFormat::List,
        PlainLines => InputFormat::List,
        SqlInsert | Template | Text => return None,
    })
}

fn compare_tables(
    expected: &Table,
    actual: &Table,
    compare_headers: bool,
    diffs: &mut Vec<String>,
) -> usize {
    let mut total = 0usize;
    if compare_headers && expected.headers != actual.headers {
        total += 1;
        diffs.push(crate::i18n::t(
            "diff_header",
            &[
                &format!("{:?}", expected.headers),
                &format!("{:?}", actual.headers),
            ],
        ));
    }
    if expected.rows.len() != actual.rows.len() {
        total += 1;
        diffs.push(crate::i18n::t(
            "diff_row_count",
            &[&expected.rows.len(), &actual.rows.len()],
        ));
    }
    if expected.width() != actual.width() {
        total += 1;
        diffs.push(crate::i18n::t(
            "diff_column_count",
            &[&expected.width(), &actual.width()],
        ));
    }
    for (ri, (er, ar)) in expected.rows.iter().zip(actual.rows.iter()).enumerate() {
        for (ci, ec) in er.iter().enumerate() {
            let e = ec.as_text();
            let a = ar.get(ci).map(|c| c.as_text()).unwrap_or_default();
            if e != a {
                total += 1;
                if diffs.len() < MAX_DIFFS {
                    let col = expected
                        .headers
                        .get(ci)
                        .cloned()
                        .unwrap_or_else(|| format!("#{}", ci + 1));
                    let kind = if e.trim() == a.trim() {
                        crate::i18n::t("diff_kind_whitespace", &[])
                    } else {
                        crate::i18n::t("diff_kind_value", &[])
                    };
                    diffs.push(crate::i18n::t(
                        "diff_cell",
                        &[
                            &(ri + 1),
                            &col,
                            &kind,
                            &format!("{:?}", truncate(&e)),
                            &format!("{:?}", truncate(&a)),
                        ],
                    ));
                }
            }
        }
    }
    total
}

fn truncate(s: &str) -> String {
    if s.chars().count() > 40 {
        format!("{}…", s.chars().take(40).collect::<String>())
    } else {
        s.to_string()
    }
}

fn flat_text(value: &Value) -> Map<String, Value> {
    flatten_value(value)
        .into_iter()
        .map(|(k, v)| (k, Value::String(value_to_line(&v))))
        .collect()
}

fn compare_values(expected: &Value, actual: &Value, diffs: &mut Vec<String>) -> usize {
    let e = flat_text(expected);
    let a = flat_text(actual);
    let mut total = 0usize;
    for (k, ev) in &e {
        match a.get(k) {
            None => {
                total += 1;
                if diffs.len() < MAX_DIFFS {
                    diffs.push(crate::i18n::t("diff_missing", &[&k]));
                }
            }
            Some(av) if av != ev => {
                total += 1;
                if diffs.len() < MAX_DIFFS {
                    diffs.push(crate::i18n::t(
                        "diff_changed",
                        &[
                            &k,
                            &format!("{:?}", truncate(&value_to_line(ev))),
                            &format!("{:?}", truncate(&value_to_line(av))),
                        ],
                    ));
                }
            }
            _ => {}
        }
    }
    for k in a.keys() {
        if !e.contains_key(k) {
            total += 1;
            if diffs.len() < MAX_DIFFS {
                diffs.push(crate::i18n::t("diff_extra", &[&k]));
            }
        }
    }
    total
}

/// Shape the source document the way the writer did, so it can be compared
/// with the parsed-back output.
fn expected_tree(doc: &Document, format: OutputFormat, opts: &OutputOptions) -> Value {
    let mut scratch = Diagnostics::new();
    let value = to_tree(doc, &mut scratch);
    let item = opts.item_name.clone().unwrap_or_else(|| "item".to_string());
    match format {
        OutputFormat::Toml => match value {
            Value::Object(_) => value,
            Value::Array(_) => {
                let mut m = Map::new();
                m.insert(item, value);
                Value::Object(m)
            }
            scalar => {
                let mut m = Map::new();
                m.insert("value".into(), scalar);
                Value::Object(m)
            }
        },
        OutputFormat::Xml => {
            let root = opts.root_name.clone().unwrap_or_else(|| "root".to_string());
            match &value {
                Value::Object(obj) if obj.len() == 1 && opts.root_name.is_none() => {
                    let (_, inner) = obj.iter().next().unwrap();
                    if inner.is_object() {
                        return value;
                    }
                    let mut m = Map::new();
                    m.insert(root, value);
                    Value::Object(m)
                }
                Value::Array(_) => {
                    let mut inner = Map::new();
                    inner.insert(item, value);
                    let mut m = Map::new();
                    m.insert(root, Value::Object(inner));
                    Value::Object(m)
                }
                _ => {
                    let mut m = Map::new();
                    m.insert(root, value);
                    Value::Object(m)
                }
            }
        }
        OutputFormat::JsonLines => match value {
            Value::Array(_) => value,
            other => Value::Array(vec![other]),
        },
        OutputFormat::KeyValue => match value {
            Value::Object(_) => Value::Object(flatten_value(&value)),
            Value::Array(items) => {
                let mut m = Map::new();
                for (i, v) in items.iter().enumerate() {
                    m.insert((i + 1).to_string(), Value::String(value_to_line(v)));
                }
                Value::Object(m)
            }
            scalar => {
                let mut m = Map::new();
                m.insert("value".into(), scalar);
                Value::Object(m)
            }
        },
        _ => value,
    }
}

/// Validate `output` (rendered from `doc` as `format`) and build the report.
#[allow(clippy::too_many_arguments)]
pub fn validate(
    source_doc: &Document,
    doc: &Document,
    output: &str,
    format: OutputFormat,
    opts: &OutputOptions,
    mut diag: Diagnostics,
    input_bytes: usize,
    elapsed_ms: u128,
) -> ValidationReport {
    let mut differences = Vec::new();
    let mut stats = Stats {
        input_bytes,
        output_bytes: output.len(),
        elapsed_ms,
        ..Default::default()
    };
    let src = source_doc.summary();
    stats.input_rows = src.rows.or(src.items);
    stats.input_columns = src.columns;

    let round_trip = match round_trip_format(format) {
        None => {
            diag.info_t("round_trip_unavailable", &[&format.label_now()]);
            RoundTrip::Unavailable
        }
        Some(back_format) => {
            let mut scratch = Diagnostics::new();
            let parsed = match format {
                OutputFormat::PlainLines => Ok(Document::list(
                    output
                        .lines()
                        .filter(|l| !l.trim().is_empty())
                        .map(|l| l.to_string())
                        .collect(),
                )),
                _ => {
                    let table_shape =
                        matches!(format.category(), crate::model::OutputCategory::Tabular);
                    let mut popts = ParseOptions {
                        trim: Some(false),
                        ..Default::default()
                    };
                    if table_shape {
                        let t = to_table(doc, &mut scratch);
                        let emitted_header = match format {
                            OutputFormat::Csv | OutputFormat::Tsv => {
                                opts.header.unwrap_or(t.has_header)
                            }
                            OutputFormat::HtmlTable => t.has_header || opts.header.unwrap_or(false),
                            _ => true,
                        };
                        popts.has_header = Some(emitted_header);
                        if format == OutputFormat::Csv {
                            popts.delimiter = Some(opts.delimiter.unwrap_or(','));
                        }
                    }
                    parse::parse(output, back_format, &popts, &mut scratch)
                }
            };
            match parsed {
                Err(e) => {
                    diag.error_t("output_unparseable", &[&back_format.label_now(), &e]);
                    RoundTrip::Failed
                }
                Ok(back) => {
                    let out_summary = back.summary();
                    stats.output_rows = out_summary.rows.or(out_summary.items);
                    stats.output_columns = out_summary.columns;
                    let count = match format.category() {
                        crate::model::OutputCategory::Tabular => {
                            let expected = to_table(doc, &mut scratch);
                            let actual = to_table(&back, &mut scratch);
                            let header_emitted = match format {
                                OutputFormat::Csv | OutputFormat::Tsv => {
                                    opts.header.unwrap_or(expected.has_header)
                                }
                                _ => true,
                            };
                            compare_tables(
                                &expected,
                                &actual,
                                header_emitted && expected.has_header,
                                &mut differences,
                            )
                        }
                        crate::model::OutputCategory::List => {
                            let expected = to_list(doc, &mut scratch);
                            let actual = to_list(&back, &mut scratch);
                            let mut n = 0;
                            if expected.len() != actual.len() {
                                n += 1;
                                differences.push(crate::i18n::t(
                                    "diff_item_count",
                                    &[&expected.len(), &actual.len()],
                                ));
                            }
                            for (i, (e, a)) in expected.iter().zip(actual.iter()).enumerate() {
                                if e != a {
                                    n += 1;
                                    if differences.len() < MAX_DIFFS {
                                        differences.push(crate::i18n::t(
                                            "diff_item",
                                            &[
                                                &(i + 1),
                                                &format!("{:?}", truncate(e)),
                                                &format!("{:?}", truncate(a)),
                                            ],
                                        ));
                                    }
                                }
                            }
                            n
                        }
                        _ => {
                            let expected = expected_tree(doc, format, opts);
                            let actual = to_tree(&back, &mut scratch);
                            compare_values(&expected, &actual, &mut differences)
                        }
                    };
                    if count == 0 {
                        RoundTrip::Verified
                    } else {
                        diag.warn_t("round_trip_differences", &[&count]);
                        RoundTrip::Differences
                    }
                }
            }
        }
    };

    if stats.output_rows.is_none() {
        let out_summary = doc.summary();
        stats.output_rows = out_summary.rows.or(out_summary.items);
        stats.output_columns = out_summary.columns;
    }

    let lossless = match round_trip {
        RoundTrip::Verified => Some(!diag.has_warnings() && !diag.has_errors()),
        RoundTrip::Differences | RoundTrip::Failed => Some(false),
        RoundTrip::Unavailable => {
            if diag.has_warnings() || diag.has_errors() {
                Some(false)
            } else {
                None
            }
        }
    };

    let mut issues = diag.issues;
    issues.sort_by_key(|i| std::cmp::Reverse(i.severity));
    ValidationReport {
        lossless,
        round_trip,
        issues,
        differences,
        stats,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Scalar;
    use crate::output::render;
    use serde_json::json;

    fn run(doc: Document, format: OutputFormat, opts: OutputOptions) -> ValidationReport {
        let mut diag = Diagnostics::new();
        let out = render(&doc, format, &opts, &mut diag).unwrap();
        validate(&doc, &doc, &out, format, &opts, diag, 0, 0)
    }

    fn table() -> Document {
        Document::Table(Table::new(
            vec!["name".into(), "age".into()],
            vec![
                vec![Scalar::Str("Ada".into()), Scalar::Int(36)],
                vec![Scalar::Str("Linus".into()), Scalar::Null],
            ],
        ))
    }

    #[test]
    fn lossless_round_trips() {
        for f in [
            OutputFormat::Csv,
            OutputFormat::Tsv,
            OutputFormat::Json,
            OutputFormat::JsonCompact,
            OutputFormat::JsonLines,
            OutputFormat::MarkdownTable,
            OutputFormat::HtmlTable,
            OutputFormat::Yaml,
        ] {
            let r = run(table(), f, OutputOptions::default());
            assert_eq!(
                r.round_trip,
                RoundTrip::Verified,
                "{f:?}: {:?}",
                r.differences
            );
            assert_eq!(r.lossless, Some(true), "{f:?}: {:?}", r.issues);
        }
    }

    #[test]
    fn tree_round_trips() {
        let doc = Document::tree(
            json!({"server": {"host": "localhost", "port": 8080, "tags": ["a", "b"], "on": true}}),
        );
        for f in [
            OutputFormat::Json,
            OutputFormat::Yaml,
            OutputFormat::Toml,
            OutputFormat::Xml,
            OutputFormat::KeyValue,
        ] {
            let r = run(doc.clone(), f, OutputOptions::default());
            assert_eq!(
                r.round_trip,
                RoundTrip::Verified,
                "{f:?}: {:?}",
                r.differences
            );
        }
    }

    #[test]
    fn toml_null_is_reported_as_loss() {
        let doc = Document::tree(json!({"a": null, "b": 1}));
        let r = run(doc, OutputFormat::Toml, OutputOptions::default());
        assert_eq!(r.lossless, Some(false));
        assert!(r.issues.iter().any(|i| i.code == "toml_null_dropped"));
        assert!(r.differences.iter().any(|d| d.contains("'a' is missing")));
    }

    #[test]
    fn csv_without_header_round_trips() {
        let r = run(
            table(),
            OutputFormat::Csv,
            OutputOptions {
                header: Some(false),
                ..Default::default()
            },
        );
        assert_eq!(r.round_trip, RoundTrip::Verified, "{:?}", r.differences);
    }

    #[test]
    fn unavailable_formats() {
        let r = run(table(), OutputFormat::SqlInsert, OutputOptions::default());
        assert_eq!(r.round_trip, RoundTrip::Unavailable);
        assert_eq!(r.lossless, None);
        assert!(r.issues.iter().any(|i| i.code == "round_trip_unavailable"));
    }

    #[test]
    fn nested_values_flagged_in_csv() {
        let doc = Document::tree(json!([{"a": {"b": 1}}, {"a": {"b": 2}}]));
        let r = run(doc, OutputFormat::Csv, OutputOptions::default());
        assert_eq!(r.round_trip, RoundTrip::Verified);
        assert_eq!(r.lossless, Some(false));
        assert!(r.issues.iter().any(|i| i.code == "flattened"));
    }

    #[test]
    fn list_round_trips() {
        let doc = Document::list(vec!["one".into(), "two".into()]);
        for f in [
            OutputFormat::MarkdownList,
            OutputFormat::NumberedList,
            OutputFormat::PlainLines,
        ] {
            let r = run(doc.clone(), f, OutputOptions::default());
            assert_eq!(
                r.round_trip,
                RoundTrip::Verified,
                "{f:?}: {:?}",
                r.differences
            );
        }
    }

    #[test]
    fn stats_are_filled() {
        let r = run(table(), OutputFormat::Json, OutputOptions::default());
        assert_eq!(r.stats.input_rows, Some(2));
        assert_eq!(r.stats.output_rows, Some(2));
        assert!(r.stats.output_bytes > 0);
    }
}
