# PasteCompiler

**Convert anything you copied into the format you need.**

PasteCompiler is an open-source desktop app (Rust + Tauri + React) that takes whatever is on your clipboard — a table copied from Excel, a JSON blob, CSV, YAML, TOML, XML, a Markdown or HTML table, `.env`-style key/value pairs, a bulleted list or plain text — detects what it is, and converts it into the format you need while checking that no data was lost along the way.

- **Automatic detection** of the input format (with delimiter and header-row sniffing for CSV-like data) and a ranked list of sensible output formats.
- **17 output formats**: JSON (pretty / compact / lines), CSV, TSV for Excel & Google Sheets, Markdown table, HTML table, YAML, TOML, XML, SQL `INSERT` (Postgres / MySQL / SQLite, optional `CREATE TABLE`), Markdown & numbered lists, plain lines, key/value pairs, custom `{{templates}}` and plain text.
- **Validation after every conversion**: the output is parsed back and compared with the input (row counts, columns, cell values, nested keys), and every lossy step — flattened nesting, dropped `null`s in TOML, padded ragged rows, renamed duplicate headers, sanitized XML names, type coercions… — is reported with its severity.
- **Recipes**: reusable, declarative conversion pipelines (select / drop / rename columns, filter, sort, limit, dedupe, split, computed columns, case conversion, regex replace, flatten / unflatten, line operations, …) plus an output format and its options. Save them, run them with one click, export them as JSON files or short share codes (`pcr1.…`) and import recipes other people made.
- **Describe it in plain language**: type "only the name and email columns, sorted by name, as SQL inserts for table users" and get a recipe that is tested on your data before you see it. Works **fully offline** with the built-in rule engine, or with a local model via **Ollama**, any **OpenAI-compatible** server (LM Studio, llama.cpp, vLLM…) or **Anthropic Claude** with your own key.
- **Local first**: nothing leaves your machine unless you configure a remote AI provider — and even then you decide whether a data sample is sent.

---

## Install / run

Prerequisites: [Rust](https://rustup.rs) (stable), Node.js ≥ 18, and the usual Tauri system dependencies for your platform (Xcode Command Line Tools on macOS; see the [Tauri prerequisites](https://tauri.app/start/prerequisites/)).

```bash
npm install
npm run tauri dev        # development build with hot reload
npm run tauri build      # release bundle in target/release/bundle/
```

On macOS the release build produces `PasteCompiler.app` (and a `.dmg`) under `target/release/bundle/`. The app is not code-signed, so the first launch needs a right-click → *Open*. The `.dmg` step drives Finder through AppleScript and needs the Automation permission the first time; if it stalls, build just the app with `npm run tauri build -- --bundles app`.

### Using the UI in a browser (optional)

The React UI can also run in a normal browser against a small HTTP dev server that exposes the same commands as the desktop app — handy for UI work and for debugging without Tauri:

```bash
cargo run -p pastecompiler-core --example devserver   # engine on http://127.0.0.1:4567
npm run dev                                           # UI on http://localhost:1420
```

Recipes and settings created this way are kept in a separate `pastecompiler-dev` folder (override with `PASTECOMPILER_DATA_DIR`).

### Tests

```bash
cargo test --workspace   # Rust: detection, parsers, writers, validation, recipes, AI providers (mocked)
npm test                 # frontend helpers (vitest)
npm run typecheck        # TypeScript
```

---

## How it works

```
clipboard text ─▶ detect ─▶ parse ─▶ Document ─▶ recipe steps ─▶ render ─▶ output
                                        │                                  │
                                        └──────────── validate ◀───────────┘
```

Everything is in the `pastecompiler-core` crate (`crates/pastecompiler-core`); the Tauri app in `src-tauri` is a thin shell that exposes the core as commands, and the React UI in `src` calls those commands.

| Module | What it does |
| --- | --- |
| `detect` | Scores candidate formats (JSON, JSON Lines, CSV/TSV with delimiter sniffing, Markdown & HTML tables, YAML, TOML, XML, key/value, lists, text) and guesses whether a CSV has a header row. |
| `parse` | Turns text in a known format into a `Document`: a typed `Table`, a JSON-like `Tree`, a `List` or `Text`. |
| `coerce` | Converts between those shapes (nested JSON → flat table with dotted columns, table → array of objects, …) and records what was lossy. |
| `recipe` | The `Recipe` schema, the step interpreter and a tolerant parser for hand-written or model-generated recipes. |
| `output` | One writer per output format. |
| `validate` | Parses the output back, compares it with the source and builds the report (`lossless`, round-trip status, issues, differences, stats). |
| `ai` | Prompt building, the Ollama / OpenAI-compatible / Anthropic clients, and the offline rule engine. |
| `store`, `settings` | Recipes and settings persisted as JSON in the app data directory. |

Type inference is deliberately conservative: a cell becomes a number only when rendering it back yields exactly the same text, so `007`, `3.10`, `+5` and `1e5` stay strings and nothing silently changes.

---

## Recipes

A recipe is a JSON document:

```json
{
  "name": "Active users → SQL",
  "description": "Keep active users, newest first, as INSERT statements",
  "input": { "format": "auto" },
  "steps": [
    { "op": "filter_rows", "column": "status", "condition": "eq", "value": "active" },
    { "op": "sort_rows", "column": "created_at", "descending": true },
    { "op": "select_columns", "columns": ["id", "name", "email"] },
    { "op": "header_case", "case": "snake" }
  ],
  "output": { "format": "sql_insert", "table_name": "users", "create_table": true, "dialect": "postgres" }
}
```

### Steps

| `op` | Fields | Notes |
| --- | --- | --- |
| `select_columns` | `columns` | keep only these columns, in this order |
| `drop_columns` | `columns` | |
| `rename_column` | `from`, `to` | |
| `filter_rows` | `column`, `condition`, `value` | `condition`: `eq` `ne` `contains` `not_contains` `starts_with` `ends_with` `gt` `gte` `lt` `lte` `regex` `empty` `not_empty` |
| `sort_rows` | `column`, `descending` | numeric when both sides are numbers, otherwise case-insensitive text; empty cells last |
| `limit` | `count`, `offset` | |
| `map_column` | `column`, `transform`, `arg` | `transform`: `upper` `lower` `title` `trim` `snake_case` `camel_case` `pascal_case` `kebab_case` `to_number` `to_string` `to_bool` `round` (arg = decimals) `prefix` `suffix` `length` `default` |
| `add_column` | `name`, `template` | e.g. `"{{first}} {{last}}"`; filters: `upper lower trim title snake camel pascal kebab json sql html length default:"x" pad:10 truncate:10`; `{{_index}}`, `{{_index0}}`, `{{_count}}` are available |
| `split_column` | `column`, `separator`, `into` | |
| `dedupe` | `columns` | empty list = whole row |
| `transpose` | | |
| `flatten` | | nested JSON → table with dotted columns (`address.city`, `tags.0`) |
| `unflatten` | | dotted columns → nested JSON |
| `header_case` | `case` | `snake` `camel` `pascal` `kebab` `upper` `lower` `title` |
| `replace` | `pattern`, `replacement`, `regex`, `column` | |
| `trim_cells` | | |
| `fill_empty` | `value`, `column` | |
| `text_lines` | `transform` | `sort` `sort_desc` `dedupe` `trim` `remove_empty` `upper` `lower` `reverse` — for plain text and lists |
| `to_table` / `to_tree` / `to_list` / `to_text` | | explicit shape changes (rarely needed) |

Steps that need a table (most of them) convert other shapes automatically and note it in the report.

### Output formats and options

| `format` | Options |
| --- | --- |
| `json` | `indent` |
| `json_compact`, `json_lines` | |
| `csv` | `delimiter`, `quote_all`, `header`, `crlf`, `null_as` |
| `tsv` | `header` — pastes straight into Excel / Numbers / Google Sheets |
| `markdown_table` | `align`, `null_as` |
| `html_table` | `indent` |
| `yaml` | |
| `toml` | `item_name` |
| `xml` | `root_name`, `item_name`, `indent`, `declaration` — `@attr` keys become attributes, `#text` becomes text content |
| `sql_insert` | `table_name`, `dialect` (`postgres` `mysql` `sqlite`), `create_table`, `batch` |
| `markdown_list` | `bullet` |
| `numbered_list`, `plain_lines` | |
| `key_value` | `separator` (use `=` for `.env` files) |
| `template` | `template`, `separator`, `prefix`, `suffix` — the template is rendered once per row |
| `text` | |

The recipe parser is tolerant: `"output": "csv"`, `"type"` instead of `"op"`, `">"` instead of `"gt"`, `"uppercase"` instead of `"upper"`, `"order": "desc"` and similar variations are all accepted, which is what makes model-generated recipes robust.

### Sharing

*Recipes → Share code* gives you a short `pcr1.…` string (base64url of the recipe JSON). Paste it into *Recipes → Import* on any other machine, or export/import `.recipe.json` files. Imports that collide with an existing recipe get a fresh id, so nothing is overwritten silently. Built-in example recipes live in [`recipes/`](recipes/).

---

## Natural-language recipes ("Describe" tab)

| Provider | Where it runs | Setup |
| --- | --- | --- |
| **Offline rules** (default) | in-process | nothing — understands output formats, sort/filter/select/drop/rename, limits, dedupe, case changes, SQL table names & dialects, templates, line operations; English with some Turkish synonyms |
| **Ollama** | your machine | install [Ollama](https://ollama.com), `ollama pull llama3.1`, keep `ollama serve` running |
| **OpenAI-compatible** | your machine or a server you trust | any `/v1/chat/completions` endpoint (LM Studio, llama.cpp, vLLM, LocalAI…) |
| **Anthropic Claude** | Anthropic API | paste your API key in Settings (stored locally); default model `claude-opus-5-5`, refusal fallbacks enabled |

Whatever the provider, the generated recipe is **test-run on your data** before it is shown. If it fails, the model gets up to two repair rounds with the exact error; if it still fails (or the provider is unreachable) the offline rules take over and the UI tells you why. Sending a sample of your data to the model can be turned off in Settings.

---

## Project layout

```
crates/pastecompiler-core/   Rust core library (all logic + tests)
src-tauri/                   Tauri application (commands, state, bundling config)
src/                         React + TypeScript UI
recipes/                     Built-in example recipes (embedded at compile time)
```

Data is stored in the platform app-data directory (macOS: `~/Library/Application Support/com.pastecompiler.app/`): `recipes/*.json` and `settings.json`.

## Languages

The UI and all engine messages (validation report, differences, errors, offline-rule explanations) are available in **English** and **Turkish**. The app follows the system language by default; change it under *Settings → Behaviour → Language*. Every engine call carries a `lang` argument, and the Rust side formats messages through the catalog in `crates/pastecompiler-core/src/i18n.rs`; built-in recipes carry a `translations` map for their names and descriptions. When an AI provider is used, the model is asked to write the recipe name, description and explanation in the user's language.

## Keyboard shortcuts

| Shortcut | Action |
| --- | --- |
| `⌘⇧V` / `Ctrl+Shift+V` | paste from clipboard and convert |
| `⌘↩` / `Ctrl+Enter` | copy the output |
| `⌘,` / `Ctrl+,` | settings |

## License

MIT — see [LICENSE](LICENSE).

---

## Türkçe özet

PasteCompiler, kopyaladığın herhangi bir veriyi (Excel tablosu, JSON, CSV, YAML, TOML, XML, Markdown/HTML tablosu, `.env`, liste, düz metin) otomatik algılayıp istediğin formata dönüştüren açık kaynak bir masaüstü uygulamasıdır. Her dönüşümden sonra çıktı geri ayrıştırılıp girdiyle karşılaştırılır; veri kaybı, format hatası ve tutarsızlıklar rapor edilir. Dönüşüm adımlarını **tarif** olarak kaydedip paylaşabilir (`pcr1.…` paylaşım kodu veya JSON dosyası), tarifleri **doğal dille** tanımlayabilirsin — tamamen çevrimdışı kural motoruyla ya da Ollama / OpenAI-uyumlu sunucu / Anthropic ile. Arayüz ve tüm motor mesajları **Türkçe** ve İngilizce olarak sunulur; dil varsayılan olarak sistem dilini izler, *Ayarlar → Davranış → Dil* bölümünden değiştirilebilir.

```bash
npm install
npm run tauri dev     # geliştirme
npm run tauri build   # target/release/bundle/ altında .app ve .dmg
cargo test --workspace && npm test
```
