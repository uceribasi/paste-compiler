//! Content type detection for clipboard text.

use crate::model::InputFormat;
use crate::parse;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Candidate {
    pub format: InputFormat,
    pub confidence: f32,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Detection {
    pub format: InputFormat,
    pub confidence: f32,
    /// Delimiter for CSV-like input (`,`, `;`, `\t`, `|`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delimiter: Option<char>,
    /// Whether the first row of a table looks like a header row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_header: Option<bool>,
    pub reasons: Vec<String>,
    pub candidates: Vec<Candidate>,
}

fn re(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("valid regex"))
}

static MD_SEPARATOR: OnceLock<Regex> = OnceLock::new();
static BULLET: OnceLock<Regex> = OnceLock::new();
static KEY_VALUE: OnceLock<Regex> = OnceLock::new();

pub fn md_separator_re() -> &'static Regex {
    re(
        &MD_SEPARATOR,
        r"^\s*\|?\s*:?-{1,}:?\s*(\|\s*:?-{1,}:?\s*)*\|?\s*$",
    )
}

pub fn bullet_re() -> &'static Regex {
    re(&BULLET, r"^\s*(?:[-*+•·▪●‣◦]|\d{1,4}[.)])\s+\S")
}

pub fn key_value_re() -> &'static Regex {
    re(
        &KEY_VALUE,
        r"^\s*([^\s:=#\[\]{}][^:=]{0,120}?)\s*[:=]\s*(\S.*)$",
    )
}

pub(crate) fn non_empty_lines(input: &str) -> Vec<&str> {
    input
        .lines()
        .map(|l| l.trim_end_matches('\r'))
        .filter(|l| !l.trim().is_empty())
        .collect()
}

/// Strip a UTF-8 BOM, if present.
pub fn strip_bom(input: &str) -> &str {
    input.strip_prefix('\u{feff}').unwrap_or(input)
}

fn looks_like_html(lower: &str) -> bool {
    [
        "<!doctype html",
        "<html",
        "<body",
        "<div",
        "<p>",
        "<p ",
        "<span",
        "<br",
        "<ul",
        "<ol",
        "<li",
    ]
    .iter()
    .any(|tag| lower.contains(tag))
}

fn is_json_lines(lines: &[&str]) -> bool {
    lines.len() >= 2
        && lines.iter().all(|l| {
            let t = l.trim();
            (t.starts_with('{') || t.starts_with('['))
                && serde_json::from_str::<Value>(t)
                    .map(|v| v.is_object() || v.is_array())
                    .unwrap_or(false)
        })
}

pub fn is_markdown_table(lines: &[&str]) -> bool {
    if lines.len() < 2 {
        return false;
    }
    let first = lines[0];
    let sep = lines[1];
    first.contains('|')
        && sep.contains('-')
        && md_separator_re().is_match(sep)
        && (sep.contains('|') || first.matches('|').count() >= 1)
}

/// Pick the best delimiter for CSV-like text. Returns `(delimiter, score)`.
pub fn sniff_delimiter(input: &str) -> Option<(char, f32)> {
    let mut best: Option<(char, f32)> = None;
    for delim in ['\t', ',', ';', '|'] {
        let Ok(rows) = parse::delimited_rows(input, delim, true) else {
            continue;
        };
        if rows.is_empty() {
            continue;
        }
        let counts: Vec<usize> = rows.iter().map(|r| r.len()).collect();
        let mode = {
            let mut m = std::collections::HashMap::new();
            for c in &counts {
                *m.entry(*c).or_insert(0usize) += 1;
            }
            m.into_iter()
                .max_by_key(|(c, n)| (*n, *c))
                .map(|(c, _)| c)
                .unwrap_or(0)
        };
        if mode < 2 {
            continue;
        }
        let consistent = counts.iter().filter(|c| **c == mode).count() as f32 / counts.len() as f32;
        let mut score = if rows.len() == 1 {
            if delim == '\t' {
                0.75
            } else {
                0.4
            }
        } else if consistent >= 0.999 {
            if delim == '\t' {
                0.95
            } else {
                0.9
            }
        } else if consistent >= 0.8 {
            0.6
        } else {
            continue;
        };
        // Long fields with spaces look like prose, not data.
        let cells: Vec<&String> = rows.iter().flatten().collect();
        if !cells.is_empty() && delim != '\t' {
            let avg_len =
                cells.iter().map(|c| c.chars().count()).sum::<usize>() as f32 / cells.len() as f32;
            let spaced =
                cells.iter().filter(|c| c.trim().contains(' ')).count() as f32 / cells.len() as f32;
            if avg_len > 40.0 && spaced > 0.6 {
                score -= 0.35;
            }
        }
        if score <= 0.0 {
            continue;
        }
        match best {
            Some((_, s)) if s >= score => {}
            _ => best = Some((delim, score)),
        }
    }
    best
}

/// Detect the most likely format of `raw`.
pub fn detect(raw: &str) -> Detection {
    let input = strip_bom(raw);
    let trimmed = input.trim();
    let mut cands: Vec<Candidate> = Vec::new();
    let mut delimiter: Option<char> = None;
    let mut has_header: Option<bool> = None;

    if trimmed.is_empty() {
        return Detection {
            format: InputFormat::Text,
            confidence: 1.0,
            delimiter: None,
            has_header: None,
            reasons: vec!["input is empty".into()],
            candidates: vec![],
        };
    }

    let lines = non_empty_lines(trimmed);
    let lower = trimmed.to_ascii_lowercase();

    // JSON ---------------------------------------------------------------
    match serde_json::from_str::<Value>(trimmed) {
        Ok(v) if v.is_object() || v.is_array() => cands.push(Candidate {
            format: InputFormat::Json,
            confidence: 1.0,
            reason: "parses as a JSON object or array".into(),
        }),
        Ok(Value::String(_)) => cands.push(Candidate {
            format: InputFormat::Json,
            confidence: 0.6,
            reason: "parses as a JSON string".into(),
        }),
        _ => {
            if is_json_lines(&lines) {
                cands.push(Candidate {
                    format: InputFormat::JsonLines,
                    confidence: 0.95,
                    reason: "every line is a JSON value".into(),
                });
            }
        }
    }

    // HTML / XML -----------------------------------------------------------
    if trimmed.starts_with('<') || lower.contains("<table") {
        if lower.contains("<table") && lower.contains("<tr") {
            cands.push(Candidate {
                format: InputFormat::HtmlTable,
                confidence: 0.95,
                reason: "contains an HTML <table> element".into(),
            });
        } else if roxmltree::Document::parse(trimmed).is_ok() && !looks_like_html(&lower) {
            cands.push(Candidate {
                format: InputFormat::Xml,
                confidence: 0.95,
                reason: "well-formed XML document".into(),
            });
        } else if looks_like_html(&lower) {
            cands.push(Candidate {
                format: InputFormat::Text,
                confidence: 0.55,
                reason: "HTML markup without a table; treated as text".into(),
            });
        }
    }

    // Markdown table -------------------------------------------------------
    if is_markdown_table(&lines) {
        cands.push(Candidate {
            format: InputFormat::MarkdownTable,
            confidence: 1.0,
            reason: "pipe table with a `---` separator row".into(),
        });
    }

    // TOML -----------------------------------------------------------------
    if (trimmed.contains('=') || trimmed.contains('[')) && !trimmed.starts_with('<') {
        if let Ok(table) = toml::from_str::<toml::Table>(trimmed) {
            if !table.is_empty() {
                let has_section = lines.iter().any(|l| {
                    let t = l.trim();
                    t.starts_with('[') && t.ends_with(']')
                });
                cands.push(Candidate {
                    format: InputFormat::Toml,
                    confidence: if has_section { 0.9 } else { 0.78 },
                    reason: if has_section {
                        "parses as TOML with sections".into()
                    } else {
                        "parses as TOML key/value pairs".into()
                    },
                });
            }
        }
    }

    // Delimited (CSV/TSV/;/|) ----------------------------------------------
    if let Some((delim, score)) = sniff_delimiter(trimmed) {
        let format = if delim == '\t' {
            InputFormat::Tsv
        } else {
            InputFormat::Csv
        };
        let name = match delim {
            '\t' => "tab".to_string(),
            c => format!("'{}'", c),
        };
        cands.push(Candidate {
            format,
            confidence: score,
            reason: format!("rows consistently separated by {name}"),
        });
        delimiter = Some(delim);
    }

    // YAML -----------------------------------------------------------------
    if !trimmed.starts_with('<') && !trimmed.starts_with('{') && !trimmed.starts_with('[') {
        if let Ok(docs) = parse::yaml_documents(trimmed) {
            let structured = docs.iter().all(|d| {
                matches!(d, Value::Object(o) if !o.is_empty())
                    || matches!(d, Value::Array(a) if !a.is_empty())
            });
            if structured && !docs.is_empty() {
                let has_structure_lines = lines.iter().any(|l| {
                    let t = l.trim_start();
                    t.starts_with("- ") || (l.len() != t.len() && t.contains(':')) || t == "---"
                });
                let kv_lines = lines.iter().filter(|l| key_value_re().is_match(l)).count();
                let conf = if has_structure_lines {
                    0.88
                } else if lines.len() >= 2 && kv_lines == lines.len() {
                    0.8
                } else if lines.len() == 1 {
                    0.55
                } else {
                    0.5
                };
                cands.push(Candidate {
                    format: InputFormat::Yaml,
                    confidence: conf,
                    reason: "parses as a YAML mapping or sequence".into(),
                });
            }
        }
    }

    // Key/value pairs -------------------------------------------------------
    {
        let matched = lines.iter().filter(|l| key_value_re().is_match(l)).count();
        if matched >= 1 && (matched as f32 / lines.len() as f32) >= 0.6 {
            let uses_equals = lines
                .iter()
                .filter(|l| l.contains('=') && !l.contains(": "))
                .count()
                > lines.len() / 2;
            let conf = if lines.len() == 1 {
                0.5
            } else if uses_equals {
                0.76
            } else {
                0.7
            };
            cands.push(Candidate {
                format: InputFormat::KeyValue,
                confidence: conf,
                reason: format!("{matched} of {} lines look like key: value", lines.len()),
            });
        }
    }

    // Lists -----------------------------------------------------------------
    {
        let bullets = lines.iter().filter(|l| bullet_re().is_match(l)).count();
        if lines.len() >= 2 && (bullets as f32 / lines.len() as f32) >= 0.8 {
            cands.push(Candidate {
                format: InputFormat::List,
                confidence: 0.92,
                reason: format!(
                    "{bullets} of {} lines start with a bullet or number",
                    lines.len()
                ),
            });
        } else if lines.len() == 1 && bullets == 1 {
            cands.push(Candidate {
                format: InputFormat::List,
                confidence: 0.55,
                reason: "single bulleted line".into(),
            });
        }
    }

    // Text fallback ---------------------------------------------------------
    let text_conf = if cands.is_empty() { 1.0 } else { 0.5 };
    if !cands.iter().any(|c| c.format == InputFormat::Text) {
        cands.push(Candidate {
            format: InputFormat::Text,
            confidence: text_conf,
            reason: "plain text".into(),
        });
    }

    cands.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let best = cands[0].clone();

    if matches!(best.format, InputFormat::Csv | InputFormat::Tsv) {
        let delim = delimiter.unwrap_or(if best.format == InputFormat::Tsv {
            '\t'
        } else {
            ','
        });
        if let Ok(rows) = parse::delimited_rows(trimmed, delim, true) {
            has_header = Some(parse::guess_has_header(&rows));
        }
    } else {
        delimiter = None;
    }

    let reasons: Vec<String> = cands
        .iter()
        .take(3)
        .map(|c| format!("{}: {}", c.format.label(), c.reason))
        .collect();
    Detection {
        format: best.format,
        confidence: best.confidence,
        delimiter,
        has_header,
        reasons,
        candidates: cands,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(s: &str) -> InputFormat {
        detect(s).format
    }

    #[test]
    fn detects_json() {
        assert_eq!(fmt(r#"{"a": 1, "b": [1,2]}"#), InputFormat::Json);
        assert_eq!(fmt("[1, 2, 3]"), InputFormat::Json);
        assert_eq!(fmt("  [ {\"x\": 1} ]  "), InputFormat::Json);
    }

    #[test]
    fn detects_json_lines() {
        assert_eq!(
            fmt("{\"a\":1}\n{\"a\":2}\n{\"a\":3}"),
            InputFormat::JsonLines
        );
    }

    #[test]
    fn detects_csv_and_delimiters() {
        let d = detect("name,age,city\nAda,36,London\nLinus,54,Helsinki");
        assert_eq!(d.format, InputFormat::Csv);
        assert_eq!(d.delimiter, Some(','));
        assert_eq!(d.has_header, Some(true));

        let d = detect("name;age\nAda;36\nLinus;54");
        assert_eq!(d.format, InputFormat::Csv);
        assert_eq!(d.delimiter, Some(';'));

        let d = detect("a|b|c\n1|2|3");
        assert_eq!(d.format, InputFormat::Csv);
        assert_eq!(d.delimiter, Some('|'));
    }

    #[test]
    fn detects_tsv_from_excel_copy() {
        let d = detect("Name\tAge\tCity\r\nAda\t36\tLondon\r\nLinus\t54\tHelsinki\r\n");
        assert_eq!(d.format, InputFormat::Tsv);
        assert_eq!(d.delimiter, Some('\t'));
        assert_eq!(d.has_header, Some(true));
        // A single Excel row still counts as TSV.
        assert_eq!(fmt("1\t2\t3"), InputFormat::Tsv);
    }

    #[test]
    fn detects_markdown_table() {
        let md = "| Name | Age |\n|------|-----|\n| Ada  | 36  |\n| Linus | 54 |";
        assert_eq!(fmt(md), InputFormat::MarkdownTable);
        let md2 = "Name | Age\n--- | ---\nAda | 36";
        assert_eq!(fmt(md2), InputFormat::MarkdownTable);
    }

    #[test]
    fn detects_html_table_and_xml() {
        assert_eq!(
            fmt("<table><tr><th>a</th></tr><tr><td>1</td></tr></table>"),
            InputFormat::HtmlTable
        );
        assert_eq!(
            fmt("<?xml version=\"1.0\"?><root><item id=\"1\">x</item></root>"),
            InputFormat::Xml
        );
        assert_eq!(
            fmt("<users><user><name>Ada</name></user></users>"),
            InputFormat::Xml
        );
        assert_eq!(fmt("<div><p>Hello</p></div>"), InputFormat::Text);
    }

    #[test]
    fn detects_yaml_toml_keyvalue() {
        assert_eq!(
            fmt("name: Ada\nage: 36\ntags:\n  - a\n  - b"),
            InputFormat::Yaml
        );
        assert_eq!(fmt("- apple\n- banana\n- cherry"), InputFormat::List);
        assert_eq!(
            fmt("[server]\nhost = \"localhost\"\nport = 8080"),
            InputFormat::Toml
        );
        assert_eq!(fmt("title = \"x\"\ncount = 3"), InputFormat::Toml);
        assert_eq!(
            fmt("HOST=localhost\nPORT=8080\nDEBUG=true"),
            InputFormat::KeyValue
        );
        assert_eq!(fmt("name: Ada\nage: 36"), InputFormat::Yaml);
    }

    #[test]
    fn detects_lists_and_text() {
        assert_eq!(fmt("- one\n- two\n* three"), InputFormat::List);
        assert_eq!(fmt("1. first\n2. second\n3. third"), InputFormat::List);
        assert_eq!(
            fmt("Hello, world. This is just a sentence, nothing more."),
            InputFormat::Text
        );
        assert_eq!(fmt("The quick brown fox jumps over the lazy dog.\nAnother line of prose here, with commas, and more words to read."), InputFormat::Text);
        assert_eq!(fmt(""), InputFormat::Text);
    }

    #[test]
    fn strips_bom() {
        let d = detect("\u{feff}a,b\n1,2");
        assert_eq!(d.format, InputFormat::Csv);
    }

    #[test]
    fn candidates_are_sorted() {
        let d = detect("a,b\n1,2");
        assert!(d
            .candidates
            .windows(2)
            .all(|w| w[0].confidence >= w[1].confidence));
        assert!(d.candidates.iter().any(|c| c.format == InputFormat::Text));
    }
}
