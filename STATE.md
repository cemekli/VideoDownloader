# VideoVault — Proje Durumu

| Bilgi | Değer |
|---|---|
| Son güncelleme | 9 Kasım 2026 |
| Referans belge | [SPEC.md](./SPEC.md) v1.9 |
| Aktif faz | **F5 — Yetkili oturum** (F0–F3 ✅, F4 ertelendi) |
| Genel ilerleme | 45 / 74 alt görev doğrulandı |
| Tek komutla doğrulama | `npm run verify` → `test-artifacts/verify/report.md` |
| Son doğrulama | 9 Kasım 2026: GitHub Actions (windows-latest + ubuntu-latest) tüm kapılar geçti, [CI #42674665024](https://github.com/cemekli/VideoDownloader/actions/runs/42674665024) (PR #35). |

## Bu dosya nasıl kullanılır

Bu dosya projenin tek durum kaynağıdır. Her faz bir **çıktı** üretir ve bir **çıkış kapısı** ile kapanır. Her alt görevin kimliği (ör. `F1.4`), SPEC bağı, nasıl doğrulanacağı ve durumu vardır.

**Durum işaretleri**

| İşaret | Anlam |
|---|---|
| ⬜ | Başlanmadı |
| 🟨 | Devam ediyor veya kısmen doğrulandı |
| ✅ | Doğrulandı: kanıtı “Doğrulama” sütununda yazılı |
| ⛔ | Engelli: neden “Engeller” tablosunda |
| ⏭️ | Ertelendi: karar ve yeni faz yazılı |

**Kurallar**

1. Bir alt görev yalnızca kanıtla ✅ olur: geçen otomatik test, `verify` raporu veya tarihli manuel test kaydı. “Kod yazıldı” tek başına ✅ değildir (SPEC §25.4, §27.1).
2. Bir faz, tüm P0 alt görevleri ✅ olduğunda ve çıkış kapısındaki her madde işaretlendiğinde kapanır.
3. Canlı erişim veya cihaz gerektiren testler yapılamıyorsa “not run” yazılır; fixture sonucu canlı başarı sayılmaz.
4. Her alt görev kendi otomatik testleriyle birlikte biter. Yeni test, ilgili AC kimliğini test adında veya yorumunda taşır.
5. SPEC değişirse etkilenen alt görevler burada güncellenir.

## Faz özeti

| Faz | Hedef | SPEC | Çıktı | Durum |
|---|---|---|---|---|
| F0 | Temel ve test altyapısı | M0 | Windows'ta açılan boş uygulama, yeşil CI, ilk ADR'ler | ✅ |
| F1 | Çevrimdışı kütüphane çekirdeği | M1 (1/2) | Ağsız kaydet, import et, ara | ✅ |
| F2 | Düzenleme, oynatma, taşınabilirlik | M1 (2/2) | İndirmesiz tam Windows kütüphanesi | ✅ |
| F3 | İndirme motoru ve kalıcı kuyruk | M2 (1/2) | İzinli videoyu indirip oynatan uygulama | ✅ |
| F4 | Windows dağıtımı ve MVP sürümü | M2 (2/2) | İmzalı installer ile Windows MVP | ⏸ F5'ten sonraya ertelendi |
| F5 | Yetkili oturum | M3 | VK araştırma raporu ve varsa desteklenen yöntem | 🟨 başladı (F5.1) |
| F6 | Transkript ve zaman kodlu arama | M4 | AI V2 sürümü | 🟨 |
| F7 | Anlamsal ve görsel arama | M5 | Hibrit ve görsel arama | ⬜ |
| F8 | Android | M6 | Android uygulaması | ⬜ |

## Test altyapısı

Her faz bu altyapının üzerine test ekler. Komutlar depo kökünden çalışır.

| Katman | Araç | Konum | Komut | Durum |
|---|---|---|---|---|
| Fixture güvenliği | `scripts/verify.mjs` | `tests/fixtures/` | `npm run verify -- --only repo` | ✅ çalışıyor |
| Rust birim testleri | `cargo test` | `crates/*/src` | `npm run test:rust` | ✅ çalışıyor |
| Rust fixture testleri | `cargo test` + paylaşılan JSON | `crates/*/tests`, `tests/fixtures/` | `npm run test:rust` | ✅ çalışıyor |
| Tauri IPC entegrasyonu | `tauri::test` (MockRuntime) + gerçek ACL ve origin denetimi | `apps/desktop/src-tauri/tests/ipc.rs` | `npm run test:rust` | ✅ çalışıyor |
| IPC sözleşmesi | paylaşılan JSON; Rust ve Vitest aynı dosyayı kullanır | `tests/fixtures/ipc-contract.json` | `npm run verify` | ✅ çalışıyor |
| Rust stil ve lint | `cargo fmt`, `cargo clippy -D warnings` | tüm crate'ler | `npm run verify -- --only rust` | ✅ çalışıyor |
| TypeScript tip kontrolü | `tsc --noEmit` | `apps/desktop` | `npm run verify -- --only web` | ✅ çalışıyor |
| UI birim/bileşen testleri | Vitest + Testing Library (jsdom) | `apps/desktop/src/**/*.test.tsx` | `npm run test:web` | ✅ çalışıyor |
| E2E (fixture IPC) | Playwright; 1366×768, 1920×1080, 800 px | `apps/desktop/e2e/` | `npm run e2e` | ✅ çalışıyor |
| Görsel inceleme | Playwright ekran görüntüleri | `test-artifacts/screenshots/` | `npm run e2e` | ✅ çalışıyor |
| CI | GitHub Actions, Windows + Linux | `.github/workflows/ci.yml` | push/PR | ✅ ilk çalışma yeşil |
| SQLite migration/integration | `cargo test` + geçici DB dosyaları | `crates/vv-persistence/tests/` | `npm run test:rust` | ✅ çalışıyor |
| yt-dlp çıktı fixture'ları | gerçek yt-dlp 2026.10.26 çalışmaları (yerel sunucu) + kaynak metinleri | `crates/vv-ytdlp/tests/`, `tests/fixtures/ytdlp/` | `npm run test:rust`; yeniden yakalama `npm run fixtures:ytdlp` | ✅ çalışıyor |
| Gerçek uygulama E2E | Playwright + WebView2 CDP, test exe'si (`e2e-cdp`) | `apps/desktop/e2e-desktop/` | `npm run desktop:e2e-exe` + `npm run e2e:desktop` | ✅ çalışıyor (yalnızca Windows) |
| Güvenlik testleri | canary secret, kötü niyetli URL/yol | `tests/fixtures/` + Rust | — | 🟨 URL politikası ve yt-dlp stderr redaksiyonu (CANARY) var; gerisi F3–F4 |
| Erişilebilirlik | Playwright + axe | `apps/desktop/e2e/` | — | ⬜ F2.9 |
| Performans | sürümlü veri üreticisi + benchmark | `tests/perf/` | — | ⬜ F0.12 |
| Kapsama matrisi | P0 gereksinim ↔ AC ↔ test | `docs/testing.md` | — | ⬜ F0.11 |

Önemli sınır: `e2e/` testleri fixture IPC ile çalışır ve gerçek IPC'yi kanıtlamaz. Gerçek pencere, gerçek IPC, yetki ve CSP `e2e-desktop/` ile yalnızca Windows'ta test edilir; Linux'ta gerçek IPC yolu `tests/ipc.rs` ile sahte pencereyle test edilir. Windows alt süreçleri `crates/vv-process/tests/process.rs` ile test edilir (F0.10, ADR-0008).

---

## F0 — Temel ve test altyapısı (M0)

**Hedef:** Tüm sonraki fazların üzerine kurulacağı iskeleti, test altyapısını ve M0 teknik kararlarını hazırlamak.

**Çıktı:** Windows 11'de açılan boş VideoVault penceresi, Windows ve Linux'ta yeşil CI, `docs/` altındaki zorunlu dokümanlar ve DEC-01…DEC-06 için ADR'ler.

| ID | Alt görev | SPEC bağı | Doğrulama | Durum |
|---|---|---|---|---|
| F0.1 | Depo iskeleti: Rust workspace, `vv-domain` crate, React + TypeScript + Vite arayüzü | §10.1, §10.3 | `npm run verify` 8/8 (6 Kas 2026, Linux) | ✅ |
| F0.2 | Test altyapısı: Vitest, Playwright (3 görünüm), paylaşılan fixture'lar, `verify` betiği, fixture sır taraması, CI tanımı | §25.1, §25.2, §25.4 | `npm run verify` 8/8; sır taraması sahte bir sızıntıyı yakaladı | ✅ |
| F0.3 | Tauri 2 kabuğu, tipli `ping`/`status` komutu, gerçek Tauri IPC istemcisi | §10.1, §18 | Windows 11'de pencere açılır; IPC entegrasyon testi geçer | ✅ IPC testleri 9/9 (Windows + Linux), Vitest 17/17; [CI #42261547079](https://github.com/cemekli/VideoDownloader/actions/runs/42261547079) Windows açılış testi: pencere “VideoVault” açık kaldı; 6 Kas 2026 Windows 11'de elle açıldı, “Desktop v0.0.1” gösterdi |
| F0.4 | Gerçek uygulama üzerinde E2E altyapısı seçimi ve ilk test | §25.1 | ADR + gerçek IPC ile bir E2E testi | ✅ ADR-0001 (`docs/decisions.md`); `e2e-desktop` 4/4 Windows'ta gerçek exe ile: durum rozeti, `ping`, yetki dışı komut reddi, CSP; [CI #42271413057](https://github.com/cemekli/VideoDownloader/actions/runs/42271413057) |
| F0.5 | SQLite, migration altyapısı, WAL kararı, paketlenen SQLite'ta FTS5 testi | §11.7, §16.1, DEC-06 | migration ileri/geri testi; FTS5 testi paketlenen derlemede geçer | ✅ ADR-0002; `vv-persistence` 16/16 (Windows + Linux); gerçek exe'de `packaged SQLite 3.53.2, FTS5=true, schema v1`, `e2e-desktop` 5/5; [CI #42275530245](https://github.com/cemekli/VideoDownloader/actions/runs/42275530245) |
| F0.6 | yt-dlp fixture adaptörü: JSON, progress ve hata çıktısı ayrıştırma | DL-05, §14.1 | sabit çıktılarla integration testleri | ✅ ADR-0003; `vv-ytdlp` 31/31 (Windows + Linux): 15 gerçek yt-dlp 2026.10.26 çalışması + 20 kaynak metni vakası; [CI #42281072226](https://github.com/cemekli/VideoDownloader/actions/runs/42281072226) |
| F0.7 | Yakalama URL politikası: şema, kimlik bilgisi, özel/yerel IP reddi | CAP-02, §20.2, AC-29 (sözdizimsel kısım) | `url-policy.json` fixture'ı 33 vaka + 4 birim testi geçer | ✅ |
| F0.8 | Araç ve lisans araştırması: yt-dlp + JS runtime, FFmpeg build, WebView2 codec/container matrisi. Üç PR: **a** oynatma matrisi (DEC-04), **b** yt-dlp + JS runtime (DEC-02), **c** FFmpeg (DEC-03) | DEC-02, DEC-03, DEC-04 | `docs/decisions.md` ADR'leri ve gerçek dosyalarla matris | ✅ a ✅: ADR-0004; WebView2 156'da 14 dosyanın 11'i oynuyor, HEVC yalnızca sesle (`partial`); MP4 H.264/AAC zorunlu testi geçti; [CI #42361348985](https://github.com/cemekli/VideoDownloader/actions/runs/42361348985). b ✅: ADR-0005; `vv-tools` 8/8; `tools/manifest.json`; gerçek yt-dlp.exe + deno.exe Windows'ta doğrulandı, PATH'teki Deno yok sayılıyor; [CI #42365085585](https://github.com/cemekli/VideoDownloader/actions/runs/42365085585). c ✅: ADR-0006; BtbN FFmpeg 9.0.2 LGPL; `vv-ffmpeg` 6/6, `vv-tools` 9/9; izin listeleriyle HLS/concat/URL reddi (0 HTTP isteği); 20 dk'lık MKV remux Windows'ta 609 ms; gerçek exe'lerle Windows ve Linux'ta [CI #42367817432](https://github.com/cemekli/VideoDownloader/actions/runs/42367817432) |
| F0.9 | Dış araçların ağ denetimi uygulanabilirliği | §20.2, DEC-05 | ADR + redirect/DNS testleri | ✅ ADR-0007; yerel denetim proxy'si `vv-netguard` 7/7; gerçek yt-dlp ile 12 vakada kanaryaya 0 bağlantı (kontrolde 1); Windows ve Linux [CI #42369808856](https://github.com/cemekli/VideoDownloader/actions/runs/42369808856) |
| F0.10 | Single-instance kilidi ve Windows Job Object ile alt süreç yönetimi | INV-13, §13.2 | AC-38 + Windows smoke testi | ✅ ADR-0008; kütüphane kilidi, `vv-process` (temiz ortam, Job Object, ağaç öldürme; Windows çökme testinde sahipsiz süreç yok), single-instance eklentisi; AC-38 gerçek uygulama E2E 3/3 (ikinci açılış 0 koduyla 22 ms'de, kilit katmanı 3 koduyla); Windows ve Linux [CI #42373404361](https://github.com/cemekli/VideoDownloader/actions/runs/42373404361) |
| F0.11 | Zorunlu dokümanlar ve kapsama matrisi | §27 | `docs/architecture.md`, `decisions.md`, `threat-model.md`, `roadmap.md`, `platform-support.md`, `testing.md` mevcut | ✅ Altı doküman da var (`architecture.md`, `roadmap.md`, `platform-support.md` yeni; `threat-model.md` §20.1 durumuyla tamamlandı). Kapsama matrisi: `docs/coverage.json` → `docs/testing.md`, 82/82 P0/INV/NFR bağlı; `verify` adımı `coverage-matrix` eksik bağı ve eskimiş tabloyu reddediyor; Windows ve Linux [CI #42376811094](https://github.com/cemekli/VideoDownloader/actions/runs/42376811094) |
| F0.12 | Benchmark ortamı ve sürümlü veri üreticisi (10.000 öğe, 100.000 segment) | NFR-01…07, DEC-17 | aynı seed ile aynı çıktı testi | ✅ ADR-0009; `vv-benchdata` (kendi PRNG'si, golden SHA-256), `npm run bench` (BM-01…03, makine sınıflaması), referans makine ve yöntem `docs/testing.md`'de; 7 test; Windows ve Linux [CI #42376811094](https://github.com/cemekli/VideoDownloader/actions/runs/42376811094) |
| F0.13 | Git deposu ve CI'ın Windows'ta ilk yeşil çalışması | §25.1 | GitHub Actions'ta iki işletim sistemi de geçer | ✅ [CI #42253170546](https://github.com/cemekli/VideoDownloader/actions/runs/42253170546) |

**Çıkış kapısı**

- [x] `npm run verify` Windows ve Linux CI'da geçiyor (CI Windows Server 2025; Windows 11'de yerel `verify` henüz çalıştırılmadı, engeller tablosu).
- [x] Windows 11'de Tauri penceresi açılıyor (F0.3, elle), gerçek IPC çağrısı test ediliyor (F0.4, CI).
- [x] DEC-01…DEC-06 kararları kanıtıyla `docs/decisions.md` içinde (ADR-0010, ADR-0005, ADR-0006, ADR-0004, ADR-0007, ADR-0002).
- [x] Zorunlu dokümanlar ve kapsama matrisi mevcut (F0.11).

---

## F1 — Çevrimdışı kütüphane çekirdeği (M1, 1. yarı)

**Hedef:** Ağ olmadan URL kaydetme, yerel dosya içe aktarma ve arama.

**Çıktı:** URL kaydedip yerel video içe aktaran, ikisinde de arama yapan ve yeniden başlatmada veriyi koruyan Windows uygulaması.

| ID | Alt görev | SPEC bağı | Doğrulama | Durum |
|---|---|---|---|---|
| F1.1 | `LibraryItem`, `MediaAsset`, `StorageRoot`, `UrlAlias`, `DuplicateCandidate` şeması ve repository'ler | §11 | migration ve repository testleri: `vv-persistence/tests/library_schema.rs` (11 test; enum↔CHECK, unique, cascade, revision, veriyle ileri/geri migration). ADR-0011 ; Windows + Linux CI [CI #42382304235](https://github.com/cemekli/VideoDownloader/actions/runs/42382304235) | ✅ |
| F1.2 | Bookmark kaydı: atomik kayıt + `SearchDocument`, duplicate ve alias kontrolü | CAP-01…06, CAP-08 | AC-01, AC-02, AC-03, AC-36, AC-52: `vv-persistence/tests/bookmark.rs` (9 test; AC-45 sinyali dahil). CAP-01'in ekran kısmı F1.8'de ; Windows + Linux CI [CI #42382304235](https://github.com/cemekli/VideoDownloader/actions/runs/42382304235) | ✅ |
| F1.3 | Platform tanıma ve URL normalizasyonu, `normalization_version` | §6, §11.7 | normalizasyon fixture'ı (`tests/fixtures/normalize.json`, 33 vaka) + AC-44 (`vv-persistence/tests/renormalize.rs`) ; Windows + Linux CI [CI #42382304235](https://github.com/cemekli/VideoDownloader/actions/runs/42382304235) | ✅ |
| F1.4 | Yerel import: kısıtlı ffprobe, hash, duplicate adayı, thumbnail | IMP-01…07, §14.1 | AC-04, AC-34, AC-35, AC-59: `vv-import/tests/import.rs` (gerçek ffprobe/FFmpeg 9.0.2; thumbnail, iptal, link testleri dahil). Dosya seçici ve sürükle-bırak (IMP-01) F1.7'de. ADR-0012; Windows + Linux CI [CI #42385646501](https://github.com/cemekli/VideoDownloader/actions/runs/42385646501) | ✅ |
| F1.5 | Depolama kökü durumu: `missing` ve `disconnected` ayrımı | DATA-09 | AC-47: `vv-import/tests/import.rs` (kök çıkarılınca `disconnected`, varlıklar `missing` olmuyor, geri gelince doğrulanıyor); Windows + Linux CI [CI #42385646501](https://github.com/cemekli/VideoDownloader/actions/runs/42385646501) | ✅ |
| F1.6 | FTS5 arama, güvenli sorgu oluşturucu, sıralama, Türkçe arama fixture'ı | SEA-01…05, §16.1 | AC-05, AC-31, Türkçe fixture (`tests/fixtures/search-tr.json`, 21 vaka): `vv-persistence/tests/search.rs`. Filtre chip'leri (SEA-02 UI) F1.7'de; Windows + Linux CI [CI #42385646501](https://github.com/cemekli/VideoDownloader/actions/runs/42385646501) | ✅ |
| F1.7 | Library ekranı: grid/liste, inspector, filtreler, boş ve hata durumları | §9.2, §9.3 | Fixture E2E 3 görünümde (`e2e/library-shell.spec.ts`, 15 test × 3; ilk kullanım, yükleniyor, hata, sonuç yok, çevrimdışı, 2.000 öğe, inspector, liste ekran görüntüleri), Vitest (`App.test.tsx`, `selection.test.ts`), Rust (`library_view.rs`, `tests/ipc.rs`, `library_commands.rs`), gerçek uygulamada `list_items` (`desktop-shell.spec.ts`). ADR-0013. Windows'ta elle kontrol (MT-04) 8 Kas 2026'da geçti: Import → Files… ile dosya, sürükle-bırak ile klasör import edildi, küçük resimler geldi. | ✅ Kanıt: Cuma'nın Windows 11 `npm run verify` raporu (8 Kas 2026 15:12, gerçek uygulama E2E dahil hepsi geçti) + Linux `verify` + [CI #42491832081](https://github.com/cemekli/VideoDownloader/actions/runs/42491832081) Linux yeşil; Windows elle kontrol listesi 8 Kas 15:22'de geçti. |
| F1.8 | Add URL ekranı ve `Ctrl+K` | §8.1, §9.4 | Komutlar `check_url`, `save_url`, `restore_item` (`capture_commands.rs` birim testleri, `tests/ipc.rs` kontrat testleri). Yolculuk 1 fixture E2E 3 görünümde (`e2e/add-url.spec.ts`: iki birincil eylem, yapıştırmadan karta < 5 s, hata/duplicate/kaldırılmış durumları ve ekran görüntüleri), Vitest (`AddUrl.test.tsx`: Ctrl+K, Ctrl+V, çevrimdışı, AC-45 geri yükleme). NFR-01: 10.000 öğede p95 1,4 ms (`tests/save_latency.rs`, Linux debug). Gerçek uygulamada kaydet → yeniden başlat → ara (`e2e-desktop/add-url.spec.ts`, AC-01). ADR-0014. | ✅ Kanıt: Cuma'nın Windows 11 `npm run verify` raporu (8 Kas 2026 15:12, gerçek uygulama E2E dahil hepsi geçti) + Linux `verify` + [CI #42491832081](https://github.com/cemekli/VideoDownloader/actions/runs/42491832081) Linux yeşil; Windows elle kontrol listesi 8 Kas 15:22'de geçti. |

**Çıkış kapısı**

- [x] Ağ kapalıyken kaydet, import et, ara ve yeniden başlat akışı Windows'ta çalışıyor: `e2e-desktop/offline-flow.spec.ts` Windows'ta geçti, ağsız elle deneme 8 Kas 2026'da geçti.
- [x] Yukarıdaki AC'ler otomatik testte geçiyor (Linux ve Windows `verify`, 8 Kas 2026).
- [x] Yolculuk 1 (beş saniyede kaydet) ve 2 (eski bookmark'ı bul) için E2E testleri var: `e2e/add-url.spec.ts`, `e2e/journeys.spec.ts` (hatırlanan kelime, not içinden arama, filtre, ayrıntı, kaynak sayfa; yanlış kelimede "sonuç yok" ve tek adımda geri dönüş).

---

## F2 — Düzenleme, oynatma ve taşınabilirlik (M1, 2. yarı)

**Hedef:** Kütüphaneyi düzenleme, yerel oynatma, yedekleme ve çevrimdışı mod.

**Çıktı:** M1 tamam: indirme olmadan tam çalışan Windows kütüphanesi.

| ID | Alt görev | SPEC bağı | Doğrulama | Durum |
|---|---|---|---|---|
| F2.1 | Etiketler, koleksiyonlar, akıllı koleksiyonlar | ORG-01…10 | AC-06, AC-39, AC-46, AC-48 | ✅ Kanıt: Cuma'nın Windows 11 `npm run verify` raporu (8 Kas 2026 15:12, gerçek uygulama E2E dahil hepsi geçti) + Linux `verify` + [CI #42491832081](https://github.com/cemekli/VideoDownloader/actions/runs/42491832081) Linux yeşil; Windows elle kontrol listesi 8 Kas 15:22'de geçti. |
| F2.2 | Toplu işlemler, Undo, soft-delete, Recently removed, sahipsiz medya | §7.3 | AC-07, AC-37, AC-45, AC-53, AC-60 | ✅ Kanıt: Cuma'nın Windows 11 `npm run verify` raporu (8 Kas 2026 15:12, gerçek uygulama E2E dahil hepsi geçti) + Linux `verify` + [CI #42491832081](https://github.com/cemekli/VideoDownloader/actions/runs/42491832081) Linux yeşil; Windows elle kontrol listesi 8 Kas 15:22'de geçti. |
| F2.3 | Duplicate yönetimi ve önizlemeli birleştirme | ORG-11 | birleştirme ve Undo testleri | ✅ Kanıt: Cuma'nın Windows 11 `npm run verify` raporu (8 Kas 2026 15:12, gerçek uygulama E2E dahil hepsi geçti) + Linux `verify` + [CI #42491832081](https://github.com/cemekli/VideoDownloader/actions/runs/42491832081) Linux yeşil; Windows elle kontrol listesi 8 Kas 15:22'de geçti. |
| F2.4 | Oynatıcı, güvenli local protocol, kaldığı yerden devam, remux önizlemesi | PLAY-01…04, PLAY-07 | AC-32, AC-49, NFR-10 | ✅ ADR-0016. `vvmedia` protokolü (`tests/media_protocol.rs`), konum kuralı ve AC-49 yeniden başlatma (`vv-persistence`), `player_commands.rs` + `tests/ipc.rs`; fixture E2E `e2e/player.spec.ts` 3 görünümde (devam 0:02, seek ±500 ms, HEVC `partial`/`failed` + harici oynatıcı, bağlı olmayan sürücü), Vitest `Data.test.tsx`; gerçek uygulamada çevrimdışı oynatma `e2e-desktop/offline-flow.spec.ts`. PLAY-07 daraltıldı: matriste remux gerektiren container yok (ADR-0016). Kanıt: Cuma'nın Windows 11 `npm run verify` raporu (8 Kas 2026 15:12, gerçek uygulama E2E dahil hepsi geçti) + Linux `verify` + [CI #42491832081](https://github.com/cemekli/VideoDownloader/actions/runs/42491832081) Linux yeşil; Windows elle kontrol listesi 8 Kas 15:22'de geçti. |
| F2.5 | JSON export/import, kök eşleme, relink | DATA-01…03, DATA-06, §15 | AC-15, AC-16, AC-30, AC-51 | ✅ ADR-0017. `vv-persistence/tests/portable.rs` (AC-15 gidiş-dönüş, AC-16, AC-30 symlink/junction, AC-51, yer tutucu kök, çakışma politikası); `data_commands.rs`; arayüz `e2e/settings.spec.ts`. Kanıt: Cuma'nın Windows 11 `npm run verify` raporu (8 Kas 2026 15:12, gerçek uygulama E2E dahil hepsi geçti) + Linux `verify` + [CI #42491832081](https://github.com/cemekli/VideoDownloader/actions/runs/42491832081) Linux yeşil; Windows elle kontrol listesi 8 Kas 15:22'de geçti. |
| F2.6 | Tam DB yedeği, geri yükleme, migration öncesi yedek | §15.3 | AC-50, AC-57 | ✅ ADR-0017. `restore.rs` + `tests/portable.rs` (AC-50 iki test, eski şemadan geri yükleme, AC-57, ret durumları); arayüz `e2e/settings.spec.ts`. Kanıt: Cuma'nın Windows 11 `npm run verify` raporu (8 Kas 2026 15:12, gerçek uygulama E2E dahil hepsi geçti) + Linux `verify` + [CI #42491832081](https://github.com/cemekli/VideoDownloader/actions/runs/42491832081) Linux yeşil; Windows elle kontrol listesi 8 Kas 15:22'de geçti. |
| F2.7 | Offline Mode | DATA-05 | AC-17 | ✅ ADR-0016. `vv-netguard` DNS'ten önce ret (`offline_mode_refuses_every_connection_before_dns_ac17`), `open_source` reddi, kalıcı şerit; `e2e/settings.spec.ts`, `Data.test.tsx`, gerçek uygulama `e2e-desktop/offline-flow.spec.ts`. Kanıt: Cuma'nın Windows 11 `npm run verify` raporu (8 Kas 2026 15:12, gerçek uygulama E2E dahil hepsi geçti) + Linux `verify` + [CI #42491832081](https://github.com/cemekli/VideoDownloader/actions/runs/42491832081) Linux yeşil; Windows elle kontrol listesi 8 Kas 15:22'de geçti. |
| F2.8 | İlk açılış, tema, çeviri anahtarı altyapısı | §9.6, §9.7 | E2E | ✅ ADR-0018. `src/i18n` (anahtar kataloğu, `Intl` çoğul/sayı/tarih), açık/koyu/sistem tema, beş adımlı ilk açılış; `e2e/settings.spec.ts` (klavyeyle beş adım, Skip, tema yeniden yüklemede korunur), `Data.test.tsx`. Kanıt: Cuma'nın Windows 11 `npm run verify` raporu (8 Kas 2026 15:12, gerçek uygulama E2E dahil hepsi geçti) + Linux `verify` + [CI #42491832081](https://github.com/cemekli/VideoDownloader/actions/runs/42491832081) Linux yeşil; Windows elle kontrol listesi 8 Kas 15:22'de geçti. |
| F2.9 | Erişilebilirlik temel kontrolleri (axe, klavye, 200% ölçek) | NFR-12 | AC-27 + otomatik axe taraması | ✅ ADR-0018. `e2e/a11y.spec.ts`: 7 yüzey × 2 tema axe'te critical/serious bulgu yok; 683×384'te (%200) yatay kaydırma yok, klavyeyle kaydet/ara. Bulunan sorun: dar pencerede üst çubuk taşıyordu, artık kırılıyor. Kanıt: Cuma'nın Windows 11 `npm run verify` raporu (8 Kas 2026 15:12, gerçek uygulama E2E dahil hepsi geçti) + Linux `verify` + [CI #42491832081](https://github.com/cemekli/VideoDownloader/actions/runs/42491832081) Linux yeşil; Windows elle kontrol listesi 8 Kas 15:22'de geçti. |

**Çıkış kapısı**

- [x] SPEC §3'teki M1 çıkış koşulu: ağsız kaydet–düzenle–ara–oynat akışı yeniden başlatmada veriyi koruyor (`e2e-desktop/offline-flow.spec.ts` Windows'ta geçti; elle 8 Kas 2026).
- [x] Yolculuk 5 (büyük Inbox'ı düzenle) için E2E testi var: `e2e/journeys.spec.ts` (2.000 öğede klavyeyle 20 seçim, toplu etiket, Undo, arşiv, `Ctrl+Z`; seçim korunuyor).
- [x] JSON ve DB yedeğinden geri dönüş Windows'ta doğrulandı (elle kontrol, 8 Kas 2026).

---

## F3 — İndirme motoru ve kalıcı kuyruk (M2, 1. yarı)

**Hedef:** İzinli içeriği güvenli ve dayanıklı biçimde indirmek.

**Çıktı:** İzinli gerçek bir videoyu indirip uygulama içinde oynatan uygulama.

| ID | Alt görev | SPEC bağı | Doğrulama | Durum |
|---|---|---|---|---|
| F3.1 | Kalıcı iş kuyruğu: lease, generation, retry, `cancel_reason`, bağımlılıklar | §12.3, §13 | AC-10, AC-11, AC-58 + çökme testleri | ✅ Migration 0005 + `vv-persistence::jobs`, 16 Rust testi (`tests/jobs.rs`: AC-10/AC-11 kuyruk tarafı, AC-58, DB yeniden açılarak çökme kurtarma); ADR-0019; CI #42495755186 (Linux + Windows) yeşil, PR #23 |
| F3.2 | Yönetilen araç manifesti (yt-dlp, JS runtime, FFmpeg) ve sağlık kontrolü | §14.1, §23.1 | AC-25; `TOOL_MISSING` ve `TOOL_BLOCKED` testleri | ✅ `vv-engine::toolset` (SHA-256 + sürüm, `ok`/`missing`/`blocked`/`checksum_mismatch`), açılışta arka planda kontrol, Settings → Diagnostics (MT-11); gerçek araçlarla `tests/real_ytdlp.rs` (değişik bayt, eksik dosya, Unix'te çalıştırılamayan dosya); ADR-0021; Linux `npm run verify` ✅ (9 Kas 2026); CI [#42592325796](https://github.com/cemekli/VideoDownloader/actions/runs/42592325796) Linux ve Windows yeşil (9 Kas 2026) Elle: Cuma'nın Windows 11 gerçek indirmeleri (9 Kas 2026, `docs/platform-support.md`). |
| F3.3 | Ağ politikasının uygulanması (F0.9 kararına göre) | §20.2 | AC-29 tam | ✅ Her yt-dlp çalışması kendi `vv-netguard` proxy'si arkasında; gerçek yt-dlp ile canary sunucuya, 169.254.169.254'e ve `[::ffff:127.0.0.1]`'e redirect → `URL_BLOCKED`, canary'ye 0 bağlantı (`tests/real_ytdlp.rs`, AC-29); ADR-0021; Linux `npm run verify` ✅ (9 Kas 2026); CI [#42592325796](https://github.com/cemekli/VideoDownloader/actions/runs/42592325796) Linux ve Windows yeşil (9 Kas 2026) Elle: Cuma'nın Windows 11 gerçek indirmeleri (9 Kas 2026, `docs/platform-support.md`). |
| F3.4 | Uzak metadata ve thumbnail (backend üzerinden) | CAP-04, CAP-07, §3 | fixture testleri + gerçek herkese açık URL | ✅ Metadata işi (başlık, kanal, süre, tarih, formatlar, JPEG thumbnail ≤ 4 MiB), migration 0006 `remote_formats`; fixture testleri (`tests/engine.rs`) + yerel sunucudan gerçek yt-dlp ile metadata ve thumbnail; gerçek herkese açık URL Cuma'nın seçeceği URL ile bekliyor; CI [#42592325796](https://github.com/cemekli/VideoDownloader/actions/runs/42592325796) Linux ve Windows yeşil (9 Kas 2026) Elle: Cuma'nın Windows 11 gerçek indirmeleri (9 Kas 2026, `docs/platform-support.md`). |
| F3.5 | Format listesi, kalite önayarları, oynatılabilir hedef container | DL-01, DL-02 | fixture testleri | ✅ `vv-engine::formats`: `best`/`1080`/`720`/`audio`, HEVC'den kaçınma, mp4/webm/mkv hedefi, kesin/yaklaşık boyut; birim testleri + IPC testi (arayüz yalnızca önayar adı gönderir); Linux `npm run verify` ✅ (9 Kas 2026); CI [#42592325796](https://github.com/cemekli/VideoDownloader/actions/runs/42592325796) Linux ve Windows yeşil (9 Kas 2026) Elle: Cuma'nın Windows 11 gerçek indirmeleri (9 Kas 2026, `docs/platform-support.md`). |
| F3.6 | İndirme, birleştirme, bütünlük doğrulaması, staging'den final konuma | DL-03…07, §14.3 | AC-08, AC-09, AC-12 | ✅ `vv-engine::download`: staging → doğrulama (ffprobe, boyut, SHA-256) → checkpoint → yerine taşıma → `succeed_with`; AC-08 (merge), AC-09 (bilinmeyen toplam), AC-12 (disk/kota), bozuk dosya, staging dışı yol, çökme sonrası benimseme (`tests/engine.rs`) + yerel sunucudan gerçek indirme (`tests/real_ytdlp.rs`); Linux `npm run verify` ✅ (9 Kas 2026); CI [#42592325796](https://github.com/cemekli/VideoDownloader/actions/runs/42592325796) Linux ve Windows yeşil (9 Kas 2026) Elle: Cuma'nın Windows 11 gerçek indirmeleri (9 Kas 2026, `docs/platform-support.md`). |
| F3.7 | Hata sınıflandırması ve durum eşlemesi | §12.1, §19 | tablo bazlı birim testleri | ✅ §19'un tüm kodları (`ErrorCode`), `vv-persistence::classify` (iş davranışı + §12.1 eşlemesi), 7 tablo bazlı Rust testi (`tests/classify.rs`, kod listesi SPEC.md'den okunur) + `labels.test.ts`; ADR-0020; CI #42500414511 (Linux + Windows) yeşil, PR #24 |
| F3.8 | Downloads ekranı | §9.3 | Yolculuk 6 E2E | ✅ Downloads ekranı (aktif/dikkat isteyen/tamamlanan, faz, gerçek ve belirsiz ilerleme, retry/cancel/kısmi dosya silme), Download penceresi, 11 yeni komut; `Downloads.test.tsx`, Yolculuk 6 E2E (`e2e/journeys.spec.ts`, 3 ekran boyutu), axe taraması; Linux `npm run verify` ✅ (9 Kas 2026); CI [#42592325796](https://github.com/cemekli/VideoDownloader/actions/runs/42592325796) Linux ve Windows yeşil (9 Kas 2026) Elle: Cuma'nın Windows 11 gerçek indirmeleri (9 Kas 2026, `docs/platform-support.md`). |
| F3.9 | Depolama kotası ve kök taşıma (`storage_move`) | DATA-07, DATA-08 | AC-33, AC-43 | ✅ Kota ve kullanım (Settings), `storage_move` adım tablosu; AC-33 (`tests/engine.rs`), AC-43 her adımda öldürülen taşıma devam ediyor ve kopya kaybolmuyor (`tests/storage_move.rs`); Linux `npm run verify` ✅ (9 Kas 2026); CI [#42592325796](https://github.com/cemekli/VideoDownloader/actions/runs/42592325796) Linux ve Windows yeşil (9 Kas 2026) Elle: Cuma'nın Windows 11 gerçek indirmeleri (9 Kas 2026, `docs/platform-support.md`). |
| F3.10 | Gizli veri redaksiyonu ve canary testleri | §20.3 | AC-18 | ✅ Sahte yt-dlp'nin hata çıktısındaki ve metadata alanlarındaki canary (`VVCANARY-7f3a91`) veritabanının metin alanlarında, olaylarda, JSON dışa aktarımda ve iş listesinde yok (AC-18, `tests/engine.rs`); kaynakta yalnızca `şema://host`; Linux `npm run verify` ✅ (9 Kas 2026); CI [#42592325796](https://github.com/cemekli/VideoDownloader/actions/runs/42592325796) Linux ve Windows yeşil (9 Kas 2026) Elle: Cuma'nın Windows 11 gerçek indirmeleri (9 Kas 2026, `docs/platform-support.md`). |
| F3.11 | Platform destek matrisi (izinli gerçek URL'lerle) | §6 | `docs/platform-support.md` tarihli manuel kanıt | ✅ 9 Kas 2026, Cuma, Windows 11: YouTube, Facebook, Instagram, TikTok, X, VK ve Twitch (VOD) indirildi; Dailymotion da PR #26 düzeltmesinden sonra indirildi; Reddit girişsiz engelli (F5.2'ye kaldı). |

**Çıkış kapısı**

- [x] En az bir izinli gerçek indirme uçtan uca `manual verified`: Cuma, Windows 11, 9 Kas 2026 09:30. YouTube videosu 720p indirildi ve uygulamada oynadı. İlk denemede medya klasörünün diski BitLocker ile kilitliydi; iş `blocked`/`storage_root` oldu, kilit açılınca Try again ile bitti.
- [x] Yukarıdaki AC'ler otomatik testte geçiyor: Linux ve Windows CI her PR'da yeşil (son: [CI #42627721003](https://github.com/cemekli/VideoDownloader/actions/runs/42627721003), PR #30).
- [x] Destek iddiaları yalnızca tarihli kanıtla `docs/platform-support.md` içinde (9 Kas 2026).

---

## F4 — Windows dağıtımı ve MVP sürümü (M2, 2. yarı)

> **Sıra değişikliği (9 Kas 2026, Cuma'nın kararı):** F4, F5'ten sonraya ertelendi. Uygulamayı şimdilik yalnızca Cuma kullanıyor (release exe yeterli); Reddit ve oturum isteyen içerik F5'e bağlı. F5 kod olarak F4'e bağlı değil; F5.1 kendi güvenlik testlerini taşır.

**Hedef:** Uygulamayı güvenli biçimde paketleyip ilk sürümü yayımlamak.

**Çıktı:** Windows MVP: imzalı installer, güncelleme akışı, sürüm notları.

| ID | Alt görev | SPEC bağı | Doğrulama | Durum |
|---|---|---|---|---|
| F4.1 | Installer, WebView2 stratejisi, lisans ve NOTICE dosyaları | §23.1 | temiz Windows 11'de kurulum | ⬜ |
| F4.2 | İmzalı güncelleme ve geri dönüş yolu | §23.2, DEC-16 | AC-25 | ⬜ |
| F4.3 | Temiz kurulum ve kaldırma | §23.3 | AC-26 | ⬜ |
| F4.4 | Güvenlik kapıları: IPC/CSP, yol, komut enjeksiyonu | §20.1 | AC-28, AC-30, AC-38, AC-51, AC-52 + güvenlik paketi | ⬜ |
| F4.5 | Performans bütçeleri | NFR-01…07, NFR-09 | benchmark raporu | ⬜ |
| F4.6 | Manuel test matrisi | §25.4 | tarihli kanıt raporu | ⬜ |
| F4.7 | Sürüm notları, bilinen kısıtlar, kapsama matrisi kontrolü | §26.1 | çıkış kapısı listesi | ⬜ |

**Çıkış kapısı:** SPEC §26.1'deki tüm maddeler işaretli.

---

## F5 — Yetkili oturum (M3)

**Hedef:** Kullanıcının izniyle oturum gerektiren içeriğe erişimi araştırmak ve mümkünse desteklemek.

**Çıktı:** VK test matrisi raporu ve varsa desteklenen oturum yöntemi.

| ID | Alt görev | SPEC bağı | Doğrulama | Durum |
|---|---|---|---|---|
| F5.1 | `AuthProfile`, izin akışı, işletim sistemi secret store | §11.6, §20.3, DEC-08 | canary testi; secret DB'de yok | ✅ PR #32, CI run 42634841520 yeşil (Linux + Windows; DPAPI testi Windows'ta): `vv-secrets`, migration 0012, `vv_persistence::auth`, canary testi `crates/vv-persistence/tests/auth.rs`, ADR-0022. |
| F5.2 | `--cookies-from-browser` (Firefox ve Chrome) ve hata teşhisi | §21.1 | VK-01…05, VK-09 | ✅ PR #35: Firefox (Chrome Windows'ta app-bound şifreli, F5.7'ye), deneme sırası ve kanıt, `COOKIE_ACCESS_FAILED` teşhisi, Settings → Sessions ve "Connect a session"; ADR-0023. CI [#42674665024](https://github.com/cemekli/VideoDownloader/actions/runs/42674665024) Linux ve Windows yeşil. Elle: Cuma, Windows 11, release exe, Firefox oturumuyla gerçek Reddit gönderisi indirildi ve oynatıldı (9 Kas 2026). |
| F5.3 | Çerez dosyası için iş başına çalışma kopyası | §21.1 | AC-55, VK-10 | 🟨 PR #36 merge edildi, CI yeşil: dosya bir kez okunur, yalnızca platformun çerezleri DPAPI'de saklanır; her çalıştırmada yalnızca kullanıcıya açık klasörde kopya, iş sonunda (başarı/hata/iptal) ve çökmeden sonra açılışta silinir; sahipsiz secret uzlaştırması. ADR-0024. ✅ için CI yeşil ve Cuma'nın Windows'ta çerez dosyasıyla gerçek indirmesi (VK-10). |
| F5.4 | Oturum doğrulama kanıtı kuralı ve `verify_session` | §12.1 | AC-19, AC-54 | ✅ PR #36: `ACCESS_DENIED` de (a) kanıtına girer; profille `ACCESS_DENIED` profile dokunmaz (AC-19); `verify_session` giriş gerektirmiş öğeyle (a) kanıtı; (b) yok (`docs/platform-support.md`). ADR-0024. CI [#42682887513](https://github.com/cemekli/VideoDownloader/actions/runs/42682887513) Linux ve Windows yeşil. Elle: Cuma, Windows 11, release exe, Reddit Firefox oturumunda Check now geçti (9 Kas 2026, 22:52). |
| F5.5 | Sessions ekranı | §9.3 | E2E | ✅ PR #36: kenar çubuğunda Sessions; durum, son doğrulama, izinler, Check now, Reconnect, Clear, Remove, giriş sayfası; `e2e/sessions.spec.ts`, axe taraması. ADR-0024. CI #42682887513 yeşil. Elle: Cuma ekranı Windows'ta kullandı (9 Kas 2026). F5.2'de bağlanmış oturumda doğrulama izni olmadığı için Check now gizliydi; artık izin verilene kadar pasif görünür ve ne yapılacağını söyler. |
| F5.6 | VK canlı test matrisi | §21.2 | VK-01…10 tarihli kayıt | ⬜ |
| F5.7 | Uygulamaya ait tarayıcı profili PoC (araştırma) | §21.3, DEC-10 | ADR | 🟨 yazıldı: Sessions'ta "Sign in here, in a VideoVault window"; özel (InPrivate) giriş penceresi, uygulama komutlarına erişimi yok, yalnızca https; Done'da yalnızca platformun çerezleri DPAPI'ye (`app_profile`), motor çerez dosyası gibi çalıştırır. ADR-0025. ✅ için CI yeşil ve Cuma'nın Windows'ta gerçek girişle indirmesi (Reddit, VK-04). |

**Çıkış kapısı:** SPEC §26.3. “Araştırma tamamlandı” ile “özellik destekleniyor” sonuçları ayrı kaydedilir.

---

## F6 — Transkript ve zaman kodlu arama (M4)

**Hedef:** İsteğe bağlı yerel transkripsiyon ve konuşma içinde arama.

**Çıktı:** AI V2 sürümü.

| ID | Alt görev | SPEC bağı | Doğrulama | Durum |
|---|---|---|---|---|
| F6.1 | Model kaydı, izinli indirme, checksum | §17.1 | AC-20 | 🟨 yazıldı: migration 0013 (transkript, segment, arama parçası, `model_installation`), `tools/models.json` kataloğu (whisper.cpp çok dilli modelleri, sabitlenmiş commit, SHA-256 + yayımlanmış SHA-1), Settings → Transcription ve izin penceresi, `model_download` işi (sürdürme, ağ koruması politikası, yalnızca Hugging Face). ADR-0026. ✅ için CI yeşil ve Cuma'nın Windows'ta gerçek model indirmesi. |
| F6.2 | Mevcut altyazıyı tercih etme ve yerel Whisper sağlayıcısı (CPU) | AI-A01, AI-A02, DEC-11 | AC-21 | ⬜ |
| F6.3 | Ses parçalama, checkpoint ve iptal | AI-A03, AI-A04 | AC-23 | ⬜ |
| F6.4 | Segment doğrulama, transkript FTS, gruplanmış sonuçlar | SEA-06, §16.2 | AC-22, NFR-04 | ⬜ |
| F6.5 | Transkript paneli, zaman koduna gitme, not dışa aktarma | PLAY-06, DATA-11 | AC-22, NFR-10 | ⬜ |
| F6.6 | Kaynak sınırları ve AI Processing ekranı | AI-A05, AI-A06 | düşük bellek senaryosu testi | ⬜ |
| F6.7 | Türkçe/İngilizce kalite ve hız raporu | AI-A07 | ölçüm raporu | ⬜ |

**Çıkış kapısı:** SPEC §26.2.

---

## F7 — Anlamsal ve görsel arama (M5)

**Hedef:** Farklı kelimelerle aranan konuyu ve ekrandaki metni bulmak.

**Çıktı:** Hibrit (kelime + anlam) arama ve OCR/görsel arama.

| ID | Alt görev | SPEC bağı | Doğrulama | Durum |
|---|---|---|---|---|
| F7.1 | Embedding sağlayıcısı, vector repository, reindex generation | §16.3, DEC-12 | AC-24 | ⬜ |
| F7.2 | Hibrit sıralama | SEA-07 | AC-40 | ⬜ |
| F7.3 | AI etiket önerileri (`AIProposal`) | AI-B01…06 | birim + E2E | ⬜ |
| F7.4 | Sahne değişimi ve kare örnekleme bütçesi, OCR | AI-C01…03, DEC-13 | AC-41 | ⬜ |
| F7.5 | Görsel embedding (opt-in) | AI-C03, AI-C04 | kaynak türü etiket testi | ⬜ |
| F7.6 | Yerel LLM özet ve soru-cevap (isteğe bağlı) | AI-C05, AI-C06 | zaman koduna dayanma ve çekimserlik testi | ⬜ |

**Çıkış kapısı:** SPEC §26.4.

---

## F8 — Android (M6)

**Hedef:** Paylaş menüsünden kayıt ve mobilde çevrimdışı kütüphane.

**Çıktı:** Gerçek Android cihazda çalışan uygulama.

| ID | Alt görev | SPEC bağı | Doğrulama | Durum |
|---|---|---|---|---|
| F8.1 | Tauri Android uygunluk araştırması | DEC-14 | ADR + örnek build | ⬜ |
| F8.2 | Share → Save akışı | §24 | AC-42 | ⬜ |
| F8.3 | Mobil kütüphane ve alt gezinme | §24 | cihaz testi | ⬜ |
| F8.4 | Scoped storage/SAF, arka plan işleri, bildirim izni | §24 | cihaz testi | ⬜ |
| F8.5 | Android'de indirme motoru uygunluğu | §24 | PoC raporu | ⬜ |
| F8.6 | Mağaza politikası ve lisans kontrolü | §20.4 | kontrol listesi | ⬜ |

**Çıkış kapısı:** SPEC §26.5.

---

## Engeller ve açık noktalar

| Tarih | Konu | Etki | Gereken |
|---|---|---|---|
| 6 Kas 2026 | `verify` Windows'ta CI'da geçti, yerel makinede çalıştırılmadı | Geliştirici makinesinde kurulum doğrulanmadı | Yerelde `npm run setup`, `npm run verify` ve `npm run desktop` |
| 7 Kas 2026 | Oynatma matrisi yalnızca CI'da (GPU'suz) ölçüldü | HEVC'nin GPU'lu makinede durumu bilinmiyor; karar etkilenmez | Windows 11'de `npm run desktop:e2e-exe` + `npm run e2e:desktop` |
| 6 Kas 2026 | Uygulama simgesi geçici (otomatik üretilmiş) | Görsel kimlik yok | F4 öncesi gerçek simge |

## Sonraki adım

Kaldığımız yer ve ilk adımlar: [HANDOFF.md](./HANDOFF.md).

**F0, F1, F2 ve F3 tamamlandı** (M1 ve M2'nin ilk yarısı bitti); F5.1, F5.2, F5.4 ve F5.5 de ✅, toplam 45/74. F3: PR #23–#30, ADR-0019…0021; gerçek indirmeler Cuma'nın Windows 11 testleriyle (9 Kas 2026, `docs/platform-support.md`). **F4 ertelendi** (Cuma'nın kararı, 9 Kas 2026). **F6 başladı** (10 Kas 2026; F5'te kalan elle testler: F5.3, F5.7, VK).

## Değişiklik günlüğü

| Tarih | Değişiklik |
|---|---|
| 10 Kas 2026 | F6 başladı. F6.1 yazıldı (🟨): migration 0013 (transkript, segment, arama parçası ve FTS, `model_installation`), whisper.cpp model kataloğu `tools/models.json`, izinli ve sürdürülebilir `model_download` işi (SHA-256 + yayımlanmış SHA-1), Settings → Transcription; 4 yeni komut. ADR-0026. |
| 10 Kas 2026 | F5.7 yazıldı (🟨): uygulama içi giriş penceresi (InPrivate WebView2), yalnızca platformun çerezleri secret store'a, `app_profile` yöntemi çalışır hâle geldi; 3 yeni komut. ADR-0025. VK takipçilere özel video bilinen kısıt (yt-dlp tek başına da açamıyor), VK-01…10 not run. |
| 9 Kas 2026 | F5.4 ve F5.5 ✅: Cuma'nın Reddit oturumunda Check now geçti. Check now artık doğrulama izni yokken gizlenmiyor, pasif görünüp izni gösteriyor. |
| 9 Kas 2026 | F5.3–F5.5 yazıldı (🟨): çerez dosyası ve iş başına çalışma kopyası, `verify_session` ve `ACCESS_DENIED` kanıtı, Sessions ekranı, 4 yeni komut. ADR-0024. |
| 9 Kas 2026 | F5.2 ✅: Reddit Firefox oturumuyla indi ve oynadı (Cuma, release exe). Taze derlemeden sonraki ilk açılışta thumbnail'lerin ~10 s gecikmesi ve pencerenin bir kez küçülmesi uygulamadan değil, Avast Antivirus'ün yeni exe'yi ve araçları taramasından; Avast kapalıyken thumbnail'ler hemen geldi (Cuma). F4.7 bilinen kısıtlarına yazılacak. |
| 9 Kas 2026 | F5.2: Firefox çereziyle oturum (`--cookies-from-browser firefox:<klasör>`), profilsiz `LOGIN_REQUIRED` sonrası izinli profille yeniden deneme ve §12.1 kanıtı, Settings → Sessions, indirme satırında "Connect a session"; engellenen iş kodunu saklar. ADR-0023. |
| 9 Kas 2026 | F5 başladı. F5.1: secret store crate'i `vv-secrets` (Windows DPAPI, opak `vvsec:` referansı), `auth_profile` tablosu (migration 0012), izin kuralları `vv_persistence::auth`, canary testi; ADR-0022. |
| 9 Kas 2026 | Windows testinden: thumbnail ve medya protokolleri pencerenin thread'i yerine arka planda cevap verir (yüzlerce thumbnail isteği pencereyi kilitliyor, Windows onu "yanıt vermiyor" sayıp hayalet pencereyle değiştiriyordu: titreme, tam ekrandan çıkma, açılışta ~20 s bekleme). Thumbnail okuyucusu iş parçacığı başına bir kez açılır. |
| 9 Kas 2026 | F3 tamamlandı: F3.2–F3.6, F3.8–F3.11 ✅, `docs/platform-support.md` ilk tarihli platform sonuçlarıyla dolduruldu. F4 ertelendi, sıradaki faz F5 (Cuma'nın kararı). Faz özetinde F1/F2 de ✅ olarak düzeltildi. |
| 9 Kas 2026 | Windows testinden: thumbnail'ler en yeni öğeden başlar (grid'in üstü önce dolar); liste yenilemesi yüklenmiş sayfaları korur ve değişmeyen veriyi yeniden çizmez (grid zıplaması). Reddit medya bağlantıları (`packaged-media.redd.it` vb.) artık gönderi sanılmaz: SPEC v1.9, normalizasyon sürümü 5. |
| 9 Kas 2026 | Reddit bilinen platform oldu (gönderi, `redd.it`; paylaşım bağlantısı ve `v.redd.it` çevrimiçi çözülür): SPEC v1.8, normalizasyon sürümü 4, migration 0011. |
| 9 Kas 2026 | Thumbnail üretimi paralel: çekirdek sayısının yarısı kadar (1–4) FFmpeg aynı anda. Pencere titremesinin yalnızca `tauri dev`'de olduğu exe ile doğrulandı (Cuma). Twitch geçmiş yayın indirmesi çalıştı (Cuma). |
| 9 Kas 2026 | Canlı yayın indirilmiyor (SPEC §6): metadata `is_live` kaydediyor (migration 0010), Download penceresi nedenini söylüyor, motor ve komut reddediyor. Yerel videoların thumbnail'leri artık arka planda üretiliyor: FFmpeg veritabanı kilidi tutulmadan çalışıyor, başarısız öğe sırayı tıkamıyor, her yeni thumbnail ekrana hemen geliyor (önceden açılışta ve import sonunda 200'lük parti kilit tutarak üretiliyordu). |
| 9 Kas 2026 | Twitch bilinen platform oldu (VOD, klip; kanal sayfası canlı yayın): SPEC v1.7, normalizasyon sürümü 3, migration 0009. Geliştirme derlemesinde araç özeti hızlandı (`sha2` opt-level 3); liste art arda gelen metadata için tek seferde yenileniyor. |
| 9 Kas 2026 | Altı platformda gerçek indirme denendi (Cuma). Düzeltmeler: codec'i bildirilmeyen biçimler artık "ses/görüntü yok" sayılmıyor (X'te sessiz inen video, Dailymotion doğrudan dosya bağlantısında hiçbir önayarın sunulmaması); birleştirme MKV yerine önce MP4/WebM'e; yeniden ulaşılan depolama kökü "Drive disconnected" olarak kalmıyor. Dailymotion bilinen platform oldu: SPEC v1.6, normalizasyon sürümü 2, migration 0008. |
| 9 Kas 2026 | İlk gerçek indirme (Windows, YouTube 720p) elle doğrulandı. Elle denemeden düzeltmeler: üst çubukta indirme göstergesi, inspector'da öğenin indirme durumu, `storage_root` ve `model` engellerinin metni, YouTube'un yalnızca tahmin verdiği toplam boyutun "about" olarak gösterilmesi (migration 0007 `job.bytes_estimate`). |
| 9 Kas 2026 | F3.2–F3.6, F3.8–F3.10 yazıldı (🟨): `vv-engine` crate'i (araç kontrolü, yt-dlp çalıştırma ağ kapısı arkasında, metadata ve thumbnail, önayarlar, indirme/doğrulama/yerleştirme, kota, `storage_move`, zamanlayıcı), migration 0006 (`remote_formats`, `storage_move_step`), sahte yt-dlp ve gerçek yt-dlp testleri, uygulamada motor ve 11 yeni komut, Downloads ekranı, Download penceresi, Settings'te depolama ve Diagnostics, Yolculuk 6 E2E. Öğe kaldırılınca açık işleri iptal ediliyor. ADR-0021. |
| 8 Kas 2026 | F1.7, F1.8 ve F2.1–F2.9 ✅; F1 ve F2 çıkış kapıları işaretlendi (Windows `verify` + elle kontrol). Elle kontrolde bulunan: Play'e basınca video başlamıyordu (PR #22 ile düzeltildi). |
| 8 Kas 2026 | F2.4–F2.9 ve çıkış kapısı testleri: migration 0004 (`playback_state`, `app_setting`), `vvmedia` protokolü ve oynatıcı (devam, klavye, HEVC algılama), JSON dışa/içe aktarma (önizleme, kök eşleme, güvenli yol), DB yedeği ve denetimli geri yükleme, Offline Mode (`vv-netguard` DNS öncesi ret), çeviri anahtarları, açık tema, beş adımlı ilk açılış; 20 yeni komut. E2E: oynatıcı, ayarlar, ilk açılış, axe (iki tema), %200 ölçek, Yolculuk 2 ve 5; gerçek uygulamada çevrimdışı akış. PLAY-07 daraltıldı (ADR-0016). |
| 8 Kas 2026 | F2.1–F2.3 başladı: migration 0003 (etiket, koleksiyon, akıllı koleksiyon, `operation_log`, `orphan_media`), etiket/koleksiyon/favori/arşiv/kaldırma işlemleri, oturum başına Undo, 30 günlük Recently removed ve açılışta temizlik, sahipsiz medya ve yeniden import, önizlemeli öğe birleştirme. 28 yeni komut. Arayüz: Inbox, koleksiyon ağacı ve menüsü, akıllı koleksiyon düzenleyicisi, etiketler, toplu işlem çubuğu, Undo bildirimi ve `Ctrl+Z`, inspector'da etiket/koleksiyon/not düzenleme, Recently removed, Duplicates. ADR-0015. |
| 8 Kas 2026 | F1.8 başladı: `check_url`, `save_url`, `restore_item` komutları; Add URL diyaloğu (önizleme, alan hatası, duplicate, Recently removed → Restore, çevrimdışı metni), `Ctrl+K` aramasında bağlantı önerisi, metin alanı dışında `Ctrl+V` ile yakalama. NFR-01 gerçek şemada ölçüldü (`tests/save_latency.rs`). ADR-0014. |
| 8 Kas 2026 | F1.7 başladı: uygulama kütüphaneyi `<data>/db/library.sqlite`'ta açıyor; `list_items`, `get_item`, `import_media` komutları, Rust tarafında dosya diyaloğu ve sürükle-bırak (`tauri-plugin-dialog` 2.8.4), `vvthumb` thumbnail protokolü (CSP `img-src`), açılışta `check_storage` ve eksik küçük resimler. Library ekranı: gezinme, arama (`Ctrl+K`), filtre chip'leri, sıralama, sanallaştırılmış grid/liste, çoklu seçim, inspector, ilk kullanım/yükleniyor/hata/sonuç yok/çevrimdışı durumları. `docs/design-system.md`, ADR-0013. |
| 7 Kas 2026 | F1.4–F1.6 başladı: `vv-import` crate'i (import, hash, duplicate adayı, thumbnail, relink, `check_storage`), ffprobe'a çözünürlük/kanal/dil, FTS sorgu oluşturucu ve Türkçe arama fixture'ı, `FILE_MISSING` ve `STORAGE_ROOT_UNAVAILABLE` hata kodları. ADR-0012. |
| 7 Kas 2026 | F1.4, F1.5 ve F1.6 ✅: Windows ve Linux CI'da geçti ([CI #42385646501](https://github.com/cemekli/VideoDownloader/actions/runs/42385646501)), PR #14 merge edildi. |
| 7 Kas 2026 | F1.1, F1.2 ve F1.3 ✅: Windows ve Linux CI'da geçti ([CI #42382304235](https://github.com/cemekli/VideoDownloader/actions/runs/42382304235)). SPEC v1.5 (§11.7 kenar durumu). |
| 7 Kas 2026 | F1.1–F1.3 başladı: migration 0002 (kütüphane şeması, STRICT, FTS5 `search_document`), `vv-persistence::library` repository'leri, `save_bookmark` (BEGIN IMMEDIATE, eşleşme sırası kanonik URL → alias → platform kimliği), `vv-domain::normalize` (sürüm 1, 7 platform), yeniden normalizasyon (AC-44), `DUPLICATE_ITEM` ve `REVISION_CONFLICT` hata kodları. ADR-0011. |
| 7 Kas 2026 | İlk referans sınıfı benchmark (Windows 11, i7-13700HX): BM-01 p95 2,8 ms, BM-02 3,8 ms, BM-03 11,3 ms; ADR-0009'a eklendi. |
| 7 Kas 2026 | F0.11 ve F0.12 ✅ ([CI #42376811094](https://github.com/cemekli/VideoDownloader/actions/runs/42376811094); golden hash Windows'ta da tuttu). DEC-01 için ADR-0010. F0 çıkış kapısı işaretlendi. |
| 7 Kas 2026 | F0.12 başladı: `vv-benchdata` sürümlü veri üreticisi (10.000 öğe, 100.000 segment, golden hash), `npm run bench` ve CI'da bilgi amaçlı benchmark adımı, referans makine tanımı. ADR-0009. |
| 7 Kas 2026 | F0.10 ✅ (PR #10). F0.11 başladı: `docs/architecture.md`, `docs/roadmap.md`, `docs/platform-support.md`; `threat-model.md`'ye §20.1 durum tablosu; `docs/coverage.json` ve `scripts/coverage-matrix.mjs` ile 82 P0/INV/NFR satırlık kapsama matrisi (29 kimlikli manuel test, 5 benchmark), `verify` adımı. |
| 7 Kas 2026 | F0.10 ✅: Windows ve Linux CI'da geçti ([CI #42373404361](https://github.com/cemekli/VideoDownloader/actions/runs/42373404361)). |
| 7 Kas 2026 | F0.9 ✅ (PR #9). F0.10 başladı: kütüphane kilidi (`vv-persistence::lock`), `vv-process` crate'i (temiz ortam, Windows Job Object, iptal/zaman aşımında ağaç öldürme), `tauri-plugin-single-instance` 2.5.5, `get_diagnostics`'e `libraryLock` ve `secondLaunches`, AC-38 E2E. ADR-0008. |
| 7 Kas 2026 | F0.9 başladı: `vv-netguard` (yerel denetim proxy'si: port 80/443, DNS sonrası public IP, rebinding yok, jeton), yt-dlp `NETWORK_GUARD` bayrakları, `scripts/netguard-probe.mjs` ve CI adımı, `docs/threat-model.md`. ADR-0007. |
| 7 Kas 2026 | F0.8 ✅ (F0.8c, Windows ve Linux CI'da gerçek exe'lerle geçti). F0.8c: `tools/manifest.json`'a FFmpeg/ffprobe 9.0.2 (BtbN LGPL, tarihli etiket), `vv-ffmpeg` crate'i (`-protocol_whitelist file` + `-format_whitelist`, `file:` yolları), `INPUT_REJECTED` ve `MEDIA_CORRUPT` hata kodları, `scripts/ffmpeg-probe.mjs` (AC-59 kanaryası, remux ölçümü). ADR-0006. |
| 7 Kas 2026 | F0.8b başladı: `tools/manifest.json` (yt-dlp 2026.10.26 GPG imzalı, Deno 2.9.8), `vv-tools` crate'i, `CHECKSUM_MISMATCH` hata kodu, `--no-js-runtimes --js-runtimes deno:<yol> --no-remote-components`, `scripts/fetch-tools.mjs` ve CI adımı. ADR-0005. |
| 7 Kas 2026 | F0.8a ölçüldü (Windows CI, WebView2 156): MKV/MOV/MP4/WebM, H.264/VP9/AV1 ile oynuyor; HEVC yalnızca ses. Kararlar ADR-0004'te: HEVC oynatılamaz sayılır, PLAY-07 kapsamı daralır. |
| 7 Kas 2026 | F0.8 üç PR'a bölündü. F0.8a başladı: 14 test videosu, Range destekli medya protokolü prototipi (yalnızca test derlemesinde), WebView2 oynatma matrisi testi, ADR-0004. |
| 6 Kas 2026 | F0.6 ✅: ADR-0003, `vv-ytdlp` crate'i, gerçek yt-dlp 2026.10.26 fixture'ları; Windows ve Linux CI'da geçti ([CI #42281072226](https://github.com/cemekli/VideoDownloader/actions/runs/42281072226)). Gün sonu devir notu: HANDOFF.md. |
| 6 Kas 2026 | F0.5 ✅: ADR-0002 (rusqlite bundled, WAL, synchronous=FULL), checksum'lı migration altyapısı, migration öncesi yedek, FTS5 gerçek exe'de doğrulandı ([CI #42275530245](https://github.com/cemekli/VideoDownloader/actions/runs/42275530245)). |
| 6 Kas 2026 | F0.4 ✅: ADR-0001 (Playwright + WebView2 CDP), ayrı test exe'si (`e2e-cdp`), 4 gerçek uygulama testi Windows CI'da geçti. |
| 6 Kas 2026 | F0.3 ✅: Windows CI açılış testi ve Windows 11'de elle doğrulama. Test exe'lerine Windows manifesti gömüldü (0xC0000139 düzeltmesi). |
| 6 Kas 2026 | F0.3 başladı: Tauri 2 kabuğu, `ping`/`get_status`, IPC sözleşme fixture'ı, ACL ve CSP testleri, Windows açılış testi. F0.13 ✅. |
| 6 Kas 2026 | STATE.md oluşturuldu: 9 faz, 74 alt görev. F0.1, F0.2 ve F0.7 tamamlandı ve Linux'ta doğrulandı. |
