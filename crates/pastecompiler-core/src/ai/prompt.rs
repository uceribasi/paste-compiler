//! Prompt construction for recipe generation.

use crate::model::{DocSummary, InputFormat};
use crate::recipe::Recipe;
use serde::{Deserialize, Serialize};

/// What the model is told about the pasted data.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SampleContext {
    pub format: Option<InputFormat>,
    pub delimiter: Option<char>,
    pub summary: Option<DocSummary>,
    /// Truncated sample of the pasted text (empty when the user disabled sharing).
    pub sample: String,
}

pub const SYSTEM_PROMPT: &str = r#"You write conversion recipes for PasteCompiler, a desktop tool that converts copied data (tables, JSON, CSV, YAML, text...) into other formats.

Reply with ONE JSON object and nothing else (no prose, no code fences). Schema:

{
  "name": "short title",
  "description": "one sentence",
  "explanation": "one or two sentences for the user explaining what the recipe does",
  "input": { "format": "auto" | "json" | "json_lines" | "csv" | "tsv" | "markdown_table" | "html_table" | "yaml" | "toml" | "xml" | "key_value" | "list" | "text",
             "delimiter": ",", "has_header": true },          // all optional; prefer "auto"
  "steps": [ ... ],                                           // zero or more, applied in order
  "output": { "format": "<output format>", ...options }
}

Steps (field "op"):
- {"op":"select_columns","columns":["a","b"]}              keep only these columns, in this order
- {"op":"drop_columns","columns":["a"]}
- {"op":"rename_column","from":"old","to":"new"}
- {"op":"filter_rows","column":"age","condition":"gt","value":"30"}
      condition: eq | ne | contains | not_contains | starts_with | ends_with | gt | gte | lt | lte | regex | empty | not_empty
- {"op":"sort_rows","column":"name","descending":false}
- {"op":"limit","count":10,"offset":0}
- {"op":"map_column","column":"name","transform":"upper","arg":null}
      transform: upper | lower | title | trim | snake_case | camel_case | pascal_case | kebab_case | to_number | to_string | to_bool | round (arg = decimals) | prefix (arg) | suffix (arg) | length | default (arg)
- {"op":"add_column","name":"label","template":"{{first}} {{last}}"}   templates use {{column}} and filters: upper, lower, trim, title, snake, camel, kebab, json, sql, default:"x", truncate:10
- {"op":"split_column","column":"full_name","separator":" ","into":["first","last"]}
- {"op":"dedupe","columns":[]}                            empty = whole row
- {"op":"transpose"}
- {"op":"flatten"}                                        nested JSON -> table with dotted columns (a.b, tags.0)
- {"op":"unflatten"}                                      dotted columns -> nested JSON
- {"op":"header_case","case":"snake"}                     snake | camel | pascal | kebab | upper | lower | title
- {"op":"replace","pattern":"foo","replacement":"bar","regex":false,"column":null}
- {"op":"trim_cells"}
- {"op":"fill_empty","value":"n/a","column":null}
- {"op":"text_lines","transform":"sort"}                  sort | sort_desc | dedupe | trim | remove_empty | upper | lower | reverse (for plain text / lists)
- {"op":"to_table"} {"op":"to_tree"} {"op":"to_list"} {"op":"to_text"}   explicit shape changes (rarely needed; the writer converts automatically)

Output formats and their options:
- json (indent), json_compact, json_lines
- csv (delimiter, quote_all, header, crlf, null_as), tsv (header)  — tsv is what Excel / Google Sheets paste as a grid
- markdown_table (align), html_table
- yaml, toml (item_name), xml (root_name, item_name, declaration)
- sql_insert (table_name, dialect: postgres|mysql|sqlite, create_table, batch)
- markdown_list (bullet), numbered_list, plain_lines
- key_value (separator, e.g. "=" for .env files)
- template (template, separator, prefix, suffix) — e.g. "{{name}} <{{email}}>" renders once per row
- text

Rules:
1. Use the exact column names from the sample. Never invent columns that are not in the sample unless you create them with add_column or split_column.
2. Keep the step list minimal; the writer already converts between tables, JSON, lists and text.
3. If the user does not say which output format they want, keep the input's own format (csv -> csv, json -> json) or choose json.
4. Column values in filters are strings; numeric comparisons are handled automatically.
5. When the request cannot be expressed with these steps, do the closest thing and say what is missing in "explanation".
6. The user's instruction may be in any language; keep JSON keys, ops and format ids in English, but write "name", "description" and "explanation" in the user's language ({lang_name})."#;

/// The system prompt with the user's language filled in.
pub fn system_prompt(lang: crate::i18n::Lang) -> String {
    let lang_name = match lang {
        crate::i18n::Lang::En => "English",
        crate::i18n::Lang::Tr => "Turkish (Türkçe)",
    };
    SYSTEM_PROMPT.replace("{lang_name}", lang_name)
}

fn truncate_sample(sample: &str, max_lines: usize, max_chars: usize) -> String {
    let mut out = String::new();
    for (i, line) in sample.lines().enumerate() {
        if i >= max_lines || out.len() + line.len() > max_chars {
            out.push_str("\n... (truncated)");
            break;
        }
        if i > 0 {
            out.push('\n');
        }
        out.push_str(line);
    }
    out
}

impl SampleContext {
    pub fn describe(&self) -> String {
        let mut lines = Vec::new();
        if let Some(f) = self.format {
            let delim = match self.delimiter {
                Some('\t') => " (tab separated)".to_string(),
                Some(c) => format!(" (delimiter '{c}')"),
                None => String::new(),
            };
            lines.push(format!("Detected input format: {}{}", f.label(), delim));
        }
        if let Some(s) = &self.summary {
            match s.kind {
                crate::model::DocKind::Table => {
                    lines.push(format!(
                        "Shape: table with {} rows and {} columns (header row: {})",
                        s.rows.unwrap_or(0),
                        s.columns.unwrap_or(0),
                        s.has_header.unwrap_or(true)
                    ));
                    if let (Some(h), Some(t)) = (&s.headers, &s.column_types) {
                        let cols: Vec<String> = h
                            .iter()
                            .zip(t.iter())
                            .map(|(h, t)| format!("{h} ({t})"))
                            .collect();
                        lines.push(format!("Columns: {}", cols.join(", ")));
                    }
                }
                crate::model::DocKind::Tree => lines.push(format!(
                    "Shape: JSON-like {} with {} top-level entries",
                    s.tree_kind.clone().unwrap_or_default(),
                    s.items.unwrap_or(0)
                )),
                crate::model::DocKind::List => {
                    lines.push(format!("Shape: list with {} items", s.items.unwrap_or(0)))
                }
                crate::model::DocKind::Text => lines.push(format!(
                    "Shape: plain text with {} lines",
                    s.items.unwrap_or(0)
                )),
            }
        }
        lines.join("\n")
    }
}

/// Build the user message for a fresh generation.
pub fn build_user_prompt(
    instruction: &str,
    ctx: &SampleContext,
    existing: Option<&Recipe>,
    sample_lines: usize,
) -> String {
    let mut parts = Vec::new();
    let described = ctx.describe();
    if !described.is_empty() {
        parts.push(described);
    }
    if !ctx.sample.trim().is_empty() {
        parts.push(format!(
            "Sample of the pasted data:\n```\n{}\n```",
            truncate_sample(&ctx.sample, sample_lines, 4000)
        ));
    } else {
        parts.push(
            "(The user chose not to share the data itself; rely on the description above.)"
                .to_string(),
        );
    }
    if let Some(r) = existing {
        parts.push(format!(
            "Existing recipe to modify:\n{}",
            serde_json::to_string(r).unwrap_or_default()
        ));
    }
    parts.push(format!(
        "Instruction from the user:\n{}",
        instruction.trim()
    ));
    parts.push("Return only the JSON recipe object.".to_string());
    parts.join("\n\n")
}

/// Build the follow-up message after a failed attempt.
pub fn build_repair_prompt(error: &str) -> String {
    format!(
        "That recipe did not work: {error}\n\nFix the problem and return only the corrected JSON recipe object. Use only the ops, conditions, transforms and output formats from the schema, and only columns that exist in the sample."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DocKind, DocSummary};

    #[test]
    fn user_prompt_includes_context_and_sample() {
        let ctx = SampleContext {
            format: Some(InputFormat::Csv),
            delimiter: Some(';'),
            summary: Some(DocSummary {
                kind: DocKind::Table,
                rows: Some(3),
                columns: Some(2),
                headers: Some(vec!["a".into(), "b".into()]),
                has_header: Some(true),
                items: None,
                tree_kind: None,
                column_types: Some(vec!["integer".into(), "string".into()]),
            }),
            sample: "a;b\n1;x\n2;y".into(),
        };
        let p = build_user_prompt("make json", &ctx, None, 20);
        assert!(p.contains("Detected input format: CSV (delimiter ';')"));
        assert!(p.contains("Columns: a (integer), b (string)"));
        assert!(p.contains("a;b\n1;x"));
        assert!(p.contains("Instruction from the user:\nmake json"));
    }

    #[test]
    fn sample_is_truncated() {
        let long: String = (0..100)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let t = truncate_sample(&long, 5, 4000);
        assert_eq!(t.lines().count(), 6);
        assert!(t.ends_with("(truncated)"));
        let ctx = SampleContext {
            sample: String::new(),
            ..Default::default()
        };
        assert!(build_user_prompt("x", &ctx, None, 20).contains("chose not to share"));
    }

    #[test]
    fn system_prompt_names_the_language() {
        assert!(system_prompt(crate::i18n::Lang::Tr).contains("Turkish"));
        assert!(system_prompt(crate::i18n::Lang::En).contains("(English)"));
        assert!(!system_prompt(crate::i18n::Lang::En).contains("{lang_name}"));
    }

    #[test]
    fn system_prompt_mentions_every_output_format() {
        for f in crate::model::OutputFormat::all() {
            assert!(
                SYSTEM_PROMPT.contains(&f.id()),
                "system prompt is missing {}",
                f.id()
            );
        }
    }
}
