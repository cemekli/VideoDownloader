# Mimari

Bu belge VideoVault'un bileşenlerini, güven sınırlarını, veri ve iş akışını ve paketlemesini anlatır (SPEC §10, §20.1, §27). Hedef mimari SPEC §10'dadır. Burada **bugün kodda olan** ile **henüz yazılmamış** olan ayrı gösterilir. Kararların gerekçesi [decisions.md](./decisions.md)'de, tehditler [threat-model.md](./threat-model.md)'de, iş sırası [roadmap.md](./roadmap.md) ve [STATE.md](../STATE.md)'dedir.

Son güncelleme: 7 Kasım 2026, F0.12.

## Bileşenler

```text
apps/desktop/src            React + TypeScript + Vite arayüzü
        |
        |  Tipli Tauri komutları (capabilities/main.json, CSP)
        v
apps/desktop/src-tauri      Tauri 2 kabuğu: komutlar, tek süreç, medya protokolü
        |
        +-- crates/vv-domain       kimlikler, hata kodları, URL politikası (saf, IO yok)
        +-- crates/vv-persistence  SQLite (rusqlite bundled), migration, yedek, kütüphane kilidi, AuthProfile
        +-- crates/vv-secrets      OS korumalı secret store (DPAPI), opak referans
        +-- crates/vv-process      alt süreç: temiz ortam, Windows Job Object, ağaç öldürme
        |       +-- crates/vv-ytdlp     yt-dlp argümanları, JSON/progress/hata ayrıştırma
        |       +-- crates/vv-ffmpeg    ffprobe/FFmpeg argümanları, biçim sınıflandırma
        +-- crates/vv-import       yerel import, kök durumu, relink, thumbnail
        +-- crates/vv-netguard     araçların ağ trafiği için yerel denetim proxy'si
        +-- crates/vv-tools        yönetilen araç manifesti, hash/imza, sürüm ve lisans kontrolü

crates/vv-benchdata         benchmark verisi ve ölçümler (uygulamaya girmez)
```

| Bileşen | Sorumluluk | SPEC modülü (§10.3) | Durum | Karar |
|---|---|---|---|---|
| `apps/desktop/src` | Library kabuğu, IPC istemcisi (gerçek ve fixture), bileşenler | UI | İskelet: kütüphane listesi fixture verisiyle | F0.1, F0.3 |
| `apps/desktop/src-tauri` | Pencere, komutlar (`ping`, `get_status`, `get_diagnostics`), tek süreç, test derlemesinde medya protokolü | application (giriş) | Çalışıyor; iş komutları yok | ADR-0001, ADR-0004, ADR-0008 |
| `vv-domain` | Hata kodları ve `DomainError`, yakalama URL politikası, platform tanıma ve URL normalizasyonu (sürüm 1) | domain, security | Kısmen | F0.7, ADR-0011 |
| `vv-persistence` | Bağlantı ayarları (WAL, `synchronous=FULL`), checksum'lı migration, migration öncesi yedek, `videovault.lock`; `library`: kütüphane şeması (migration 0002), repository'ler, `save_bookmark`, yeniden normalizasyon, FTS arama (`search`) | persistence | Şema ve bookmark kaydı hazır; uygulamaya F1.8'de bağlanır | ADR-0002, ADR-0008, ADR-0011 |
| `vv-import` | Yerel dosya/klasör import, kök erişilebilirliği ve `missing` durumu, relink, thumbnail | media, library | Hazır; uygulamaya F1.7'de bağlanır | ADR-0012 |
| `vv-process` | Araç süreçlerini temiz ortamla, Job Object içinde başlatma; iptal ve zaman aşımı | jobs (süreç denetimi) | Hazır; `vv-import` ve `vv-engine` kullanır | ADR-0008 |
| `vv-ytdlp` | yt-dlp için sabit argümanlar, çıktı ayrıştırma, hata eşleme | connectors | Fixture ile test edildi | ADR-0003 |
| `vv-ffmpeg` | Kısıtlı ffprobe/FFmpeg çağrıları, referanslı girdilerin reddi | media | Fixture ve gerçek build ile test edildi | ADR-0006 |
| `vv-netguard` | yt-dlp bağlantılarını port, DNS sonrası IP ve jetonla denetleme | security | Gerçek yt-dlp ile test edildi | ADR-0007 |
| `vv-tools` | `tools/manifest.json`: sürüm pin, SHA-256, GPG imzası, FFmpeg lisans kontrolü | diagnostics, security | Hazır | ADR-0005, ADR-0006 |
| `vv-benchdata` | Sürümlü benchmark verisi (10.000 öğe, 100.000 segment) ve BM-01…03 | — (test aracı) | Hazır; uygulamaya girmez | ADR-0009 |
| `vv-secrets` | Gizli değerler için işletim sistemi korumalı store (Windows DPAPI), opak `vvsec:` referansı; düz metin backend'i yok | auth | Hazır; `vv-persistence::auth` kullanır, uygulamaya F5.2'de bağlanır | ADR-0022 |
| `vv-engine` | İş zamanlayıcı ve işçiler: araç kontrolü, metadata, önayarlar, indirme/doğrulama/yerleştirme, kota, `storage_move`; kuyruk `vv-persistence::jobs` | jobs, connectors, storage | Uygulamada çalışıyor | ADR-0019, ADR-0020, ADR-0021 |
| Import/export, relink | JSON şema, kök eşleme | portability | Yok | F2.5 |
| Yerel AI sağlayıcıları | Döküm, embedding, OCR | ai | Yok | F6, F7 |

Bağımlılık yönü tek yönlüdür: `vv-domain` hiçbir crate'e bağlı değildir, `vv-secrets` hiçbir iç crate'e bağlı değildir, `vv-persistence` yalnızca `vv-domain` ve `vv-secrets`'e, `vv-import` `vv-persistence`, `vv-ffmpeg` ve `vv-process`'e bağlıdır; uygulama kabuğu crate'leri birleştirir. Domain kuralları executable yollarına bağlı değildir (SPEC §10.3).

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

1. **Yakalama:** UI → `save_bookmark` komutu (SPEC §18.1, F1.2) → **`vv-domain::url_policy`** (şema, kimlik bilgisi, yerel adres) → **`vv-domain::normalize`** (platform, kanonik URL, sır redaksiyonu) → `vv-persistence::library::bookmark` tek `BEGIN IMMEDIATE` işleminde eşleşme araması, yoksa `LibraryItem` + `SearchDocument` → Inbox. Servis hazır (ADR-0011); komut ve ekran F1.8'de.
2. **Metadata (M2):** iş kuyruğu → **`vv-process`** ile yt-dlp → **`vv-netguard`** proxy'si → **`vv-ytdlp`** ayrıştırma → kullanıcı alanlarını ezmeyen güncelleme (INV-12).
3. **İndirme (M2):** format seçimi → yt-dlp (native indirici) → staging → bütünlük doğrulaması → final konum → `MediaAsset` (INV-02).
4. **Yerel import (M1):** dosya seçimi → **`vv-ffmpeg`** ile kısıtlı ffprobe → hash → duplicate adayı → küçük resim işi.
5. **Oynatma:** WebView2 `<video>` ← yerel medya protokolü (Range destekli; **test derlemesinde prototip**, F2.4'te üretim). HEVC oynatılamaz sayılır (ADR-0004).
6. **Arama:** FTS5 (`SearchDocument`); paketlenen SQLite'ta FTS5 **doğrulandı**, sorgu oluşturucu F1.6'da.

Uygulama açılışı bugün: tek süreç eklentisi → **kütüphane kilidi** → veritabanı → indirme motoru (arka planda araç kontrolü, sonra açılış uzlaştırması: lease'lerin toparlanması, yarım kalan taşıma temizliği, bitmiş işlerin staging klasörleri, sonra zamanlayıcı) → pencere (ADR-0021).

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
