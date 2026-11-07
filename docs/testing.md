# Test rehberi

Durum takibi [STATE.md](../STATE.md) içindedir. Bu belge testlerin nasıl çalıştırılacağını ve yeni testin nereye ekleneceğini anlatır.

## Kurulum (bir kez)

Gerekenler: Rust (stable, `rustfmt` ve `clippy` ile), Node 26 (LTS, sürüm `.nvmrc` dosyasında).

- Windows: WebView2 (Windows 11'de yüklü gelir) ve Visual Studio C++ Build Tools.
- Linux: Tauri sistem kütüphaneleri: `sudo apt-get install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev`

```bash
npm run setup        # web bağımlılıkları + Playwright Chromium
```

## Çalıştırma

| Komut | Ne yapar |
|---|---|
| `npm run verify` | Tüm kapılar: fixture sır taraması, Rust fmt/clippy/test, TS tip kontrolü, Vitest, Vite build, Playwright E2E, Windows'ta gerçek uygulama E2E |
| `npm run verify:fast` | E2E ve build hariç hızlı kontrol |
| `node scripts/verify.mjs --only rust` | Yalnızca bir grup (`repo`, `rust`, `web`, `e2e`, `desktop`) veya adım kimliği |
| `npm run test:rust` | Rust testleri |
| `npm run test:web` | Vitest |
| `npm run e2e` | Playwright (fixture IPC, 3 görünüm) |
| `npm run dev:fixture` | Arayüzü fixture verisiyle tarayıcıda açar |
| `npm run desktop` | Masaüstü uygulamasını geliştirme modunda açar (Tauri + Vite) |
| `npm run desktop:build` | Release derlemesi ve Windows installer |
| `npm run desktop:exe` | Installer'sız release exe (`target/release/videovault.exe`) |
| `npm run desktop:e2e-exe` | E2E test exe'si (`target/release/videovault-e2e.exe`, cargo özelliği `e2e-cdp`) |
| `npm run e2e:desktop` | Gerçek uygulama E2E: test exe'sini açar, WebView2'ye bağlanır (yalnızca Windows, önce `desktop:e2e-exe`) |
| `npm run fixtures:ytdlp` | yt-dlp fixture'larını yeniden yakalar (yalnızca geliştirici; `VV_YTDLP` ile sabitlenmiş yt-dlp sürümü ve PATH'te ffmpeg gerekir, bkz. ADR-0003). `verify` bunu çalıştırmaz |
| `npm run tools:fetch` | Sabitlenmiş yt-dlp ve Deno'yu `tools/bin/` altına indirir; hash, GPG imzası, sürüm ve JS runtime kontrolü yapar (ADR-0005). CI her iki işletim sisteminde çalıştırır; `verify` çalıştırmaz (ağ gerekir) |
| `npm run fixtures:media` | Oynatma matrisi medyasını (`tests/fixtures/media/`) ffmpeg ile yeniden üretir (yalnızca geliştirici, ADR-0004). `verify` bunu çalıştırmaz |

`verify` sonucu `test-artifacts/verify/report.md` ve `report.json` dosyalarına yazılır. Her adım “automated pass”, “automated fail” veya “not run” olarak raporlanır (SPEC §25.4). JUnit çıktıları `test-artifacts/vitest/` ve `test-artifacts/e2e/`, ekran görüntüleri `test-artifacts/screenshots/` altındadır.

Önceden kurulu bir Chromium kullanmak için `PW_CHROMIUM_EXECUTABLE` ortam değişkenine tarayıcının yolunu verin.

## Yeni test nereye eklenir

| Ne test ediliyor | Nereye | Not |
|---|---|---|
| Saf iş kuralı (URL, durum makinesi, hata eşleme) | `crates/vv-domain/src/*.rs` içinde `#[cfg(test)]` | IO yok, her işletim sisteminde çalışır |
| Çok vakalı kural | `tests/fixtures/<ad>.json` + `crates/*/tests/<ad>_fixture.rs` | Beklenen sonuç fixture'da yazılır; tek doğruluk kaynağı fixture'dır |
| Veritabanı, migration, yedek | `crates/vv-persistence/tests/*.rs` | Gerçek dosyalarla, `tempfile` dizininde; yeni migration için ileri/geri testi otomatik çalışır |
| IPC komutu (yeni Tauri komutu) | `apps/desktop/src-tauri/tests/ipc.rs` + `tests/fixtures/ipc-contract.json` | Komut `build.rs` listesine ve `capabilities/main.json`'a da eklenir; aksi halde ACL reddeder |
| UI bileşeni | `apps/desktop/src/**/<Bileşen>.test.tsx` | Fixture istemcisiyle |
| Kullanıcı yolculuğu | `apps/desktop/e2e/*.spec.ts` | 3 görünümde çalışır |
| yt-dlp çıktısı ayrıştırma ve hata eşleme | `crates/vv-ytdlp/tests/fixtures.rs` + `tests/fixtures/ytdlp/` | Yeni hata metni: yakalanabiliyorsa yakalama betiğine senaryo, değilse kaynağıyla birlikte `errors-source.json`'a vaka |
| Gerçek uygulama davranışı (IPC, yetki, CSP, pencere) | `apps/desktop/e2e-desktop/*.spec.ts` | Windows'ta gerçek exe ile; ADR-0001 |
| Yönetilen araçlar: manifest, hash, sürüm, JS runtime, FFmpeg lisansı | `crates/vv-tools/tests/tools.rs` + `tools/manifest.json` + `tests/fixtures/tools/` | Araç sürümü değişince `health-output.json` yeniden yakalanır; ADR-0005, ADR-0006 |
| FFmpeg/ffprobe bayrakları, biçim sınıflandırma, referanslı girdi reddi (AC-59) | `crates/vv-ffmpeg/tests/ffmpeg.rs` + `tests/fixtures/ffmpeg/` | Gerçek build ile `npm run tools:fetch` (CI'da her çalışmada); yeni build: `node scripts/ffmpeg-probe.mjs --ffmpeg … --ffprobe … --update`; ADR-0006 |
| Ağ denetimi: politika, proxy, gerçek yt-dlp (AC-29) | `crates/vv-netguard/tests/netguard.rs` + `scripts/netguard-probe.mjs` → `test-artifacts/netguard/` | Yeni yönlendirme/manifest vakası probe'un `cases` listesine; kanarya her vakada 0 bağlantı almalı; ADR-0007 |
| Alt süreç: temiz ortam, iptal/zaman aşımında ağaç öldürme, çökmede sahipsiz süreç yok | `crates/vv-process/tests/process.rs` (yardımcı: `src/bin/vv-process-helper.rs`) | Çökme testi yalnızca Windows'ta (Job Object); ADR-0008 |
| Tek süreç ve kütüphane kilidi (INV-13, AC-38) | `crates/vv-persistence/tests/lock.rs` + `apps/desktop/e2e-desktop/single-instance.spec.ts` | E2E her uygulamaya geçici `VV_DATA_DIR` verir; ADR-0008 |
| Yerel medya protokolü (range, yol sınırı) | `apps/desktop/src-tauri/tests/media_protocol.rs` | ADR-0004 |
| WebView2 oynatma matrisi (DEC-04) | `apps/desktop/e2e-desktop/playback-matrix.spec.ts` → `test-artifacts/playback-matrix/` | Yalnızca MP4 H.264/AAC zorunlu; diğerleri ölçüm. Yeni format: `make-playback-media.mjs` `MATRIX` satırı |

Kurallar:

- Test adı veya yorumu ilgili SPEC kimliğini taşır (ör. `AC-29`, `INV-02`).
- Fixture'lara gerçek URL'ler, çerezler, token'lar veya kişisel dosyalar konmaz. Redaksiyon testlerindeki sahte değerler `CANARY` kelimesini içerir. `verify` bunu denetler.
- Fixture ile geçen test, canlı platform başarısı sayılmaz.
- Fixture IPC ile geçen E2E testi, gerçek Tauri IPC'nin çalıştığını kanıtlamaz. `tests/ipc.rs` gerçek IPC yolunu sahte pencereyle test eder; gerçek pencere ve WebView2 için `e2e-desktop/` vardır (yalnızca Windows).

## Fixture'lar

| Dosya | İçerik | Kullanan |
|---|---|---|
| `tests/fixtures/url-policy.json` | 33 URL vakası ve beklenen sonuç | `vv-domain` fixture testi |
| `tests/fixtures/ipc-contract.json` | IPC komutlarının ve `DomainError`'ın tel biçimi (sözleşme v1) | `vv-desktop` IPC testleri, Vitest |
| `tests/fixtures/ytdlp/args.json` | yt-dlp'ye her zaman verilen sabit bayraklar ve progress şablonu; sabitlenmiş yt-dlp sürümü | `vv-ytdlp` (sapma testi), yakalama betiği |
| `tests/fixtures/ytdlp/runs/*.json` | Yerel sunucuya karşı gerçek yt-dlp 2026.10.26 çalışmaları: metadata (HLS, doğrudan dosya, playlist), progress (bilinen/bilinmeyen toplam, fragment), iptal (SIGINT, kill), 7 hata | `vv-ytdlp` integration testleri |
| `tests/fixtures/tools/health-output.json` | Sabitlenmiş yt-dlp ve Deno'nun gerçek `--version` çıktıları, 6 `JS runtimes` satırı (PATH'teki Deno dahil) ve Deno izin denemesi | `vv-tools` testleri |
| `tests/fixtures/ffmpeg/args.json` | FFmpeg/ffprobe sabit bayrakları ve biçim izin listesi; sabitlenen build | `vv-ffmpeg` (sapma testi), `ffmpeg-probe.mjs` |
| `tests/fixtures/ffmpeg/probe/*.json` | Sabitlenen ffprobe 9.0.2'nin 14 matris dosyası için kısıtlı JSON çıktısı | `vv-ffmpeg`; CI'da gerçek build ile karşılaştırılır |
| `tests/fixtures/ffmpeg/rejections.json` | HLS/concat/URL girdileri ve iki kontrol: çıkış, reddedilen demuxer, kanaryaya gelen HTTP isteği | `vv-ffmpeg`; CI'da gerçek build ile karşılaştırılır |
| `tests/fixtures/ffmpeg/version-output.json` | Sabitlenen build'in ve GPL'li dağıtım build'inin gerçek `-version` çıktısı | `vv-tools` |
| `tests/fixtures/media/*` + `manifest.json` | 14 adet 3 sn'lik test videosu (MP4/WebM/MKV/MOV × H.264/HEVC/VP8/VP9/AV1, AAC/Opus/Vorbis), sha256 ve ffprobe özetiyle | `media_protocol.rs`, oynatma matrisi |
| `tests/fixtures/ytdlp/errors-source.json` | Yerelde üretilemeyen 20 hata satırı (giriş, bölge, DRM, araç eksik, Windows ağ hataları, CANARY redaksiyon vakası); her biri yt-dlp kaynak dosyasıyla | `vv-ytdlp` integration testleri |
| `tests/fixtures/library-small.json` | 6 sentetik öğe: karışık platform, uzun başlık, Türkçe, emoji, bilinmeyen süre, eksik dosya | Arayüz fixture istemcisi, Vitest, Playwright |

## Kapsama matrisi

Her P0 gereksinim en az bir AC'ye veya kimlikli bir manuel teste bağlanır (SPEC §25.3). Matris F0.11 kapsamında bu bölüme eklenecek.
