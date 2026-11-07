# Mimari

Bu belge VideoVault'un bileşenlerini, güven sınırlarını, veri ve iş akışını ve paketlemesini anlatır (SPEC §10, §20.1, §27). Hedef mimari SPEC §10'dadır. Burada **bugün kodda olan** ile **henüz yazılmamış** olan ayrı gösterilir. Kararların gerekçesi [decisions.md](./decisions.md)'de, tehditler [threat-model.md](./threat-model.md)'de, iş sırası [roadmap.md](./roadmap.md) ve [STATE.md](../STATE.md)'dedir.

Son güncelleme: 7 Kasım 2026, F0.11.

## Bileşenler

```text
apps/desktop/src            React + TypeScript + Vite arayüzü
        |
        |  Tipli Tauri komutları (capabilities/main.json, CSP)
        v
apps/desktop/src-tauri      Tauri 2 kabuğu: komutlar, tek süreç, medya protokolü
        |
        +-- crates/vv-domain       kimlikler, hata kodları, URL politikası (saf, IO yok)
        +-- crates/vv-persistence  SQLite (rusqlite bundled), migration, yedek, kütüphane kilidi
        +-- crates/vv-process      alt süreç: temiz ortam, Windows Job Object, ağaç öldürme
        |       +-- crates/vv-ytdlp     yt-dlp argümanları, JSON/progress/hata ayrıştırma
        |       +-- crates/vv-ffmpeg    ffprobe/FFmpeg argümanları, biçim sınıflandırma
        +-- crates/vv-netguard     araçların ağ trafiği için yerel denetim proxy'si
        +-- crates/vv-tools        yönetilen araç manifesti, hash/imza, sürüm ve lisans kontrolü
```

| Bileşen | Sorumluluk | SPEC modülü (§10.3) | Durum | Karar |
|---|---|---|---|---|
| `apps/desktop/src` | Library kabuğu, IPC istemcisi (gerçek ve fixture), bileşenler | UI | İskelet: kütüphane listesi fixture verisiyle | F0.1, F0.3 |
| `apps/desktop/src-tauri` | Pencere, komutlar (`ping`, `get_status`, `get_diagnostics`), tek süreç, test derlemesinde medya protokolü | application (giriş) | Çalışıyor; iş komutları yok | ADR-0001, ADR-0004, ADR-0008 |
| `vv-domain` | Hata kodları ve `DomainError`, yakalama URL politikası | domain, security | Kısmen | F0.7 |
| `vv-persistence` | Bağlantı ayarları (WAL, `synchronous=FULL`), checksum'lı migration, migration öncesi yedek, `videovault.lock` | persistence | Altyapı hazır; alan tabloları F1.1'de | ADR-0002, ADR-0008 |
| `vv-process` | Araç süreçlerini temiz ortamla, Job Object içinde başlatma; iptal ve zaman aşımı | jobs (süreç denetimi) | Hazır; henüz çağıran yok (F3) | ADR-0008 |
| `vv-ytdlp` | yt-dlp için sabit argümanlar, çıktı ayrıştırma, hata eşleme | connectors | Fixture ile test edildi | ADR-0003 |
| `vv-ffmpeg` | Kısıtlı ffprobe/FFmpeg çağrıları, referanslı girdilerin reddi | media | Fixture ve gerçek build ile test edildi | ADR-0006 |
| `vv-netguard` | yt-dlp bağlantılarını port, DNS sonrası IP ve jetonla denetleme | security | Gerçek yt-dlp ile test edildi | ADR-0007 |
| `vv-tools` | `tools/manifest.json`: sürüm pin, SHA-256, GPG imzası, FFmpeg lisans kontrolü | diagnostics, security | Hazır | ADR-0005, ADR-0006 |
| İş zamanlayıcı (lease, retry, bağımlılık) | Kalıcı kuyruk | jobs | Yok | F3.1 |
| Import/export, relink | JSON şema, kök eşleme | portability | Yok | F2.5 |
| Yerel AI sağlayıcıları | Döküm, embedding, OCR | ai | Yok | F6, F7 |

Bağımlılık yönü tek yönlüdür: `vv-domain` hiçbir crate'e bağlı değildir; uygulama kabuğu crate'leri birleştirir. Domain kuralları executable yollarına bağlı değildir (SPEC §10.3).

## Güven sınırları

| Sınır | İçeride | Dışarıda | Denetim | Kanıt |
|---|---|---|---|---|
| Webview ↔ backend | Rust süreç | Webview içeriği (yerel paket) | Yalnızca `capabilities/main.json`'daki komutlar; CSP; `freezePrototype`; core/plugin komutları kapalı | `tests/ipc.rs`, `e2e-desktop/desktop-shell.spec.ts` |
| Backend ↔ araç süreçleri | Uygulama | yt-dlp, Deno, FFmpeg, ffprobe | Argüman dizisi (kabuk yok), temiz ortam, Job Object, sabit sürüm ve hash | ADR-0005, ADR-0006, ADR-0008 |
| Araç ↔ ağ | Yerel denetim proxy'si | İnternet | Port 80/443, DNS sonrası tüm adresler public, jeton | ADR-0007, `scripts/netguard-probe.mjs` |
| Araç ↔ dosya sistemi | Medya kökleri | Kök dışı yollar | FFmpeg yalnızca `file:` ve biçim izin listesi; yol sınırı F1.4/F3.6'da | ADR-0006 |
| Süreç ↔ süreç | Kilidi tutan süreç | Aynı kütüphaneyi açmak isteyen ikinci süreç | Tek süreç eklentisi + kütüphane kilidi | ADR-0008 |
| Kullanıcı verisi ↔ dışa aktarım | Kütüphane | JSON export, log, diagnostics | Alan izin listesi, redaksiyon (F3.10) | Yok |

## Veri ve iş akışı

Hedef akış (SPEC §8, §13). Kalın adımlar bugün kodda vardır.

1. **Yakalama:** UI → `save_bookmark` komutu (SPEC §18.1, F1.2) → **`vv-domain::url_policy`** (şema, kimlik bilgisi, yerel adres) → tek SQLite işleminde `LibraryItem` + `SearchDocument` → Inbox.
2. **Metadata (M2):** iş kuyruğu → **`vv-process`** ile yt-dlp → **`vv-netguard`** proxy'si → **`vv-ytdlp`** ayrıştırma → kullanıcı alanlarını ezmeyen güncelleme (INV-12).
3. **İndirme (M2):** format seçimi → yt-dlp (native indirici) → staging → bütünlük doğrulaması → final konum → `MediaAsset` (INV-02).
4. **Yerel import (M1):** dosya seçimi → **`vv-ffmpeg`** ile kısıtlı ffprobe → hash → duplicate adayı → küçük resim işi.
5. **Oynatma:** WebView2 `<video>` ← yerel medya protokolü (Range destekli; **test derlemesinde prototip**, F2.4'te üretim). HEVC oynatılamaz sayılır (ADR-0004).
6. **Arama:** FTS5 (`SearchDocument`); paketlenen SQLite'ta FTS5 **doğrulandı**, sorgu oluşturucu F1.6'da.

Uygulama açılışı bugün: tek süreç eklentisi → **kütüphane kilidi** → pencere. Veritabanı henüz açılmıyor; açılış uzlaştırması (lease'lerin toparlanması) F3.1'de eklenecek.

## Paketleme

| Konu | Bugün | Hedef |
|---|---|---|
| Uygulama | `npm run desktop:exe` → `target/release/videovault.exe`; test derlemesi `videovault-e2e.exe` (`e2e-cdp` özelliği) | NSIS installer (F4.1) |
| WebView2 | Sistemdeki runtime (CI'da 156) | Installer stratejisi F4.1 |
| SQLite | `rusqlite` bundled, FTS5 dahil | Aynı |
| Araçlar | `npm run tools:fetch` → `tools/bin/<platform>/` (yt-dlp 2026.10.26, Deno 2.9.8, FFmpeg/ffprobe 9.0.2 LGPL) | Installer içinde, manifestle doğrulanan sürümler |
| Güncelleme | Yok | İmzalı güncelleme ve geri dönüş (F4.2, DEC-16) |
| Lisans | ADR-0005, ADR-0006'da araç lisansları | NOTICE dosyaları (F4.1) |

## Depo düzeni

| Yol | İçerik |
|---|---|
| `SPEC.md`, `STATE.md`, `HANDOFF.md` | Sözleşme, iş durumu (SPEC §27'deki `TASKS.md`'nin görevini `STATE.md` üstlenir), devir notu |
| `crates/` | Rust kütüphaneleri |
| `apps/desktop/` | Arayüz, Tauri kabuğu, E2E testleri |
| `tests/fixtures/` | Paylaşılan fixture'lar (IPC sözleşmesi, yt-dlp, FFmpeg, medya, araçlar) |
| `tools/manifest.json` | Yönetilen araçların sürüm ve hash pin'leri |
| `scripts/` | `verify`, araç indirme, probe ve fixture yakalama betikleri |
| `docs/` | Bu belge ve diğer zorunlu dokümanlar |
| `test-artifacts/` | Test çıktıları (sır içermez, depoya girmez) |
