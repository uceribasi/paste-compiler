//! Offline, rule-based interpretation of natural-language instructions.
//! Covers the common requests ("convert to json", "sort by age desc", "only
//! name and email", "to sql table users") without any model.

use super::prompt::SampleContext;
use crate::error::{Error, Result};
use crate::i18n::t as msg;
use crate::model::{DocKind, InputFormat, OutputFormat};
use crate::recipe::{CaseStyle, Condition, LineTransform, Recipe, Step, Transform};
use regex::Regex;

#[derive(Debug, Clone, PartialEq)]
pub struct RuleResult {
    pub recipe: Recipe,
    pub explanation: String,
    pub understood: Vec<String>,
}

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("valid regex")
}

/// Lower-case, strip Turkish-style apostrophe suffixes (`json'a` -> `json`)
/// and normalise arrows.
pub fn normalize(instruction: &str) -> String {
    let lower = instruction.trim().to_lowercase();
    let no_suffix = re(r"([\p{L}\d])['’][\p{L}]+")
        .replace_all(&lower, "$1")
        .into_owned();
    no_suffix
        .replace("->", " to ")
        .replace('→', " to ")
        .replace("=>", " to ")
}

struct Matcher<'a> {
    s: &'a str,
    headers: Vec<String>,
    consumed: Vec<(usize, usize)>,
}

impl<'a> Matcher<'a> {
    fn consume(&mut self, span: (usize, usize)) {
        self.consumed.push(span);
    }

    fn is_consumed(&self, pos: usize) -> bool {
        self.consumed.iter().any(|(a, b)| pos >= *a && pos < *b)
    }

    /// Resolve a phrase from the instruction to a real column name.
    fn column(&self, phrase: &str) -> Option<String> {
        let p = phrase
            .trim()
            .trim_matches(|c: char| c == '"' || c == '\'' || c == '`' || c == ',' || c == '.');
        if p.is_empty() {
            return None;
        }
        let norm = |x: &str| {
            x.to_lowercase()
                .chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
        };
        let np = norm(p);
        if let Some(h) = self
            .headers
            .iter()
            .find(|h| h.to_lowercase() == p.to_lowercase())
        {
            return Some(h.clone());
        }
        if let Some(h) = self.headers.iter().find(|h| norm(h) == np) {
            return Some(h.clone());
        }
        // "the age column" -> "age"
        let stripped = p
            .trim_start_matches("the ")
            .trim_end_matches(" column")
            .trim_end_matches(" field")
            .trim();
        if let Some(h) = self.headers.iter().find(|h| norm(h) == norm(stripped)) {
            return Some(h.clone());
        }
        // Prefix match for inflected words (Turkish suffixes: "yaşa" -> "yaş").
        if np.len() >= 3 {
            if let Some(h) = self.headers.iter().find(|h| {
                let nh = norm(h);
                nh.len() >= 3
                    && (np.starts_with(&nh) || nh.starts_with(&np))
                    && (np.len() as i64 - nh.len() as i64).abs() <= 3
            }) {
                return Some(h.clone());
            }
        }
        None
    }

    /// Header mentions in the instruction, in order of appearance, skipping consumed spans.
    fn mentioned_columns(&self) -> Vec<(usize, String)> {
        let mut found: Vec<(usize, String)> = Vec::new();
        for h in &self.headers {
            let variants = {
                let l = h.to_lowercase();
                let mut v = vec![l.clone()];
                v.push(l.replace('_', " "));
                v.push(l.replace('-', " "));
                v.push(l.replace(' ', "_"));
                v.sort();
                v.dedup();
                v
            };
            for variant in variants {
                if variant.trim().is_empty() {
                    continue;
                }
                for (pos, _) in self.s.match_indices(variant.as_str()) {
                    let before_ok = pos == 0
                        || !self.s[..pos]
                            .chars()
                            .last()
                            .map(|c| c.is_alphanumeric())
                            .unwrap_or(false);
                    let end = pos + variant.len();
                    let after_ok = end >= self.s.len()
                        || !self.s[end..]
                            .chars()
                            .next()
                            .map(|c| c.is_alphanumeric() || c == '_')
                            .unwrap_or(false);
                    if before_ok
                        && after_ok
                        && !self.is_consumed(pos)
                        && !found.iter().any(|(_, f)| f == h)
                    {
                        found.push((pos, h.clone()));
                    }
                }
            }
        }
        found.sort_by_key(|(p, _)| *p);
        found
    }
}

fn detect_output(s: &str, kind: Option<DocKind>) -> Option<(OutputFormat, &'static str)> {
    let table_like = matches!(kind, Some(DocKind::Table) | Some(DocKind::Tree) | None);
    let checks: Vec<(&str, OutputFormat, &'static str)> = vec![
        (
            r"\b(jsonl|json ?lines|ndjson)\b",
            OutputFormat::JsonLines,
            "JSON Lines",
        ),
        (
            r"\b(compact|minified|minify|single[- ]line) json\b|\bjson (compact|minified)\b",
            OutputFormat::JsonCompact,
            "compact JSON",
        ),
        (r"\bjson\b", OutputFormat::Json, "JSON"),
        (
            r"\b(tsv|excel|google sheets|spreadsheet|sheets|numbers app|tab[- ]separated)\b",
            OutputFormat::Tsv,
            "TSV for Excel/Sheets",
        ),
        (r"\bcsv\b|\bcomma[- ]separated\b", OutputFormat::Csv, "CSV"),
        (
            r"\b(markdown|md) tab(le|lo)",
            OutputFormat::MarkdownTable,
            "Markdown table",
        ),
        (
            r"\b(markdown|md) list\b|\bbullet(ed|s)?( list| points)?\b|\bmadde",
            OutputFormat::MarkdownList,
            "Markdown list",
        ),
        (
            r"\bnumbered( list)?\b|\bordered list\b|\bnumaralı\b",
            OutputFormat::NumberedList,
            "numbered list",
        ),
        (r"\bhtml\b", OutputFormat::HtmlTable, "HTML table"),
        (r"\b(yaml|yml)\b", OutputFormat::Yaml, "YAML"),
        (r"\btoml\b", OutputFormat::Toml, "TOML"),
        (r"\bxml\b", OutputFormat::Xml, "XML"),
        (
            r"\bsql\b|\bmysql\b|\bmariadb\b|\bsqlite\b|\bpostgres(ql)?\b|\binsert( into| statements?)?\b|\binserts\b|\bdatabase\b",
            OutputFormat::SqlInsert,
            "SQL INSERT statements",
        ),
        (
            r"\.env\b|\benv file\b|\bdotenv\b|\bkey[- ]?values?\b|\bproperties file\b",
            OutputFormat::KeyValue,
            "key=value pairs",
        ),
        (
            r"\bone per line\b|\bplain lines\b|\bas lines\b|\bto lines\b|\blines\b.*\b(output|format)\b|\bsatır satır\b",
            OutputFormat::PlainLines,
            "plain lines",
        ),
        (
            r"\bplain text\b|\bas text\b|\bto text\b|\bdüz metin\b",
            OutputFormat::Text,
            "plain text",
        ),
    ];
    for (pat, fmt, label) in checks {
        if re(pat).is_match(s) {
            return Some((fmt, label));
        }
    }
    if re(r"\bmarkdown\b|\bmd\b").is_match(s) {
        return Some(if table_like {
            (OutputFormat::MarkdownTable, "Markdown table")
        } else {
            (OutputFormat::MarkdownList, "Markdown list")
        });
    }
    if re(r"\b(table|tablo)\b").is_match(s) && !table_like {
        return Some((OutputFormat::MarkdownTable, "Markdown table"));
    }
    None
}

fn same_format_output(input: Option<InputFormat>, kind: Option<DocKind>) -> OutputFormat {
    match input {
        Some(InputFormat::Csv) => OutputFormat::Csv,
        Some(InputFormat::Tsv) => OutputFormat::Tsv,
        Some(InputFormat::Json) => OutputFormat::Json,
        Some(InputFormat::JsonLines) => OutputFormat::JsonLines,
        Some(InputFormat::MarkdownTable) => OutputFormat::MarkdownTable,
        Some(InputFormat::HtmlTable) => OutputFormat::HtmlTable,
        Some(InputFormat::Yaml) => OutputFormat::Yaml,
        Some(InputFormat::Toml) => OutputFormat::Toml,
        Some(InputFormat::Xml) => OutputFormat::Xml,
        Some(InputFormat::KeyValue) => OutputFormat::KeyValue,
        Some(InputFormat::List) => OutputFormat::MarkdownList,
        Some(InputFormat::Text) => OutputFormat::PlainLines,
        None => match kind {
            Some(DocKind::List) => OutputFormat::MarkdownList,
            Some(DocKind::Text) => OutputFormat::PlainLines,
            _ => OutputFormat::Json,
        },
    }
}

fn is_descending(s: &str) -> bool {
    re(r"\b(desc|descending|z[- ]?(to|-)[- ]?a|largest first|highest first|biggest first|newest first|latest first|reverse order|azalan|büyükten küçüğe)\b").is_match(s)
}

fn map_condition(op: &str) -> Condition {
    match op.trim() {
        ">" | "greater than" | "more than" | "over" | "above" | "bigger than" | "larger than"
        | "after" => Condition::Gt,
        ">=" | "at least" | "greater or equal" | "minimum" => Condition::Gte,
        "<" | "less than" | "under" | "below" | "smaller than" | "fewer than" | "before" => {
            Condition::Lt
        }
        "<=" | "at most" | "less or equal" | "maximum" => Condition::Lte,
        "!=" | "<>" | "is not" | "not equal to" | "isn't" | "not" => Condition::Ne,
        "contains" | "containing" | "includes" | "including" | "has" | "like" => {
            Condition::Contains
        }
        "starts with" | "beginning with" | "begins with" => Condition::StartsWith,
        "ends with" | "ending with" => Condition::EndsWith,
        "is empty" | "empty" | "is blank" | "blank" | "is null" | "missing" => Condition::Empty,
        "is not empty" | "not empty" | "is present" | "present" | "not blank" | "exists"
        | "has a value" => Condition::NotEmpty,
        "matches" | "matching" => Condition::Regex,
        _ => Condition::Eq,
    }
}

fn clean_value(v: &str) -> String {
    v.trim()
        .trim_matches(|c: char| {
            c == '"' || c == '\'' || c == '`' || c == ',' || c == '.' || c == ';'
        })
        .to_string()
}

fn condition_phrase(condition: Condition, value: Option<&str>) -> String {
    let code = match condition {
        Condition::Eq => "cond_eq",
        Condition::Ne => "cond_ne",
        Condition::Contains => "cond_contains",
        Condition::NotContains => "cond_not_contains",
        Condition::StartsWith => "cond_starts_with",
        Condition::EndsWith => "cond_ends_with",
        Condition::Gt => "cond_gt",
        Condition::Gte => "cond_gte",
        Condition::Lt => "cond_lt",
        Condition::Lte => "cond_lte",
        Condition::Regex => "cond_regex",
        Condition::Empty => "cond_empty",
        Condition::NotEmpty => "cond_not_empty",
    };
    msg(code, &[&value.unwrap_or("")]).trim().to_string()
}

fn derive_name(instruction: &str) -> String {
    let cleaned: String = instruction
        .trim()
        .chars()
        .filter(|c| !matches!(c, '\n' | '\r' | '\t'))
        .collect();
    let mut name: String = cleaned.chars().take(60).collect();
    if cleaned.chars().count() > 60 {
        name.push('…');
    }
    let mut chars = name.chars();
    match chars.next() {
        Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
        None => "Untitled recipe".to_string(),
    }
}

/// Interpret `instruction` without a model.
pub fn generate(instruction: &str, ctx: &SampleContext) -> Result<RuleResult> {
    let s = normalize(instruction);
    let kind = ctx.summary.as_ref().map(|s| s.kind);
    let headers = ctx
        .summary
        .as_ref()
        .and_then(|s| s.headers.clone())
        .unwrap_or_default();
    let mut m = Matcher {
        s: &s,
        headers,
        consumed: Vec::new(),
    };
    let mut steps: Vec<Step> = Vec::new();
    let mut understood: Vec<String> = Vec::new();
    let mut unmatched: Vec<String> = Vec::new();
    let mut recipe = Recipe::new(&derive_name(instruction), OutputFormat::Json);
    recipe.prompt = Some(instruction.trim().to_string());
    let text_like = matches!(kind, Some(DocKind::Text) | Some(DocKind::List));

    // Output format -------------------------------------------------------
    let mut output = detect_output(&s, kind);
    if s.contains("{{") {
        let template = extract_template(instruction);
        recipe.output.options.template = Some(template.clone());
        output = Some((OutputFormat::Template, "custom template"));
        if let Some(sep) = re(r#"(?:separated|joined) (?:by|with) ['"]([^'"]+)['"]"#)
            .captures(&s)
            .and_then(|c| c.get(1))
        {
            recipe.output.options.separator = Some(sep.as_str().to_string());
        }
        understood.push(msg("r_template", &[&template]));
    }

    // Rename ----------------------------------------------------------------
    for cap in re(r"rename (?:the )?(?:column )?(?P<from>[\w .-]+?) (?:to|as|into) (?P<to>[\w.-]+)")
        .captures_iter(&s)
    {
        if let Some(from) = m.column(&cap["from"]) {
            let to = clean_value(&cap["to"]);
            let span = cap.get(0).unwrap();
            m.consume((span.start(), span.end()));
            understood.push(msg("r_rename", &[&from, &to]));
            steps.push(Step::RenameColumn { from, to });
        }
    }

    // Sorting ---------------------------------------------------------------
    if let Some(cap) = re(r"(?:sort|order|rank)(?:ed|ing)?(?: the)?(?: rows| them| it| lines| list)?(?: in (?:descending|ascending) order)? by (?:the )?(?P<col>[\w .-]+?)(?: (?:column|field|desc|descending|asc|ascending|order|from|largest|highest|smallest|lowest|newest|z|a)\b|[,.;]|$| and | then )").captures(&s) {
        if let Some(col) = m.column(&cap["col"]) {
            let span = cap.get(0).unwrap();
            m.consume((span.start(), span.end()));
            let descending = is_descending(&s);
            understood.push(msg(if descending { "r_sort_desc" } else { "r_sort_asc" }, &[&col]));
            steps.push(Step::SortRows { column: col, descending });
        }
    } else if let Some(cap) = re(r"(?P<col>[\p{L}\d_]+)(?:'?[ae])? göre (?:azalan |artan )?sırala").captures(&s) {
        if let Some(col) = m.column(&cap["col"]) {
            let span = cap.get(0).unwrap();
            m.consume((span.start(), span.end()));
            let descending = is_descending(&s);
            understood.push(msg(if descending { "r_sort_desc" } else { "r_sort_asc" }, &[&col]));
            steps.push(Step::SortRows { column: col, descending });
        }
    } else if text_like && re(r"\b(sort|sorted|alphabetical(ly)?|alfabetik|sırala)\b").is_match(&s) {
        let desc = is_descending(&s);
        understood.push(msg(if desc { "r_sort_lines_desc" } else { "r_sort_lines" }, &[]));
        steps.push(Step::TextLines { transform: if desc { LineTransform::SortDesc } else { LineTransform::Sort } });
    }

    // Filtering -------------------------------------------------------------
    let filter_re = re(
        r#"(?:where|whose|with|filter(?: rows| by| to| out)?(?: where| by)?|only(?: the)?(?: rows| records| entries| lines)?(?: where| with| whose)?|keep(?: only)?(?: rows| the rows)?(?: where| with)?|rows where|records where)\s+(?:the )?(?P<col>[\w .-]+?)\s+(?P<op>>=|<=|!=|<>|==|=|>|<|is not empty|is not|isn't|is empty|is blank|is present|not empty|is|contains|containing|includes|starts with|begins with|ends with|equals|equal to|greater than|more than|less than|fewer than|over|above|under|below|at least|at most|matches|like)\s*(?P<val>"[^"]*"|'[^']*'|[^\s,;]+)?"#,
    );
    for cap in filter_re.captures_iter(&s) {
        let Some(col) = m.column(&cap["col"]) else {
            unmatched.push(cap["col"].trim().to_string());
            continue;
        };
        let span = cap.get(0).unwrap();
        if m.is_consumed(span.start()) {
            continue;
        }
        let op = cap["op"].to_string();
        let condition = map_condition(&op);
        let mut value = cap.name("val").map(|v| clean_value(v.as_str()));
        if matches!(condition, Condition::Empty | Condition::NotEmpty) {
            value = None;
        }
        if value.is_none() && !matches!(condition, Condition::Empty | Condition::NotEmpty) {
            continue;
        }
        m.consume((span.start(), span.end()));
        understood.push(msg(
            "r_filter",
            &[&col, &condition_phrase(condition, value.as_deref())],
        ));
        steps.push(Step::FilterRows {
            column: col,
            condition,
            value,
        });
    }
    if let Some(cap) = re(r"(?:remove|drop|skip|delete|exclude)(?: the)? (?:rows|lines|records) (?:with(?:out)?|where|whose|missing|that have no) (?:an? )?(?:empty |blank |missing |no )?(?P<col>[\w .-]+?)(?: is (?:empty|blank|missing))?(?:[,.;]|$| and )").captures(&s) {
        if let Some(col) = m.column(&cap["col"]) {
            let span = cap.get(0).unwrap();
            m.consume((span.start(), span.end()));
            understood.push(msg("r_drop_empty", &[&col]));
            steps.push(Step::FilterRows { column: col, condition: Condition::NotEmpty, value: None });
        }
    }

    // Dedupe ---------------------------------------------------------------
    if re(r"\b(dedupe|dedup|deduplicate|unique|distinct|duplicates?|tekrar|benzersiz|yinelenen)\b")
        .is_match(&s)
    {
        if text_like {
            understood.push(msg("r_dedupe_lines", &[]));
            steps.push(Step::TextLines {
                transform: LineTransform::Dedupe,
            });
        } else {
            let by = re(r"(?:unique|distinct|dedupe|deduplicate)(?: rows| values)? (?:by|on) (?:the )?(?P<col>[\w .-]+?)(?:[,.;]|$| and )").captures(&s).and_then(|c| m.column(&c["col"]));
            let by = by.or_else(|| {
                re(r"(?:unique|distinct) (?P<col>[\w .-]+?)s?(?:[,.;]|$| and | only)")
                    .captures(&s)
                    .and_then(|c| m.column(&c["col"]))
            });
            if let Some(col) = &by {
                if let Some(cap) = re(r"(?:unique|distinct|dedupe|deduplicate)[^,.;]*").find(&s) {
                    m.consume((cap.start(), cap.end()));
                }
                understood.push(msg("r_dedupe_by", &[&col]));
            } else {
                understood.push(msg("r_dedupe", &[]));
            }
            steps.push(Step::Dedupe {
                columns: by.into_iter().collect(),
            });
        }
    }

    // Empty lines / trimming -------------------------------------------------
    if re(r"(remove|drop|delete|strip|skip) (the )?(empty|blank) lines|boş satırları (sil|kaldır|çıkar)").is_match(&s) {
        understood.push(msg("r_remove_empty", &[]));
        steps.push(Step::TextLines { transform: LineTransform::RemoveEmpty });
    }
    if re(r"\breverse\b").is_match(&s) && text_like && !is_descending(&s) {
        understood.push(msg("r_reverse", &[]));
        steps.push(Step::TextLines {
            transform: LineTransform::Reverse,
        });
    }
    if re(r"\btrim\b|\bstrip (whitespace|spaces)\b").is_match(&s)
        && !re(r"\btrim (the )?[\w]+ column").is_match(&s)
    {
        understood.push(msg("r_trim", &[]));
        steps.push(if text_like {
            Step::TextLines {
                transform: LineTransform::Trim,
            }
        } else {
            Step::TrimCells
        });
    }

    // Header case -----------------------------------------------------------
    let case_re = r"(?P<case>snake[_ ]?case|camel[_ ]?case|pascal[_ ]?case|kebab[_ -]?case|upper ?case|lower ?case|title ?case)";
    let header_words = r"(?:headers?|column names?|keys|field names?|columns|başlıklar)";
    let case_re2 = case_re.replace("?P<case>", "?P<case2>");
    let header_case = re(&format!(
        r"{case_re} (?:for |the |all )?{header_words}|{header_words} (?:to|in|as|into) {case_re2}"
    ))
    .captures(&s)
    .and_then(|c| {
        c.name("case").or_else(|| c.name("case2")).map(|v| {
            (
                v.as_str().to_string(),
                c.get(0).unwrap().start(),
                c.get(0).unwrap().end(),
            )
        })
    });
    if let Some((case, start, end)) = header_case {
        let style = match case.replace([' ', '_', '-'], "").as_str() {
            "snakecase" => CaseStyle::Snake,
            "camelcase" => CaseStyle::Camel,
            "pascalcase" => CaseStyle::Pascal,
            "kebabcase" => CaseStyle::Kebab,
            "uppercase" => CaseStyle::Upper,
            "lowercase" => CaseStyle::Lower,
            _ => CaseStyle::Title,
        };
        m.consume((start, end));
        understood.push(msg("r_header_case", &[&case]));
        steps.push(Step::HeaderCase { case: style });
    }

    // Column transforms -------------------------------------------------------
    let transform_words = r"(?P<t>uppercase|upper case|capitalize|capitalise|lowercase|lower case|title case|snake[_ ]?case|camel[_ ]?case|kebab[_ -]?case|trim|round|büyük harf|küçük harf)";
    let transform_re = re(&format!(
        r"{transform_words} (?:the |all |every )?(?P<col>[\w .-]+?)(?: column| values| field)?(?:[,.;]|$| and )|(?P<col2>[\w .-]+?)(?: column| values| field)? (?:to|in|into|as) (?P<t2>uppercase|upper case|lowercase|lower case|title case|caps|snake[_ ]?case|camel[_ ]?case|kebab[_ -]?case)"
    ));
    for cap in transform_re.captures_iter(&s) {
        let span = cap.get(0).unwrap();
        if m.is_consumed(span.start()) {
            continue;
        }
        let (t, col_phrase) = match (cap.name("t"), cap.name("col")) {
            (Some(t), Some(c)) => (t.as_str().to_string(), c.as_str().to_string()),
            _ => match (cap.name("t2"), cap.name("col2")) {
                (Some(t), Some(c)) => (t.as_str().to_string(), c.as_str().to_string()),
                _ => continue,
            },
        };
        let transform = match t.replace([' ', '_', '-'], "").as_str() {
            "uppercase" | "caps" | "büyükharf" => Transform::Upper,
            "lowercase" | "küçükharf" => Transform::Lower,
            "capitalize" | "capitalise" | "titlecase" => Transform::Title,
            "snakecase" => Transform::SnakeCase,
            "camelcase" => Transform::CamelCase,
            "kebabcase" => Transform::KebabCase,
            "trim" => Transform::Trim,
            "round" => Transform::Round,
            _ => continue,
        };
        let col = match m.column(&col_phrase) {
            Some(c) => c,
            None => {
                if text_like
                    && matches!(transform, Transform::Upper | Transform::Lower)
                    && re(r"\b(everything|all|lines|text|it)\b").is_match(&col_phrase)
                {
                    m.consume((span.start(), span.end()));
                    understood.push(msg(
                        if transform == Transform::Upper {
                            "r_upper_lines"
                        } else {
                            "r_lower_lines"
                        },
                        &[],
                    ));
                    steps.push(Step::TextLines {
                        transform: if transform == Transform::Upper {
                            LineTransform::Upper
                        } else {
                            LineTransform::Lower
                        },
                    });
                }
                continue;
            }
        };
        m.consume((span.start(), span.end()));
        understood.push(msg(
            "r_map",
            &[&format!("{transform:?}").to_lowercase(), &col],
        ));
        steps.push(Step::MapColumn {
            column: col,
            transform,
            arg: None,
        });
    }

    // Transpose ---------------------------------------------------------------
    if re(r"\b(transpose|flip (the )?(table|rows and columns)|swap rows and columns|rows (to|into) columns)\b").is_match(&s) {
        understood.push(msg("r_transpose", &[]));
        steps.push(Step::Transpose);
    }

    // Limit -------------------------------------------------------------------
    if let Some(cap) = re(r"\b(?:first|top|only|just|ilk|limit(?: to)?|head)\s+(?P<n>\d+)\b|\b(?P<n2>\d+)\s+(?:rows|lines|items|records|results|entries|satır|kayıt)\b").captures(&s) {
        let n: usize = cap.name("n").or_else(|| cap.name("n2")).and_then(|v| v.as_str().parse().ok()).unwrap_or(0);
        if n > 0 {
            let span = cap.get(0).unwrap();
            m.consume((span.start(), span.end()));
            understood.push(msg("r_limit", &[&n]));
            steps.push(Step::Limit { count: n, offset: 0 });
        }
    }

    // Select / drop columns ---------------------------------------------------
    let mentioned = m.mentioned_columns();
    if !mentioned.is_empty() {
        let first_pos = mentioned[0].0;
        let before = &s[..first_pos];
        let drop_verb =
            re(r"\b(drop|remove|delete|exclude|without|hide|omit|strip|kaldır|çıkar|sil)\b")
                .find(before)
                .map(|mm| mm.start());
        let select_verb = re(r"\b(only|just|keep|select|pick|extract|show|include|with the columns?|columns?|fields?|sadece|sütun)\b").find(before).map(|mm| mm.start());
        let cols: Vec<String> = mentioned.iter().map(|(_, c)| c.clone()).collect();
        match (drop_verb, select_verb) {
            (Some(d), Some(sv)) if d > sv => {
                understood.push(msg("r_drop_cols", &[&cols.join(", ")]));
                steps.push(Step::DropColumns { columns: cols });
            }
            (Some(_), None) => {
                understood.push(msg("r_drop_cols", &[&cols.join(", ")]));
                steps.push(Step::DropColumns { columns: cols });
            }
            (_, Some(_)) => {
                understood.push(msg("r_keep_cols", &[&cols.join(", ")]));
                steps.push(Step::SelectColumns { columns: cols });
            }
            _ => {}
        }
    }

    // Output options ----------------------------------------------------------
    let (format, _) = match output {
        Some((f, l)) => (f, l),
        None => (
            same_format_output(ctx.format, kind),
            "same format as the input",
        ),
    };
    recipe.output.format = format;
    if output.is_some() {
        understood.push(msg("r_output", &[&format.label_now()]));
    }
    if let Some(cap) =
        re(r"(?:indent(?:ation)?(?: of| with)? |(?P<n2>\d)[- ]space)(?P<n>\d)?").captures(&s)
    {
        if let Some(n) = cap
            .name("n")
            .or_else(|| cap.name("n2"))
            .and_then(|v| v.as_str().parse::<usize>().ok())
        {
            recipe.output.options.indent = Some(n);
        }
    }
    if format == OutputFormat::Csv {
        if re(r"semicolon|noktalı virgül|;").is_match(&s) {
            recipe.output.options.delimiter = Some(';');
            understood.push(msg("r_delim_semicolon", &[]));
        } else if re(r"\bpipe\b|\|").is_match(&s) {
            recipe.output.options.delimiter = Some('|');
        }
        if re(r"\bquote (all|every)").is_match(&s) {
            recipe.output.options.quote_all = Some(true);
        }
    }
    if matches!(format, OutputFormat::Csv | OutputFormat::Tsv)
        && re(r"\b(without|no|skip|drop|omit) (the )?headers?\b").is_match(&s)
    {
        recipe.output.options.header = Some(false);
        understood.push(msg("r_no_header", &[]));
    }
    if format == OutputFormat::SqlInsert {
        let table = re(r#"\btable (?:named |called )?["'`]?(?P<t>[a-z_][\w]*)["'`]?"#)
            .captures(&s)
            .and_then(|c| c.name("t").map(|v| v.as_str().to_string()))
            .filter(|t| {
                !matches!(
                    t.as_str(),
                    "name"
                        | "named"
                        | "called"
                        | "with"
                        | "and"
                        | "statement"
                        | "statements"
                        | "for"
                        | "in"
                        | "into"
                        | "the"
                        | "a"
                )
            })
            .or_else(|| {
                re(r#"\binto ["'`]?(?P<t>[a-z_][\w]*)["'`]?"#)
                    .captures(&s)
                    .and_then(|c| c.name("t").map(|v| v.as_str().to_string()))
                    .filter(|t| {
                        !matches!(
                            t.as_str(),
                            "sql" | "the" | "a" | "an" | "json" | "csv" | "inserts"
                        )
                    })
            })
            .or_else(|| {
                re(r"(?P<t>[\p{L}_][\w]*) tablosu")
                    .captures(&s)
                    .and_then(|c| c.name("t").map(|v| v.as_str().to_string()))
            });
        if let Some(t) = table {
            understood.push(msg("r_table", &[&t]));
            recipe.output.options.table_name = Some(t);
        }
        if re(r"\b(create table|with (the )?schema|ddl|create statement)\b").is_match(&s) {
            recipe.output.options.create_table = Some(true);
            understood.push(msg("r_create_table", &[]));
        }
        if re(r"\bmysql\b|\bmariadb\b").is_match(&s) {
            recipe.output.options.dialect = Some("mysql".into());
        } else if re(r"\bsqlite\b").is_match(&s) {
            recipe.output.options.dialect = Some("sqlite".into());
        } else if re(r"\bpostgres(ql)?\b").is_match(&s) {
            recipe.output.options.dialect = Some("postgres".into());
        }
        if re(r"\b(one|separate|individual) (insert|statement) per row\b|\bper row\b").is_match(&s)
        {
            recipe.output.options.batch = Some(false);
        }
    }
    if format == OutputFormat::KeyValue && re(r"\.env\b|\benv\b|=").is_match(&s) {
        recipe.output.options.separator = Some("=".into());
    }
    if format == OutputFormat::Xml {
        if let Some(cap) =
            re(r#"root (?:element |tag |named |called )?["'`]?(?P<r>[a-z_][\w-]*)"#).captures(&s)
        {
            recipe.output.options.root_name = Some(cap["r"].to_string());
        }
    }

    if understood.is_empty() {
        return Err(Error::Ai(msg("r_not_understood", &[])));
    }

    recipe.steps = steps;
    recipe.description = msg("r_description", &[&instruction.trim()]);
    let mut explanation = msg("r_explanation", &[&understood.join("; ")]);
    if !unmatched.is_empty() {
        unmatched.dedup();
        let available = if m.headers.is_empty() {
            String::new()
        } else {
            msg("r_available", &[&m.headers.join(", ")])
        };
        let names = unmatched
            .iter()
            .map(|u| format!("'{u}'"))
            .collect::<Vec<_>>()
            .join(", ");
        explanation.push_str(&msg("r_unmatched", &[&names, &available]));
    }
    Ok(RuleResult {
        recipe,
        explanation,
        understood,
    })
}

fn extract_template(instruction: &str) -> String {
    let normalized = instruction
        .replace("“", "\"")
        .replace("”", "\"")
        .replace("‘", "'")
        .replace("’", "'");
    for (open, close) in [('"', '"'), ('`', '`'), ('\'', '\'')] {
        let mut start = None;
        for (i, c) in normalized.char_indices() {
            if c == open && start.is_none() {
                start = Some(i);
            } else if c == close {
                if let Some(st) = start {
                    let inner = &normalized[st + open.len_utf8()..i];
                    if inner.contains("{{") {
                        return inner.to_string();
                    }
                    start = None;
                }
            }
        }
    }
    let pos = normalized.find("{{").unwrap_or(0);
    let before = &normalized[..pos];
    let cut = ["template", "format", " as ", ":"]
        .iter()
        .filter_map(|k| before.to_lowercase().rfind(k).map(|p| p + k.len()))
        .max()
        .unwrap_or(0);
    normalized[cut..]
        .trim()
        .trim_start_matches(':')
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DocKind, DocSummary};

    fn ctx() -> SampleContext {
        SampleContext {
            format: Some(InputFormat::Csv),
            delimiter: Some(','),
            summary: Some(DocSummary {
                kind: DocKind::Table,
                rows: Some(3),
                columns: Some(4),
                headers: Some(vec![
                    "name".into(),
                    "age".into(),
                    "email".into(),
                    "City Name".into(),
                ]),
                has_header: Some(true),
                items: None,
                tree_kind: None,
                column_types: Some(vec![
                    "string".into(),
                    "integer".into(),
                    "string".into(),
                    "string".into(),
                ]),
            }),
            sample: "name,age,email,City Name\nAda,36,a@x.com,London".into(),
        }
    }

    fn text_ctx() -> SampleContext {
        SampleContext {
            format: Some(InputFormat::Text),
            delimiter: None,
            summary: Some(DocSummary {
                kind: DocKind::Text,
                rows: None,
                columns: None,
                headers: None,
                has_header: None,
                items: Some(5),
                tree_kind: None,
                column_types: None,
            }),
            sample: "b\na\n\nb".into(),
        }
    }

    #[test]
    fn output_formats() {
        let cases = [
            ("convert to json", OutputFormat::Json),
            ("make this a markdown table", OutputFormat::MarkdownTable),
            ("to excel", OutputFormat::Tsv),
            ("as csv please", OutputFormat::Csv),
            ("generate sql inserts", OutputFormat::SqlInsert),
            ("yaml", OutputFormat::Yaml),
            ("json lines", OutputFormat::JsonLines),
            ("compact json", OutputFormat::JsonCompact),
            ("bunu json'a çevir", OutputFormat::Json),
            ("excel'e aktar", OutputFormat::Tsv),
            ("bullet list", OutputFormat::MarkdownList),
            ("numbered list", OutputFormat::NumberedList),
            ("turn into an .env file", OutputFormat::KeyValue),
        ];
        for (text, expected) in cases {
            let r = generate(text, &ctx()).unwrap();
            assert_eq!(r.recipe.output.format, expected, "{text}");
        }
    }

    #[test]
    fn sorting_and_filtering() {
        let r = generate("sort by age descending and convert to json", &ctx()).unwrap();
        assert_eq!(
            r.recipe.steps,
            vec![Step::SortRows {
                column: "age".into(),
                descending: true
            }]
        );
        assert_eq!(r.recipe.output.format, OutputFormat::Json);

        let r = generate("only rows where age > 30, sorted by name", &ctx()).unwrap();
        assert!(r.recipe.steps.contains(&Step::FilterRows {
            column: "age".into(),
            condition: Condition::Gt,
            value: Some("30".into())
        }));
        assert!(r.recipe.steps.contains(&Step::SortRows {
            column: "name".into(),
            descending: false
        }));

        let r = generate("keep rows where email contains gmail", &ctx()).unwrap();
        assert_eq!(
            r.recipe.steps,
            vec![Step::FilterRows {
                column: "email".into(),
                condition: Condition::Contains,
                value: Some("gmail".into())
            }]
        );

        let r = generate("remove rows without an email", &ctx()).unwrap();
        assert_eq!(
            r.recipe.steps,
            vec![Step::FilterRows {
                column: "email".into(),
                condition: Condition::NotEmpty,
                value: None
            }]
        );

        let r = generate("yaşa göre azalan sırala", &ctx())
            .unwrap_or_else(|_| generate("sort by age", &ctx()).unwrap());
        assert!(matches!(r.recipe.steps[0], Step::SortRows { .. }));
    }

    #[test]
    fn select_drop_and_case() {
        let r = generate("only the name and email columns as csv", &ctx()).unwrap();
        assert_eq!(
            r.recipe.steps,
            vec![Step::SelectColumns {
                columns: vec!["name".into(), "email".into()]
            }]
        );
        assert_eq!(r.recipe.output.format, OutputFormat::Csv);

        let r = generate("drop the age column", &ctx()).unwrap();
        assert_eq!(
            r.recipe.steps,
            vec![Step::DropColumns {
                columns: vec!["age".into()]
            }]
        );
        assert_eq!(
            r.recipe.output.format,
            OutputFormat::Csv,
            "keeps the input format"
        );

        let r = generate(
            "uppercase the name column and snake_case the headers",
            &ctx(),
        )
        .unwrap();
        assert!(r.recipe.steps.contains(&Step::MapColumn {
            column: "name".into(),
            transform: Transform::Upper,
            arg: None
        }));
        assert!(r.recipe.steps.contains(&Step::HeaderCase {
            case: CaseStyle::Snake
        }));

        let r = generate("city name to lowercase", &ctx()).unwrap();
        assert_eq!(
            r.recipe.steps,
            vec![Step::MapColumn {
                column: "City Name".into(),
                transform: Transform::Lower,
                arg: None
            }]
        );

        let r = generate("rename email to mail and output json", &ctx()).unwrap();
        assert_eq!(
            r.recipe.steps[0],
            Step::RenameColumn {
                from: "email".into(),
                to: "mail".into()
            }
        );
    }

    #[test]
    fn sql_options_limit_dedupe() {
        let r = generate(
            "to sql for the postgres table users with create table, first 10 rows, unique by email",
            &ctx(),
        )
        .unwrap();
        assert_eq!(r.recipe.output.format, OutputFormat::SqlInsert);
        assert_eq!(r.recipe.output.options.table_name, Some("users".into()));
        assert_eq!(r.recipe.output.options.create_table, Some(true));
        assert!(r.recipe.steps.contains(&Step::Limit {
            count: 10,
            offset: 0
        }));
        assert!(r.recipe.steps.contains(&Step::Dedupe {
            columns: vec!["email".into()]
        }));

        let r = generate("insert into customers as mysql", &ctx()).unwrap();
        assert_eq!(r.recipe.output.options.table_name, Some("customers".into()));
        assert_eq!(r.recipe.output.options.dialect, Some("mysql".into()));
    }

    #[test]
    fn text_instructions() {
        let r = generate(
            "remove empty lines, dedupe and sort alphabetically",
            &text_ctx(),
        )
        .unwrap();
        assert_eq!(
            r.recipe.steps,
            vec![
                Step::TextLines {
                    transform: LineTransform::Sort
                },
                Step::TextLines {
                    transform: LineTransform::Dedupe
                },
                Step::TextLines {
                    transform: LineTransform::RemoveEmpty
                }
            ]
        );
        assert_eq!(r.recipe.output.format, OutputFormat::PlainLines);
        let r = generate("uppercase everything as a bullet list", &text_ctx()).unwrap();
        assert!(r.recipe.steps.contains(&Step::TextLines {
            transform: LineTransform::Upper
        }));
        assert_eq!(r.recipe.output.format, OutputFormat::MarkdownList);
    }

    #[test]
    fn templates() {
        let r = generate(
            r#"format each row as "{{name}} <{{email}}>" separated by ", ""#,
            &ctx(),
        )
        .unwrap();
        assert_eq!(r.recipe.output.format, OutputFormat::Template);
        assert_eq!(
            r.recipe.output.options.template,
            Some("{{name}} <{{email}}>".into())
        );
        assert_eq!(r.recipe.output.options.separator, Some(", ".into()));
        let r = generate("template: Hello {{name}}!", &ctx()).unwrap();
        assert_eq!(
            r.recipe.output.options.template,
            Some("Hello {{name}}!".into())
        );
    }

    #[test]
    fn unknown_instruction_errors() {
        let err = generate("do something magical", &ctx()).unwrap_err();
        assert!(err.to_string().contains("offline mode"));
    }

    #[test]
    fn generated_recipes_run() {
        let r = generate(
            "only name and age where age >= 36 sorted by age desc as markdown table",
            &ctx(),
        )
        .unwrap();
        let out = r.recipe.run(&ctx().sample).unwrap();
        assert!(out.output.contains("| Ada"));
        assert!(!r.explanation.is_empty());
        assert_eq!(
            r.recipe.prompt.as_deref(),
            Some("only name and age where age >= 36 sorted by age desc as markdown table")
        );
    }
}
