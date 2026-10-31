#!/usr/bin/env python3
"""Generate the dependency-free Turkish and English product pages."""
from pathlib import Path
from string import Template
import html
import json

ROOT = Path(__file__).resolve().parent
REPO = 'https://github.com/uceribasi/paste-compiler'
DMG = REPO + '/releases/download/v0.1.0/PasteCompiler_0.1.0_aarch64.dmg'

COPY = {
 'tr': {
  'title': 'PasteCompiler — Kopyaladığın veri. İhtiyacın olan biçim.',
  'description': 'CSV, JSON, SQL ve daha fazlası. Kopyaladığın veriyi dönüştür, değişiklikleri gör ve iş akışlarını tarif olarak kaydet. Ücretsiz, açık kaynak Mac uygulaması.',
  'skip': 'İçeriğe geç', 'nav_demo': 'Dene', 'nav_formats': 'Biçimler', 'nav_recipes': 'Tarifler', 'nav_download': 'İndir', 'menu': 'Menüyü aç',
  'eyebrow': 'PANONDAN İŞ AKIŞINA', 'headline': 'Kopyaladığın veri.<br><em>İhtiyacın olan biçim.</em>',
  'intro': 'Bir tablo, bir JSON, dağınık bir liste. PasteCompiler ne kopyaladığını anlar, istediğin biçime dönüştürür ve nelerin değiştiğini gösterir.',
  'download': 'Mac için ücretsiz indir', 'try': 'Tarayıcıda dene', 'meta': 'Apple Silicon · macOS 11+ · v0.1.0',
  'proof': ['17 çıktı biçimi', 'Temel dönüşümler çevrimdışı', 'Ücretsiz ve açık kaynak'],
  'visual_input': 'KOPYALADIĞIN', 'visual_output': 'İHTİYACIN OLAN', 'visual_caption': 'Daha az elle düzeltme. Daha çok işini bitirme.',
  'demo_eyebrow': 'KÜÇÜK BİR DÖNÜŞÜM. BÜYÜK BİR RAHATLIK.', 'demo_title': 'Veriyi yapıştır.<br>Diğer taraftan devam et.',
  'demo_intro': 'Örnek veriyi düzenle, çıktı biçimini seç ve sonucu kopyala. Bu küçük demo tamamen tarayıcında çalışır.',
  'sample_label': 'Örnek veri', 'sample_people': 'Kişiler', 'sample_products': 'Ürünler', 'input_label': 'Girdi · CSV veya TSV', 'output_label': 'Çıktı', 'copy': 'Çıktıyı kopyala',
  'demo_note': 'Tarayıcı demosu 4 çıktı biçimini gösterir. Otomatik algılama, 17 biçim, tarifler ve ayrıntılı dönüşüm raporu masaüstü uygulamasında.',
  'report_label': 'ÖNCE DÖNÜŞTÜR. SONRA KONTROL ET.', 'report_title': 'Bir şey değiştiyse,<br>haberinin olsun.',
  'report_intro': 'Masaüstü uygulaması, desteklenen çıktıları yeniden okuyup girdiyle karşılaştırır. Satırlar, sütunlar, hücreler ve iç içe anahtarlar kontrol edilir.',
  'report_items': [('Satır ve sütunlar', 'Eksilen satırları ve değişen yapıyı gör.'), ('Hücreler ve türler', 'Dönüşümde değişen değerleri fark et.'), ('Olası kayıplar', 'Düzleştirilen iç içe veriler ve diğer uyarılar görünür olur.')],
  'report_example': 'Rapor örnekleri', 'report_ok': 'Desteklenen çıktı yeniden okundu', 'report_warn': 'İç içe veri düzleştirildi', 'report_na': 'SQL için geri okuma kontrolü yok',
  'report_limit': 'SQL, özel şablonlar ve düz metin çıktılarında geri okuma doğrulaması desteklenmez. Uygulama bu durumu raporda açıkça belirtir.',
  'formats_label': 'AYNI VERİ. FARKLI İŞLER.', 'formats_title': 'Tablodan koda.<br>Listeden veritabanına.', 'formats_intro': 'Masaüstünde 17 çıktı biçimi. İhtiyacın olan araç hangisiyse, verini oraya taşı.',
  'format_groups': [('01', 'Yapılandırılmış veri', 'JSON, JSON Lines, YAML, TOML ve XML.'), ('02', 'Tablolar', 'CSV, TSV, Markdown ve HTML tabloları.'), ('03', 'Veritabanları', 'Postgres, MySQL ve SQLite için SQL INSERT. İsteğe bağlı CREATE TABLE.'), ('04', 'Kendi düzenin', 'Listeler, anahtar/değer çiftleri, düz metin ve özel {{şablonlar}}.')],
  'formats_all': 'Tüm biçimleri ve seçenekleri gör',
  'recipes_label': 'BİR KEZ HAZIRLA. TEKRAR TEKRAR KULLAN.', 'recipes_title': 'Tekrarlayan işler<br>tarife dönüşsün.',
  'recipes_intro': 'Sütunları seç, filtrele, sırala ve yinelenenleri temizle. Adımları bir tarif olarak kaydet; bir sonraki veri için tek tıkla çalıştır.',
  'recipe_flow': ['Sütunları seç', 'Ada göre sırala', 'SQL olarak ver'], 'recipe_example': 'ÖRNEK TARİF · KİŞİLER → SQL', 'recipe_export': 'JSON dosyası veya kısa paylaşım koduyla dışa aktar.',
  'describe_title': 'Ne istediğini anlat.', 'describe_body': 'Doğal dille tarif oluştur. Yerleşik kural motoru çevrimdışı çalışır; istersen Ollama, OpenAI uyumlu bir sunucu veya Anthropic kullan. Yerleşik kurallar İngilizce ve bazı Türkçe ifadeleri anlar.',
  'privacy_label': 'VERİN SENDE KALSIN', 'privacy_title': 'Panonun buluta<br>ihtiyacı yok.',
  'privacy_body': 'Temel dönüşümler ve yerleşik tarif kuralları bilgisayarında çalışır. Uzak bir AI sağlayıcısı ayarlarsan veri örneği gönderilip gönderilmeyeceğini sen seçersin.',
  'privacy_tags': ['Hesap gerektirmeyen temel dönüşümler', 'AI kullanımı isteğe bağlı', 'Yerel model desteği'],
  'install_label': 'MAC’İNDE KENDİ VERİNLE DENE', 'install_title': 'Panona küçük<br>bir süper güç.', 'install_intro': 'Hazır Mac uygulamasını indir. Temel dönüşümler için Rust, Node.js veya AI hesabı kurman gerekmez.',
  'install_steps': [('İndir', 'Apple Silicon (M1 ve sonrası) ve macOS 11+ için DMG dosyasını indir.'), ('Uygulamalar’a taşı', 'DMG dosyasını aç ve PasteCompiler’ı Uygulamalar klasörüne sürükle.'), ('Yapıştır ve dönüştür', 'Uygulamayı aç, verini yapıştır ve istediğin çıktı biçimini seç.')],
  'install_note': 'v0.1.0 henüz Developer ID ile imzalı veya Apple tarafından notarize edilmiş değil. İlk açılış için kurulum rehberine göz at.',
  'guide': 'Kurulum rehberi', 'release': 'Sürüm notları', 'checksum': 'SHA-256 sağlama toplamları', 'requirements': 'Apple Silicon · macOS 11+ · DMG, 3.8 MiB',
  'faq_title': 'Birkaç kısa cevap.', 'faqs': [
   ('Ücretsiz mi?', 'Evet. PasteCompiler MIT lisanslı ve açık kaynak. Temel dönüşümler için abonelik gerekmez. Seçtiğin uzak AI hizmetinin ayrı kullanım ücretleri olabilir.'),
   ('Windows, Linux veya Intel Mac’te çalışır mı?', 'Bu sürümün hazır indirmesi yalnızca Apple Silicon Mac içindir. Intel Mac, Windows ve Linux için kaynak koddan derleme seçeneği bulunur; bu sürümde hazır paket sunulmuyor.'),
   ('AI hesabı veya API anahtarı gerekiyor mu?', 'Hayır. Biçim algılama, temel dönüşümler, tarifler ve yerleşik doğal dil kuralları AI hesabı olmadan çalışır. Model bağlantısı isteğe bağlıdır.'),
   ('Verim sunucuya gönderiliyor mu?', 'Temel işlemler yereldir. Uzak bir AI sağlayıcısı kullanırsan veri örneği paylaşımını Ayarlar’dan kontrol edersin. Bu sayfadaki küçük demo da verini tarayıcıda işler.'),
   ('Her dönüşümün kayıpsız olduğu garanti mi?', 'Hayır. Bazı biçimler aynı veri yapısını ifade edemez. Uygulama desteklenen çıktıları yeniden okuyarak farkları ve olası kayıpları raporlar. SQL, özel şablonlar ve düz metinde bu kontrol kullanılamaz.'),
   ('Tariflerimi paylaşabilir miyim?', 'Evet. Tarifleri JSON dosyası veya pcr1. ile başlayan kısa bir paylaşım kodu olarak dışa aktarabilir ve başka bir cihazda içe aktarabilirsin.')],
  'footer_line': 'Kopyaladığın yerden, ihtiyacın olan yere.', 'source': 'Kaynak kodu', 'feedback': 'Geri bildirim', 'license': 'Açık kaynak · MIT',
  'js': {'rows': 'satır', 'columns': 'sütun', 'ready': 'Dönüştürüldü', 'empty': 'Dönüştürmek için başlık satırı ve en az bir veri satırı ekle.', 'quotes': 'CSV tırnaklarını kontrol et.', 'ragged': 'Her satırın başlık satırıyla aynı sayıda hücresi olmalı.', 'headers': 'Sütun başlıkları dolu ve birbirinden farklı olmalı.', 'limit': 'Bu demo en fazla 200 satır ve 20.000 karakter kabul eder.', 'copied': 'Çıktı kopyalandı.', 'copy_fail': 'Çıktıyı seçip kopyalayabilirsin.', 'sql_note': 'SQL çıktısı için geri okuma doğrulaması yok.', 'demo_note': 'Ayrıntılı doğrulama masaüstü uygulamasında.', 'invalid': 'Girdiyi kontrol et'}
 },
 'en': {
  'title': 'PasteCompiler — What you copied. In the format you need.',
  'description': 'Convert clipboard data between CSV, JSON, SQL and more. Review changes and save repeatable conversion recipes. A free, open-source desktop app for Mac.',
  'skip': 'Skip to content', 'nav_demo': 'Try it', 'nav_formats': 'Formats', 'nav_recipes': 'Recipes', 'nav_download': 'Download', 'menu': 'Open menu',
  'eyebrow': 'FROM CLIPBOARD TO WORKFLOW', 'headline': 'What you copied.<br><em>In the format you need.</em>',
  'intro': 'A spreadsheet, a JSON blob, a messy list. PasteCompiler detects what you copied, converts it into the format you need, and shows you what changed.',
  'download': 'Download free for Mac', 'try': 'Try it in your browser', 'meta': 'Apple Silicon · macOS 11+ · v0.1.0',
  'proof': ['17 output formats', 'Core conversions work offline', 'Free and open source'],
  'visual_input': 'WHAT YOU COPIED', 'visual_output': 'WHAT YOU NEED', 'visual_caption': 'Less fixing by hand. More getting things done.',
  'demo_eyebrow': 'A SMALL CONVERSION. A BIG RELIEF.', 'demo_title': 'Paste your data.<br>Pick up on the other side.',
  'demo_intro': 'Edit the sample, choose an output format, and copy the result. This small demo runs entirely in your browser.',
  'sample_label': 'Sample data', 'sample_people': 'People', 'sample_products': 'Products', 'input_label': 'Input · CSV or TSV', 'output_label': 'Output', 'copy': 'Copy output',
  'demo_note': 'The browser demo shows 4 output formats. Automatic detection, 17 formats, recipes and detailed conversion reports are in the desktop app.',
  'report_label': 'CONVERT FIRST. THEN CHECK.', 'report_title': 'If something changes,<br>you should know.',
  'report_intro': 'The desktop app parses supported outputs back and compares them with the input. Rows, columns, cell values and nested keys are checked.',
  'report_items': [('Rows and columns', 'See missing rows and changes in structure.'), ('Values and types', 'Spot values that changed during conversion.'), ('Potential data loss', 'Flattened nesting and other warnings stay visible.')],
  'report_example': 'Report examples', 'report_ok': 'Supported output parsed back', 'report_warn': 'Nested data was flattened', 'report_na': 'No round-trip check for SQL',
  'report_limit': 'SQL, custom templates and plain text do not support round-trip verification. The app explicitly marks it as unavailable in the report.',
  'formats_label': 'SAME DATA. DIFFERENT JOBS.', 'formats_title': 'From table to code.<br>From list to database.', 'formats_intro': '17 output formats on desktop. Whatever tool comes next, take your data with you.',
  'format_groups': [('01', 'Structured data', 'JSON, JSON Lines, YAML, TOML and XML.'), ('02', 'Tables', 'CSV, TSV, Markdown and HTML tables.'), ('03', 'Databases', 'SQL INSERT for Postgres, MySQL and SQLite. Optional CREATE TABLE.'), ('04', 'Your own structure', 'Lists, key/value pairs, plain text and custom {{templates}}.')],
  'formats_all': 'See every format and option',
  'recipes_label': 'SET IT UP ONCE. USE IT AGAIN.', 'recipes_title': 'Turn repeat work<br>into a recipe.',
  'recipes_intro': 'Select columns, filter, sort and deduplicate. Save the steps as a recipe and run them on your next dataset with one click.',
  'recipe_flow': ['Select columns', 'Sort by name', 'Export as SQL'], 'recipe_example': 'EXAMPLE RECIPE · PEOPLE → SQL', 'recipe_export': 'Export as a JSON file or a short share code.',
  'describe_title': 'Say what you need.', 'describe_body': 'Describe a recipe in plain language. Built-in rules work offline; optionally connect Ollama, an OpenAI-compatible server or Anthropic. The built-in rules understand English and some Turkish expressions.',
  'privacy_label': 'KEEP YOUR DATA WITH YOU', 'privacy_title': 'Your clipboard<br>doesn’t need a cloud.',
  'privacy_body': 'Core conversions and built-in recipe rules run on your computer. If you configure a remote AI provider, you decide whether to send a sample of your data.',
  'privacy_tags': ['Core conversions without an account', 'AI is optional', 'Local models supported'],
  'install_label': 'TRY YOUR OWN DATA ON YOUR MAC', 'install_title': 'A little superpower<br>for your clipboard.', 'install_intro': 'Download the ready-to-run Mac app. Core conversions need no Rust, Node.js or AI account.',
  'install_steps': [('Download', 'Get the DMG for Apple Silicon (M1 or newer) and macOS 11+.'), ('Move to Applications', 'Open the DMG and drag PasteCompiler into your Applications folder.'), ('Paste and convert', 'Open the app, paste your data and choose the output format you need.')],
  'install_note': 'v0.1.0 is not Developer ID signed or Apple-notarized yet. See the installation guide for the first launch.',
  'guide': 'Installation guide', 'release': 'Release notes', 'checksum': 'SHA-256 checksums', 'requirements': 'Apple Silicon · macOS 11+ · DMG, 3.8 MiB',
  'faq_title': 'A few quick answers.', 'faqs': [
   ('Is it free?', 'Yes. PasteCompiler is open source under the MIT license. Core conversions need no subscription. Any remote AI service you choose may charge separately.'),
   ('What about Windows, Linux or Intel Macs?', 'The ready-to-run download for this release is only for Apple Silicon Macs. Intel Mac, Windows and Linux users can build from source; this release does not include prebuilt packages for those platforms.'),
   ('Do I need an AI account or API key?', 'No. Format detection, core conversions, recipes and built-in natural-language rules work without an AI account. Connecting a model is optional.'),
   ('Is my data sent to a server?', 'Core operations run locally. If you connect a remote AI provider, you control data sample sharing in Settings. The small demo on this page also processes data in your browser.'),
   ('Is every conversion guaranteed to be lossless?', 'No. Some formats cannot represent the same structure. The app parses supported outputs back to report differences and potential loss. SQL, custom templates and plain text do not support this check.'),
   ('Can I share my recipes?', 'Yes. Export recipes as JSON files or short share codes starting with pcr1. and import them on another machine.')],
  'footer_line': 'From where you copied it, to where you need it.', 'source': 'Source code', 'feedback': 'Feedback', 'license': 'Open source · MIT',
  'js': {'rows': 'rows', 'columns': 'columns', 'ready': 'Converted', 'empty': 'Add a header row and at least one data row to convert.', 'quotes': 'Check your CSV quotation marks.', 'ragged': 'Every row must have the same number of cells as the header.', 'headers': 'Column headings must be nonempty and unique.', 'limit': 'This demo accepts up to 200 rows and 20,000 characters.', 'copied': 'Output copied.', 'copy_fail': 'Select the output and copy it manually.', 'sql_note': 'SQL has no round-trip verification.', 'demo_note': 'Detailed verification is in the desktop app.', 'invalid': 'Check the input'}
 }
}

PAGE = Template('''<!doctype html>
<html lang="$lang"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>$title</title><meta name="description" content="$description"><meta name="theme-color" content="#102b30">
<meta property="og:title" content="$title"><meta property="og:description" content="$description"><meta property="og:type" content="website">
<link rel="icon" href="$prefix/assets/logo.svg" type="image/svg+xml"><link rel="stylesheet" href="$prefix/styles.css">
<link rel="alternate" hreflang="tr" href="$tr_url"><link rel="alternate" hreflang="en" href="$en_url">
</head><body>
<a class="skip" href="#main">$skip</a>
<header class="site-header wrap"><a class="brand" href="#main"><img src="$prefix/assets/logo.svg" width="42" height="42" alt=""><span>Paste<span class="brand-light">Compiler</span></span></a>
<button type="button" class="menu-button" aria-label="$menu" aria-expanded="false" aria-controls="main-nav">☰</button>
<nav id="main-nav" aria-label="$nav_label"><a href="#demo">$nav_demo</a><a href="#formats">$nav_formats</a><a href="#recipes">$nav_recipes</a><a href="#download">$nav_download</a></nav>
<div class="header-end"><div class="languages" aria-label="$language_label"><a href="$tr_url" $tr_current>TR</a><span>/</span><a href="$en_url" $en_current>EN</a></div><a class="button small" href="$dmg">$nav_download <span aria-hidden="true">↗</span></a></div></header>
<main id="main">
<section class="hero wrap" aria-labelledby="hero-title"><div class="hero-copy"><div class="eyebrow"><span class="status-dot"></span>$eyebrow</div><h1 id="hero-title">$headline</h1><p class="hero-intro">$intro</p><div class="hero-actions"><a class="button" href="$dmg">$download <span aria-hidden="true">↓</span></a><a class="text-link" href="#demo">$try <span aria-hidden="true">↗</span></a></div><p class="meta">$meta</p></div>
<div class="hero-visual" aria-label="$visual_caption"><div class="orbit-label">.csv <span>→</span> .json</div><div class="data-card input-card"><div class="card-label"><span class="tiny-dot amber"></span>$visual_input<span class="card-type">CSV</span></div><div class="mini-table"><div>id</div><div>name</div><div>role</div><div>007</div><div>Ada</div><div>design</div><div>008</div><div>Can</div><div>dev</div><div>009</div><div>Mina</div><div>ops</div></div></div><div class="convert-arrow" aria-hidden="true">↳</div><div class="data-card output-card"><div class="card-label"><span class="tiny-dot"></span>$visual_output<span class="card-type">JSON</span></div><pre><code><span class="syntax-muted">[</span>
  { <span class="syntax-key">"id"</span>: <span class="syntax-string">"007"</span>, <span class="syntax-key">"name"</span>: <span class="syntax-string">"Ada"</span>, <span class="syntax-key">"role"</span>: <span class="syntax-string">"design"</span> },
  { <span class="syntax-key">"id"</span>: <span class="syntax-string">"008"</span>, <span class="syntax-key">"name"</span>: <span class="syntax-string">"Can"</span>, <span class="syntax-key">"role"</span>: <span class="syntax-string">"dev"</span> },
  { <span class="syntax-key">"id"</span>: <span class="syntax-string">"009"</span>, <span class="syntax-key">"name"</span>: <span class="syntax-string">"Mina"</span>, <span class="syntax-key">"role"</span>: <span class="syntax-string">"ops"</span> }
<span class="syntax-muted">]</span></code></pre><div class="visual-bottom"><span aria-hidden="true">✓</span><span>007 → "007"</span><span class="local-indicator">local</span></div></div><p class="visual-caption">$visual_caption</p></div>
</section>
<div class="proof-strip wrap">$proof_html</div>
<section class="demo-section section wrap" id="demo" aria-labelledby="demo-title"><div class="section-top"><div><div class="eyebrow">$demo_eyebrow</div><h2 id="demo-title">$demo_title</h2></div><p class="section-intro">$demo_intro</p></div>
<div class="workbench"><div class="bench-top"><div class="window-dots" aria-hidden="true"><i></i><i></i><i></i></div><span>PasteCompiler / playground</span><span class="bench-local"><span class="tiny-dot"></span>local</span></div>
<div class="bench-controls"><div class="sample-picker" role="group" aria-label="$sample_label"><span>$sample_label</span><button type="button" data-sample="people" aria-pressed="true">$sample_people</button><button type="button" data-sample="products" aria-pressed="false">$sample_products</button></div><div class="format-tabs" role="tablist" aria-label="$output_label"><button type="button" role="tab" id="format-json" aria-selected="true" aria-controls="output-panel" data-format="json">JSON</button><button type="button" role="tab" id="format-sql" tabindex="-1" aria-selected="false" aria-controls="output-panel" data-format="sql">SQL</button><button type="button" role="tab" id="format-markdown" tabindex="-1" aria-selected="false" aria-controls="output-panel" data-format="markdown">Markdown</button><button type="button" role="tab" id="format-tsv" tabindex="-1" aria-selected="false" aria-controls="output-panel" data-format="tsv">TSV</button></div></div>
<div class="editor-grid"><div class="editor-pane"><div class="pane-heading"><label for="demo-input">$input_label</label><span class="file-tag">data.csv</span></div><textarea id="demo-input" spellcheck="false" autocapitalize="off" autocomplete="off" aria-describedby="demo-scope">id,name,role
007,Ada,design
008,Can,dev
009,Mina,ops</textarea></div><div class="editor-pane output-pane" role="tabpanel" aria-labelledby="format-json" id="output-panel"><div class="pane-heading"><span>$output_label</span><button type="button" class="copy-button" id="copy-output">$copy <span aria-hidden="true">⧉</span></button></div><pre id="demo-output" tabindex="0"></pre></div></div><div class="bench-status" role="status" aria-live="polite"><span id="demo-status"></span><span id="demo-validation"></span></div></div><p class="demo-scope" id="demo-scope">$demo_note</p></section>
<section class="report-section section" aria-labelledby="report-title"><div class="wrap report-layout"><div><div class="eyebrow">$report_label</div><h2 id="report-title">$report_title</h2><p class="section-intro">$report_intro</p><div class="report-list">$report_items_html</div></div><div class="report-visual"><div class="report-card"><div class="report-card-top"><span class="mono">conversion.report</span><span class="report-badge">$report_example</span></div><div class="report-row"><span class="report-symbol good">✓</span><div><strong>$report_ok</strong><code>JSON → document → compare</code></div></div><div class="report-row"><span class="report-symbol warn">!</span><div><strong>$report_warn</strong><code>profile.city → "profile.city"</code></div></div><div class="report-row"><span class="report-symbol neutral">—</span><div><strong>$report_na</strong><code>round_trip: unavailable</code></div></div></div><p class="report-limit">$report_limit</p></div></div></section>
<section class="section wrap" id="formats" aria-labelledby="formats-title"><div class="section-top"><div><div class="eyebrow">$formats_label</div><h2 id="formats-title">$formats_title</h2></div><p class="section-intro">$formats_intro</p></div><div class="format-ribbon" aria-hidden="true"><span>{ JSON }</span><span>CSV</span><span>SQL_</span><span>YAML</span><span>&lt;XML&gt;</span><span>TSV</span><span>Markdown</span></div><div class="format-groups">$formats_html</div><a class="text-link" href="$repo#output-formats-and-options" target="_blank" rel="noopener noreferrer">$formats_all <span aria-hidden="true">↗</span></a></section>
<section class="recipes-section section" id="recipes" aria-labelledby="recipes-title"><div class="wrap recipe-layout"><div><div class="eyebrow">$recipes_label</div><h2 id="recipes-title">$recipes_title</h2><p class="section-intro">$recipes_intro</p><p class="recipe-export">↗ $recipe_export</p></div><div class="recipe-card"><div class="mono recipe-caption">$recipe_example</div><div class="recipe-flow">$recipe_steps_html</div><div class="describe"><span class="prompt-icon" aria-hidden="true">&gt;_</span><div><h3>$describe_title</h3><p>$describe_body</p></div></div></div></div></section>
<section class="privacy-section section wrap" aria-labelledby="privacy-title"><div class="privacy-symbol" aria-hidden="true"><svg viewBox="0 0 80 80" fill="none" stroke="currentColor" stroke-width="2"><rect x="18" y="35" width="44" height="33" rx="7"/><path d="M27 35V24a13 13 0 0 1 26 0v11M40 47v9"/></svg></div><div class="privacy-copy"><div class="eyebrow">$privacy_label</div><h2 id="privacy-title">$privacy_title</h2><p class="section-intro">$privacy_body</p><div class="privacy-tags">$privacy_tags_html</div></div></section>
<section class="download-section section" id="download" aria-labelledby="download-title"><div class="wrap download-layout"><div><div class="eyebrow">$install_label</div><h2 id="download-title">$install_title</h2><p class="section-intro">$install_intro</p><div class="download-product"><img src="$prefix/assets/logo.svg" width="72" height="72" alt=""><div><strong>PasteCompiler</strong><span>v0.1.0 · macOS</span></div></div><a class="button" href="$dmg">$download <span aria-hidden="true">↓</span></a><p class="meta">$requirements</p><div class="download-links"><a href="$repo/releases/tag/v0.1.0" target="_blank" rel="noopener noreferrer">$release ↗</a><a href="$repo/releases/download/v0.1.0/SHA256SUMS.txt">$checksum ↗</a></div></div><div class="install-column">$install_steps_html<p class="install-note">$install_note <a href="$repo#download-the-app" target="_blank" rel="noopener noreferrer">$guide ↗</a></p></div></div></section>
<section class="section wrap faq-section" aria-labelledby="faq-title"><div class="eyebrow">FAQ</div><h2 id="faq-title">$faq_title</h2><div class="faq-list">$faqs_html</div></section>
</main><footer class="wrap footer"><div><a class="brand" href="#main"><img src="$prefix/assets/logo.svg" width="34" height="34" alt="">PasteCompiler</a><p>$footer_line</p><span class="meta">$license</span></div><div class="footer-links"><a href="$repo" target="_blank" rel="noopener noreferrer">$source ↗</a><a href="$repo/issues" target="_blank" rel="noopener noreferrer">$feedback ↗</a><a href="$repo/releases" target="_blank" rel="noopener noreferrer">$release ↗</a></div></footer>
<div id="toast" class="toast" role="status" aria-live="polite"></div><script type="application/json" id="demo-copy">$js_copy</script><script src="$prefix/script.js" defer></script>
</body></html>''')

for lang, data in COPY.items():
    d = dict(data)
    esc = html.escape
    d.update(lang=lang, prefix='..' if lang == 'en' else '.', repo=REPO, dmg=DMG,
             tr_url='../' if lang == 'en' else './', en_url='./' if lang == 'en' else 'en/',
             tr_current='aria-current="page"' if lang == 'tr' else '', en_current='aria-current="page"' if lang == 'en' else '',
             language_label='Sayfa dili' if lang == 'tr' else 'Page language', nav_label='Ana menü' if lang == 'tr' else 'Main navigation')
    d['proof_html'] = ''.join('<span><span aria-hidden="true">✓</span> ' + esc(s) + '</span>' for s in d['proof'])
    d['report_items_html'] = ''.join('<div><span aria-hidden="true">↳</span><p><strong>' + esc(a) + '</strong><span>' + esc(b) + '</span></p></div>' for a, b in d['report_items'])
    d['formats_html'] = ''.join('<article><span class="mono format-num">' + n + '</span><h3>' + esc(a) + '</h3><p>' + esc(b) + '</p></article>' for n, a, b in d['format_groups'])
    d['recipe_steps_html'] = ''.join('<div><span class="step-num">0' + str(i + 1) + '</span><strong>' + esc(s) + '</strong><span aria-hidden="true">' + ('↓' if i < 2 else '✓') + '</span></div>' for i, s in enumerate(d['recipe_flow']))
    d['privacy_tags_html'] = ''.join('<span>✓ ' + esc(s) + '</span>' for s in d['privacy_tags'])
    d['install_steps_html'] = ''.join('<article class="install-step"><span>0' + str(i + 1) + '</span><div><h3>' + esc(a) + '</h3><p>' + esc(b) + '</p></div></article>' for i, (a, b) in enumerate(d['install_steps']))
    d['faqs_html'] = ''.join('<details><summary>' + esc(q) + '<span aria-hidden="true">+</span></summary><p>' + esc(a) + '</p></details>' for q, a in d['faqs'])
    d['js_copy'] = json.dumps(d['js'], ensure_ascii=False).replace('<', '\\u003c')
    target = ROOT / ('en/index.html' if lang == 'en' else 'index.html')
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(PAGE.substitute(d), encoding='utf-8')
    print(target.relative_to(ROOT))
