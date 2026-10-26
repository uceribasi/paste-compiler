//! Message catalog for user-facing engine text (English and Turkish).
//!
//! The current language is thread-local: entry points (Tauri commands, the
//! dev server) call [`Lang::set`] before doing any work, and every message is
//! produced through [`t`] at the moment it is created.

use serde::{Deserialize, Serialize};
use std::cell::Cell;
use std::fmt::Display;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    #[default]
    En,
    Tr,
}

thread_local! {
    static CURRENT: Cell<Lang> = const { Cell::new(Lang::En) };
}

impl Lang {
    /// Parse a BCP-47 tag or setting value (`tr`, `tr-TR`, `en`, `system`...).
    pub fn from_tag(tag: &str) -> Option<Lang> {
        let lower = tag.trim().to_lowercase();
        if lower.starts_with("tr") {
            Some(Lang::Tr)
        } else if lower.starts_with("en") {
            Some(Lang::En)
        } else {
            None
        }
    }

    pub fn current() -> Lang {
        CURRENT.with(|c| c.get())
    }

    pub fn set(lang: Lang) {
        CURRENT.with(|c| c.set(lang));
    }

    /// Set the language for this thread from an optional tag; `None` or an
    /// unknown tag means English.
    pub fn enter(tag: Option<&str>) -> Lang {
        let lang = tag.and_then(Lang::from_tag).unwrap_or_default();
        Lang::set(lang);
        lang
    }

    pub fn tag(&self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Tr => "tr",
        }
    }
}

fn fill(template: &str, args: &[&dyn Display]) -> String {
    let mut out = template.to_string();
    for (i, arg) in args.iter().enumerate() {
        out = out.replace(&format!("{{{i}}}"), &arg.to_string());
    }
    out
}

/// Format message `code` in the current language.
pub fn t(code: &str, args: &[&dyn Display]) -> String {
    t_in(Lang::current(), code, args)
}

/// Format message `code` in `lang`.
pub fn t_in(lang: Lang, code: &str, args: &[&dyn Display]) -> String {
    let (en, tr) = catalog(code);
    let template = match lang {
        Lang::En => en,
        Lang::Tr => tr,
    };
    fill(template, args)
}

/// English and Turkish templates for every message code. Unknown codes fall
/// back to printing the arguments.
fn catalog(code: &str) -> (&'static str, &'static str) {
    match code {
        // --- parsing -----------------------------------------------------
        "crlf_line_endings" => ("input uses Windows (CRLF) line endings; output uses LF", "girdi Windows (CRLF) satır sonları kullanıyor; çıktı LF kullanır"),
        "jsonl_bad_line" => ("line {0} is not valid JSON and was skipped: {1}", "{0}. satır geçerli JSON değil ve atlandı: {1}"),
        "jsonl_bad_lines" => ("{0} lines were not valid JSON and were skipped", "{0} satır geçerli JSON değildi ve atlandı"),
        "no_header" => ("no header row detected; generated column names (column_1, column_2, ...)", "başlık satırı algılanmadı; sütun adları üretildi (column_1, column_2, ...)"),
        "mixed_types" => ("column '{0}' mixes {1} values", "'{0}' sütununda {1} değerleri karışık"),
        "empty_table" => ("the table has a header but no data rows", "tabloda başlık var ama veri satırı yok"),
        "decimal_comma" => ("{0} cell(s) look like decimal-comma numbers (e.g. 3,14) and were kept as text", "{0} hücre ondalık virgüllü sayıya benziyor (örn. 3,14) ve metin olarak bırakıldı"),
        "markdown_prefix_ignored" => ("{0} line(s) before the table were ignored", "tablodan önceki {0} satır yok sayıldı"),
        "multiple_tables" => ("{0} tables found; only the first one was converted", "{0} tablo bulundu; yalnızca ilki dönüştürüldü"),
        "html_spans_ignored" => ("colspan/rowspan attributes were ignored; merged cells may be misaligned", "colspan/rowspan öznitelikleri yok sayıldı; birleştirilmiş hücreler kaymış olabilir"),
        "xml_attributes" => ("XML attributes are mapped to `@name` keys and text content to `#text`", "XML öznitelikleri `@ad` anahtarlarına, metin içeriği `#text` anahtarına eşlendi"),
        "kv_lines_ignored" => ("{0} line(s) without a key/value separator were ignored", "anahtar/değer ayırıcısı olmayan {0} satır yok sayıldı"),
        "kv_duplicate_keys" => ("{0} duplicate key(s); the last value was kept", "{0} yinelenen anahtar; son değer tutuldu"),
        "ragged_rows" => ("{0} row(s) had a different number of cells than the header; missing cells were filled with empty values", "{0} satırın hücre sayısı başlıktan farklıydı; eksik hücreler boş değerle dolduruldu"),
        "headers_adjusted" => ("header names adjusted: {0}", "başlık adları düzeltildi: {0}"),
        "empty_header_renamed" => ("empty header #{0} -> {1}", "boş başlık #{0} -> {1}"),
        "duplicate_header_renamed" => ("duplicate header {0} -> {1}", "yinelenen başlık {0} -> {1}"),
        "detection_fallback" => ("{0} could not be parsed ({1}); parsed as {2} instead", "{0} olarak ayrıştırılamadı ({1}); bunun yerine {2} olarak ayrıştırıldı"),
        // parse errors
        "err_invalid_json" => ("invalid JSON: {0}", "geçersiz JSON: {0}"),
        "err_no_json_lines" => ("no valid JSON lines found", "geçerli JSON satırı bulunamadı"),
        "err_invalid_delimited" => ("invalid delimited data: {0}", "geçersiz ayrılmış veri: {0}"),
        "err_no_rows" => ("no rows found", "satır bulunamadı"),
        "err_md_needs_rows" => ("a Markdown table needs a header row and a separator row", "Markdown tablosu için bir başlık satırı ve bir ayırıcı satır gerekir"),
        "err_md_no_separator" => ("no `|---|` separator row found", "`|---|` ayırıcı satırı bulunamadı"),
        "err_md_separator_first" => ("separator row found before the header", "ayırıcı satır başlıktan önce geldi"),
        "err_html_no_table" => ("no <table> element found", "<table> öğesi bulunamadı"),
        "err_html_no_rows" => ("the table has no rows", "tabloda satır yok"),
        "err_invalid_yaml" => ("invalid YAML: {0}", "geçersiz YAML: {0}"),
        "err_empty_yaml" => ("empty YAML", "boş YAML"),
        "err_invalid_toml" => ("invalid TOML: {0}", "geçersiz TOML: {0}"),
        "err_invalid_xml" => ("invalid XML: {0}", "geçersiz XML: {0}"),
        "err_no_key_values" => ("no key: value pairs found", "anahtar: değer çifti bulunamadı"),
        "err_delimiter_ascii" => ("delimiter {0} must be a single ASCII character", "ayırıcı {0} tek bir ASCII karakteri olmalı"),
        "err_could_not_parse" => ("could not parse input", "girdi ayrıştırılamadı"),
        "err_unknown_input_format" => ("unknown input format '{0}'", "bilinmeyen girdi formatı '{0}'"),
        // --- shape coercion --------------------------------------------------
        "nested_stringified" => ("nested value in '{0}' was stored as a JSON string", "'{0}' içindeki iç içe değer JSON metni olarak saklandı"),
        "nested_stringified_rows" => ("nested values inside array rows were stored as JSON strings", "dizi satırlarındaki iç içe değerler JSON metni olarak saklandı"),
        "flattened" => ("nested objects/arrays were flattened into dotted column names (e.g. address.city, tags.0)", "iç içe nesneler/diziler noktalı sütun adlarına düzleştirildi (örn. address.city, tags.0)"),
        "flattened_keys" => ("nested values were flattened into dotted keys", "iç içe değerler noktalı anahtarlara düzleştirildi"),
        "flattened_kv" => ("nested keys were written with dots (a.b.c)", "iç içe anahtarlar noktalı yazıldı (a.b.c)"),
        "sparse_keys" => ("records have different keys; missing values were filled with empty cells", "kayıtların anahtarları farklı; eksik değerler boş hücreyle dolduruldu"),
        "mixed_array" => ("array mixes objects and scalars; scalars were placed in a 'value' column", "dizi nesne ve düz değerleri karıştırıyor; düz değerler 'value' sütununa kondu"),
        "envelope_unwrapped" => ("used the records inside '{0}' as table rows", "'{0}' içindeki kayıtlar tablo satırı olarak kullanıldı"),
        "map_of_records" => ("object keys were placed in a 'key' column", "nesne anahtarları 'key' sütununa kondu"),
        "text_as_rows" => ("each line of the text became a table row", "metnin her satırı bir tablo satırı oldu"),
        "text_as_items" => ("each line of the text became an array item", "metnin her satırı bir dizi öğesi oldu"),
        "dotted_headers" => ("headers contain dots; use the `unflatten` step to rebuild nested objects", "başlıklar nokta içeriyor; iç içe nesneleri geri kurmak için `unflatten` adımını kullan"),
        "rows_joined" => ("multi-column rows were joined with ', '; use a template for custom formatting", "çok sütunlu satırlar ', ' ile birleştirildi; özel biçim için şablon kullan"),
        "coerced_to_table" => ("input was converted to a table to apply the `{0}` step", "`{0}` adımını uygulamak için girdi tabloya dönüştürüldü"),
        // --- writers ------------------------------------------------------------
        "md_newlines" => ("line breaks inside cells were written as <br>", "hücre içindeki satır sonları <br> olarak yazıldı"),
        "toml_null_dropped" => ("TOML cannot represent null; '{0}' was dropped", "TOML null değeri gösteremez; '{0}' atıldı"),
        "toml_array_wrapped" => ("top-level array was wrapped in [[{0}]] tables", "en üst düzey dizi [[{0}]] tablolarına sarıldı"),
        "xml_names_sanitized" => ("some keys were not valid XML element names and were adjusted (invalid characters -> `_`)", "bazı anahtarlar geçerli XML öğe adı değildi ve düzeltildi (geçersiz karakterler -> `_`)"),
        "xml_single_item" => ("a one-element array is written as a single element; XML cannot distinguish it from a scalar", "tek öğeli dizi tek öğe olarak yazılır; XML bunu düz değerden ayırt edemez"),
        "sql_generated_columns" => ("the input had no header; generated column names were used", "girdide başlık yoktu; üretilmiş sütun adları kullanıldı"),
        "sql_no_rows" => ("no rows to insert", "eklenecek satır yok"),
        "kv_blocks" => ("each row was written as a block of key/value lines separated by a blank line", "her satır boş satırla ayrılmış anahtar/değer bloğu olarak yazıldı"),
        "template_unknown_var" => ("template variable '{{{0}}}' does not match any column; it renders empty", "şablon değişkeni '{{{0}}}' hiçbir sütunla eşleşmiyor; boş olarak işlenir"),
        "err_sql_dialect" => ("unknown SQL dialect '{0}' (use postgres, mysql or sqlite)", "bilinmeyen SQL lehçesi '{0}' (postgres, mysql veya sqlite kullan)"),
        "err_template_missing" => ("the template output needs a `template` option, e.g. \"{{name}} <{{email}}>\"", "şablon çıktısı için bir `template` seçeneği gerekir, örn. \"{{name}} <{{email}}>\""),
        "err_toml_unrepresentable" => ("cannot represent this data as TOML: {0}", "bu veri TOML olarak gösterilemiyor: {0}"),
        "err_yaml_serialize" => ("YAML serialization failed: {0}", "YAML yazılamadı: {0}"),
        // --- recipe steps ---------------------------------------------------------
        "to_number_failed" => ("column '{0}': {1} could not be converted to a number and became empty", "'{0}' sütunu: {1} sayıya çevrilemedi ve boş bırakıldı"),
        "to_bool_failed" => ("column '{0}': {1} is not a recognised boolean and became empty", "'{0}' sütunu: {1} tanınan bir mantıksal değer değil ve boş bırakıldı"),
        "columns_left_out" => ("{0} column(s) were left out by select_columns", "select_columns {0} sütunu dışarıda bıraktı"),
        "columns_dropped" => ("{0} column(s) dropped", "{0} sütun kaldırıldı"),
        "rows_filtered" => ("{0} of {1} row(s) kept by the filter", "filtre {1} satırdan {0} tanesini tuttu"),
        "rows_limited" => ("{0} of {1} row(s) kept by limit", "limit {1} satırdan {0} tanesini tuttu"),
        "split_overflow" => ("{0} row(s) had more parts than target columns; the remainder was kept in the last column", "{0} satırda hedef sütundan fazla parça vardı; kalan kısım son sütunda tutuldu"),
        "rows_deduped" => ("{0} duplicate row(s) removed", "{0} yinelenen satır kaldırıldı"),
        "lines_deduped" => ("{0} duplicate line(s) removed", "{0} yinelenen satır kaldırıldı"),
        "lines_removed" => ("{0} empty line(s) removed", "{0} boş satır kaldırıldı"),
        "cells_filled" => ("{0} empty cell(s) filled with {1}", "{0} boş hücre {1} ile dolduruldu"),
        "err_column_not_found" => ("column '{0}' not found; available columns: {1}", "'{0}' sütunu bulunamadı; mevcut sütunlar: {1}"),
        "err_split_targets" => ("split_column needs at least one target column name in `into`", "split_column için `into` içinde en az bir hedef sütun adı gerekir"),
        "err_invalid_regex" => ("invalid regex '{0}': {1}", "geçersiz düzenli ifade '{0}': {1}"),
        "err_step_failed" => ("step {0} ({1}): {2}", "{0}. adım ({1}): {2}"),
        "err_no_recipe" => ("no recipe given", "tarif verilmedi"),
        "check_no_name" => ("recipe has no name", "tarifin adı yok"),
        "check_schema_version" => ("recipe schema version {0} is newer than supported ({1})", "tarif şema sürümü {0}, desteklenen sürümden ({1}) daha yeni"),
        "check_template_missing" => ("template output needs a `template` option", "şablon çıktısı için `template` seçeneği gerekir"),
        "check_step_regex" => ("step {0}: invalid regex: {1}", "{0}. adım: geçersiz düzenli ifade: {1}"),
        "check_step_no_columns" => ("step {0}: no columns given", "{0}. adım: sütun verilmedi"),
        "err_recipe_no_json" => ("no JSON object found in the recipe text", "tarif metninde JSON nesnesi bulunamadı"),
        "err_recipe_invalid_json" => ("recipe is not valid JSON: {0}", "tarif geçerli JSON değil: {0}"),
        "err_recipe_not_object" => ("recipe must be a JSON object", "tarif bir JSON nesnesi olmalı"),
        "err_unknown_output_format" => ("unknown output format '{0}'", "bilinmeyen çıktı formatı '{0}'"),
        "err_recipe_no_output" => ("recipe has no `output` section", "tarifte `output` bölümü yok"),
        "err_recipe_invalid" => ("recipe is not valid: {0}", "tarif geçerli değil: {0}"),
        "err_share_string" => ("invalid share string: {0}", "geçersiz paylaşım kodu: {0}"),
        "err_recipe_not_found" => ("recipe '{0}' not found", "'{0}' tarifi bulunamadı"),
        "imported_suffix" => ("(imported)", "(içe aktarıldı)"),
        // --- templates -----------------------------------------------------------
        "err_unknown_filter" => ("unknown filter '{0}'", "bilinmeyen süzgeç '{0}'"),
        "err_unclosed_template" => ("unclosed '{{' in template", "şablonda kapanmamış '{{'"),
        "err_unknown_variable" => ("unknown variable '{0}'", "bilinmeyen değişken '{0}'"),
        // --- validation ------------------------------------------------------------
        "round_trip_unavailable" => ("{0} cannot be parsed back, so the round-trip check was skipped", "{0} geri ayrıştırılamadığı için gidiş-dönüş kontrolü atlandı"),
        "output_unparseable" => ("the output could not be parsed back as {0}: {1}", "çıktı {0} olarak geri ayrıştırılamadı: {1}"),
        "round_trip_differences" => ("{0} difference(s) found when parsing the output back; see the differences list", "çıktı geri ayrıştırıldığında {0} fark bulundu; farklar listesine bak"),
        "diff_header" => ("header row differs: expected {0}, got {1}", "başlık satırı farklı: beklenen {0}, bulunan {1}"),
        "diff_row_count" => ("row count differs: {0} rows in, {1} rows out", "satır sayısı farklı: girişte {0}, çıkışta {1} satır"),
        "diff_column_count" => ("column count differs: {0} in, {1} out", "sütun sayısı farklı: girişte {0}, çıkışta {1}"),
        "diff_cell" => ("row {0} column '{1}': {2} ({3} -> {4})", "{0}. satır '{1}' sütunu: {2} ({3} -> {4})"),
        "diff_kind_whitespace" => ("whitespace changed", "boşluklar değişti"),
        "diff_kind_value" => ("value changed", "değer değişti"),
        "diff_missing" => ("'{0}' is missing from the output", "'{0}' çıktıda yok"),
        "diff_changed" => ("'{0}': {1} -> {2}", "'{0}': {1} -> {2}"),
        "diff_extra" => ("'{0}' appeared in the output but not in the input", "'{0}' çıktıda var ama girdide yoktu"),
        "diff_item_count" => ("item count differs: {0} in, {1} out", "öğe sayısı farklı: girişte {0}, çıkışta {1}"),
        "diff_item" => ("item {0}: {1} -> {2}", "{0}. öğe: {1} -> {2}"),
        // --- AI / providers ----------------------------------------------------------
        "ai_describe_first" => ("please describe the conversion you want", "lütfen istediğin dönüşümü tarif et"),
        "ai_no_json" => ("the reply did not contain a JSON object", "yanıt bir JSON nesnesi içermiyordu"),
        "ai_invalid_json" => ("the reply was not valid JSON: {0}", "yanıt geçerli JSON değildi: {0}"),
        "ai_gave_up" => ("the model could not produce a working recipe after {0} attempts (last error: {1})", "model {0} denemede çalışan bir tarif üretemedi (son hata: {1})"),
        "ai_fallback_failed" => ("{0}. Offline fallback also failed: {1}", "{0}. Çevrimdışı yedek de başarısız oldu: {1}"),
        "ai_rules_no_model" => ("the rule-based provider does not call a model", "kural tabanlı sağlayıcı model çağırmaz"),
        "hint_ollama" => ("could not reach Ollama at {0}. Is it running? Start it with `ollama serve` and pull a model (e.g. `ollama pull llama3.1`).", "{0} adresindeki Ollama'ya ulaşılamadı. Çalışıyor mu? `ollama serve` ile başlat ve bir model indir (örn. `ollama pull llama3.1`)."),
        "hint_openai" => ("could not reach the OpenAI-compatible server at {0}. Check that it is running and the base URL (usually ending in /v1) is right.", "{0} adresindeki OpenAI-uyumlu sunucuya ulaşılamadı. Çalıştığından ve temel URL'nin (genellikle /v1 ile biter) doğru olduğundan emin ol."),
        "hint_anthropic" => ("could not reach {0}. Check your internet connection.", "{0} adresine ulaşılamadı. İnternet bağlantını kontrol et."),
        "hint_none" => ("no connection needed", "bağlantı gerekmez"),
        "err_anthropic_key" => ("an Anthropic API key is required (Settings → AI)", "Anthropic API anahtarı gerekli (Ayarlar → Yapay zekâ)"),
        "err_anthropic_key_short" => ("an Anthropic API key is required", "Anthropic API anahtarı gerekli"),
        "err_ollama" => ("Ollama error: {0}", "Ollama hatası: {0}"),
        "err_ollama_empty" => ("Ollama returned no message content", "Ollama boş yanıt döndürdü"),
        "err_server" => ("server error: {0}", "sunucu hatası: {0}"),
        "err_server_empty" => ("the server returned no message content", "sunucu boş yanıt döndürdü"),
        "err_anthropic" => ("Anthropic API error: {0}", "Anthropic API hatası: {0}"),
        "err_model_declined" => ("the model declined: {0}", "model reddetti: {0}"),
        "err_model_declined_default" => ("the model declined this request", "model bu isteği reddetti"),
        "err_model_no_text" => ("the model returned no text", "model metin döndürmedi"),
        "test_rules" => ("Offline rule-based mode: no connection needed.", "Çevrimdışı kural modu: bağlantı gerekmez."),
        "test_ollama_ok" => ("Connected to Ollama. {0} model(s) installed{1}", "Ollama'ya bağlanıldı. {0} model kurulu{1}"),
        "test_ollama_missing" => (". Note: '{0}' is not installed; run `ollama pull {0}` or pick one of the installed models.", ". Not: '{0}' kurulu değil; `ollama pull {0}` çalıştır veya kurulu modellerden birini seç."),
        "test_openai_ok" => ("Connected. The server lists {0} model(s).", "Bağlanıldı. Sunucu {0} model listeliyor."),
        "test_anthropic_ok" => ("Connected to the Anthropic API. Model: {0}.", "Anthropic API'ye bağlanıldı. Model: {0}."),
        // --- offline rules --------------------------------------------------------
        "r_template" => ("render each row with the template `{0}`", "her satırı `{0}` şablonuyla işle"),
        "r_rename" => ("rename column '{0}' to '{1}'", "'{0}' sütununu '{1}' olarak yeniden adlandır"),
        "r_sort_desc" => ("sort by '{0}' descending", "'{0}' sütununa göre azalan sırala"),
        "r_sort_asc" => ("sort by '{0}' ascending", "'{0}' sütununa göre artan sırala"),
        "r_sort_lines_desc" => ("sort lines descending", "satırları azalan sırala"),
        "r_sort_lines" => ("sort lines alphabetically", "satırları alfabetik sırala"),
        "r_filter" => ("keep rows where '{0}' {1}", "'{0}' {1} olan satırları tut"),
        "r_drop_empty" => ("drop rows where '{0}' is empty", "'{0}' boş olan satırları at"),
        "r_dedupe_lines" => ("remove duplicate lines", "yinelenen satırları kaldır"),
        "r_dedupe_by" => ("remove duplicate rows by '{0}'", "'{0}' değerine göre yinelenen satırları kaldır"),
        "r_dedupe" => ("remove duplicate rows", "yinelenen satırları kaldır"),
        "r_remove_empty" => ("remove empty lines", "boş satırları kaldır"),
        "r_reverse" => ("reverse the lines", "satırları ters çevir"),
        "r_trim" => ("trim whitespace", "boşlukları kırp"),
        "r_header_case" => ("convert headers to {0}", "başlıkları {0} biçimine çevir"),
        "r_upper_lines" => ("uppercase all lines", "tüm satırları büyük harfe çevir"),
        "r_lower_lines" => ("lowercase all lines", "tüm satırları küçük harfe çevir"),
        "r_map" => ("apply {0} to '{1}'", "'{1}' sütununa {0} uygula"),
        "r_transpose" => ("transpose rows and columns", "satır ve sütunları yer değiştir"),
        "r_limit" => ("keep the first {0} rows", "ilk {0} satırı tut"),
        "r_drop_cols" => ("drop columns {0}", "{0} sütunlarını kaldır"),
        "r_keep_cols" => ("keep only columns {0}", "yalnızca {0} sütunlarını tut"),
        "r_output" => ("output as {0}", "{0} olarak çıktı ver"),
        "r_delim_semicolon" => ("use ';' as the delimiter", "ayırıcı olarak ';' kullan"),
        "r_no_header" => ("omit the header row", "başlık satırını atla"),
        "r_table" => ("insert into table '{0}'", "'{0}' tablosuna ekle"),
        "r_create_table" => ("include a CREATE TABLE statement", "CREATE TABLE ifadesi ekle"),
        "r_explanation" => ("Offline rule-based interpretation: {0}.", "Çevrimdışı kural tabanlı yorum: {0}."),
        "r_unmatched" => (" Could not match {0} to a column, so that condition was skipped{1}.", " {0} hiçbir sütunla eşleşmedi, bu koşul atlandı{1}."),
        "r_available" => (" (available: {0})", " (mevcut: {0})"),
        "r_description" => ("Generated offline from: {0}", "Çevrimdışı üretildi: {0}"),
        "r_not_understood" => (
            "I could not understand that instruction in offline mode. Try phrases like “convert to json”, “sort by age descending”, “only the name and email columns”, “where status is active”, “to sql table users” or “uppercase the name column”. Configure an AI provider in Settings for free-form instructions.",
            "Bu komutu çevrimdışı modda anlayamadım. “json'a çevir”, “yaşa göre azalan sırala”, “sadece name ve email sütunları”, “where status is active”, “to sql table users” veya “uppercase the name column” gibi ifadeler dene. Serbest komutlar için Ayarlar'dan bir yapay zekâ sağlayıcısı yapılandır.",
        ),
        // condition phrases used by r_filter
        "cond_eq" => ("equals {0}", "{0} değerine eşit"),
        "cond_ne" => ("is not {0}", "{0} değerinden farklı"),
        "cond_contains" => ("contains {0}", "{0} içeren"),
        "cond_not_contains" => ("does not contain {0}", "{0} içermeyen"),
        "cond_starts_with" => ("starts with {0}", "{0} ile başlayan"),
        "cond_ends_with" => ("ends with {0}", "{0} ile biten"),
        "cond_gt" => ("is greater than {0}", "{0} değerinden büyük"),
        "cond_gte" => ("is at least {0}", "en az {0}"),
        "cond_lt" => ("is less than {0}", "{0} değerinden küçük"),
        "cond_lte" => ("is at most {0}", "en çok {0}"),
        "cond_regex" => ("matches {0}", "{0} ile eşleşen"),
        "cond_empty" => ("is empty", "boş"),
        "cond_not_empty" => ("is not empty", "boş olmayan"),
        // sort/case words
        "word_and" => (" and ", " ve "),
        "word_descending" => ("descending", "azalan"),
        "word_ascending" => ("ascending", "artan"),
        _ => ("{0}", "{0}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_placeholders_in_both_languages() {
        assert_eq!(
            t_in(Lang::En, "rows_filtered", &[&2, &5]),
            "2 of 5 row(s) kept by the filter"
        );
        assert_eq!(
            t_in(Lang::Tr, "rows_filtered", &[&2, &5]),
            "filtre 5 satırdan 2 tanesini tuttu"
        );
        assert_eq!(t_in(Lang::Tr, "nope_code", &[&"x"]), "x");
    }

    #[test]
    fn thread_local_language() {
        assert_eq!(Lang::current(), Lang::En);
        Lang::set(Lang::Tr);
        assert_eq!(t("sql_no_rows", &[]), "eklenecek satır yok");
        Lang::set(Lang::En);
        assert_eq!(t("sql_no_rows", &[]), "no rows to insert");
        assert_eq!(Lang::enter(Some("tr-TR")), Lang::Tr);
        assert_eq!(Lang::enter(Some("system")), Lang::En);
        assert_eq!(Lang::enter(None), Lang::En);
        assert_eq!(Lang::from_tag("TR"), Some(Lang::Tr));
        assert_eq!(Lang::Tr.tag(), "tr");
    }
}
