# PasteCompiler product website

Static, bilingual landing and download site. `/` is Turkish; `/en/` is English.
The desktop app and Rust engine are unchanged. This folder has no npm dependencies.

## Update

Edit translations and markup in `build.py`, then run `python3 website/build.py`
from the repository root. Commit both generated pages along with source changes.
Styles and the four-format browser demo live in `styles.css` and `script.js`.

## Preview

`python3 -m http.server 4174 --directory website`

## Coolify

- Repository: `https://github.com/uceribasi/paste-compiler`
- Branch: `main`
- Build pack: Static
- Base directory: `/website`
- Internal port: `80`
- No build command: HTML is checked in and ready to serve.

The browser demo accepts CSV/TSV and generates JSON, PostgreSQL-style SQL,
Markdown tables, and TSV locally. It is a deliberately small JavaScript example,
not the desktop Rust engine or its round-trip validation. It makes no network
requests and does not persist clipboard contents. Download buttons point to the
existing v0.1.0 Apple Silicon macOS release. Change them in `build.py` when a new
release is ready.
