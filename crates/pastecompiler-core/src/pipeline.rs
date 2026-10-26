//! High-level entry points: analyze pasted text, convert it, run recipes.

use crate::detect::{detect, Detection};
use crate::error::{Error, Result};
use crate::model::{Diagnostics, DocKind, DocSummary, Document, InputFormat, OutputFormat, Scalar};
use crate::output::{render, OutputOptions};
use crate::parse::{parse, ParseOptions};
use crate::recipe::{apply_steps, Recipe, Step};
use crate::validate::{validate, ValidationReport};
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// Maximum number of rows/items included in previews sent to the UI.
pub const PREVIEW_ROWS: usize = 50;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FormatInfo {
    pub id: String,
    pub label: String,
    pub category: String,
    pub extension: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Analysis {
    pub detection: Detection,
    pub summary: DocSummary,
    pub issues: Vec<crate::model::Issue>,
    /// Output formats that make sense for this input, best first.
    pub suggestions: Vec<FormatInfo>,
    /// Table preview (header + first rows) when the input is tabular.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<TablePreview>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TablePreview {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub total_rows: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConvertRequest {
    pub input: String,
    /// Force an input format; `None` means auto-detect.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_format: Option<InputFormat>,
    pub parse: ParseOptions,
    pub steps: Vec<Step>,
    pub output: Option<OutputFormat>,
    pub options: OutputOptions,
    /// Language for messages in the report (`en` or `tr`); defaults to the thread's current language.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lang: Option<crate::i18n::Lang>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConvertResult {
    pub output: String,
    pub output_format: OutputFormat,
    pub detection: Detection,
    pub input_summary: DocSummary,
    pub result_summary: DocSummary,
    pub report: ValidationReport,
}

pub fn format_info(f: OutputFormat) -> FormatInfo {
    FormatInfo {
        id: f.id(),
        label: f.label().to_string(),
        category: serde_json::to_value(f.category())
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_default(),
        extension: f.extension().to_string(),
    }
}

/// Suggested output formats for a document, best first.
pub fn suggest_outputs(doc: &Document, input: InputFormat) -> Vec<OutputFormat> {
    use OutputFormat::*;
    let mut list: Vec<OutputFormat> = match doc.kind() {
        DocKind::Table => vec![
            Json,
            Csv,
            Tsv,
            MarkdownTable,
            SqlInsert,
            HtmlTable,
            Yaml,
            JsonLines,
            Xml,
            Template,
            KeyValue,
            MarkdownList,
            Toml,
            JsonCompact,
            PlainLines,
            NumberedList,
            Text,
        ],
        DocKind::Tree => vec![
            Json,
            Yaml,
            Csv,
            MarkdownTable,
            Toml,
            Xml,
            JsonCompact,
            Tsv,
            KeyValue,
            SqlInsert,
            HtmlTable,
            JsonLines,
            Template,
            MarkdownList,
            PlainLines,
            NumberedList,
            Text,
        ],
        DocKind::List => vec![
            MarkdownList,
            NumberedList,
            Json,
            PlainLines,
            Csv,
            MarkdownTable,
            Template,
            Yaml,
            Tsv,
            JsonCompact,
            JsonLines,
            HtmlTable,
            KeyValue,
            SqlInsert,
            Xml,
            Toml,
            Text,
        ],
        DocKind::Text => vec![
            PlainLines,
            MarkdownList,
            Json,
            NumberedList,
            Csv,
            MarkdownTable,
            Template,
            Text,
            Yaml,
            Tsv,
            JsonCompact,
            JsonLines,
            HtmlTable,
            KeyValue,
            SqlInsert,
            Xml,
            Toml,
        ],
    };
    // Don't suggest converting into the same format first.
    let same = match input {
        InputFormat::Json => Some(Json),
        InputFormat::Csv => Some(Csv),
        InputFormat::Tsv => Some(Tsv),
        InputFormat::MarkdownTable => Some(MarkdownTable),
        InputFormat::HtmlTable => Some(HtmlTable),
        InputFormat::Yaml => Some(Yaml),
        InputFormat::Toml => Some(Toml),
        InputFormat::Xml => Some(Xml),
        InputFormat::JsonLines => Some(JsonLines),
        InputFormat::KeyValue => Some(KeyValue),
        _ => None,
    };
    if let Some(s) = same {
        if let Some(pos) = list.iter().position(|f| *f == s) {
            let f = list.remove(pos);
            list.push(f);
        }
    }
    list
}

fn parse_with_fallback(
    input: &str,
    forced: Option<InputFormat>,
    opts: &ParseOptions,
    diag: &mut Diagnostics,
) -> Result<(Detection, Document)> {
    let mut detection = detect(input);
    let mut parse_opts = opts.clone();
    if parse_opts.delimiter.is_none() {
        parse_opts.delimiter = detection.delimiter;
    }
    if parse_opts.has_header.is_none() {
        parse_opts.has_header = detection.has_header;
    }
    if let Some(f) = forced {
        if f != detection.format {
            // Delimiter / header guesses belong to the detected format only.
            if !matches!(f, InputFormat::Csv | InputFormat::Tsv) {
                parse_opts.delimiter = opts.delimiter;
                parse_opts.has_header = opts.has_header;
            } else if parse_opts.delimiter.is_none() || detection.format != f {
                parse_opts.delimiter = opts.delimiter.or(if f == InputFormat::Tsv {
                    Some('\t')
                } else {
                    Some(',')
                });
                parse_opts.has_header = opts.has_header;
            }
            detection.format = f;
            detection.confidence = 1.0;
            detection
                .reasons
                .insert(0, format!("{}: selected manually", f.label()));
        }
        let doc = parse(input, f, &parse_opts, diag)?;
        return Ok((detection, doc));
    }
    let candidates: Vec<InputFormat> = {
        let mut v: Vec<InputFormat> = detection.candidates.iter().map(|c| c.format).collect();
        if !v.contains(&InputFormat::Text) {
            v.push(InputFormat::Text);
        }
        v
    };
    let mut last_err: Option<Error> = None;
    for (i, f) in candidates.iter().enumerate() {
        let mut attempt = Diagnostics::new();
        let mut po = parse_opts.clone();
        if !matches!(f, InputFormat::Csv | InputFormat::Tsv) {
            po.delimiter = opts.delimiter;
            po.has_header = opts.has_header;
        }
        match parse(input, *f, &po, &mut attempt) {
            Ok(doc) => {
                diag.extend(attempt);
                if i > 0 {
                    diag.warn_t(
                        "detection_fallback",
                        &[
                            &detection.format.label_now(),
                            &last_err.as_ref().map(|e| e.to_string()).unwrap_or_default(),
                            &f.label_now(),
                        ],
                    );
                    detection.format = *f;
                    detection.confidence = detection
                        .candidates
                        .iter()
                        .find(|c| c.format == *f)
                        .map(|c| c.confidence)
                        .unwrap_or(0.5);
                }
                return Ok((detection, doc));
            }
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| Error::Parse(crate::i18n::t("err_could_not_parse", &[]))))
}

fn table_preview(doc: &Document) -> Option<TablePreview> {
    match doc {
        Document::Table(t) => Some(TablePreview {
            headers: t.headers.clone(),
            rows: t
                .rows
                .iter()
                .take(PREVIEW_ROWS)
                .map(|r| r.iter().map(Scalar::as_text).collect())
                .collect(),
            total_rows: t.rows.len(),
        }),
        Document::Tree { .. } => {
            let mut scratch = Diagnostics::new();
            let t = crate::coerce::to_table(doc, &mut scratch);
            if t.width() == 0 {
                return None;
            }
            Some(TablePreview {
                headers: t.headers.clone(),
                rows: t
                    .rows
                    .iter()
                    .take(PREVIEW_ROWS)
                    .map(|r| r.iter().map(Scalar::as_text).collect())
                    .collect(),
                total_rows: t.rows.len(),
            })
        }
        _ => None,
    }
}

/// Detect and parse `input`, returning what the UI needs to offer conversions.
pub fn analyze(input: &str, forced: Option<InputFormat>, opts: &ParseOptions) -> Result<Analysis> {
    let mut diag = Diagnostics::new();
    let (detection, doc) = parse_with_fallback(input, forced, opts, &mut diag)?;
    let suggestions = suggest_outputs(&doc, detection.format)
        .into_iter()
        .map(format_info)
        .collect();
    Ok(Analysis {
        summary: doc.summary(),
        preview: table_preview(&doc),
        detection,
        issues: diag.issues,
        suggestions,
    })
}

/// Run a full conversion: detect, parse, apply steps, render and validate.
pub fn convert(req: &ConvertRequest) -> Result<ConvertResult> {
    if let Some(lang) = req.lang {
        crate::i18n::Lang::set(lang);
    }
    let started = Instant::now();
    let mut diag = Diagnostics::new();
    let (detection, source_doc) =
        parse_with_fallback(&req.input, req.input_format, &req.parse, &mut diag)?;
    let output_format = req
        .output
        .unwrap_or_else(|| suggest_outputs(&source_doc, detection.format)[0]);
    let doc = apply_steps(source_doc.clone(), &req.steps, &mut diag)?;
    let output = render(&doc, output_format, &req.options, &mut diag)?;
    let report = validate(
        &source_doc,
        &doc,
        &output,
        output_format,
        &req.options,
        diag,
        req.input.len(),
        started.elapsed().as_millis(),
    );
    Ok(ConvertResult {
        output,
        output_format,
        detection,
        input_summary: source_doc.summary(),
        result_summary: doc.summary(),
        report,
    })
}

impl Recipe {
    /// Build a conversion request that applies this recipe to `input`.
    pub fn to_request(&self, input: &str) -> ConvertRequest {
        ConvertRequest {
            input: input.to_string(),
            input_format: self.input.format,
            parse: self.input.options.clone(),
            steps: self.steps.clone(),
            output: Some(self.output.format),
            options: self.output.options.clone(),
            lang: None,
        }
    }

    /// Apply this recipe to `input`.
    pub fn run(&self, input: &str) -> Result<ConvertResult> {
        let problems = self.check();
        if !problems.is_empty() {
            return Err(Error::Recipe(problems.join("; ")));
        }
        convert(&self.to_request(input))
    }
}

/// Every output format with its metadata (for UI pickers).
pub fn output_formats() -> Vec<FormatInfo> {
    OutputFormat::all()
        .iter()
        .map(|f| format_info(*f))
        .collect()
}

/// Every input format with its metadata (for UI pickers).
pub fn input_formats() -> Vec<FormatInfo> {
    InputFormat::all()
        .iter()
        .map(|f| FormatInfo {
            id: f.id(),
            label: f.label().to_string(),
            category: String::new(),
            extension: String::new(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate::RoundTrip;

    const CSV: &str = "name,age,city\nAda,36,London\nLinus,54,Helsinki\nGrace,85,New York";

    #[test]
    fn analyze_csv() {
        let a = analyze(CSV, None, &ParseOptions::default()).unwrap();
        assert_eq!(a.detection.format, InputFormat::Csv);
        assert_eq!(a.summary.rows, Some(3));
        assert_eq!(a.suggestions[0].id, "json");
        assert_eq!(a.suggestions.last().unwrap().id, "csv");
        let p = a.preview.unwrap();
        assert_eq!(p.headers, vec!["name", "age", "city"]);
        assert_eq!(p.rows.len(), 3);
    }

    #[test]
    fn convert_csv_to_json_and_back() {
        let req = ConvertRequest {
            input: CSV.into(),
            output: Some(OutputFormat::Json),
            ..Default::default()
        };
        let r = convert(&req).unwrap();
        assert!(r.output.contains("\"name\": \"Ada\""));
        assert_eq!(r.report.round_trip, RoundTrip::Verified);
        assert_eq!(r.report.lossless, Some(true));
        let back = convert(&ConvertRequest {
            input: r.output.clone(),
            output: Some(OutputFormat::Csv),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(back.output, CSV);
    }

    #[test]
    fn convert_with_steps_to_sql() {
        let req = ConvertRequest {
            input: CSV.into(),
            steps: vec![
                Step::FilterRows {
                    column: "age".into(),
                    condition: crate::recipe::Condition::Lt,
                    value: Some("60".into()),
                },
                Step::SortRows {
                    column: "name".into(),
                    descending: false,
                },
            ],
            output: Some(OutputFormat::SqlInsert),
            options: OutputOptions {
                table_name: Some("people".into()),
                ..Default::default()
            },
            ..Default::default()
        };
        let r = convert(&req).unwrap();
        assert_eq!(r.output, "INSERT INTO \"people\" (\"name\", \"age\", \"city\") VALUES\n  ('Ada', 36, 'London'),\n  ('Linus', 54, 'Helsinki');");
        assert_eq!(r.result_summary.rows, Some(2));
        assert!(r.report.issues.iter().any(|i| i.code == "rows_filtered"));
    }

    #[test]
    fn forced_input_format() {
        let req = ConvertRequest {
            input: "a;b\n1;2".into(),
            input_format: Some(InputFormat::Csv),
            parse: ParseOptions {
                delimiter: Some(';'),
                ..Default::default()
            },
            output: Some(OutputFormat::JsonCompact),
            ..Default::default()
        };
        let r = convert(&req).unwrap();
        assert_eq!(r.output, r#"[{"a":1,"b":2}]"#);
        // Force plain text to be read as a list.
        let req = ConvertRequest {
            input: "x\ny".into(),
            input_format: Some(InputFormat::List),
            output: Some(OutputFormat::JsonCompact),
            ..Default::default()
        };
        assert_eq!(convert(&req).unwrap().output, r#"["x","y"]"#);
    }

    #[test]
    fn detection_fallback_when_parse_fails() {
        // Looks like a markdown table but the separator row is actually missing dashes in a way the parser rejects.
        let req = ConvertRequest {
            input: "{\"a\": 1,}".into(),
            output: Some(OutputFormat::Text),
            ..Default::default()
        };
        let r = convert(&req).unwrap();
        assert_eq!(r.detection.format, InputFormat::Text);
    }

    #[test]
    fn default_output_is_first_suggestion() {
        let r = convert(&ConvertRequest {
            input: CSV.into(),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(r.output_format, OutputFormat::Json);
    }

    #[test]
    fn recipe_runs_end_to_end() {
        let mut recipe = Recipe::new("Markdown summary", OutputFormat::MarkdownTable);
        recipe.steps = vec![
            Step::SelectColumns {
                columns: vec!["name".into(), "city".into()],
            },
            Step::Limit {
                count: 2,
                offset: 0,
            },
        ];
        let r = recipe.run(CSV).unwrap();
        assert_eq!(r.output.lines().count(), 4);
        assert!(r.output.contains("| Ada"));
        let req = recipe.to_request(CSV);
        assert_eq!(req.output, Some(OutputFormat::MarkdownTable));
    }

    #[test]
    fn turkish_messages_when_requested() {
        let req = ConvertRequest {
            input: r#"[{"a": {"b": 1}}, {"a": {"b": 2}}]"#.into(),
            output: Some(OutputFormat::Csv),
            lang: Some(crate::i18n::Lang::Tr),
            ..Default::default()
        };
        let r = convert(&req).unwrap();
        let flattened = r
            .report
            .issues
            .iter()
            .find(|i| i.code == "flattened")
            .expect("flattened issue");
        assert!(
            flattened.message.contains("düzleştirildi"),
            "{}",
            flattened.message
        );
        crate::i18n::Lang::set(crate::i18n::Lang::En);
        let r = convert(&ConvertRequest {
            lang: Some(crate::i18n::Lang::En),
            ..req
        })
        .unwrap();
        assert!(r
            .report
            .issues
            .iter()
            .any(|i| i.message.contains("flattened")));
    }

    #[test]
    fn format_lists() {
        assert_eq!(output_formats().len(), OutputFormat::all().len());
        assert_eq!(input_formats().len(), InputFormat::all().len());
    }
}
