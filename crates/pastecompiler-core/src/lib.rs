//! PasteCompiler core library: detects the format of pasted text, parses it
//! into an intermediate document, applies recipe steps, writes any supported
//! output format and validates the result for data loss.

pub mod ai;
pub mod coerce;
pub mod detect;
pub mod error;
pub mod i18n;
pub mod model;
pub mod output;
pub mod parse;
pub mod pipeline;
pub mod recipe;
pub mod settings;
pub mod store;
pub mod template;
pub mod validate;

pub use ai::{generate_recipe, AiSettings, GeneratedRecipe, Provider};
pub use detect::{detect, Detection};
pub use error::{Error, Result};
pub use i18n::Lang;
pub use model::{
    Diagnostics, DocKind, DocSummary, Document, InputFormat, Issue, OutputFormat, Scalar, Severity,
    Table,
};
pub use output::OutputOptions;
pub use parse::ParseOptions;
pub use pipeline::{
    analyze, convert, input_formats, output_formats, Analysis, ConvertRequest, ConvertResult,
    FormatInfo,
};
pub use recipe::{Recipe, Step};
pub use settings::AppSettings;
pub use store::{builtin_recipes, RecipeStore};
pub use validate::{RoundTrip, ValidationReport};
