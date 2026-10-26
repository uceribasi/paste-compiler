# PasteCompiler

[English](README.md) · [Türkçe](README.tr.md)

**Kopyaladığın veriyi ihtiyacın olan formata dönüştür.**

PasteCompiler, panondaki verinin formatını otomatik algılayıp istediğin formata dönüştüren açık kaynak bir masaüstü uygulamasıdır. Excel'den kopyalanan tablolar, JSON, CSV, YAML, TOML, XML, Markdown veya HTML tabloları, `.env` biçimindeki anahtar/değer çiftleri, listeler ve düz metinlerle çalışır. Rust, Tauri ve React ile geliştirilmiştir.

- **Otomatik format algılama:** CSV benzeri verilerde ayırıcıyı ve başlık satırını tahmin eder, uygun çıktı formatlarını sıralar.
- **17 çıktı seçeneği:** JSON (girintili, sıkıştırılmış veya satır bazlı), CSV, Excel ve Google Sheets için TSV, Markdown tablosu, HTML tablosu, YAML, TOML, XML, SQL `INSERT` (Postgres, MySQL veya SQLite; isteğe bağlı `CREATE TABLE`), Markdown listesi, numaralı liste, düz satırlar, anahtar/değer çiftleri, özel `{{şablonlar}}` ve düz metin.
- **Her dönüşümden sonra rapor:** Desteklenen çıktı formatları yeniden ayrıştırılarak satır sayıları, sütunlar, hücre değerleri ve iç içe anahtarlardaki farklılıklar kontrol edilir. İç içe yapının düzleştirilmesi, TOML çıktısında `null` değerlerin atılması, eksik hücrelerin tamamlanması, tekrarlanan başlıkların yeniden adlandırılması, XML adlarının düzeltilmesi ve tür dönüşümleri gibi tespit edilen değişiklikler önem dereceleriyle raporlanır. SQL, özel şablonlar ve düz metin çıktıları yeniden ayrıştırılarak doğrulanamaz; raporda bu kontrolün kullanılamadığı belirtilir.
- **Tarifler:** Sütun seçme, silme veya yeniden adlandırma; filtreleme, sıralama, sınırlandırma, tekrarları kaldırma, bölme, hesaplanan sütunlar ekleme, harf biçimini değiştirme, düzenli ifadeyle değiştirme ve iç içe yapıları dönüştürme gibi adımları çıktı seçenekleriyle birlikte kaydet. Tarifleri tek tıkla çalıştır, JSON dosyası veya `pcr1.…` paylaşım koduyla paylaş, başkalarının tariflerini içe aktar.
- **Ne istediğini tarif et:** Örneğin, “sadece ad ve eposta sütunları, json olarak” yaz. Girdi verisi varsa oluşturulan tarif gösterilmeden önce bu veri üzerinde denenir. Yerleşik kural motoru **tamamen çevrimdışı** çalışır. İstersen **Ollama**, **OpenAI uyumlu** bir sunucu (LM Studio, llama.cpp, vLLM vb.) veya kendi API anahtarınla **Anthropic Claude** kullanabilirsin. Kural motoru belirli komut kalıplarını ve bazı Türkçe karşılıklarını tanır; her doğal dil isteğini yorumlayamaz.
- **Yerel çalışma:** Temel dönüşümler bilgisayarında yapılır. Uzak bir yapay zekâ sağlayıcısı yapılandırmadığın sürece veriler dışarı gönderilmez; yapılandırdığında da veri örneği gönderilip gönderilmeyeceğini ayarlardan seçebilirsin.

---

## Kurulum ve çalıştırma

Gereksinimler: [Rust](https://rustup.rs) (kararlı sürüm), Node.js 22.x (22.12 veya üzeri), 24.x ya da 26 ve üzeri; ayrıca işletim sistemine uygun Tauri bağımlılıkları. Bu Node.js sürümleri hem derleme araçları hem de test bağımlılıklarıyla uyumludur. macOS için Xcode Command Line Tools gerekir; diğer platformlar ve ayrıntılar için [Tauri gereksinimlerine](https://tauri.app/start/prerequisites/) bakabilirsin.

Proje klasöründe çalıştır:

```bash
npm install
npm run tauri dev        # değişiklikleri anında yükleyen geliştirme sürümü
npm run tauri build      # target/release/bundle/ altında dağıtım paketi
```

macOS derlemesi, `target/release/bundle/` altında `PasteCompiler.app` ve bir `.dmg` dosyası üretir. Uygulama kod imzası taşımadığı için ilk açılışta sağ tıklayıp **Aç** seçeneğini kullanman gerekebilir. `.dmg` oluşturma adımı Finder'ı AppleScript üzerinden kontrol eder ve ilk seferde Otomasyon izni ister. Bu adım takılırsa yalnızca uygulama paketini oluşturabilirsin:

```bash
npm run tauri build -- --bundles app
```

### Arayüzü tarayıcıda çalıştırma (isteğe bağlı)

React arayüzü, masaüstü uygulamasıyla aynı komutları sunan küçük bir HTTP geliştirme sunucusuyla normal tarayıcıda da çalışabilir. Bu yöntem arayüz geliştirmek ve Tauri'yi açmadan hata ayıklamak için kullanılabilir.

İki ayrı terminalde çalıştır:

```bash
cargo run -p pastecompiler-core --example devserver   # motor: http://127.0.0.1:4567
npm run dev                                         # arayüz: http://localhost:1420
```

Bu şekilde oluşturulan tarifler ve ayarlar ayrı bir `pastecompiler-dev` klasöründe saklanır. Konumu `PASTECOMPILER_DATA_DIR` ortam değişkeniyle değiştirebilirsin.

### Testler

```bash
cargo test --workspace   # Rust: algılama, ayrıştırma, çıktı, doğrulama, tarifler ve taklit sunucularla AI sağlayıcıları
npm test                 # arayüz yardımcıları ve dil desteği (Vitest)
npm run typecheck        # TypeScript tür kontrolü
```

---

## Nasıl çalışır?

```text
pano metni → format algılama → ayrıştırma → Document → tarif adımları → çıktı üretimi
                                             │                            │
                                             └────── doğrulama ◀──────────┘
```

Tüm temel işlemler `crates/pastecompiler-core` içindeki `pastecompiler-core` kütüphanesinde bulunur. `src-tauri` içindeki Tauri uygulaması bu işlevleri komut olarak sunar; `src` içindeki React arayüzü de bu komutları çağırır.

| Modül | Görevi |
| --- | --- |
| `detect` | Aday formatları puanlar: JSON, JSON Lines, ayırıcı algılamalı CSV/TSV, Markdown ve HTML tabloları, YAML, TOML, XML, anahtar/değer çiftleri, listeler ve metin. CSV'de başlık satırı olup olmadığını tahmin eder. |
| `parse` | Formatı bilinen metni bir `Document` yapısına dönüştürür: tür bilgisi taşıyan `Table`, JSON benzeri `Tree`, `List` veya `Text`. |
| `coerce` | Bu veri yapıları arasında dönüşüm yapar; örneğin iç içe JSON'u noktalı sütun adları olan bir tabloya veya tabloyu nesne dizisine dönüştürür. Veri kaybına yol açan değişiklikleri kaydeder. |
| `recipe` | `Recipe` şemasını, adımları çalıştıran yorumlayıcıyı ve elle ya da model tarafından oluşturulan tarifler için esnek ayrıştırıcıyı içerir. |
| `output` | Her çıktı formatı için veriyi metne dönüştüren işlevleri içerir. |
| `validate` | Desteklenen çıktıları yeniden ayrıştırır, farklılıkları kontrol eder ve raporu oluşturur: `lossless`, yeniden ayrıştırma kontrolünün durumu, sorunlar, farklar ve istatistikler. SQL, şablon ve düz metin için bu kontrolün kullanılamadığını bildirir. |
| `ai` | İstemleri, Ollama / OpenAI uyumlu / Anthropic istemcilerini ve çevrimdışı kural motorunu içerir. |
| `store`, `settings` | Tarifleri ve ayarları uygulama veri klasöründe JSON olarak saklar. |

Tür çıkarımı özellikle temkinlidir: Bir hücre ancak sayıya dönüştürülüp tekrar metne çevrildiğinde aynı yazımı koruyorsa sayı olarak kabul edilir. Böylece `007`, `3.10`, `+5` ve `1e5` metin olarak kalır; yazımları sessizce değişmez.

---

## Tarifler

Tarif, dönüşüm adımlarını ve çıktı seçeneklerini tanımlayan bir JSON belgesidir:

```json
{
  "name": "Aktif kullanıcılar → SQL",
  "description": "Aktif kullanıcıları en yeniden eskiye sıralayıp INSERT ifadeleri üret",
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

Bu örnek, girdide `status`, `created_at`, `id`, `name` ve `email` sütunları olduğunu varsayar. Tarifin adı ve açıklaması Türkçe olabilir; `op`, `format` ve işlem adları gibi JSON şema alanları aşağıdaki biçimde kullanılmalıdır.

### Adımlar

| `op` | Alanlar | Açıklama |
| --- | --- | --- |
| `select_columns` | `columns` | Yalnızca belirtilen sütunları, verilen sırayla tutar. |
| `drop_columns` | `columns` | Belirtilen sütunları kaldırır. |
| `rename_column` | `from`, `to` | Bir sütunun adını değiştirir. |
| `filter_rows` | `column`, `condition`, `value` | Koşullar: `eq` `ne` `contains` `not_contains` `starts_with` `ends_with` `gt` `gte` `lt` `lte` `regex` `empty` `not_empty`. |
| `sort_rows` | `column`, `descending` | İki değer de sayıysa sayısal, aksi durumda büyük/küçük harf duyarsız metin sıralaması yapar. Boş hücreler sona gelir. |
| `limit` | `count`, `offset` | Başlangıç konumunu ve alınacak satır sayısını belirler. |
| `map_column` | `column`, `transform`, `arg` | Dönüşümler: `upper` `lower` `title` `trim` `snake_case` `camel_case` `pascal_case` `kebab_case` `to_number` `to_string` `to_bool` `round` (`arg` = ondalık basamak sayısı) `prefix` `suffix` `length` `default`. |
| `add_column` | `name`, `template` | Örnek: `"{{first}} {{last}}"`. Filtreler: `upper lower trim title snake camel pascal kebab json sql html length default:"x" pad:10 truncate:10`. `{{_index}}`, `{{_index0}}` ve `{{_count}}` kullanılabilir. |
| `split_column` | `column`, `separator`, `into` | Bir sütunu ayırıcıya göre yeni sütunlara böler. |
| `dedupe` | `columns` | Tekrarları kaldırır. Sütun listesi boşsa satırın tamamını karşılaştırır. |
| `transpose` | | Satırlarla sütunların yerini değiştirir. |
| `flatten` | | İç içe JSON'u noktalı sütun adları olan tabloya dönüştürür (`address.city`, `tags.0`). |
| `unflatten` | | Noktalı sütun adlarından iç içe JSON oluşturur. |
| `header_case` | `case` | `snake` `camel` `pascal` `kebab` `upper` `lower` `title`. |
| `replace` | `pattern`, `replacement`, `regex`, `column` | Metin veya düzenli ifade kullanarak değiştirme yapar. |
| `trim_cells` | | Hücrelerin başındaki ve sonundaki boşlukları temizler. |
| `fill_empty` | `value`, `column` | Boş hücreleri belirtilen değerle doldurur. |
| `text_lines` | `transform` | Düz metin ve listeler için: `sort` `sort_desc` `dedupe` `trim` `remove_empty` `upper` `lower` `reverse`. |
| `to_table` / `to_tree` / `to_list` / `to_text` | | Veri yapısını açıkça değiştirir; çoğu işlemde buna gerek yoktur. |

Tablo gerektiren adımlar, diğer veri yapılarını gerektiğinde otomatik dönüştürür ve bunu rapora ekler.

### Çıktı formatları ve seçenekleri

| `format` | Seçenekler |
| --- | --- |
| `json` | `indent` |
| `json_compact`, `json_lines` | |
| `csv` | `delimiter`, `quote_all`, `header`, `crlf`, `null_as` |
| `tsv` | `header` — doğrudan Excel, Numbers veya Google Sheets'e yapıştırılabilir. |
| `markdown_table` | `align`, `null_as` |
| `html_table` | `indent` |
| `yaml` | |
| `toml` | `item_name` |
| `xml` | `root_name`, `item_name`, `indent`, `declaration` — `@attr` anahtarları XML özniteliğine, `#text` metin içeriğine dönüşür. |
| `sql_insert` | `table_name`, `dialect` (`postgres` `mysql` `sqlite`), `create_table`, `batch` |
| `markdown_list` | `bullet` |
| `numbered_list`, `plain_lines` | |
| `key_value` | `separator` — `.env` dosyaları için `=` kullanılır. |
| `template` | `template`, `separator`, `prefix`, `suffix` — şablon her satır için bir kez uygulanır. |
| `text` | |

Tarif ayrıştırıcısı esnektir: `"output": "csv"`, `"op"` yerine `"type"`, `"gt"` yerine `">"`, `"upper"` yerine `"uppercase"` ve `"order": "desc"` gibi kullanımları da kabul eder. Bu, model tarafından oluşturulan tariflerdeki yazım farklılıklarını karşılamaya yardımcı olur.

### Paylaşma

**Tarifler → Paylaşım kodu** seçeneği, tarif JSON'unun base64url kodlaması olan bir `pcr1.…` metni üretir. Bu kodu başka bir bilgisayarda **Tarifler → İçe aktar** bölümüne yapıştırabilir veya `.recipe.json` dosyalarını dışa/içe aktarabilirsin.

İçe aktarılan tarifin kimliği mevcut bir tarifle çakışırsa yeni bir kimlik atanır; mevcut tarif sessizce değiştirilmez. Yerleşik örnek tarifler [`recipes/`](recipes/) klasöründedir.

---

## Doğal dille tarif oluşturma (“Tarif et” sekmesi)

| Sağlayıcı | Nerede çalışır? | Kurulum |
| --- | --- | --- |
| **Çevrimdışı kurallar** (varsayılan) | Uygulamanın içinde | Ek kurulum gerekmez. Çıktı formatlarını, sıralama/filtreleme/sütun seçme/silme/adlandırma işlemlerini, sınırları, tekrarları kaldırmayı, harf biçimlerini, SQL tablo adlarını ve lehçelerini, şablonları ve satır işlemlerini belirli kalıplarla yorumlar. İngilizce ve bazı Türkçe karşılıkları destekler. |
| **Ollama** | Bilgisayarında | [Ollama](https://ollama.com) kur, `ollama pull llama3.1` ile modeli indir ve `ollama serve` sürecini açık tut. |
| **OpenAI uyumlu** | Bilgisayarında veya güvendiğin bir sunucuda | `/v1/chat/completions` sunan bir uç nokta kullan: LM Studio, llama.cpp, vLLM, LocalAI vb. |
| **Anthropic Claude** | Anthropic API | Kendi API anahtarını Ayarlar'a gir; anahtar yerel olarak saklanır. Uygulamadaki varsayılan model adı `claude-opus-5-5` olarak ayarlanmıştır; hesabının erişebildiği bir model seçebilirsin. |

Sağlayıcıdan bağımsız olarak, girdi verisi varsa üretilen tarif gösterilmeden önce **bu veri üzerinde denenir**. Tarif geçersizse veya deneme başarısızsa modele hata mesajıyla birlikte en fazla iki düzeltme fırsatı verilir. Hâlâ çalışmıyorsa ya da sağlayıcıya ulaşılamıyorsa, ilgili ayar açıksa çevrimdışı kurallar denenir. Bu seçenek varsayılan olarak açıktır. Arayüz neden kurallara dönüldüğünü gösterir; kurallar da isteği yorumlayamıyorsa hata bildirilir.

Verinden örnek gönderilmesini Ayarlar'dan kapatabilirsin. Bu seçenek, uzak sağlayıcıya hiç istek yapılmayacağı anlamına gelmez; yazdığın talimat ve tarif oluşturma bağlamı yine seçtiğin sağlayıcıya gönderilir. Tamamen yerel kullanım için çevrimdışı kuralları veya bilgisayarında çalışan bir modeli seç.

---

## Proje yapısı

```text
crates/pastecompiler-core/   Rust çekirdek kütüphanesi (temel işlemler ve testler)
src-tauri/                   Tauri uygulaması (komutlar, durum yönetimi, paketleme ayarları)
src/                         React ve TypeScript arayüzü
recipes/                     Yerleşik örnek tarifler (derleme sırasında uygulamaya eklenir)
```

Veriler işletim sisteminin uygulama veri klasöründe saklanır. macOS'ta bu konum `~/Library/Application Support/com.pastecompiler.app/` şeklindedir. Tarifler `recipes/*.json`, ayarlar ise `settings.json` dosyasında tutulur.

## Dil desteği

Arayüz ve tüm motor mesajları — doğrulama raporu, farklılıklar, hatalar ve çevrimdışı kural açıklamaları — **Türkçe** ve **İngilizce** olarak sunulur. Uygulama varsayılan olarak sistem dilini izler. Dili **Ayarlar → Davranış → Dil / Language** bölümünden değiştirebilirsin.

Her motor çağrısı bir `lang` alanı taşır. Rust tarafındaki mesajlar `crates/pastecompiler-core/src/i18n.rs` kataloğundan oluşturulur. Yerleşik tariflerin adları ve açıklamaları `translations` alanında bulunur. Yapay zekâ kullanıldığında modelden tarif adını, açıklamasını ve işlem özetini kullanıcının dilinde yazması istenir.

## Klavye kısayolları

Uygulama penceresi etkinken:

| Kısayol | İşlem |
| --- | --- |
| `⌘⇧V` / `Ctrl+Shift+V` | Panodaki veriyi yapıştır ve dönüştür. |
| `⌘↩` / `Ctrl+Enter` | Çıktıyı kopyala. |
| `⌘,` / `Ctrl+,` | Ayarları aç. |

## Lisans

MIT — ayrıntılar için [LICENSE](LICENSE) dosyasına bakabilirsin.
