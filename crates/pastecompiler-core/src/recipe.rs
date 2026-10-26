//! Recipes: declarative, shareable conversion pipelines.

use crate::coerce::{self, table_to_value, to_list, to_table, to_text, to_tree, unflatten_map};
use crate::error::{Error, Result};
use crate::model::{Diagnostics, Document, InputFormat, OutputFormat, Scalar, Table};
use crate::output::OutputOptions;
use crate::parse::ParseOptions;
use crate::template;
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const SCHEMA_VERSION: u32 = 1;
pub const SHARE_PREFIX: &str = "pcr1.";

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn schema_version() -> u32 {
    SCHEMA_VERSION
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct InputSpec {
    /// Expected input format; `None` means auto-detect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<InputFormat>,
    #[serde(flatten)]
    pub options: ParseOptions,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputSpec {
    pub format: OutputFormat,
    #[serde(flatten)]
    pub options: OutputOptions,
}

impl Default for OutputSpec {
    fn default() -> Self {
        OutputSpec {
            format: OutputFormat::Json,
            options: OutputOptions::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recipe {
    #[serde(default = "new_id")]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "schema_version")]
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    /// The natural-language instruction this recipe was generated from, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default)]
    pub input: InputSpec,
    #[serde(default)]
    pub steps: Vec<Step>,
    #[serde(default)]
    pub output: OutputSpec,
    #[serde(default)]
    pub builtin: bool,
    /// Localized name/description keyed by language tag (e.g. `tr`).
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub translations: std::collections::HashMap<String, RecipeText>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RecipeText {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
}

impl Recipe {
    /// Name in `lang`, falling back to the original name.
    pub fn name_in(&self, lang: crate::i18n::Lang) -> &str {
        self.translations
            .get(lang.tag())
            .filter(|t| !t.name.trim().is_empty())
            .map(|t| t.name.as_str())
            .unwrap_or(&self.name)
    }

    pub fn new(name: &str, output: OutputFormat) -> Recipe {
        Recipe {
            id: new_id(),
            name: name.to_string(),
            description: String::new(),
            schema_version: SCHEMA_VERSION,
            author: None,
            tags: vec![],
            created_at: Some(chrono::Utc::now().to_rfc3339()),
            updated_at: None,
            prompt: None,
            input: InputSpec::default(),
            steps: vec![],
            output: OutputSpec {
                format: output,
                options: OutputOptions::default(),
            },
            builtin: false,
            translations: std::collections::HashMap::new(),
        }
    }

    pub fn touch(&mut self) {
        self.updated_at = Some(chrono::Utc::now().to_rfc3339());
        if self.created_at.is_none() {
            self.created_at = self.updated_at.clone();
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// Encode as a compact shareable string (`pcr1.<base64url json>`).
    pub fn to_share_string(&self) -> String {
        let json = serde_json::to_string(self).unwrap_or_default();
        format!(
            "{SHARE_PREFIX}{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json.as_bytes())
        )
    }

    /// Parse a recipe from a share string or raw JSON.
    pub fn from_share_string(input: &str) -> Result<Recipe> {
        let t = input.trim();
        if let Some(rest) = t.strip_prefix(SHARE_PREFIX) {
            let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(rest.trim())
                .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(rest.trim()))
                .map_err(|e| Error::Recipe(crate::i18n::t("err_share_string", &[&e])))?;
            let json = String::from_utf8(bytes)
                .map_err(|e| Error::Recipe(crate::i18n::t("err_share_string", &[&e])))?;
            return parse_recipe_lenient(&json);
        }
        parse_recipe_lenient(t)
    }

    /// Validate the recipe without running it.
    pub fn check(&self) -> Vec<String> {
        let mut problems = Vec::new();
        if self.name.trim().is_empty() {
            problems.push(crate::i18n::t("check_no_name", &[]));
        }
        if self.schema_version > SCHEMA_VERSION {
            problems.push(crate::i18n::t(
                "check_schema_version",
                &[&self.schema_version, &SCHEMA_VERSION],
            ));
        }
        if self.output.format == OutputFormat::Template
            && self
                .output
                .options
                .template
                .as_deref()
                .unwrap_or("")
                .trim()
                .is_empty()
        {
            problems.push(crate::i18n::t("check_template_missing", &[]));
        }
        for (i, step) in self.steps.iter().enumerate() {
            if let Step::Replace {
                pattern,
                regex: true,
                ..
            } = step
            {
                if let Err(e) = regex::Regex::new(pattern) {
                    problems.push(crate::i18n::t("check_step_regex", &[&(i + 1), &e]));
                }
            }
            if let Step::FilterRows {
                condition: Condition::Regex,
                value,
                ..
            } = step
            {
                if let Err(e) = regex::Regex::new(value.as_deref().unwrap_or("")) {
                    problems.push(crate::i18n::t("check_step_regex", &[&(i + 1), &e]));
                }
            }
            if let Step::SelectColumns { columns } | Step::DropColumns { columns } = step {
                if columns.is_empty() {
                    problems.push(crate::i18n::t("check_step_no_columns", &[&(i + 1)]));
                }
            }
        }
        problems
    }
}

// ---------------------------------------------------------------------------
// Steps
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Condition {
    Eq,
    Ne,
    Contains,
    NotContains,
    StartsWith,
    EndsWith,
    Gt,
    Gte,
    Lt,
    Lte,
    Regex,
    Empty,
    NotEmpty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Transform {
    Upper,
    Lower,
    Title,
    Trim,
    SnakeCase,
    CamelCase,
    PascalCase,
    KebabCase,
    ToNumber,
    ToString,
    ToBool,
    Round,
    Prefix,
    Suffix,
    Length,
    Default,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseStyle {
    Snake,
    Camel,
    Pascal,
    Kebab,
    Upper,
    Lower,
    Title,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineTransform {
    Sort,
    SortDesc,
    Dedupe,
    Trim,
    RemoveEmpty,
    Upper,
    Lower,
    Reverse,
}

/// One transformation in a recipe. Most steps work on tables; other document
/// shapes are converted to a table first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Step {
    /// Keep only these columns, in this order.
    SelectColumns {
        columns: Vec<String>,
    },
    DropColumns {
        columns: Vec<String>,
    },
    RenameColumn {
        from: String,
        to: String,
    },
    FilterRows {
        column: String,
        condition: Condition,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<String>,
    },
    SortRows {
        column: String,
        #[serde(default)]
        descending: bool,
    },
    Limit {
        count: usize,
        #[serde(default)]
        offset: usize,
    },
    MapColumn {
        column: String,
        transform: Transform,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        arg: Option<String>,
    },
    /// Add a column computed from a template, e.g. `{{first}} {{last}}`.
    AddColumn {
        name: String,
        template: String,
    },
    SplitColumn {
        column: String,
        separator: String,
        into: Vec<String>,
    },
    Dedupe {
        #[serde(default)]
        columns: Vec<String>,
    },
    Transpose,
    /// Convert nested data into a flat table with dotted column names.
    Flatten,
    /// Rebuild nested objects from dotted column names.
    Unflatten,
    HeaderCase {
        case: CaseStyle,
    },
    Replace {
        pattern: String,
        replacement: String,
        #[serde(default)]
        regex: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        column: Option<String>,
    },
    TrimCells,
    FillEmpty {
        value: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        column: Option<String>,
    },
    TextLines {
        transform: LineTransform,
    },
    ToTable,
    ToTree,
    ToList,
    ToText,
}

impl Step {
    pub fn name(&self) -> &'static str {
        match self {
            Step::SelectColumns { .. } => "select_columns",
            Step::DropColumns { .. } => "drop_columns",
            Step::RenameColumn { .. } => "rename_column",
            Step::FilterRows { .. } => "filter_rows",
            Step::SortRows { .. } => "sort_rows",
            Step::Limit { .. } => "limit",
            Step::MapColumn { .. } => "map_column",
            Step::AddColumn { .. } => "add_column",
            Step::SplitColumn { .. } => "split_column",
            Step::Dedupe { .. } => "dedupe",
            Step::Transpose => "transpose",
            Step::Flatten => "flatten",
            Step::Unflatten => "unflatten",
            Step::HeaderCase { .. } => "header_case",
            Step::Replace { .. } => "replace",
            Step::TrimCells => "trim_cells",
            Step::FillEmpty { .. } => "fill_empty",
            Step::TextLines { .. } => "text_lines",
            Step::ToTable => "to_table",
            Step::ToTree => "to_tree",
            Step::ToList => "to_list",
            Step::ToText => "to_text",
        }
    }
}

fn ensure_table(doc: Document, step: &str, diag: &mut Diagnostics) -> Table {
    match doc {
        Document::Table(t) => t,
        other => {
            diag.info_t("coerced_to_table", &[&step]);
            to_table(&other, diag)
        }
    }
}

fn column_or_err(table: &Table, name: &str) -> Result<usize> {
    table.column_index(name).ok_or_else(|| {
        Error::Recipe(crate::i18n::t(
            "err_column_not_found",
            &[&name, &table.headers.join(", ")],
        ))
    })
}

fn apply_case(s: &str, case: CaseStyle) -> String {
    use heck::{ToKebabCase, ToLowerCamelCase, ToSnakeCase, ToUpperCamelCase};
    match case {
        CaseStyle::Snake => s.to_snake_case(),
        CaseStyle::Camel => s.to_lower_camel_case(),
        CaseStyle::Pascal => s.to_upper_camel_case(),
        CaseStyle::Kebab => s.to_kebab_case(),
        CaseStyle::Upper => s.to_uppercase(),
        CaseStyle::Lower => s.to_lowercase(),
        CaseStyle::Title => template::title_case(s),
    }
}

fn apply_transform(
    cell: &Scalar,
    transform: Transform,
    arg: Option<&str>,
    diag: &mut Diagnostics,
    column: &str,
) -> Scalar {
    use heck::{ToKebabCase, ToLowerCamelCase, ToSnakeCase, ToUpperCamelCase};
    let text = cell.as_text();
    match transform {
        Transform::Upper => Scalar::Str(text.to_uppercase()),
        Transform::Lower => Scalar::Str(text.to_lowercase()),
        Transform::Title => Scalar::Str(template::title_case(&text)),
        Transform::Trim => match cell {
            Scalar::Str(s) => Scalar::Str(s.trim().to_string()),
            other => other.clone(),
        },
        Transform::SnakeCase => Scalar::Str(text.to_snake_case()),
        Transform::CamelCase => Scalar::Str(text.to_lower_camel_case()),
        Transform::PascalCase => Scalar::Str(text.to_upper_camel_case()),
        Transform::KebabCase => Scalar::Str(text.to_kebab_case()),
        Transform::ToNumber => {
            if cell.is_null() {
                return Scalar::Null;
            }
            if cell.is_numeric() {
                return cell.clone();
            }
            let cleaned: String = text
                .trim()
                .replace(',', ".")
                .chars()
                .filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-' || *c == 'e' || *c == 'E')
                .collect();
            match Scalar::infer(&cleaned) {
                s if s.is_numeric() => s,
                _ => match cleaned.parse::<f64>() {
                    Ok(f) if f.is_finite() => Scalar::Float(f),
                    _ => {
                        diag.warn_t("to_number_failed", &[&column, &format!("{text:?}")]);
                        Scalar::Null
                    }
                },
            }
        }
        Transform::ToString => {
            if cell.is_null() {
                Scalar::Null
            } else {
                Scalar::Str(text)
            }
        }
        Transform::ToBool => match text.trim().to_lowercase().as_str() {
            "" => Scalar::Null,
            "true" | "yes" | "y" | "1" | "on" | "evet" => Scalar::Bool(true),
            "false" | "no" | "n" | "0" | "off" | "hayır" | "hayir" => Scalar::Bool(false),
            _ => {
                diag.warn_t("to_bool_failed", &[&column, &format!("{text:?}")]);
                Scalar::Null
            }
        },
        Transform::Round => {
            let decimals: i32 = arg.and_then(|a| a.trim().parse().ok()).unwrap_or(0);
            match cell.as_f64() {
                Some(f) if !cell.is_null() => {
                    let factor = 10f64.powi(decimals);
                    let rounded = (f * factor).round() / factor;
                    if decimals <= 0 && rounded.abs() < i64::MAX as f64 {
                        Scalar::Int(rounded as i64)
                    } else {
                        Scalar::Float(rounded)
                    }
                }
                _ => cell.clone(),
            }
        }
        Transform::Prefix => {
            if cell.is_null() {
                Scalar::Null
            } else {
                Scalar::Str(format!("{}{text}", arg.unwrap_or("")))
            }
        }
        Transform::Suffix => {
            if cell.is_null() {
                Scalar::Null
            } else {
                Scalar::Str(format!("{text}{}", arg.unwrap_or("")))
            }
        }
        Transform::Length => Scalar::Int(text.chars().count() as i64),
        Transform::Default => {
            if cell.is_null() || text.is_empty() {
                Scalar::infer(arg.unwrap_or(""))
            } else {
                cell.clone()
            }
        }
    }
}

fn compare_cells(a: &Scalar, b: &Scalar) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (a.is_null(), b.is_null()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }
    if let (Some(x), Some(y)) = (a.as_f64(), b.as_f64()) {
        if a.is_numeric() || b.is_numeric() {
            return x.partial_cmp(&y).unwrap_or(Ordering::Equal);
        }
    }
    let (ta, tb) = (a.as_text().to_lowercase(), b.as_text().to_lowercase());
    ta.cmp(&tb).then_with(|| a.as_text().cmp(&b.as_text()))
}

fn matches_condition(cell: &Scalar, condition: Condition, value: Option<&str>) -> Result<bool> {
    let text = cell.as_text();
    let v = value.unwrap_or("");
    let numeric = |f: fn(f64, f64) -> bool| -> bool {
        match (cell.as_f64(), v.trim().replace(',', ".").parse::<f64>()) {
            (Some(a), Ok(b)) if !cell.is_null() => f(a, b),
            _ => f_str(&text, v, f),
        }
    };
    fn f_str(a: &str, b: &str, f: fn(f64, f64) -> bool) -> bool {
        let ord = a.to_lowercase().cmp(&b.to_lowercase());
        let (x, y) = match ord {
            std::cmp::Ordering::Less => (0.0, 1.0),
            std::cmp::Ordering::Equal => (0.0, 0.0),
            std::cmp::Ordering::Greater => (1.0, 0.0),
        };
        f(x, y)
    }
    Ok(match condition {
        Condition::Eq => {
            text.eq_ignore_ascii_case(v)
                || (cell.as_f64().is_some()
                    && !cell.is_null()
                    && v.parse::<f64>().ok() == cell.as_f64())
        }
        Condition::Ne => {
            !(text.eq_ignore_ascii_case(v)
                || (cell.as_f64().is_some()
                    && !cell.is_null()
                    && v.parse::<f64>().ok() == cell.as_f64()))
        }
        Condition::Contains => text.to_lowercase().contains(&v.to_lowercase()),
        Condition::NotContains => !text.to_lowercase().contains(&v.to_lowercase()),
        Condition::StartsWith => text.to_lowercase().starts_with(&v.to_lowercase()),
        Condition::EndsWith => text.to_lowercase().ends_with(&v.to_lowercase()),
        Condition::Gt => numeric(|a, b| a > b),
        Condition::Gte => numeric(|a, b| a >= b),
        Condition::Lt => numeric(|a, b| a < b),
        Condition::Lte => numeric(|a, b| a <= b),
        Condition::Regex => regex::Regex::new(v)
            .map_err(|e| Error::Recipe(crate::i18n::t("err_invalid_regex", &[&v, &e])))?
            .is_match(&text),
        Condition::Empty => cell.is_null() || text.trim().is_empty(),
        Condition::NotEmpty => !(cell.is_null() || text.trim().is_empty()),
    })
}

fn map_strings_in_value(v: &mut Value, f: &dyn Fn(&str) -> String) {
    match v {
        Value::String(s) => *s = f(s),
        Value::Array(a) => a.iter_mut().for_each(|x| map_strings_in_value(x, f)),
        Value::Object(o) => o.values_mut().for_each(|x| map_strings_in_value(x, f)),
        _ => {}
    }
}

fn apply_lines(doc: Document, transform: LineTransform, diag: &mut Diagnostics) -> Document {
    let (mut lines, was_text): (Vec<String>, bool) = match &doc {
        Document::Text { text } => (text.lines().map(|l| l.to_string()).collect(), true),
        Document::List { items } => (items.clone(), false),
        other => (to_list(other, diag), false),
    };
    match transform {
        LineTransform::Sort => lines.sort_by_key(|l| l.to_lowercase()),
        LineTransform::SortDesc => {
            lines.sort_by_key(|l| l.to_lowercase());
            lines.reverse();
        }
        LineTransform::Dedupe => {
            let before = lines.len();
            let mut seen = std::collections::HashSet::new();
            lines.retain(|l| seen.insert(l.clone()));
            if lines.len() != before {
                diag.info_t("lines_deduped", &[&(before - lines.len())]);
            }
        }
        LineTransform::Trim => lines = lines.iter().map(|l| l.trim().to_string()).collect(),
        LineTransform::RemoveEmpty => {
            let before = lines.len();
            lines.retain(|l| !l.trim().is_empty());
            if lines.len() != before {
                diag.info_t("lines_removed", &[&(before - lines.len())]);
            }
        }
        LineTransform::Upper => lines = lines.iter().map(|l| l.to_uppercase()).collect(),
        LineTransform::Lower => lines = lines.iter().map(|l| l.to_lowercase()).collect(),
        LineTransform::Reverse => lines.reverse(),
    }
    if was_text {
        Document::text(lines.join("\n"))
    } else {
        Document::list(lines)
    }
}

/// Apply a single step.
pub fn apply_step(doc: Document, step: &Step, diag: &mut Diagnostics) -> Result<Document> {
    let name = step.name();
    Ok(match step {
        Step::SelectColumns { columns } => {
            let t = ensure_table(doc, name, diag);
            let idx: Vec<usize> = columns
                .iter()
                .map(|c| column_or_err(&t, c))
                .collect::<Result<_>>()?;
            let headers = idx.iter().map(|i| t.headers[*i].clone()).collect();
            let rows = t
                .rows
                .iter()
                .map(|r| {
                    idx.iter()
                        .map(|i| r.get(*i).cloned().unwrap_or(Scalar::Null))
                        .collect()
                })
                .collect();
            let dropped = t.width() - idx.len();
            if dropped > 0 {
                diag.info_t("columns_left_out", &[&dropped]);
            }
            Document::Table(Table {
                headers,
                rows,
                has_header: t.has_header,
            })
        }
        Step::DropColumns { columns } => {
            let t = ensure_table(doc, name, diag);
            let drop: Vec<usize> = columns
                .iter()
                .map(|c| column_or_err(&t, c))
                .collect::<Result<_>>()?;
            let keep: Vec<usize> = (0..t.width()).filter(|i| !drop.contains(i)).collect();
            let headers = keep.iter().map(|i| t.headers[*i].clone()).collect();
            let rows = t
                .rows
                .iter()
                .map(|r| {
                    keep.iter()
                        .map(|i| r.get(*i).cloned().unwrap_or(Scalar::Null))
                        .collect()
                })
                .collect();
            diag.info_t("columns_dropped", &[&drop.len()]);
            Document::Table(Table {
                headers,
                rows,
                has_header: t.has_header,
            })
        }
        Step::RenameColumn { from, to } => {
            let mut t = ensure_table(doc, name, diag);
            let i = column_or_err(&t, from)?;
            t.headers[i] = to.clone();
            t.has_header = true;
            Document::Table(t)
        }
        Step::FilterRows {
            column,
            condition,
            value,
        } => {
            let mut t = ensure_table(doc, name, diag);
            let i = column_or_err(&t, column)?;
            let before = t.rows.len();
            let mut kept = Vec::with_capacity(before);
            for row in t.rows.into_iter() {
                let cell = row.get(i).cloned().unwrap_or(Scalar::Null);
                if matches_condition(&cell, *condition, value.as_deref())? {
                    kept.push(row);
                }
            }
            t.rows = kept;
            diag.info_t("rows_filtered", &[&t.rows.len(), &before]);
            Document::Table(t)
        }
        Step::SortRows { column, descending } => {
            let mut t = ensure_table(doc, name, diag);
            let i = column_or_err(&t, column)?;
            t.rows.sort_by(|a, b| {
                let ord = compare_cells(
                    a.get(i).unwrap_or(&Scalar::Null),
                    b.get(i).unwrap_or(&Scalar::Null),
                );
                if *descending {
                    ord.reverse()
                } else {
                    ord
                }
            });
            Document::Table(t)
        }
        Step::Limit { count, offset } => {
            let mut t = ensure_table(doc, name, diag);
            let before = t.rows.len();
            t.rows = t.rows.into_iter().skip(*offset).take(*count).collect();
            if t.rows.len() != before {
                diag.info_t("rows_limited", &[&t.rows.len(), &before]);
            }
            Document::Table(t)
        }
        Step::MapColumn {
            column,
            transform,
            arg,
        } => {
            let mut t = ensure_table(doc, name, diag);
            let i = column_or_err(&t, column)?;
            let col_name = t.headers[i].clone();
            for row in &mut t.rows {
                if let Some(cell) = row.get_mut(i) {
                    *cell = apply_transform(cell, *transform, arg.as_deref(), diag, &col_name);
                }
            }
            Document::Table(t)
        }
        Step::AddColumn {
            name: col,
            template: tpl,
        } => {
            let mut t = ensure_table(doc, name, diag);
            let rows_vars = crate::output::template_rows(&Document::Table(t.clone()), diag);
            for (row, vars) in t.rows.iter_mut().zip(rows_vars.iter()) {
                let rendered = template::render(tpl, vars, false)?;
                row.push(Scalar::infer(&rendered));
            }
            t.headers.push(col.clone());
            t.has_header = true;
            Document::Table(t)
        }
        Step::SplitColumn {
            column,
            separator,
            into,
        } => {
            let mut t = ensure_table(doc, name, diag);
            let i = column_or_err(&t, column)?;
            if into.is_empty() {
                return Err(Error::Recipe(crate::i18n::t("err_split_targets", &[])));
            }
            let mut rows = Vec::with_capacity(t.rows.len());
            let mut overflow = 0usize;
            for mut row in t.rows.drain(..) {
                let text = row.get(i).map(|c| c.as_text()).unwrap_or_default();
                let parts: Vec<&str> = if separator.is_empty() {
                    vec![text.as_str()]
                } else {
                    text.split(separator.as_str()).collect()
                };
                if parts.len() > into.len() {
                    overflow += 1;
                }
                let mut new_cells: Vec<Scalar> = (0..into.len())
                    .map(|k| {
                        if k == into.len() - 1 && parts.len() > into.len() {
                            Scalar::infer(&parts[k..].join(separator))
                        } else {
                            parts
                                .get(k)
                                .map(|p| Scalar::infer(p.trim()))
                                .unwrap_or(Scalar::Null)
                        }
                    })
                    .collect();
                row.remove(i);
                let tail = row.split_off(i);
                row.append(&mut new_cells);
                row.extend(tail);
                rows.push(row);
            }
            if overflow > 0 {
                diag.warn_t("split_overflow", &[&overflow]);
            }
            t.headers.remove(i);
            let tail: Vec<String> = t.headers.split_off(i);
            t.headers.extend(into.iter().cloned());
            t.headers.extend(tail);
            t.rows = rows;
            t.has_header = true;
            Document::Table(t)
        }
        Step::Dedupe { columns } => {
            let mut t = ensure_table(doc, name, diag);
            let idx: Vec<usize> = if columns.is_empty() {
                (0..t.width()).collect()
            } else {
                columns
                    .iter()
                    .map(|c| column_or_err(&t, c))
                    .collect::<Result<_>>()?
            };
            let before = t.rows.len();
            let mut seen = std::collections::HashSet::new();
            t.rows.retain(|r| {
                let key: Vec<String> = idx
                    .iter()
                    .map(|i| r.get(*i).map(|c| c.as_text()).unwrap_or_default())
                    .collect();
                seen.insert(key)
            });
            if t.rows.len() != before {
                diag.info_t("rows_deduped", &[&(before - t.rows.len())]);
            }
            Document::Table(t)
        }
        Step::Transpose => Document::Table(coerce::transpose(&ensure_table(doc, name, diag))),
        Step::Flatten => Document::Table(ensure_table(doc, name, diag)),
        Step::Unflatten => match doc {
            Document::Table(t) => {
                let value = table_to_value(&t);
                let items = match value {
                    Value::Array(items) => items
                        .into_iter()
                        .map(|v| match v {
                            Value::Object(o) => unflatten_map(&o),
                            other => other,
                        })
                        .collect(),
                    other => vec![other],
                };
                Document::tree(Value::Array(items))
            }
            Document::Tree { value } => Document::tree(match value {
                Value::Object(o) => unflatten_map(&o),
                Value::Array(items) => Value::Array(
                    items
                        .into_iter()
                        .map(|v| match v {
                            Value::Object(o) => unflatten_map(&o),
                            other => other,
                        })
                        .collect(),
                ),
                other => other,
            }),
            other => Document::tree(to_tree(&other, diag)),
        },
        Step::HeaderCase { case } => {
            let mut t = ensure_table(doc, name, diag);
            t.headers = t.headers.iter().map(|h| apply_case(h, *case)).collect();
            for note in t.normalize() {
                diag.warn("table_normalized", note);
            }
            Document::Table(t)
        }
        Step::Replace {
            pattern,
            replacement,
            regex: use_regex,
            column,
        } => {
            let replacer: Box<dyn Fn(&str) -> String> = if *use_regex {
                let re = regex::Regex::new(pattern).map_err(|e| {
                    Error::Recipe(crate::i18n::t("err_invalid_regex", &[&pattern, &e]))
                })?;
                let rep = replacement.clone();
                Box::new(move |s: &str| re.replace_all(s, rep.as_str()).into_owned())
            } else {
                let (p, r) = (pattern.clone(), replacement.clone());
                Box::new(move |s: &str| {
                    if p.is_empty() {
                        s.to_string()
                    } else {
                        s.replace(&p, &r)
                    }
                })
            };
            match doc {
                Document::Text { text } => Document::text(replacer(&text)),
                Document::List { items } => {
                    Document::list(items.iter().map(|i| replacer(i)).collect())
                }
                Document::Tree { mut value } => {
                    map_strings_in_value(&mut value, &replacer);
                    Document::tree(value)
                }
                Document::Table(mut t) => {
                    let only = match column {
                        Some(c) => Some(column_or_err(&t, c)?),
                        None => None,
                    };
                    for row in &mut t.rows {
                        for (i, cell) in row.iter_mut().enumerate() {
                            if only.map(|o| o == i).unwrap_or(true) {
                                if let Scalar::Str(s) = cell {
                                    *s = replacer(s);
                                }
                            }
                        }
                    }
                    Document::Table(t)
                }
            }
        }
        Step::TrimCells => match doc {
            Document::Table(mut t) => {
                for row in &mut t.rows {
                    for cell in row.iter_mut() {
                        if let Scalar::Str(s) = cell {
                            *s = s.trim().to_string();
                        }
                    }
                }
                Document::Table(t)
            }
            Document::List { items } => {
                Document::list(items.iter().map(|i| i.trim().to_string()).collect())
            }
            Document::Text { text } => Document::text(
                text.lines()
                    .map(|l| l.trim())
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
            other => other,
        },
        Step::FillEmpty { value, column } => {
            let mut t = ensure_table(doc, name, diag);
            let only = match column {
                Some(c) => Some(column_or_err(&t, c)?),
                None => None,
            };
            let fill = Scalar::infer(value);
            let mut filled = 0;
            for row in &mut t.rows {
                for (i, cell) in row.iter_mut().enumerate() {
                    if only.map(|o| o == i).unwrap_or(true)
                        && (cell.is_null() || cell.as_text().trim().is_empty())
                    {
                        *cell = fill.clone();
                        filled += 1;
                    }
                }
            }
            if filled > 0 {
                diag.info_t("cells_filled", &[&filled, &format!("{value:?}")]);
            }
            Document::Table(t)
        }
        Step::TextLines { transform } => apply_lines(doc, *transform, diag),
        Step::ToTable => Document::Table(ensure_table(doc, name, diag)),
        Step::ToTree => match doc {
            Document::Tree { .. } => doc,
            other => Document::tree(to_tree(&other, diag)),
        },
        Step::ToList => match doc {
            Document::List { .. } => doc,
            other => Document::list(to_list(&other, diag)),
        },
        Step::ToText => match doc {
            Document::Text { .. } => doc,
            other => Document::text(to_text(&other)),
        },
    })
}

/// Apply all steps in order.
pub fn apply_steps(mut doc: Document, steps: &[Step], diag: &mut Diagnostics) -> Result<Document> {
    for (i, step) in steps.iter().enumerate() {
        doc = apply_step(doc, step, diag).map_err(|e| {
            Error::Recipe(crate::i18n::t(
                "err_step_failed",
                &[&(i + 1), &step.name(), &e],
            ))
        })?;
    }
    Ok(doc)
}

// ---------------------------------------------------------------------------
// Lenient parsing (for hand-written and AI-generated recipes)
// ---------------------------------------------------------------------------

/// Extract the first JSON object from free text (code fences, prose, ...).
pub fn extract_json_object(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escape = false;
    for (i, b) in bytes.iter().enumerate().skip(start) {
        if in_str {
            if escape {
                escape = false;
            } else if *b == b'\\' {
                escape = true;
            } else if *b == b'"' {
                in_str = false;
            }
            continue;
        }
        match b {
            b'"' => in_str = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[start..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

fn normalize_step(step: &mut Value) {
    let Some(obj) = step.as_object_mut() else {
        return;
    };
    // Accept `type`/`step`/`action` as aliases for `op`.
    for alias in ["type", "step", "action", "operation"] {
        if !obj.contains_key("op") {
            if let Some(v) = obj.remove(alias) {
                obj.insert("op".into(), v);
            }
        }
    }
    if let Some(Value::String(op)) = obj.get_mut("op") {
        let norm = op.trim().to_lowercase().replace(['-', ' '], "_");
        let canonical = match norm.as_str() {
            "select" | "keep_columns" | "pick_columns" | "columns" => "select_columns",
            "drop" | "remove_columns" | "delete_columns" => "drop_columns",
            "rename" | "rename_columns" => "rename_column",
            "filter" | "where" | "filter_row" => "filter_rows",
            "sort" | "order_by" | "sort_by" | "sort_row" => "sort_rows",
            "head" | "take" | "top" | "first" => "limit",
            "map" | "transform" | "transform_column" | "map_columns" => "map_column",
            "add" | "computed_column" | "new_column" => "add_column",
            "split" => "split_column",
            "distinct" | "unique" | "deduplicate" | "dedup" => "dedupe",
            "snake_case_headers" | "rename_headers" | "headers_case" | "header_style" => {
                "header_case"
            }
            "replace_text" | "substitute" | "regex_replace" => "replace",
            "trim" => "trim_cells",
            "fill" | "fill_null" | "fill_empty_cells" => "fill_empty",
            "lines" | "text" => "text_lines",
            other => other,
        };
        *op = canonical.to_string();
    }
    // `column` given as an array for select/drop.
    if let Some(Value::Array(_)) = obj.get("column") {
        if let Some(v) = obj.remove("column") {
            obj.entry("columns").or_insert(v);
        }
    }
    if let Some(Value::String(s)) = obj.get("columns") {
        let list: Vec<Value> = s
            .split(',')
            .map(|c| Value::String(c.trim().to_string()))
            .collect();
        obj.insert("columns".into(), Value::Array(list));
    }
    if let Some(Value::String(s)) = obj.get("column") {
        if obj
            .get("op")
            .and_then(|o| o.as_str())
            .map(|o| o == "select_columns" || o == "drop_columns")
            .unwrap_or(false)
        {
            let list: Vec<Value> = s
                .split(',')
                .map(|c| Value::String(c.trim().to_string()))
                .collect();
            obj.remove("column");
            obj.insert("columns".into(), Value::Array(list));
        }
    }
    // `order: desc` -> `descending: true`
    if let Some(order) = obj.remove("order").or_else(|| obj.remove("direction")) {
        let desc = order
            .as_str()
            .map(|s| s.to_lowercase().starts_with("desc"))
            .unwrap_or(false);
        obj.entry("descending").or_insert(Value::Bool(desc));
    }
    // `operator`/`predicate` -> `condition`
    for alias in ["operator", "predicate", "cond", "comparison"] {
        if !obj.contains_key("condition") {
            if let Some(v) = obj.remove(alias) {
                obj.insert("condition".into(), v);
            }
        }
    }
    if let Some(Value::String(c)) = obj.get_mut("condition") {
        let norm = c.trim().to_lowercase().replace([' ', '-'], "_");
        *c = match norm.as_str() {
            "==" | "=" | "equals" | "equal" | "is" => "eq",
            "!=" | "<>" | "not_equals" | "not_equal" | "is_not" => "ne",
            ">" | "greater_than" | "greater" | "after" => "gt",
            ">=" | "greater_or_equal" | "at_least" => "gte",
            "<" | "less_than" | "less" | "before" => "lt",
            "<=" | "less_or_equal" | "at_most" => "lte",
            "includes" | "like" | "has" => "contains",
            "excludes" | "not_like" => "not_contains",
            "starts" | "begins_with" | "prefix" => "starts_with",
            "ends" | "suffix" => "ends_with",
            "matches" | "match" | "regexp" | "re" => "regex",
            "is_empty" | "null" | "is_null" | "blank" => "empty",
            "is_not_empty" | "not_null" | "present" | "not_blank" | "exists" => "not_empty",
            other => other,
        }
        .to_string();
    }
    if let Some(v) = obj.get("value") {
        if !v.is_string() && !v.is_null() {
            let s = crate::coerce::value_to_line(v);
            obj.insert("value".into(), Value::String(s));
        }
    }
    if let Some(Value::String(t)) = obj.get_mut("transform") {
        let norm = t.trim().to_lowercase().replace([' ', '-'], "_");
        *t = match norm.as_str() {
            "uppercase" | "to_upper" | "upper_case" | "caps" => "upper",
            "lowercase" | "to_lower" | "lower_case" => "lower",
            "titlecase" | "title_case" | "capitalize" | "capitalise" => "title",
            "snake" | "snakecase" | "to_snake_case" => "snake_case",
            "camel" | "camelcase" | "to_camel_case" => "camel_case",
            "pascal" | "pascalcase" => "pascal_case",
            "kebab" | "kebabcase" | "slug" | "slugify" => "kebab_case",
            "number" | "to_float" | "to_int" | "numeric" | "parse_number" | "int" | "float" => {
                "to_number"
            }
            "string" | "text" | "str" | "stringify" => "to_string",
            "bool" | "boolean" | "to_boolean" => "to_bool",
            "len" | "count" => "length",
            other => other,
        }
        .to_string();
    }
    if let Some(v) = obj.get("arg") {
        if !v.is_string() && !v.is_null() {
            let s = crate::coerce::value_to_line(v);
            obj.insert("arg".into(), Value::String(s));
        }
    }
    for key in [
        "decimals",
        "digits",
        "precision",
        "prefix",
        "suffix",
        "default",
    ] {
        if !obj.contains_key("arg") {
            if let Some(v) = obj.remove(key) {
                obj.insert(
                    "arg".into(),
                    Value::String(crate::coerce::value_to_line(&v)),
                );
            }
        }
    }
    if let Some(Value::String(c)) = obj.get_mut("case") {
        let norm = c.trim().to_lowercase().replace([' ', '-'], "_");
        *c = norm
            .trim_end_matches("_case")
            .replace("snakecase", "snake")
            .replace("camelcase", "camel")
            .replace("pascalcase", "pascal")
            .replace("kebabcase", "kebab")
            .replace("uppercase", "upper")
            .replace("lowercase", "lower")
            .replace("titlecase", "title");
    }
    if let Some(n) = obj.get("count").cloned() {
        if let Some(s) = n.as_str() {
            if let Ok(num) = s.trim().parse::<u64>() {
                obj.insert("count".into(), Value::from(num));
            }
        }
    }
    for alias in ["n", "limit", "rows", "max"] {
        if !obj.contains_key("count") {
            if let Some(v) = obj.remove(alias) {
                obj.insert("count".into(), v);
            }
        }
    }
    let is_text_lines = obj.get("op").and_then(|o| o.as_str()) == Some("text_lines");
    if let Some(Value::String(t)) = obj.get_mut("transform") {
        if is_text_lines {
            let norm = t.trim().to_lowercase().replace([' ', '-'], "_");
            *t = match norm.as_str() {
                "sort_asc" | "sorted" | "alphabetical" => "sort",
                "sort_descending" | "reverse_sort" => "sort_desc",
                "unique" | "distinct" | "deduplicate" => "dedupe",
                "remove_blank" | "remove_empty_lines" | "strip_empty" | "compact" => "remove_empty",
                "uppercase" => "upper",
                "lowercase" => "lower",
                other => other,
            }
            .to_string();
        }
    }
}

/// Parse a recipe from JSON text, tolerating common variations in field names.
pub fn parse_recipe_lenient(text: &str) -> Result<Recipe> {
    let json_text = extract_json_object(text)
        .ok_or_else(|| Error::Recipe(crate::i18n::t("err_recipe_no_json", &[])))?;
    let mut value: Value = serde_json::from_str(json_text)
        .map_err(|e| Error::Recipe(crate::i18n::t("err_recipe_invalid_json", &[&e])))?;
    let obj = value
        .as_object_mut()
        .ok_or_else(|| Error::Recipe(crate::i18n::t("err_recipe_not_object", &[])))?;

    // Unwrap {"recipe": {...}}
    if obj.len() == 1 && obj.contains_key("recipe") {
        if let Some(Value::Object(inner)) = obj.remove("recipe") {
            *obj = inner;
        }
    }
    if !obj.contains_key("name") || obj.get("name").map(|n| !n.is_string()).unwrap_or(true) {
        let title = obj
            .remove("title")
            .and_then(|t| t.as_str().map(|s| s.to_string()));
        obj.insert(
            "name".into(),
            Value::String(title.unwrap_or_else(|| "Untitled recipe".to_string())),
        );
    }
    if let Some(d) = obj.get("description") {
        if !d.is_string() {
            obj.insert(
                "description".into(),
                Value::String(crate::coerce::value_to_line(d)),
            );
        }
    }
    if let Some(Value::String(id)) = obj.get("id") {
        if id.trim().is_empty() {
            obj.remove("id");
        }
    } else if obj.contains_key("id") {
        obj.remove("id");
    }
    obj.remove("builtin");

    // input: "csv" | {"format": "auto", ...}
    match obj.remove("input") {
        Some(Value::String(s)) => {
            let mut m = serde_json::Map::new();
            if let Some(f) = InputFormat::parse_id(&s) {
                m.insert("format".into(), Value::String(f.id()));
            }
            obj.insert("input".into(), Value::Object(m));
        }
        Some(Value::Object(mut m)) => {
            match m.get("format").cloned() {
                Some(Value::String(s)) => match InputFormat::parse_id(&s) {
                    Some(f) => {
                        m.insert("format".into(), Value::String(f.id()));
                    }
                    None => {
                        m.remove("format");
                    }
                },
                Some(_) => {
                    m.remove("format");
                }
                None => {}
            }
            if let Some(Value::String(d)) = m.get("delimiter").cloned() {
                let ch = match d.as_str() {
                    "\\t" | "tab" | "TAB" => Some('\t'),
                    "comma" => Some(','),
                    "semicolon" => Some(';'),
                    "pipe" => Some('|'),
                    s => s.chars().next(),
                };
                match ch {
                    Some(c) => {
                        m.insert("delimiter".into(), Value::String(c.to_string()));
                    }
                    None => {
                        m.remove("delimiter");
                    }
                }
            }
            obj.insert("input".into(), Value::Object(m));
        }
        Some(_) | None => {
            obj.insert("input".into(), Value::Object(serde_json::Map::new()));
        }
    }

    // output: "csv" | {"format": "...", ...options}
    match obj.remove("output") {
        Some(Value::String(s)) => {
            let f = OutputFormat::parse_id(&s)
                .ok_or_else(|| Error::Recipe(crate::i18n::t("err_unknown_output_format", &[&s])))?;
            let mut m = serde_json::Map::new();
            m.insert("format".into(), Value::String(f.id()));
            obj.insert("output".into(), Value::Object(m));
        }
        Some(Value::Object(mut m)) => {
            if let Some(Value::Object(options)) = m.remove("options") {
                for (k, v) in options {
                    m.entry(k).or_insert(v);
                }
            }
            let fmt_str = m
                .get("format")
                .and_then(|f| f.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| "json".to_string());
            let f = OutputFormat::parse_id(&fmt_str).ok_or_else(|| {
                Error::Recipe(crate::i18n::t("err_unknown_output_format", &[&fmt_str]))
            })?;
            m.insert("format".into(), Value::String(f.id()));
            if let Some(Value::String(d)) = m.get("delimiter").cloned() {
                let ch = match d.as_str() {
                    "\\t" | "tab" => Some('\t'),
                    "comma" => Some(','),
                    "semicolon" => Some(';'),
                    "pipe" => Some('|'),
                    s => s.chars().next(),
                };
                match ch {
                    Some(c) => {
                        m.insert("delimiter".into(), Value::String(c.to_string()));
                    }
                    None => {
                        m.remove("delimiter");
                    }
                }
            }
            if let Some(Value::String(s)) = m.get("indent").cloned() {
                if let Ok(n) = s.trim().parse::<u64>() {
                    m.insert("indent".into(), Value::from(n));
                } else {
                    m.remove("indent");
                }
            }
            for key in [
                "quote_all",
                "header",
                "crlf",
                "create_table",
                "batch",
                "declaration",
                "align",
            ] {
                if let Some(Value::String(s)) = m.get(key).cloned() {
                    let b = matches!(s.trim().to_lowercase().as_str(), "true" | "yes" | "1");
                    m.insert(key.into(), Value::Bool(b));
                }
            }
            obj.insert("output".into(), Value::Object(m));
        }
        _ => return Err(Error::Recipe(crate::i18n::t("err_recipe_no_output", &[]))),
    }

    if let Some(Value::Array(steps)) = obj.get_mut("steps") {
        for step in steps.iter_mut() {
            normalize_step(step);
        }
    } else {
        obj.insert("steps".into(), Value::Array(vec![]));
    }

    let recipe: Recipe = serde_json::from_value(value)
        .map_err(|e| Error::Recipe(crate::i18n::t("err_recipe_invalid", &[&e])))?;
    Ok(recipe)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn people() -> Document {
        Document::Table(Table::new(
            vec!["name".into(), "age".into(), "city".into()],
            vec![
                vec!["Ada".into(), Scalar::Int(36), "London".into()],
                vec!["Linus".into(), Scalar::Int(54), "Helsinki".into()],
                vec!["Grace".into(), Scalar::Int(85), "New York".into()],
                vec!["Ada".into(), Scalar::Int(36), "London".into()],
            ],
        ))
    }

    fn table(doc: Document) -> Table {
        match doc {
            Document::Table(t) => t,
            other => panic!("expected table: {other:?}"),
        }
    }

    fn run(doc: Document, steps: Vec<Step>) -> Table {
        table(apply_steps(doc, &steps, &mut Diagnostics::new()).unwrap())
    }

    #[test]
    fn select_drop_rename() {
        let t = run(
            people(),
            vec![Step::SelectColumns {
                columns: vec!["city".into(), "name".into()],
            }],
        );
        assert_eq!(t.headers, vec!["city", "name"]);
        assert_eq!(t.rows[0][0], "London".into());
        let t = run(
            people(),
            vec![Step::DropColumns {
                columns: vec!["age".into()],
            }],
        );
        assert_eq!(t.headers, vec!["name", "city"]);
        let t = run(
            people(),
            vec![Step::RenameColumn {
                from: "name".into(),
                to: "full_name".into(),
            }],
        );
        assert_eq!(t.headers[0], "full_name");
        let err = apply_steps(
            people(),
            &[Step::SelectColumns {
                columns: vec!["nope".into()],
            }],
            &mut Diagnostics::new(),
        );
        assert!(err
            .unwrap_err()
            .to_string()
            .contains("column 'nope' not found"));
    }

    #[test]
    fn filter_sort_limit_dedupe() {
        let t = run(
            people(),
            vec![Step::FilterRows {
                column: "age".into(),
                condition: Condition::Gt,
                value: Some("40".into()),
            }],
        );
        assert_eq!(t.rows.len(), 2);
        let t = run(
            people(),
            vec![Step::FilterRows {
                column: "city".into(),
                condition: Condition::Contains,
                value: Some("lon".into()),
            }],
        );
        assert_eq!(t.rows.len(), 2);
        let t = run(
            people(),
            vec![Step::SortRows {
                column: "age".into(),
                descending: true,
            }],
        );
        assert_eq!(t.rows[0][0], "Grace".into());
        let t = run(
            people(),
            vec![Step::SortRows {
                column: "name".into(),
                descending: false,
            }],
        );
        assert_eq!(t.rows[0][0], "Ada".into());
        assert_eq!(t.rows[3][0], "Linus".into());
        let t = run(
            people(),
            vec![Step::Limit {
                count: 2,
                offset: 1,
            }],
        );
        assert_eq!(t.rows.len(), 2);
        assert_eq!(t.rows[0][0], "Linus".into());
        let t = run(people(), vec![Step::Dedupe { columns: vec![] }]);
        assert_eq!(t.rows.len(), 3);
        let t = run(
            people(),
            vec![Step::FilterRows {
                column: "name".into(),
                condition: Condition::Regex,
                value: Some("^[AG]".into()),
            }],
        );
        assert_eq!(t.rows.len(), 3);
    }

    #[test]
    fn map_add_split() {
        let t = run(
            people(),
            vec![Step::MapColumn {
                column: "name".into(),
                transform: Transform::Upper,
                arg: None,
            }],
        );
        assert_eq!(t.rows[0][0], "ADA".into());
        let t = run(
            people(),
            vec![Step::MapColumn {
                column: "age".into(),
                transform: Transform::Prefix,
                arg: Some("age:".into()),
            }],
        );
        assert_eq!(t.rows[0][1], "age:36".into());
        let t = run(
            people(),
            vec![Step::AddColumn {
                name: "label".into(),
                template: "{{name}} ({{city | upper}})".into(),
            }],
        );
        assert_eq!(t.headers[3], "label");
        assert_eq!(t.rows[1][3], "Linus (HELSINKI)".into());
        let t = run(
            people(),
            vec![Step::SplitColumn {
                column: "city".into(),
                separator: " ".into(),
                into: vec!["c1".into(), "c2".into()],
            }],
        );
        assert_eq!(t.headers, vec!["name", "age", "c1", "c2"]);
        assert_eq!(t.rows[2][2], "New".into());
        assert_eq!(t.rows[2][3], "York".into());
        assert_eq!(t.rows[0][3], Scalar::Null);
    }

    #[test]
    fn numeric_transforms() {
        let doc = Document::Table(Table::new(
            vec!["price".into()],
            vec![
                vec!["12,50".into()],
                vec!["abc".into()],
                vec![Scalar::Float(1.23456)],
            ],
        ));
        let mut d = Diagnostics::new();
        let t = table(
            apply_steps(
                doc,
                &[Step::MapColumn {
                    column: "price".into(),
                    transform: Transform::ToNumber,
                    arg: None,
                }],
                &mut d,
            )
            .unwrap(),
        );
        assert_eq!(t.rows[0][0], Scalar::Float(12.5));
        assert_eq!(t.rows[1][0], Scalar::Null);
        assert!(d.issues.iter().any(|i| i.code == "to_number_failed"));
        let t = run(
            Document::Table(t),
            vec![Step::MapColumn {
                column: "price".into(),
                transform: Transform::Round,
                arg: Some("2".into()),
            }],
        );
        assert_eq!(t.rows[2][0], Scalar::Float(1.23));
    }

    #[test]
    fn header_case_replace_fill_trim() {
        let doc = Document::Table(Table::new(
            vec!["First Name".into(), "E-Mail".into()],
            vec![vec![" ada ".into(), Scalar::Null]],
        ));
        let t = run(
            doc.clone(),
            vec![Step::HeaderCase {
                case: CaseStyle::Snake,
            }],
        );
        assert_eq!(t.headers, vec!["first_name", "e_mail"]);
        let t = run(
            doc.clone(),
            vec![
                Step::TrimCells,
                Step::FillEmpty {
                    value: "n/a".into(),
                    column: None,
                },
            ],
        );
        assert_eq!(t.rows[0], vec!["ada".into(), "n/a".into()]);
        let t = run(
            doc,
            vec![Step::Replace {
                pattern: "a".into(),
                replacement: "o".into(),
                regex: false,
                column: None,
            }],
        );
        assert_eq!(t.rows[0][0], " odo ".into());
        let text = apply_steps(
            Document::text("foo123bar"),
            &[Step::Replace {
                pattern: r"\d+".into(),
                replacement: "-".into(),
                regex: true,
                column: None,
            }],
            &mut Diagnostics::new(),
        )
        .unwrap();
        assert_eq!(text, Document::text("foo-bar"));
    }

    #[test]
    fn text_lines_and_shape_steps() {
        let doc = Document::text("b\n\na\nb");
        let out = apply_steps(
            doc,
            &[
                Step::TextLines {
                    transform: LineTransform::RemoveEmpty,
                },
                Step::TextLines {
                    transform: LineTransform::Dedupe,
                },
                Step::TextLines {
                    transform: LineTransform::Sort,
                },
            ],
            &mut Diagnostics::new(),
        )
        .unwrap();
        assert_eq!(out, Document::text("a\nb"));
        let tree = apply_steps(people(), &[Step::ToTree], &mut Diagnostics::new()).unwrap();
        assert!(matches!(tree, Document::Tree { .. }));
        let list = apply_steps(
            Document::tree(json!(["x", "y"])),
            &[Step::ToList],
            &mut Diagnostics::new(),
        )
        .unwrap();
        assert_eq!(list, Document::list(vec!["x".into(), "y".into()]));
        let t = run(
            Document::tree(json!([{"a": {"b": 1}}])),
            vec![Step::Flatten],
        );
        assert_eq!(t.headers, vec!["a.b"]);
        let back = apply_steps(
            Document::Table(t),
            &[Step::Unflatten],
            &mut Diagnostics::new(),
        )
        .unwrap();
        assert_eq!(back, Document::tree(json!([{"a": {"b": 1}}])));
        let t = run(people(), vec![Step::Transpose]);
        assert_eq!(t.rows.len(), 3);
    }

    #[test]
    fn share_string_round_trip() {
        let mut r = Recipe::new("CSV to SQL", OutputFormat::SqlInsert);
        r.output.options.table_name = Some("people".into());
        r.steps.push(Step::SortRows {
            column: "name".into(),
            descending: false,
        });
        let s = r.to_share_string();
        assert!(s.starts_with("pcr1."));
        let back = Recipe::from_share_string(&s).unwrap();
        assert_eq!(back, r);
        let from_json = Recipe::from_share_string(&r.to_json()).unwrap();
        assert_eq!(from_json, r);
        assert!(Recipe::from_share_string("pcr1.!!!").is_err());
    }

    #[test]
    fn lenient_parsing_accepts_variations() {
        let text = r#"Here is your recipe:
```json
{
  "title": "Top cities",
  "input": "csv",
  "steps": [
    {"type": "filter", "column": "age", "operator": ">", "value": 40},
    {"op": "sort", "column": "age", "order": "desc"},
    {"op": "select", "columns": "name, city"},
    {"op": "map", "column": "name", "transform": "uppercase"},
    {"op": "head", "n": 5}
  ],
  "output": {"format": "markdown", "options": {"align": "false"}}
}
```"#;
        let r = parse_recipe_lenient(text).unwrap();
        assert_eq!(r.name, "Top cities");
        assert_eq!(r.input.format, Some(InputFormat::Csv));
        assert_eq!(r.output.format, OutputFormat::MarkdownTable);
        assert_eq!(r.output.options.align, Some(false));
        assert_eq!(r.steps.len(), 5);
        assert_eq!(
            r.steps[0],
            Step::FilterRows {
                column: "age".into(),
                condition: Condition::Gt,
                value: Some("40".into())
            }
        );
        assert_eq!(
            r.steps[1],
            Step::SortRows {
                column: "age".into(),
                descending: true
            }
        );
        assert_eq!(
            r.steps[2],
            Step::SelectColumns {
                columns: vec!["name".into(), "city".into()]
            }
        );
        assert_eq!(
            r.steps[3],
            Step::MapColumn {
                column: "name".into(),
                transform: Transform::Upper,
                arg: None
            }
        );
        assert_eq!(
            r.steps[4],
            Step::Limit {
                count: 5,
                offset: 0
            }
        );
        let t = run(people(), r.steps.clone());
        assert_eq!(t.rows.len(), 2);
        assert_eq!(t.rows[0], vec!["GRACE".into(), "New York".into()]);
    }

    #[test]
    fn lenient_parsing_errors() {
        assert!(parse_recipe_lenient("no json here").is_err());
        assert!(parse_recipe_lenient(r#"{"name": "x"}"#).is_err());
        assert!(parse_recipe_lenient(r#"{"name": "x", "output": "bogus"}"#).is_err());
        let r = parse_recipe_lenient(
            r#"{"name": "x", "output": "excel", "steps": [{"op": "bogus_step"}]}"#,
        );
        assert!(r.is_err());
    }

    #[test]
    fn recipe_check_reports_problems() {
        let mut r = Recipe::new("", OutputFormat::Template);
        r.steps.push(Step::Replace {
            pattern: "(".into(),
            replacement: "".into(),
            regex: true,
            column: None,
        });
        let problems = r.check();
        assert_eq!(problems.len(), 3, "{problems:?}");
    }
}
