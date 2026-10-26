//! A tiny `{{ variable | filter }}` template language used by the
//! `add_column` step and the `template` output format.

use crate::error::{Error, Result};
use serde_json::Value;
use std::collections::HashMap;

/// Variables available to a template.
pub type Vars = HashMap<String, String>;

fn apply_filter(value: &str, filter: &str) -> Result<String> {
    let (name, arg) = match filter.split_once(':') {
        Some((n, a)) => (
            n.trim(),
            Some(a.trim().trim_matches(|c| c == '"' || c == '\'')),
        ),
        None => (filter.trim(), None),
    };
    Ok(match name {
        "upper" => value.to_uppercase(),
        "lower" => value.to_lowercase(),
        "trim" => value.trim().to_string(),
        "title" => title_case(value),
        "snake" | "snake_case" => heck::ToSnakeCase::to_snake_case(value),
        "camel" | "camel_case" => heck::ToLowerCamelCase::to_lower_camel_case(value),
        "pascal" | "pascal_case" => heck::ToUpperCamelCase::to_upper_camel_case(value),
        "kebab" | "kebab_case" => heck::ToKebabCase::to_kebab_case(value),
        "json" => Value::String(value.to_string()).to_string(),
        "sql" => format!("'{}'", value.replace('\'', "''")),
        "escape" | "html" => html_escape(value),
        "length" | "len" => value.chars().count().to_string(),
        "default" => {
            if value.is_empty() {
                arg.unwrap_or("").to_string()
            } else {
                value.to_string()
            }
        }
        "pad" => {
            let width: usize = arg.and_then(|a| a.parse().ok()).unwrap_or(0);
            format!("{value:<width$}")
        }
        "truncate" => {
            let n: usize = arg.and_then(|a| a.parse().ok()).unwrap_or(usize::MAX);
            value.chars().take(n).collect()
        }
        other => {
            return Err(Error::Template(crate::i18n::t(
                "err_unknown_filter",
                &[&other],
            )))
        }
    })
}

pub fn title_case(s: &str) -> String {
    s.split(' ')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Names of all variables referenced by a template.
pub fn referenced_vars(template: &str) -> Vec<String> {
    let mut vars = Vec::new();
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else { break };
        let expr = &after[..end];
        let name = expr.split('|').next().unwrap_or("").trim().to_string();
        if !name.is_empty() && !vars.contains(&name) {
            vars.push(name);
        }
        rest = &after[end + 2..];
    }
    vars
}

/// Render `template` with `vars`. Unknown variables render as empty strings
/// unless `strict` is set, in which case they are an error.
pub fn render(template: &str, vars: &Vars, strict: bool) -> Result<String> {
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            return Err(Error::Template(crate::i18n::t(
                "err_unclosed_template",
                &[],
            )));
        };
        let expr = &after[..end];
        let mut parts = expr.split('|');
        let name = parts.next().unwrap_or("").trim();
        let mut value = match vars.get(name) {
            Some(v) => v.clone(),
            None => {
                if strict {
                    return Err(Error::Template(crate::i18n::t(
                        "err_unknown_variable",
                        &[&name],
                    )));
                }
                String::new()
            }
        };
        for filter in parts {
            if !filter.trim().is_empty() {
                value = apply_filter(&value, filter)?;
            }
        }
        out.push_str(&value);
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    Ok(out.replace("\\n", "\n").replace("\\t", "\t"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> Vars {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn renders_variables_and_filters() {
        let v = vars(&[("name", "ada lovelace"), ("email", "ada@example.com")]);
        assert_eq!(
            render("{{name}} <{{ email }}>", &v, true).unwrap(),
            "ada lovelace <ada@example.com>"
        );
        assert_eq!(
            render("{{ name | title }}", &v, true).unwrap(),
            "Ada Lovelace"
        );
        assert_eq!(
            render("{{ name | upper | truncate:3 }}", &v, true).unwrap(),
            "ADA"
        );
        assert_eq!(
            render("{{ name | snake }}", &v, true).unwrap(),
            "ada_lovelace"
        );
        assert_eq!(
            render("{{ name | json }}", &v, true).unwrap(),
            "\"ada lovelace\""
        );
        assert_eq!(
            render("{{ missing | default:\"n/a\" }}", &v, false).unwrap(),
            "n/a"
        );
        assert_eq!(
            render(
                "INSERT {{ name | sql }}",
                &vars(&[("name", "O'Brien")]),
                true
            )
            .unwrap(),
            "INSERT 'O''Brien'"
        );
    }

    #[test]
    fn strict_mode_errors_on_unknown() {
        assert!(render("{{nope}}", &Vars::new(), true).is_err());
        assert_eq!(render("{{nope}}", &Vars::new(), false).unwrap(), "");
        assert!(render("{{ open", &Vars::new(), false).is_err());
        assert!(render("{{ x | bogus }}", &vars(&[("x", "1")]), false).is_err());
    }

    #[test]
    fn escapes_and_vars_list() {
        assert_eq!(render("a\\nb", &Vars::new(), false).unwrap(), "a\nb");
        assert_eq!(
            referenced_vars("{{a}} {{ b | upper }} {{a}}"),
            vec!["a", "b"]
        );
        assert_eq!(
            html_escape("<a href=\"x\">&</a>"),
            "&lt;a href=&quot;x&quot;&gt;&amp;&lt;/a&gt;"
        );
    }
}
