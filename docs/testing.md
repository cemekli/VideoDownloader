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
| `npm run bench` | Benchmark: üretilen 10.000 öğe ve 100.000 segment üzerinde BM-01…03; `test-artifacts/bench/report.md` ve `report.json` (ADR-0009). CI her çalışmada bilgi amaçlı çalıştırır |
| `npm run coverage:write` | Kapsama matrisini `docs/coverage.json`'dan bu belgeye yazar; `verify` güncel olmasını denetler |
| `npm run fixtures:media` | Oynatma matrisi medyasını (`tests/fixtures/media/`) ffmpeg ile yeniden üretir (yalnızca geliştirici, ADR-0004). `verify` bunu çalıştırmaz |

`verify` sonucu `test-artifacts/verify/report.md` ve `report.json` dosyalarına yazılır. Her adım “automated pass”, “automated fail” veya “not run” olarak raporlanır (SPEC §25.4). JUnit çıktıları `test-artifacts/vitest/` ve `test-artifacts/e2e/`, ekran görüntüleri `test-artifacts/screenshots/` altındadır.

Önceden kurulu bir Chromium kullanmak için `PW_CHROMIUM_EXECUTABLE` ortam değişkenine tarayıcının yolunu verin.

## CI ne zaman çalışır

CI (`.github/workflows/ci.yml`) her değişikliği Windows ve Linux'ta bir kez test eder (8 Kasım 2026'dan beri):

- Yalnızca pull request'lerde çalışır. Main'e merge edilen kod PR'da zaten test edildiği için main'de yeniden çalışmaz.
- Taslak (draft) PR'larda çalışmaz. PR "Ready for review" yapılınca çalışır. Bu yüzden ara işler taslak PR olarak açılır ve büyük adımlarda hazır yapılır.
- Aynı PR'a yeni bir push gelirse eski çalışma iptal edilir.
- Yalnızca Markdown dosyaları (STATE, HANDOFF, `docs/`) değiştiyse CI çalışmaz.
- GitHub'da Actions → ci → "Run workflow" ile herhangi bir dalda elle çalıştırılabilir.

**Kanıt:** CI çalışmadıysa, STATE.md'de bir alt görevi ✅ yapmak için iki yerel `npm run verify` raporu birlikte kanıt sayılır: Windows'ta Cuma'nın çalıştırdığı rapor (gerçek uygulama E2E dahil) ve Linux'ta Claude'un çalıştırdığı rapor. "Doğrulama" sütununa tarih ve iki raporun sonucu yazılır.

## Yeni test nereye eklenir

| Ne test ediliyor | Nereye | Not |
|---|---|---|
| Saf iş kuralı (URL, durum makinesi, hata eşleme) | `crates/vv-domain/src/*.rs` içinde `#[cfg(test)]` | IO yok, her işletim sisteminde çalışır |
| Çok vakalı kural | `tests/fixtures/<ad>.json` + `crates/*/tests/<ad>_fixture.rs` | Beklenen sonuç fixture'da yazılır; tek doğruluk kaynağı fixture'dır |
| Veritabanı, migration, yedek | `crates/vv-persistence/tests/*.rs` | Gerçek dosyalarla, `tempfile` dizininde; yeni migration için ileri/geri testi otomatik çalışır |
| Kütüphane şeması ve repository'ler | `crates/vv-persistence/tests/library_schema.rs` | Yeni enum değeri hem Rust `ALL` listesine hem SQL `CHECK`'e girer; test ikisini karşılaştırır |
| Bookmark kaydı, duplicate, yarış, sır sızıntısı | `crates/vv-persistence/tests/bookmark.rs` | Sır testleri kanarya değeri DB, WAL ve shm baytlarında arar; zaman `FixedClock` ile verilir |
| Arama (sorgu oluşturucu, sıralama, snippet) | `crates/vv-persistence/tests/search.rs` + `tests/fixtures/search-tr.json` | Türkçe vaka eklerken `status` alanına `match`, `no_match` ya da `limitation` yaz; snippet'in özgün metnin alt dizesi olduğu her vakada denetlenir |
| Yerel import, kök durumu, relink, thumbnail | `crates/vv-import/tests/import.rs` | Gerçek ffprobe/FFmpeg (`tools/bin`) ile; yoksa yerelde atlanır, `CI` ortam değişkeni varken düşer. Kaynak dosyaların baytları import öncesi ve sonrası karşılaştırılır |
| URL normalizasyonu | `tests/fixtures/normalize.json` (+ `crates/vv-domain/tests/normalize_fixture.rs`) | Kural değişince `NORMALIZATION_VERSION` ve fixture'daki `normalizationVersion` birlikte artar; eski kayıtlar için `tests/renormalize.rs` (AC-44) |
| IPC komutu (yeni Tauri komutu) | `apps/desktop/src-tauri/tests/ipc.rs` + `tests/fixtures/ipc-contract.json` | Komut `build.rs` listesine ve `capabilities/main.json`'a da eklenir; aksi halde ACL reddeder |
| UI bileşeni | `apps/desktop/src/**/<Bileşen>.test.tsx` | Fixture istemcisiyle |
| Kullanıcı yolculuğu | `apps/desktop/e2e/*.spec.ts` | 3 görünümde çalışır |
| yt-dlp çıktısı ayrıştırma ve hata eşleme | `crates/vv-ytdlp/tests/fixtures.rs` + `tests/fixtures/ytdlp/` | Yeni hata metni: yakalanabiliyorsa yakalama betiğine senaryo, değilse kaynağıyla birlikte `errors-source.json`'a vaka |
| Dışarıdan metin okuyan her ayrıştırıcı (yt-dlp çıktısı, altyazı, çerez dosyası, `profiles.ini`) | `crates/*/tests/fuzz.rs` (`vv-fuzz` ile) | Bkz. "Fuzz testleri". Yeni ayrıştırıcı yazınca buraya bir hedef eklenir |
| Gerçek uygulama davranışı (IPC, yetki, CSP, pencere) | `apps/desktop/e2e-desktop/*.spec.ts` | Windows'ta gerçek exe ile; ADR-0001 |
| Yönetilen araçlar: manifest, hash, sürüm, JS runtime, FFmpeg lisansı | `crates/vv-tools/tests/tools.rs` + `tools/manifest.json` + `tests/fixtures/tools/` | Araç sürümü değişince `health-output.json` yeniden yakalanır; ADR-0005, ADR-0006 |
| FFmpeg/ffprobe bayrakları, biçim sınıflandırma, referanslı girdi reddi (AC-59) | `crates/vv-ffmpeg/tests/ffmpeg.rs` + `tests/fixtures/ffmpeg/` | Gerçek build ile `npm run tools:fetch` (CI'da her çalışmada); yeni build: `node scripts/ffmpeg-probe.mjs --ffmpeg … --ffprobe … --update`; ADR-0006 |
| Ağ denetimi: politika, proxy, gerçek yt-dlp (AC-29) | `crates/vv-netguard/tests/netguard.rs` + `scripts/netguard-probe.mjs` → `test-artifacts/netguard/` | Yeni yönlendirme/manifest vakası probe'un `cases` listesine; kanarya her vakada 0 bağlantı almalı; ADR-0007 |
| Alt süreç: temiz ortam, iptal/zaman aşımında ağaç öldürme, çökmede sahipsiz süreç yok | `crates/vv-process/tests/process.rs` (yardımcı: `src/bin/vv-process-helper.rs`) | Çökme testi yalnızca Windows'ta (Job Object); ADR-0008 |
| Tek süreç ve kütüphane kilidi (INV-13, AC-38) | `crates/vv-persistence/tests/lock.rs` + `apps/desktop/e2e-desktop/single-instance.spec.ts` | E2E her uygulamaya geçici `VV_DATA_DIR` verir; ADR-0008 |
| Yeni P0 gereksinim, AC ya da manuel test | `docs/coverage.json`, ardından `npm run coverage:write` | Test adı ya da yorumu AC/MT kimliğini taşırsa "Test atfı" sütununda görünür |
| Benchmark veri üreticisi, benchmark'lar | `crates/vv-benchdata/tests/determinism.rs` + `tests/fixtures/benchdata/golden.json` | Üretilen veriyi değiştiren her değişiklik `GENERATOR_VERSION`'ı ve golden hash'leri birlikte günceller; ADR-0009 |
| Yerel medya protokolü (range, yol sınırı) | `apps/desktop/src-tauri/tests/media_protocol.rs` | ADR-0004 |
| WebView2 oynatma matrisi (DEC-04) | `apps/desktop/e2e-desktop/playback-matrix.spec.ts` → `test-artifacts/playback-matrix/` | Yalnızca MP4 H.264/AAC zorunlu; diğerleri ölçüm. Yeni format: `make-playback-media.mjs` `MATRIX` satırı |

Kurallar:

- Test adı veya yorumu ilgili SPEC kimliğini taşır (ör. `AC-29`, `INV-02`).
- Fixture'lara gerçek URL'ler, çerezler, token'lar veya kişisel dosyalar konmaz. Redaksiyon testlerindeki sahte değerler `CANARY` kelimesini içerir. `verify` bunu denetler.
- Fixture ile geçen test, canlı platform başarısı sayılmaz.
- Fixture IPC ile geçen E2E testi, gerçek Tauri IPC'nin çalıştığını kanıtlamaz. `tests/ipc.rs` gerçek IPC yolunu sahte pencereyle test eder; gerçek pencere ve WebView2 için `e2e-desktop/` vardır (yalnızca Windows).

## Fuzz testleri

Elle yazılan fixture'lar kısa ve çoğunlukla ASCII'dir. 11 Kasım 2026'daki altyazı hatası (`&gt;&gt;`'den sonra Türkçe harfte bayt dilimi panic'i) bu yüzden testlerde görünmedi. Dışarıdan metin okuyan her ayrıştırıcı bu yüzden bir de fuzz testinden geçer:

- `crates/vv-fuzz`: yalnızca test bağımlılığı, uygulamaya girmez. Sabit tohumlu SplitMix64 (her işletim sisteminde aynı vakalar); gerçek bir örneği (fixture) parçalayan, kesen, tekrarlayan, Türkçe harf, NFD, emoji, BOM, NUL, CR, yön işareti ve geçersiz UTF-8 ekleyen mutasyonlar; örneğin her karakter sınırında kesilmiş halleri.
- Hedefler: `crates/vv-ytdlp/tests/fuzz.rs` (progress satırı, hata satırı + `RunObserver`, redaksiyon, metadata JSON, altyazı izi seçimi, WebVTT, satır bölücü) ve `crates/vv-engine/tests/fuzz.rs` (Netscape çerez dosyası, Firefox `profiles.ini`, whisper-cli progress satırı, `-oj` JSON çıktısı ve WAV başlığı). Her hedef panic etmemeli ve kendi kuralını korumalı: redaksiyondan sonra URL'deki `VVCANARY` görünmez; çerez filtresi yalnızca platformun alan adlarını bırakır; göreli Firefox profili klasörden çıkmaz; altyazı işaretleri sıralı, `start < end`, kontrol karakteri yok.
- Varsayılan 2 000 vaka/hedef, `cargo test` içinde birkaç saniye. Daha uzun arama: `VV_FUZZ_ITERS=100000 cargo test --release --test fuzz`; başka tohum: `VV_FUZZ_SEED=0x1234`. Düşen vaka numarasını ve tohumu yazar; `VV_FUZZ_CASE=<n>` yalnızca onu çalıştırır.
- Düşen bir vaka bulunursa düzeltmeyle birlikte ayrı bir regresyon testi (sabit girdi) eklenir.

**UTF-8 dilimleme kuralı.** Workspace'te `clippy::string_slice` açık (`Cargo.toml` `[workspace.lints.clippy]`, `verify` `-D warnings` ile çalıştırır). `&s[a..b]` yerine `get`, `split_once`, `strip_prefix`/`strip_suffix` kullanılır. Ofset güvenliyse (ör. ASCII bir karakterin `find` sonucu) fonksiyona `#[expect(clippy::string_slice, reason = "…")]` yazılır; gerekçe zorunludur. Kural açıldığında iki gizli hata bulundu: 8 baytlık ama 8 rakam olmayan `upload_date` (`2026ğ110`) metadata işini panic ettirebilirdi; Windows'ta `profiles.ini` içindeki `Path=..\x` ya da `C:\x` göreli profil klasörünün dışına çıkabilirdi. Whisper (F6.2b) eklenince iki şey daha çıktı: `whisper.cpp` sürüm metni bayt bayt kesiliyordu (artık karakter karakter), ve whisper JSON'undaki `\u0000` gibi kaçışlar döküm metnine kontrol karakteri olarak giriyordu (artık boşluk).

## Fixture'lar

| Dosya | İçerik | Kullanan |
|---|---|---|
| `tests/fixtures/url-policy.json` | 33 URL vakası ve beklenen sonuç | `vv-domain` fixture testi |
| `tests/fixtures/search-tr.json` | 11 belge, 21 Türkçe/İngilizce arama vakası (`I/İ/ı/i`, NFD, aksansız yazım, tırnak, noktalama, bilinen sınırlar) | `vv-persistence` arama testi |
| `tests/fixtures/normalize.json` | 33 normalizasyon vakası (7 platform, kısa bağlantı, playlist, AC-36/AC-52 sır vakaları); yalnızca sentetik kimlikler | `vv-domain` normalizasyon fixture testi |
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
| `tests/fixtures/library-small.json` | 7 sentetik öğe: karışık platform, uzun başlık, Türkçe, emoji, bilinmeyen süre, eksik dosya, bağlı olmayan disk; inspector alanları | Arayüz fixture istemcisi, Vitest, Playwright |

## Benchmark

SPEC §22 performans hedeflerini **önerilen bütçeler** olarak verir ve M0'da referans cihaz ile yöntemin buraya yazılmasını ister. Karar ve gerekçe: ADR-0009.

**Referans makine (DEC-17).** Windows 11 x64, SSD, en az 8 GB RAM, en az 4 mantıksal CPU, prize takılı, güç modu "Dengeli" ya da daha yüksek. Bütçe karşılaştırması yalnızca bu sınıftaki, CI dışındaki bir makinede anlamlıdır. `scripts/bench.mjs` makineyi bu ölçütlerle sınıflar ve raporun başına yazar (`reference-class` ya da `not reference`). İlk referans sınıfı ölçüm 7 Kasım 2026'da yapıldı (ADR-0009, Kanıt).

**Veri.** `crates/vv-benchdata` sürümlü bir üreticidir: kendi SplitMix64 PRNG'si, yalnızca tam sayılar, sıralı JSON ve `\n` satır sonları. Aynı `(GENERATOR_VERSION, seed, sayılar)` her işletim sisteminde aynı baytları verir; varsayılan veri setinin SHA-256 değerleri `tests/fixtures/benchdata/golden.json`'dadır ve Windows ile Linux'ta test edilir. Varsayılanlar: seed 20261107, 10.000 öğe (bookmark, indirilmiş ve yerel video; Türkçe, İngilizce ve karışık; çok uzun başlık, emoji, NFD yazım, bilinmeyen süre, eksik küçük resim, çoklu etiket ve koleksiyon), 100.000 transkript segmenti (yalnızca medyası olan öğelerde, sıralı ve çakışmasız). URL'ler `.example` alan adlarındadır.

**Yöntem.**

| Kimlik | Ne ölçülüyor | Nasıl | Bütçe |
|---|---|---|---|
| BM-01 | Bookmark kaydı | Dolu kütüphanede 300 kez tek işlemde satır + FTS belgesi, WAL ve `synchronous=FULL` (ADR-0002) | NFR-01: p95 ≤ 500 ms (görünür sonuç dahil; burada yalnızca backend commit'i) |
| BM-02 | Lexical arama | 200 deterministik sorgu (tek sözcük, iki sözcük, önek, Türkçe harfli), önce bir kez ısıtma, sonra ölçüm; ilk 50 sonuç, bm25 sıralaması, snippet | NFR-03: backend p95 ≤ 200 ms |
| BM-03 | Döküm araması | Aynı yöntem, 100.000 segment, segment tablosuyla birleştirme | NFR-04: p95 ≤ 500 ms |
| BM-04 | Başlangıç | Uygulama kütüphaneyi açtığında (F1) soğuk ve sıcak ayrı | NFR-06: ≤ 3 s |
| BM-05 | Bellek | Liste ekranı geldiğinde (F1.7) idle ve scroll | NFR-07: ≤ 500 MB |

BM-01…03 şimdilik prototip `bench_*` tablolarında çalışır, çünkü uygulama şeması F1.1'de gelecek. Veritabanı uygulamanınkiyle aynı biçimde açılır (`vv_persistence::Database`). F4.5'te aynı ölçümler gerçek şemada tekrarlanır. Rapor; tarih, commit, CPU, mantıksal CPU sayısı, RAM, depolama türü, güç modu, işletim sistemi build'i, Rust, Node ve SQLite sürümlerini, veri setinin seed ve hash'ini içerir. Hedef karşılanmazsa sonuç gizlenmez (SPEC §22).

## Kapsama matrisi

Her P0 gereksinim en az bir AC'ye veya kimlikli bir manuel teste bağlanır (SPEC §25.3). Bağlantılar `docs/coverage.json` dosyasındadır. Aşağıdaki bölümü `npm run coverage:write` üretir; `npm run verify` her çalışmada bağlantısız gereksinim, bilinmeyen kimlik ya da eskimiş tablo bulursa başarısız olur.

<!-- coverage:begin (generated by scripts/coverage-matrix.mjs; edit docs/coverage.json) -->

Kapsam: SPEC §7'deki P0 gereksinimler, §5'teki tüm değişmezler (INV) ve §22'deki tüm NFR'ler, toplam 82. Bağlantısı olan: **82/82**. Bağlı kimliklerden birine ya da gereksinimin kendisine atıf yapan bir test dosyası olan: **66/82**.

"Test atfı" sütunu yalnızca o kimliği anan test dosyalarını listeler. Bir AC'nin karşılandığını göstermez; karşılanma durumu STATE.md'deki alt görevin kanıtıyla belirlenir.

| Gereksinim | Aşama | Özet | Bağlı AC / test | Test atfı |
|---|---|---|---|---|
| INV-01 | tüm aşamalar | Başarısız metadata, indirme veya AI işi kayıtlı bookmark'ı silmez. | AC-12, AC-19 | `crates/vv-engine/src/download.rs`, `crates/vv-engine/src/storage.rs`, `crates/vv-engine/tests/engine.rs`, `crates/vv-persistence/tests/auth.rs` |
| INV-02 | tüm aşamalar | downloaded görünümü yalnızca kullanılabilir yerel medya kaydından türetilir. Kuyrukta olm… | AC-08, AC-10 | `apps/desktop/src/App.test.tsx`, `crates/vv-engine/src/download.rs`, `crates/vv-engine/tests/engine.rs`, `crates/vv-persistence/tests/library_view.rs` |
| INV-03 | tüm aşamalar | Bir AuthProfile kaydının varlığı “erişim doğrulandı” anlamına gelmez. | AC-19, AC-54 | `crates/vv-engine/tests/engine.rs`, `crates/vv-persistence/tests/auth.rs` |
| INV-04 | tüm aşamalar | Ağ ve uzun medya işlemleri UI iş parçacığında yürütülmez. | MT-15 | henüz yok |
| INV-05 | tüm aşamalar | Kuyruk ve alan verisi, tutarlı SQLite işlemleriyle kalıcılaştırılır. | AC-03, AC-11 | `crates/vv-engine/tests/engine.rs`, `crates/vv-persistence/src/library/bookmark.rs`, `crates/vv-persistence/tests/bookmark.rs` |
| INV-06 | tüm aşamalar | Yerel import kaynak dosyasını varsayılan olarak değiştirmez veya silmez. | AC-04, AC-51 | `apps/desktop/src-tauri/src/data_commands.rs`, `crates/vv-import/tests/import.rs`, `crates/vv-persistence/src/portable.rs`, `crates/vv-persistence/tests/portable.rs` |
| INV-07 | tüm aşamalar | Arama kaynağı ile sonuç açıklaması tutarlıdır; transkript yokken transkript eşleşmesi gös… | AC-22, AC-41 | henüz yok |
| INV-08 | tüm aşamalar | Webview doğrudan shell, dosya sistemi veya gizli verilere erişmez. | MT-16 | `apps/desktop/e2e-desktop/desktop-shell.spec.ts`, `apps/desktop/src-tauri/tests/ipc.rs` |
| INV-09 | tüm aşamalar | Çerezler/kimlik bilgileri SQLite, uygulama logları ve taşınabilir yedekte bulunmaz. | AC-18, AC-55 | `crates/vv-engine/tests/engine.rs`, `crates/vv-persistence/tests/auth.rs` |
| INV-10 | tüm aşamalar | Sahte indirme sayısı, yapay AI puanı, uydurma test veya destek iddiası gösterilemez. | AC-09, AC-21 | `apps/desktop/src-tauri/src/download_commands.rs`, `apps/desktop/src/Downloads.test.tsx`, `crates/vv-engine/src/download.rs`, `crates/vv-engine/tests/engine.rs` |
| INV-11 | tüm aşamalar | Kütüphane kaydını kaldırmak ile fiziksel dosyayı silmek ayrı işlemlerdir. | AC-37, AC-53 | `apps/desktop/src-tauri/src/organize_commands.rs`, `apps/desktop/src/Organize.test.tsx` |
| INV-12 | tüm aşamalar | Kullanıcı notları ve düzeltmeleri, tekrar metadata çekme veya AI çalıştırma ile ezilmez. | AC-13, AC-60 | henüz yok |
| INV-13 | tüm aşamalar | Aynı kütüphane veritabanını ve iş kuyruğunu aynı anda yalnızca tek bir uygulama süreci aç… | AC-38 | `apps/desktop/e2e-desktop/single-instance.spec.ts`, `apps/desktop/src-tauri/src/commands.rs`, `apps/desktop/src-tauri/src/library.rs`, `crates/vv-persistence/tests/lock.rs`, `crates/vv-process/tests/process.rs` |
| CAP-01 | M1 | Global Add URL eylemi ve uygun odakta Ctrl+V ile URL girişi sağlanmalı. Pano arka planda… | MT-01, AC-27 | `apps/desktop/e2e/a11y.spec.ts`, `apps/desktop/src-tauri/src/capture_commands.rs`, `apps/desktop/src/AddUrl.test.tsx` |
| CAP-02 | M1 | URL şeması doğrulanmalı; tanıma ve yerel normalizasyon ağdan bağımsız çalışmalı. | AC-01, AC-29, AC-52 | `apps/desktop/e2e-desktop/add-url.spec.ts`, `crates/vv-domain/src/normalize.rs`, `crates/vv-domain/src/url_policy.rs`, `crates/vv-domain/tests/normalize_fixture.rs`, `crates/vv-engine/tests/real_ytdlp.rs`, `crates/vv-netguard/tests/netguard.rs`, `crates/vv-persistence/src/library/bookmark.rs`, `crates/vv-persistence/tests/bookmark.rs`, `scripts/netguard-probe.mjs` |
| CAP-03 | M1 | Save Bookmark, metadata isteği bitmeden kullanılabilmeli; kayıt ve arama belgesi atomik s… | AC-01, AC-03 | `apps/desktop/e2e-desktop/add-url.spec.ts`, `apps/desktop/src/AddUrl.test.tsx`, `crates/vv-persistence/src/library/bookmark.rs`, `crates/vv-persistence/tests/bookmark.rs` |
| CAP-04 | M1 | Metadata gelmiyorsa bağlantıdan türetilen dürüst bir başlık ve pending/failed durumu göst… | AC-01, AC-17 | `apps/desktop/e2e-desktop/add-url.spec.ts`, `apps/desktop/e2e/settings.spec.ts`, `apps/desktop/src-tauri/src/capture_commands.rs`, `apps/desktop/src/AddUrl.test.tsx`, `apps/desktop/src/Data.test.tsx`, `crates/vv-domain/src/normalize.rs`, `crates/vv-engine/src/metadata.rs`, `crates/vv-engine/tests/engine.rs`, `crates/vv-persistence/tests/bookmark.rs` |
| CAP-05 | M1 | Etiket/koleksiyon seçimi isteğe bağlı olmalı; varsayılan kayıt Inbox'a gitmeli. | MT-02 | henüz yok |
| CAP-06 | M1 | Platform+source ID veya normalize URL eşleşmesi aynı öğeye yönlendirmeli; mevcut notlar k… | AC-02, AC-03, AC-45, AC-56 | `apps/desktop/src-tauri/src/capture_commands.rs`, `apps/desktop/src/AddUrl.test.tsx`, `crates/vv-persistence/src/library/bookmark.rs`, `crates/vv-persistence/tests/bookmark.rs` |
| CAP-07 | M2 | Güvenli metadata önizlemesi varsa gösterilmeli; Download ayrı ve isteğe bağlı eylem olmal… | MT-03, AC-28 | `crates/vv-engine/src/metadata.rs` |
| CAP-08 | M1 | Geçici oturumlu medya URL'leri kalıcı kaynak URL yerine kullanılmamalı; hassas URL sorgul… | AC-36, AC-52, AC-18 | `apps/desktop/src-tauri/src/capture_commands.rs`, `crates/vv-domain/src/normalize.rs`, `crates/vv-domain/tests/normalize_fixture.rs`, `crates/vv-engine/tests/engine.rs`, `crates/vv-persistence/src/library/bookmark.rs`, `crates/vv-persistence/tests/auth.rs`, `crates/vv-persistence/tests/bookmark.rs`, `crates/vv-ytdlp/tests/fuzz.rs` |
| IMP-01 | M1 | Dosya seçici, klasör seçici ve erişilebilir alternatifi olan sürükle-bırak desteklenmeli. | MT-04 | `apps/desktop/src-tauri/src/library_commands.rs` |
| IMP-02 | M1 | MP4, MKV, WebM, MOV ve seçilen araçların desteklediği ses dosyaları ffprobe ile doğrulanm… | AC-04, AC-32, AC-59 | `apps/desktop/e2e/player.spec.ts`, `apps/desktop/src/Data.test.tsx`, `crates/vv-ffmpeg/tests/ffmpeg.rs`, `crates/vv-import/src/fsutil.rs`, `crates/vv-import/tests/import.rs`, `scripts/ffmpeg-probe.mjs` |
| IMP-03 | M1 | Süre, container, codec, boyut, çözünürlük ve ses parçası bilgisi mevcut olduğu ölçüde çık… | MT-05 | henüz yok |
| IMP-04 | M1 | Hatalı/okunamayan dosyalar dosya bazında raporlanmalı; bir hata bütün toplu importu geri… | AC-04 | `crates/vv-import/tests/import.rs` |
| IMP-05 | M1 | Tam yol eşleşmesi ve isteğe bağlı içerik hash'i yinelenen dosyayı belirlemeli. Hash eşleş… | AC-34 | `crates/vv-import/tests/import.rs` |
| IMP-06 | M1 | Küçük resim üretimi arka planda yapılmalı; eksik küçük resim kaydı veya aramayı engelleme… | MT-06 | `apps/desktop/src-tauri/src/library.rs`, `crates/vv-import/tests/import.rs` |
| IMP-07 | M1 | Kaynak dosya taşınır/silinirse öğe korunmalı, missing gösterilmeli ve yeniden bağlama sun… | AC-35 | `apps/desktop/src-tauri/src/player_commands.rs`, `crates/vv-domain/src/error.rs`, `crates/vv-import/tests/import.rs` |
| ORG-01 | M1 | Manuel koleksiyon oluşturma, yeniden adlandırma ve silme desteklenmeli; koleksiyon silme… | AC-06, AC-48 | `apps/desktop/src-tauri/src/organize_commands.rs`, `apps/desktop/src/Organize.test.tsx` |
| ORG-02 | M1 | Bir öğe birden çok koleksiyona ve etikete bağlanabilmeli. | MT-07, AC-15 | `crates/vv-persistence/src/portable.rs`, `crates/vv-persistence/tests/portable.rs` |
| ORG-03 | M1 | Kullanıcı notu, favori ve arşiv durumları kalıcı olmalı. | AC-05, AC-15 | `apps/desktop/src-tauri/src/organize_commands.rs`, `apps/desktop/src/Organize.test.tsx`, `crates/vv-persistence/src/portable.rs`, `crates/vv-persistence/tests/portable.rs`, `crates/vv-persistence/tests/search.rs` |
| ORG-04 | M1 | Çoklu seçimle etiket/koleksiyon atama, favorileme, arşivleme ve kayıttan kaldırma sağlanm… | AC-07 | `apps/desktop/e2e/journeys.spec.ts`, `apps/desktop/e2e/organize.spec.ts`, `apps/desktop/src/Organize.test.tsx` |
| ORG-05 | M1 | Toplu işlemler başarılı/başarısız öğe sayısını doğru göstermeli; geri alma mümkün olan iş… | AC-07, AC-60 | `apps/desktop/e2e/journeys.spec.ts`, `apps/desktop/e2e/organize.spec.ts`, `apps/desktop/src-tauri/src/organize_commands.rs`, `apps/desktop/src/Organize.test.tsx` |
| ORG-06 | M1 | Platform, koleksiyon, yerel durum, iş durumu, süre, dil ve tarihe göre filtreleme sağlanm… | MT-08 | henüz yok |
| ORG-09 | M1 | Etiket yeniden adlandırma, silme ve birleştirme desteklenmeli. Etkilenen öğelerin SearchD… | AC-39, AC-46 | `apps/desktop/src/Organize.test.tsx` |
| SEA-01 | M1 | SQLite FTS5 ile başlık, açıklama, kullanıcı notu, etiket ve kaynak/üretici metinleri aran… | AC-05, AC-15 | `apps/desktop/e2e/journeys.spec.ts`, `crates/vv-persistence/src/library/search.rs`, `crates/vv-persistence/src/portable.rs`, `crates/vv-persistence/tests/portable.rs`, `crates/vv-persistence/tests/search.rs` |
| SEA-02 | M1 | Metin sorgusu ve yapılandırılmış filtre birlikte uygulanmalı; seçili filtreler kaldırılab… | MT-09 | `crates/vv-persistence/tests/search.rs` |
| SEA-03 | M1 | Sonuçlar başlık, ilgili snippet ve eşleşme kaynağını göstermeli; eşleşen metin güvenli bi… | MT-10, AC-28 | `apps/desktop/src/App.test.tsx`, `crates/vv-persistence/src/library/search.rs` |
| SEA-04 | M1 | Boş sorgu filtrelenmiş kütüphane gibi çalışmalı; bozuk FTS sözdizimi anlaşılır biçimde el… | AC-31 | `apps/desktop/src-tauri/src/library_commands.rs`, `crates/vv-persistence/src/library/search.rs`, `crates/vv-persistence/tests/search.rs` |
| SEA-05 | M1 | Kaydetme, not ve etiket değişiklikleri tamamlandıktan sonra yeni aramada görünmeli. | AC-05, AC-39 | `apps/desktop/e2e-desktop/organize.spec.ts`, `apps/desktop/src/Organize.test.tsx`, `crates/vv-import/tests/import.rs`, `crates/vv-persistence/tests/search.rs` |
| SEA-06 | M4 | Transkript sonuçları öğe altında gruplanmalı; gerçek segment zamanına açılmalı. | AC-22 | henüz yok |
| SEA-07 | M5 | Çok dilli embedding ve lexical sonuçlar açıklanabilir hibrit sıralama ile birleşmeli. | AC-40 | henüz yok |
| SEA-08 | M5 | OCR ve görsel eşleşmeler konuşma eşleşmesinden ayrı etiketlenmeli. | AC-41 | henüz yok |
| DL-01 | M2 | Kullanıcı izinli içeriğin formatlarını inceleyip anlaşılır kalite önayarlarıyla indirme b… | AC-08, MT-22 | `crates/vv-engine/src/download.rs`, `crates/vv-engine/src/formats.rs`, `crates/vv-engine/tests/engine.rs` |
| DL-02 | M2 | Format seçimi gerçek format verisine dayanmalı; bilinmeyen boyut/süre tahmini kesin değer… | AC-09 | `apps/desktop/src-tauri/src/download_commands.rs`, `apps/desktop/src/Downloads.test.tsx`, `crates/vv-engine/src/download.rs`, `crates/vv-engine/src/formats.rs`, `crates/vv-engine/tests/engine.rs` |
| DL-03 | M2 | Metadata, download, merge, thumbnail ve indexing işleri ayrılmalı; bağımlılıkları ve duru… | AC-08, AC-58 | `crates/vv-engine/src/download.rs`, `crates/vv-engine/tests/engine.rs`, `crates/vv-persistence/tests/jobs.rs` |
| DL-04 | M2 | Kuyruk SQLite'da kalıcı olmalı; yeniden başlatmada yarım işler güvenli biçimde toparlanma… | AC-11, MT-28 | `crates/vv-engine/tests/engine.rs`, `crates/vv-persistence/tests/jobs.rs` |
| DL-05 | M2 | Makine tarafından okunabilir JSON/progress çıktısı kullanılmalı; stdout/stderr sınırsız b… | AC-08, AC-09 | `apps/desktop/src-tauri/src/download_commands.rs`, `apps/desktop/src/Downloads.test.tsx`, `crates/vv-engine/src/download.rs`, `crates/vv-engine/src/proc.rs`, `crates/vv-engine/tests/engine.rs`, `crates/vv-ytdlp/src/lines.rs`, `crates/vv-ytdlp/src/metadata.rs` |
| DL-06 | M2 | İptal, yeniden deneme ve desteklenen işlerde devam ettirme bulunmalı. Pause yalnızca doğr… | AC-10, AC-11 | `crates/vv-engine/src/download.rs`, `crates/vv-engine/tests/engine.rs`, `crates/vv-persistence/tests/jobs.rs` |
| DL-07 | M2 | Dosya tamlığı doğrulanmadan tamamlandı denmemeli; birleştirme sürerken %100 başarı görünm… | AC-08, AC-10 | `crates/vv-engine/src/download.rs`, `crates/vv-engine/tests/engine.rs` |
| DL-08 | M2 | Disk dolması, oturum, bölge, silinmiş içerik ve çıkarıcı hataları ayrı aksiyonlarla göste… | AC-12, AC-19, MT-23, MT-24, MT-26 | `crates/vv-engine/src/download.rs`, `crates/vv-engine/src/storage.rs`, `crates/vv-engine/tests/engine.rs`, `crates/vv-persistence/tests/auth.rs`, `crates/vv-persistence/tests/classify.rs` |
| DL-09 | M2 | Yönetilen yt-dlp/FFmpeg/ffprobe sürümü ve kaynağı diagnostics'te görünmeli. | MT-11 | `apps/desktop/src-tauri/src/download_commands.rs`, `apps/desktop/src/Downloads.test.tsx` |
| DL-10 | M2 | Dosya yolu, klasörde göster ve güvenli temizleme seçenekleri sunulmalı. | MT-12, AC-37 | henüz yok |
| PLAY-01 | M1 | Yerel dosya oynatma, ağdan bağımsız çalışmalı; seek, play/pause ve ses denetimleri erişil… | MT-13, AC-14 | `apps/desktop/e2e/player.spec.ts`, `apps/desktop/src-tauri/src/player_commands.rs` |
| PLAY-02 | M1 | Son izleme konumu ve devam etme eylemi kalıcı olmalı. | AC-49 | `apps/desktop/e2e/player.spec.ts`, `apps/desktop/src-tauri/src/player_commands.rs`, `apps/desktop/src-tauri/tests/ipc.rs`, `apps/desktop/src/Data.test.tsx`, `crates/vv-persistence/src/library/playback.rs`, `crates/vv-persistence/tests/portable.rs` |
| PLAY-03 | M1 | Kaynak URL ile yerel dosya durumu açıkça ayrılmalı. Yerel dosyası olmayan öğe için kaynak… | MT-14 | `apps/desktop/src-tauri/src/player_commands.rs`, `apps/desktop/src/Data.test.tsx` |
| PLAY-04 | M1 | Desteklenmeyen codec/container açıklanmalı; import başarısı oynatılabilirlik garantisi sa… | AC-32, MT-27 | `apps/desktop/e2e/player.spec.ts`, `apps/desktop/src-tauri/src/player_commands.rs`, `apps/desktop/src/Data.test.tsx`, `crates/vv-domain/src/error.rs` |
| PLAY-06 | M4 | Transkript tıklaması ilgili milisaniye ofsetine seek isteği göndermeli. | AC-22 | henüz yok |
| PLAY-07 | M1 | Uyumlu codec'lere sahip ama desteklenmeyen container'daki dosyalar (M0 oynatma matrisinde… | AC-32 | `apps/desktop/e2e/player.spec.ts`, `apps/desktop/src/Data.test.tsx` |
| DATA-01 | M1 | Sürümlü JSON ile metadata dışa/içe aktarılmalı; kimlik bilgileri ve çerezler dışarı çıkma… | AC-15, AC-18 | `apps/desktop/e2e/settings.spec.ts`, `crates/vv-engine/tests/engine.rs`, `crates/vv-persistence/src/portable.rs`, `crates/vv-persistence/tests/auth.rs`, `crates/vv-persistence/tests/portable.rs` |
| DATA-02 | M1 | Kullanıcı notları, etiketler, koleksiyonlar, kaynaklar ve medya referansları geri yüklene… | AC-15 | `crates/vv-persistence/src/portable.rs`, `crates/vv-persistence/tests/portable.rs` |
| DATA-03 | M1 | Eksik medya dosyaları yeniden bağlanabilmeli; portable metadata export, medya yedeği diye… | AC-15, AC-35 | `apps/desktop/e2e/player.spec.ts`, `apps/desktop/src/Data.test.tsx`, `crates/vv-import/tests/import.rs`, `crates/vv-persistence/src/portable.rs`, `crates/vv-persistence/tests/portable.rs` |
| DATA-04 | M1 | Yerel arama, düzenleme, mevcut indeksler ve yerel oynatma ağ olmadan kullanılabilmeli. | AC-01, AC-17, MT-25 | `apps/desktop/e2e-desktop/add-url.spec.ts`, `apps/desktop/e2e-desktop/offline-flow.spec.ts`, `apps/desktop/e2e/settings.spec.ts`, `apps/desktop/src/Data.test.tsx`, `crates/vv-persistence/tests/bookmark.rs` |
| DATA-05 | M1–M4 | Açık Offline Mode, metadata/indirme/model/güncelleme ağ isteklerini ve backend'in uzak kü… | AC-17 | `apps/desktop/e2e/settings.spec.ts`, `apps/desktop/src-tauri/src/data_commands.rs`, `apps/desktop/src-tauri/src/player_commands.rs`, `apps/desktop/src/Data.test.tsx`, `crates/vv-persistence/src/settings.rs` |
| DATA-06 | M1 | Hatalı/gelecek şemalı import mevcut veriyi bozmamalı; ön doğrulama ve sonuç raporu bulunm… | AC-16 | `apps/desktop/e2e/settings.spec.ts`, `crates/vv-persistence/src/portable.rs`, `crates/vv-persistence/tests/portable.rs` |
| DATA-07 | M2 | Depolama kotası, boş alan uyarısı, model ve türetilmiş dosya kullanımı görünür olmalı. | AC-33 | `apps/desktop/src-tauri/src/download_commands.rs`, `apps/desktop/src/Downloads.test.tsx`, `crates/vv-engine/src/storage.rs`, `crates/vv-engine/tests/engine.rs` |
| DATA-08 | M2 | Medya kökü ayarlardan değiştirildiğinde managed dosyalar için “taşı” veya “yalnızca yeni… | AC-43 | `apps/desktop/src-tauri/src/download_commands.rs`, `crates/vv-engine/src/storage.rs`, `crates/vv-engine/tests/storage_move.rs` |
| DATA-09 | M1 | Medya kökü veya dış referansların bulunduğu sürücü erişilemezse (ör. çıkarılmış harici di… | AC-47 | `apps/desktop/src-tauri/src/library.rs`, `crates/vv-engine/src/storage.rs`, `crates/vv-import/tests/import.rs`, `crates/vv-persistence/tests/library_view.rs` |
| NFR-01 | M0–M4 | Bookmark kaydı: Metadata beklemeden yerel commit ve görünür sonuç için p95 ≤ 500 ms; 10.0… | BM-01 | `apps/desktop/src-tauri/tests/save_latency.rs`, `crates/vv-benchdata/tests/determinism.rs` |
| NFR-02 | M0–M4 | Kullanıcı akışı: Hazır URL yapıştırıldıktan sonra ≤ 2 birincil eylem; yönlendirilmiş kull… | MT-17 | `apps/desktop/e2e-desktop/add-url.spec.ts`, `apps/desktop/e2e/add-url.spec.ts` |
| NFR-03 | M0–M4 | Lexical arama: Sıcak indekste 10.000 öğe için backend p95 ≤ 200 ms; 100 ms debounce dahil… | BM-02 | `crates/vv-benchdata/tests/determinism.rs` |
| NFR-04 | M0–M4 | Döküm araması: M4'te 100.000 segmentli fixture için p95 ≤ 500 ms başlangıç hedefi; ölçüm… | BM-03 | `crates/vv-benchdata/tests/determinism.rs` |
| NFR-05 | M0–M4 | UI tepkisi: Ağ/medya/AI işi sırasında gezinme ve iptal denetimleri yanıt verir; büyük lis… | MT-15 | henüz yok |
| NFR-06 | M0–M4 | Başlangıç: Referans cihazda 10.000 öğeli kütüphanenin temel etkileşime hazır olması için… | BM-04 | henüz yok |
| NFR-07 | M0–M4 | Bellek: AI kapalı temel kütüphane için ölçülen idle/scroll bellek değeri raporlanır; başl… | BM-05 | henüz yok |
| NFR-08 | M0–M4 | Veri dayanıklılığı: Başarılı commit edilmiş bookmark/not/ilişkiler force-close sonrasında… | AC-05, AC-11 | `crates/vv-engine/tests/engine.rs`, `crates/vv-persistence/tests/search.rs` |
| NFR-09 | M0–M4 | İptal: İptal talebi UI'da hemen görünür; normal koşullarda süreç duruşu ≤ 5 s hedef, zoru… | AC-10, MT-15 | `crates/vv-engine/src/download.rs`, `crates/vv-engine/tests/engine.rs` |
| NFR-10 | M0–M4 | Seek: Backend doğru segment ofsetini gönderir; uyumlu test medyasında oynatıcı hedefi ±50… | AC-22 | `apps/desktop/e2e/player.spec.ts` |
| NFR-11 | M0–M4 | Gizlilik: Offline Mode testinde uygulama ve yönettiği süreçlerden dış bağlantı yok; log/D… | AC-17, AC-18 | `apps/desktop/e2e/settings.spec.ts`, `apps/desktop/src/Data.test.tsx`, `crates/vv-engine/tests/engine.rs`, `crates/vv-persistence/tests/auth.rs` |
| NFR-12 | M0–M4 | Erişilebilirlik: Temel akışlar klavyeyle tamamlanır; kritik otomatik erişilebilirlik bulg… | AC-27 | `apps/desktop/e2e/a11y.spec.ts` |
| NFR-13 | M0–M4 | Paketleme: Temiz Windows 11 hedefinde son kullanıcı Python/Node/Rust kurmadan kurulum ve… | AC-26, MT-20, MT-21 | henüz yok |
| NFR-14 | M0–M4 | Uyumluluk: Windows x64 ilk paket hedefi önerilir; ARM64/Windows 10/macOS/Linux desteği ay… | MT-18 | henüz yok |

### Kimlikli manuel testler (MT)

| Kimlik | Test | Adımlar | Otomasyon |
|---|---|---|---|
| MT-01 | Add URL ve Ctrl+V ile kayıt; pano arka planda okunmuyor | Global Add URL eylemiyle ve listede odak varken Ctrl+V ile URL gir. Uygulama arka plandayken panoya URL kopyala: hiçbir şey kaydedilmez ve pano okunmaz. | elle |
| MT-02 | Etiketsiz kayıt Inbox'a düşer | Etiket/koleksiyon seçmeden kaydet; öğe Inbox'ta. Etiket ve koleksiyon seçerek kaydet; öğe ikisinde de görünür. | elle |
| MT-03 | Güvenli metadata önizlemesi; Download ayrı eylem | Desteklenen bir public URL kaydet: önizleme gösterilir, indirme kendiliğinden başlamaz; Download ayrı düğmedir. | elle |
| MT-04 | Dosya seçici, klasör seçici, sürükle-bırak ve klavye alternatifi | Üç yolla da import et; sürükle-bırakın klavyeyle erişilebilir karşılığı olduğunu doğrula. | elle |
| MT-05 | Import edilen medyanın teknik bilgisi ffprobe çıktısıyla aynı | tests/fixtures/media dosyalarını import et; süre, container, codec, boyut, çözünürlük ve ses parçası bilgisini tests/fixtures/ffmpeg beklenen değerleriyle karşılaştır. | elle |
| MT-06 | Küçük resim arka planda; eksik küçük resim kaydı ve aramayı engellemiyor | Küçük resmi üretilemeyen bir dosya import et: öğe hemen kayıtlı ve aranabilir, küçük resim yer tutucu. | elle |
| MT-07 | Bir öğe birden çok koleksiyon ve etikette | Öğeyi iki koleksiyona ve üç etikete bağla, uygulamayı kapat/aç: tüm bağlar duruyor ve filtrelerde görünüyor. | elle |
| MT-08 | Filtreler: platform, koleksiyon, yerel durum, iş durumu, süre, dil, tarih | 50 öğelik fixture kütüphanede her filtreyi tek tek ve birlikte uygula; sonuç sayılarını fixture beklentisiyle karşılaştır. | elle |
| MT-09 | Metin sorgusu ve filtre birlikte; filtreler kaldırılabilir chip | Sorgu + iki filtre uygula, chip'leri tek tek kaldır; sonuçlar her adımda doğru. | elle |
| MT-10 | Sonuçta başlık, snippet ve eşleşme kaynağı; vurgulama güvenli | Not, etiket ve başlıkta eşleşen sorgular çalıştır: eşleşme kaynağı doğru etiketli; HTML içeren metin kod olarak çalışmıyor. | elle |
| MT-11 | Diagnostics'te yönetilen araçların sürümü ve kaynağı | Settings → Diagnostics: yt-dlp, Deno, FFmpeg, ffprobe sürümü ve tools/manifest.json kaynağı görünür; yol veya kişisel veri yok. | elle |
| MT-12 | Dosya yolu, klasörde göster ve güvenli temizleme | İndirilmiş öğede yol görünür, klasörde göster çalışır; temizleme yalnızca o öğenin managed dosyasını önerir ve onay ister. | elle |
| MT-13 | Oynatıcı denetimleri klavye ve ekran okuyucuyla erişilebilir | Yerel dosyada play/pause, seek ve ses denetimini yalnızca klavyeyle kullan; Narrator denetim adlarını okur. Ağ kapalıyken de çalışır. | elle |
| MT-14 | Kaynak URL ile yerel dosya durumu ayrı; kaynakta aç | Bookmark-only öğe 'kaynakta aç' sunar, oynat sunmaz; indirilmiş öğe ikisini ayrı gösterir. | elle |
| MT-15 | Uzun iş sırasında UI yanıt veriyor | Uzun bir remux/indirme sürerken listede gezin, ara ve iptal et: etkileşim takılmıyor (NFR-05), iptal hemen görünür (NFR-09). | elle |
| MT-16 | Webview yalnızca izinli komutları çağırabiliyor | Capability dışındaki core/plugin komutlarını ve dosya/shell erişimini webview'dan dene: hepsi reddedilir. | `apps/desktop/e2e-desktop/desktop-shell.spec.ts`, `apps/desktop/src-tauri/tests/ipc.rs` |
| MT-17 | Yönlendirilmiş kullanım testi: hazır URL ≤ 2 eylem, ~5 s | Üç kişiyle panodaki URL'yi kaydetme süresini ve eylem sayısını ölç. | elle |
| MT-18 | Desteklenen platform ve sürüm listesi tarihli kanıta dayanıyor | docs/platform-support.md'deki her 'destekleniyor' satırı için tarih, ortam ve kanıt yolu var; olmayanlar 'test edilmedi'. | elle |
| MT-20 | Temiz Windows 11 kurulumu | §25.4: temiz VM'de kur, aç, temel akışlar, kaldır. | elle |
| MT-21 | Python/Node/Rust kurulu olmayan makinede kullanım | §25.4. | elle |
| MT-22 | İzinli public URL ile platform testi | §25.4: izinli, mevcut içerikle; sonuç docs/platform-support.md'ye tarihli yazılır. | elle |
| MT-23 | Silinmiş video | §25.4: kaynak silinmiş URL; ayrı hata ve aksiyon, bookmark korunur. | elle |
| MT-24 | Bölge kısıtlı video | §25.4: bölge hatası ayrı gösterilir. | elle |
| MT-25 | Ağ yok | §25.4: ağ kapalıyken kaydet, ara, oynat. | elle |
| MT-26 | Disk dolu | §25.4: küçük bir sürücüye indirme. | elle |
| MT-27 | Bozuk medya | §25.4: bozuk dosya import ve oynatma. | elle |
| MT-28 | İndirme sırasında yeniden başlatma | §25.4: indirme ortasında uygulamayı kapat/aç. | elle |
| MT-29 | Süresi dolmuş oturum (M3) | §25.4 ve §21.2. | elle |
| MT-30 | Yetkili özel VK videosu (M3) | §25.4 ve §21.2; yalnızca gerçek yetkili testle. | elle |

### Benchmark'lar (BM)

| Kimlik | Ölçüm | NFR | Çalıştıran |
|---|---|---|---|
| BM-01 | Bookmark kaydı p95, 10.000 öğe | NFR-01 | scripts/bench.mjs (crates/vv-benchdata) |
| BM-02 | Lexical arama p95, 10.000 öğe, sıcak indeks | NFR-03 | scripts/bench.mjs (crates/vv-benchdata) |
| BM-03 | Döküm araması p95, 100.000 segment | NFR-04 | scripts/bench.mjs (crates/vv-benchdata) |
| BM-04 | Başlangıç süresi, 10.000 öğe, soğuk ve sıcak | NFR-06 | not run: F1 |
| BM-05 | Idle ve scroll belleği, AI kapalı | NFR-07 | not run: F1.7 |

<!-- coverage:end -->
