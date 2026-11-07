# VideoVault — Proje Durumu

| Bilgi | Değer |
|---|---|
| Son güncelleme | 6 Kasım 2026 |
| Referans belge | [SPEC.md](./SPEC.md) v1.4 |
| Aktif faz | **F0 — Temel ve test altyapısı** |
| Genel ilerleme | 8 / 74 alt görev doğrulandı |
| Tek komutla doğrulama | `npm run verify` → `test-artifacts/verify/report.md` |
| Son doğrulama | 6 Kasım 2026, Linux x64 (geliştirme ortamı): 8/8 adım geçti. GitHub Actions (windows-latest + ubuntu-latest): tüm kapılar geçti, [CI #42253170546](https://github.com/cemekli/VideoDownloader/actions/runs/42253170546). |

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
| F0 | Temel ve test altyapısı | M0 | Windows'ta açılan boş uygulama, yeşil CI, ilk ADR'ler | 🟨 |
| F1 | Çevrimdışı kütüphane çekirdeği | M1 (1/2) | Ağsız kaydet, import et, ara | ⬜ |
| F2 | Düzenleme, oynatma, taşınabilirlik | M1 (2/2) | İndirmesiz tam Windows kütüphanesi | ⬜ |
| F3 | İndirme motoru ve kalıcı kuyruk | M2 (1/2) | İzinli videoyu indirip oynatan uygulama | ⬜ |
| F4 | Windows dağıtımı ve MVP sürümü | M2 (2/2) | İmzalı installer ile Windows MVP | ⬜ |
| F5 | Yetkili oturum | M3 | VK araştırma raporu ve varsa desteklenen yöntem | ⬜ |
| F6 | Transkript ve zaman kodlu arama | M4 | AI V2 sürümü | ⬜ |
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

Önemli sınır: `e2e/` testleri fixture IPC ile çalışır ve gerçek IPC'yi kanıtlamaz. Gerçek pencere, gerçek IPC, yetki ve CSP `e2e-desktop/` ile yalnızca Windows'ta test edilir; Linux'ta gerçek IPC yolu `tests/ipc.rs` ile sahte pencereyle test edilir. Windows alt süreçleri F0.10'a kadar test edilmiyor (SPEC §25.1).

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
| F0.8 | Araç ve lisans araştırması: yt-dlp + JS runtime, FFmpeg build, WebView2 codec/container matrisi. Üç PR: **a** oynatma matrisi (DEC-04), **b** yt-dlp + JS runtime (DEC-02), **c** FFmpeg (DEC-03) | DEC-02, DEC-03, DEC-04 | `docs/decisions.md` ADR'leri ve gerçek dosyalarla matris | 🟨 a ✅: ADR-0004; WebView2 156'da 14 dosyanın 11'i oynuyor, HEVC yalnızca sesle (`partial`); MP4 H.264/AAC zorunlu testi geçti; [CI #42361348985](https://github.com/cemekli/VideoDownloader/actions/runs/42361348985). b, c ⬜ |
| F0.9 | Dış araçların ağ denetimi uygulanabilirliği | §20.2, DEC-05 | ADR + redirect/DNS testleri | ⬜ |
| F0.10 | Single-instance kilidi ve Windows Job Object ile alt süreç yönetimi | INV-13, §13.2 | AC-38 + Windows smoke testi | ⬜ |
| F0.11 | Zorunlu dokümanlar ve kapsama matrisi | §27 | `docs/architecture.md`, `decisions.md`, `threat-model.md`, `roadmap.md`, `platform-support.md`, `testing.md` mevcut | 🟨 `testing.md` var |
| F0.12 | Benchmark ortamı ve sürümlü veri üreticisi (10.000 öğe, 100.000 segment) | NFR-01…07, DEC-17 | aynı seed ile aynı çıktı testi | ⬜ |
| F0.13 | Git deposu ve CI'ın Windows'ta ilk yeşil çalışması | §25.1 | GitHub Actions'ta iki işletim sistemi de geçer | ✅ [CI #42253170546](https://github.com/cemekli/VideoDownloader/actions/runs/42253170546) |

**Çıkış kapısı**

- [ ] `npm run verify` Windows 11 ve Linux CI'da geçiyor.
- [ ] Windows 11'de Tauri penceresi açılıyor, gerçek IPC çağrısı test ediliyor.
- [ ] DEC-01…DEC-06 kararları kanıtıyla `docs/decisions.md` içinde.
- [ ] Zorunlu dokümanlar ve kapsama matrisi mevcut.

---

## F1 — Çevrimdışı kütüphane çekirdeği (M1, 1. yarı)

**Hedef:** Ağ olmadan URL kaydetme, yerel dosya içe aktarma ve arama.

**Çıktı:** URL kaydedip yerel video içe aktaran, ikisinde de arama yapan ve yeniden başlatmada veriyi koruyan Windows uygulaması.

| ID | Alt görev | SPEC bağı | Doğrulama | Durum |
|---|---|---|---|---|
| F1.1 | `LibraryItem`, `MediaAsset`, `StorageRoot`, `UrlAlias`, `DuplicateCandidate` şeması ve repository'ler | §11 | migration ve repository testleri | ⬜ |
| F1.2 | Bookmark kaydı: atomik kayıt + `SearchDocument`, duplicate ve alias kontrolü | CAP-01…06, CAP-08 | AC-01, AC-02, AC-03, AC-36, AC-52 | ⬜ |
| F1.3 | Platform tanıma ve URL normalizasyonu, `normalization_version` | §6, §11.7 | normalizasyon fixture'ı + AC-44 | ⬜ |
| F1.4 | Yerel import: kısıtlı ffprobe, hash, duplicate adayı, thumbnail | IMP-01…07, §14.1 | AC-04, AC-34, AC-35, AC-59 | ⬜ |
| F1.5 | Depolama kökü durumu: `missing` ve `disconnected` ayrımı | DATA-09 | AC-47 | ⬜ |
| F1.6 | FTS5 arama, güvenli sorgu oluşturucu, sıralama, Türkçe arama fixture'ı | SEA-01…05, §16.1 | AC-05, AC-31, Türkçe fixture | ⬜ |
| F1.7 | Library ekranı: grid/liste, inspector, filtreler, boş ve hata durumları | §9.2, §9.3 | E2E + 3 görünümde ekran görüntüsü | ⬜ |
| F1.8 | Add URL ekranı ve `Ctrl+K` | §8.1, §9.4 | Yolculuk 1 E2E; NFR-01, NFR-02 ölçümü | ⬜ |

**Çıkış kapısı**

- [ ] Ağ kapalıyken kaydet, import et, ara ve yeniden başlat akışı Windows'ta çalışıyor.
- [ ] Yukarıdaki AC'ler otomatik testte geçiyor.
- [ ] Yolculuk 1 (beş saniyede kaydet) ve 2 (eski bookmark'ı bul) için E2E testleri var.

---

## F2 — Düzenleme, oynatma ve taşınabilirlik (M1, 2. yarı)

**Hedef:** Kütüphaneyi düzenleme, yerel oynatma, yedekleme ve çevrimdışı mod.

**Çıktı:** M1 tamam: indirme olmadan tam çalışan Windows kütüphanesi.

| ID | Alt görev | SPEC bağı | Doğrulama | Durum |
|---|---|---|---|---|
| F2.1 | Etiketler, koleksiyonlar, akıllı koleksiyonlar | ORG-01…10 | AC-06, AC-39, AC-46, AC-48 | ⬜ |
| F2.2 | Toplu işlemler, Undo, soft-delete, Recently removed, sahipsiz medya | §7.3 | AC-07, AC-37, AC-45, AC-53, AC-60 | ⬜ |
| F2.3 | Duplicate yönetimi ve önizlemeli birleştirme | ORG-11 | birleştirme ve Undo testleri | ⬜ |
| F2.4 | Oynatıcı, güvenli local protocol, kaldığı yerden devam, remux önizlemesi | PLAY-01…04, PLAY-07 | AC-32, AC-49, NFR-10 | ⬜ |
| F2.5 | JSON export/import, kök eşleme, relink | DATA-01…03, DATA-06, §15 | AC-15, AC-16, AC-30, AC-51 | ⬜ |
| F2.6 | Tam DB yedeği, geri yükleme, migration öncesi yedek | §15.3 | AC-50, AC-57 | ⬜ |
| F2.7 | Offline Mode | DATA-05 | AC-17 | ⬜ |
| F2.8 | İlk açılış, tema, çeviri anahtarı altyapısı | §9.6, §9.7 | E2E | ⬜ |
| F2.9 | Erişilebilirlik temel kontrolleri (axe, klavye, 200% ölçek) | NFR-12 | AC-27 + otomatik axe taraması | ⬜ |

**Çıkış kapısı**

- [ ] SPEC §3'teki M1 çıkış koşulu: ağsız kaydet–düzenle–ara–oynat akışı yeniden başlatmada veriyi koruyor.
- [ ] Yolculuk 5 (büyük Inbox'ı düzenle) için E2E testi var.
- [ ] JSON ve DB yedeğinden geri dönüş Windows'ta doğrulandı.

---

## F3 — İndirme motoru ve kalıcı kuyruk (M2, 1. yarı)

**Hedef:** İzinli içeriği güvenli ve dayanıklı biçimde indirmek.

**Çıktı:** İzinli gerçek bir videoyu indirip uygulama içinde oynatan uygulama.

| ID | Alt görev | SPEC bağı | Doğrulama | Durum |
|---|---|---|---|---|
| F3.1 | Kalıcı iş kuyruğu: lease, generation, retry, `cancel_reason`, bağımlılıklar | §12.3, §13 | AC-10, AC-11, AC-58 + çökme testleri | ⬜ |
| F3.2 | Yönetilen araç manifesti (yt-dlp, JS runtime, FFmpeg) ve sağlık kontrolü | §14.1, §23.1 | AC-25; `TOOL_MISSING` ve `TOOL_BLOCKED` testleri | ⬜ |
| F3.3 | Ağ politikasının uygulanması (F0.9 kararına göre) | §20.2 | AC-29 tam | ⬜ |
| F3.4 | Uzak metadata ve thumbnail (backend üzerinden) | CAP-04, CAP-07, §3 | fixture testleri + gerçek herkese açık URL | ⬜ |
| F3.5 | Format listesi, kalite önayarları, oynatılabilir hedef container | DL-01, DL-02 | fixture testleri | ⬜ |
| F3.6 | İndirme, birleştirme, bütünlük doğrulaması, staging'den final konuma | DL-03…07, §14.3 | AC-08, AC-09, AC-12 | ⬜ |
| F3.7 | Hata sınıflandırması ve durum eşlemesi | §12.1, §19 | tablo bazlı birim testleri | ⬜ |
| F3.8 | Downloads ekranı | §9.3 | Yolculuk 6 E2E | ⬜ |
| F3.9 | Depolama kotası ve kök taşıma (`storage_move`) | DATA-07, DATA-08 | AC-33, AC-43 | ⬜ |
| F3.10 | Gizli veri redaksiyonu ve canary testleri | §20.3 | AC-18 | ⬜ |
| F3.11 | Platform destek matrisi (izinli gerçek URL'lerle) | §6 | `docs/platform-support.md` tarihli manuel kanıt | ⬜ |

**Çıkış kapısı**

- [ ] En az bir izinli gerçek indirme uçtan uca `manual verified`.
- [ ] Yukarıdaki AC'ler otomatik testte geçiyor.
- [ ] Destek iddiaları yalnızca tarihli kanıtla `docs/platform-support.md` içinde.

---

## F4 — Windows dağıtımı ve MVP sürümü (M2, 2. yarı)

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
| F5.1 | `AuthProfile`, izin akışı, işletim sistemi secret store | §11.6, §20.3, DEC-08 | canary testi; secret DB'de yok | ⬜ |
| F5.2 | `--cookies-from-browser` (Firefox ve Chrome) ve hata teşhisi | §21.1 | VK-01…05, VK-09 | ⬜ |
| F5.3 | Çerez dosyası için iş başına çalışma kopyası | §21.1 | AC-55, VK-10 | ⬜ |
| F5.4 | Oturum doğrulama kanıtı kuralı ve `verify_session` | §12.1 | AC-19, AC-54 | ⬜ |
| F5.5 | Sessions ekranı | §9.3 | E2E | ⬜ |
| F5.6 | VK canlı test matrisi | §21.2 | VK-01…10 tarihli kayıt | ⬜ |
| F5.7 | Uygulamaya ait tarayıcı profili PoC (araştırma) | §21.3, DEC-10 | ADR | ⬜ |

**Çıkış kapısı:** SPEC §26.3. “Araştırma tamamlandı” ile “özellik destekleniyor” sonuçları ayrı kaydedilir.

---

## F6 — Transkript ve zaman kodlu arama (M4)

**Hedef:** İsteğe bağlı yerel transkripsiyon ve konuşma içinde arama.

**Çıktı:** AI V2 sürümü.

| ID | Alt görev | SPEC bağı | Doğrulama | Durum |
|---|---|---|---|---|
| F6.1 | Model kaydı, izinli indirme, checksum | §17.1 | AC-20 | ⬜ |
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

**F0.8b** sırada. F0.8a (oynatma matrisi, ADR-0004) PR #6'da. Ardından F0.8b (yt-dlp + JS runtime dağıtımı ve lisansı, DEC-02) ve F0.8c (FFmpeg build'i ve lisansı, DEC-03). Alternatifler: F0.9 (ağ denetimi), F0.10 (single-instance ve Job Object).

## Değişiklik günlüğü

| Tarih | Değişiklik |
|---|---|
| 7 Kas 2026 | F0.8a ölçüldü (Windows CI, WebView2 156): MKV/MOV/MP4/WebM, H.264/VP9/AV1 ile oynuyor; HEVC yalnızca ses. Kararlar ADR-0004'te: HEVC oynatılamaz sayılır, PLAY-07 kapsamı daralır. |
| 7 Kas 2026 | F0.8 üç PR'a bölündü. F0.8a başladı: 14 test videosu, Range destekli medya protokolü prototipi (yalnızca test derlemesinde), WebView2 oynatma matrisi testi, ADR-0004. |
| 6 Kas 2026 | F0.6 ✅: ADR-0003, `vv-ytdlp` crate'i, gerçek yt-dlp 2026.10.26 fixture'ları; Windows ve Linux CI'da geçti ([CI #42281072226](https://github.com/cemekli/VideoDownloader/actions/runs/42281072226)). Gün sonu devir notu: HANDOFF.md. |
| 6 Kas 2026 | F0.5 ✅: ADR-0002 (rusqlite bundled, WAL, synchronous=FULL), checksum'lı migration altyapısı, migration öncesi yedek, FTS5 gerçek exe'de doğrulandı ([CI #42275530245](https://github.com/cemekli/VideoDownloader/actions/runs/42275530245)). |
| 6 Kas 2026 | F0.4 ✅: ADR-0001 (Playwright + WebView2 CDP), ayrı test exe'si (`e2e-cdp`), 4 gerçek uygulama testi Windows CI'da geçti. |
| 6 Kas 2026 | F0.3 ✅: Windows CI açılış testi ve Windows 11'de elle doğrulama. Test exe'lerine Windows manifesti gömüldü (0xC0000139 düzeltmesi). |
| 6 Kas 2026 | F0.3 başladı: Tauri 2 kabuğu, `ping`/`get_status`, IPC sözleşme fixture'ı, ACL ve CSP testleri, Windows açılış testi. F0.13 ✅. |
| 6 Kas 2026 | STATE.md oluşturuldu: 9 faz, 74 alt görev. F0.1, F0.2 ve F0.7 tamamlandı ve Linux'ta doğrulandı. |
