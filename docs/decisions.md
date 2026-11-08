# Karar kayıtları (ADR)

SPEC §27 bu dosyayı zorunlu tutar. SPEC'teki DEC-01…DEC-17 kararları da kanıtlarıyla buraya yazılır. M0 kararları (DEC-01…06, DEC-17) ADR-0002, ADR-0004…0007, ADR-0009 ve ADR-0010'dadır; diğerleri ilgili fazda eklenir. Her kayıt: bağlam, seçenekler, karar, sonuçlar ve kanıt.

| ADR | Konu | Durum | Tarih |
|---|---|---|---|
| [ADR-0001](#adr-0001-gerçek-uygulama-e2e-aracı) | Gerçek uygulama E2E aracı | Kabul edildi | 6 Kasım 2026 |
| [ADR-0002](#adr-0002-sqlite-erişimi-wal-ve-migration-stratejisi) | SQLite erişimi, WAL ve migration stratejisi (SPEC DEC-06) | Kabul edildi | 6 Kasım 2026 |
| [ADR-0003](#adr-0003-yt-dlp-adaptörü-sabit-bayraklar-progress-protokolü-ve-hata-eşleme) | yt-dlp adaptörü: sabit bayraklar, progress protokolü, hata eşleme (SPEC DL-05, §14.1) | Kabul edildi | 6 Kasım 2026 |
| [ADR-0004](#adr-0004-webview2-oynatma-matrisi-ve-yerel-medya-protokolü) | WebView2 oynatma matrisi ve yerel medya protokolü (SPEC DEC-04) | Kabul edildi (CI ölçümü; geliştirici makinesi ölçümü açık) | 7 Kasım 2026 |
| [ADR-0005](#adr-0005-yt-dlp-ve-js-runtime-dağıtımı-ve-güven-modeli) | yt-dlp ve JS runtime (Deno) dağıtımı ve güven modeli (SPEC DEC-02) | Kabul edildi | 7 Kasım 2026 |
| [ADR-0006](#adr-0006-ffmpeg-build-lisans-ve-yerel-dosya-sınırı) | FFmpeg build'i, lisansı ve yerel dosya sınırı (SPEC DEC-03, §14.1, AC-59) | Kabul edildi | 7 Kasım 2026 |
| [ADR-0007](#adr-0007-dış-araçların-ağ-denetimi-yerel-denetim-proxysi) | Dış araçların ağ denetimi: yerel denetim proxy'si (SPEC DEC-05, §20.2, AC-29) | Kabul edildi | 7 Kasım 2026 |
| [ADR-0008](#adr-0008-tek-süreç-kütüphane-kilidi-ve-alt-süreç-ağacı) | Tek süreç, kütüphane kilidi ve alt süreç ağacı (SPEC INV-13, §13.2, AC-38) | Kabul edildi | 7 Kasım 2026 |
| [ADR-0009](#adr-0009-benchmark-ortamı-ve-sürümlü-veri-üreticisi) | Benchmark ortamı ve sürümlü veri üreticisi (SPEC §22, §25.2, DEC-17) | Kabul edildi | 7 Kasım 2026 |
| [ADR-0010](#adr-0010-sabitlenen-sürümler-ve-güncelleme-kuralı) | Sabitlenen sürümler ve güncelleme kuralı (SPEC DEC-01) | Kabul edildi | 7 Kasım 2026 |
| [ADR-0011](#adr-0011-kütüphane-şeması-url-normalizasyonu-ve-bookmark-kaydı) | Kütüphane şeması, URL normalizasyonu ve bookmark kaydı | Kabul edildi | 7 Kasım 2026 |
| [ADR-0012](#adr-0012-yerel-import-depolama-kökü-durumu-ve-arama) | Yerel import, depolama kökü durumu ve arama | Kabul edildi | 7 Kasım 2026 |
| [ADR-0013](#adr-0013-library-ekranı-komutlar-thumbnail-protokolü-ve-sanallaştırma) | Library ekranı: komutlar, thumbnail protokolü ve sanallaştırma (SPEC §9.2, §9.3, §18.2) | Kabul edildi | 8 Kasım 2026 |

---

## ADR-0001: Gerçek uygulama E2E aracı

**Bağlam.** SPEC §25.1, fixture IPC ile geçen E2E testinin gerçek Tauri IPC'nin çalıştığını kanıtlamadığını söyler. STATE.md F0.4, gerçek uygulama penceresinde çalışan bir E2E altyapısı ister. Sürüm hedefi Windows 11 ve WebView2'dir (SPEC §3). Arayüz testleri zaten Playwright ile yazılıyor.

**Seçenekler.**

| | Playwright + WebView2 CDP | tauri-driver + WebDriver |
|---|---|---|
| Nasıl | Test derlemesi WebView2'yi `--remote-debugging-port=N` ile başlatır, Playwright `connectOverCDP` ile bağlanır | `tauri-driver`, Windows'ta `msedgedriver`'ı, Linux'ta `WebKitWebDriver`'ı sarar; testler WebdriverIO/Selenium ile yazılır |
| Platform | Yalnızca Windows (WebView2) | Windows ve Linux; macOS yok |
| Ek araç | Yok; mevcut Playwright | `tauri-driver` (cargo) + WebView2 sürümüyle eşleşen `msedgedriver` + WebdriverIO |
| Test dili | Mevcut Playwright testleriyle aynı API ve assertion'lar | Ayrı bir test yığını |
| Kırılganlık | Ayrı bir test derlemesi gerekir (aşağıya bakın) | Driver sürümü WebView2 runtime sürümüyle uyumsuz kalınca kırılır |

**Karar.** Gerçek uygulama E2E testleri Playwright + WebView2 CDP ile yazılır ve Windows'ta çalışır (`apps/desktop/e2e-desktop/`).

**Gerekçe.** Sürüm hedefi Windows'tur ve orada tek test yığınıyla (Playwright) hem fixture hem gerçek uygulama testleri yazılabilir. Sürüm eşleştirmesi gereken ek driver yoktur. Microsoft, WebView2 uygulamalarının Playwright ile CDP üzerinden test edilmesini belgeler.

**Sonuçlar.**

- Linux'ta gerçek pencere E2E'si yoktur. Linux'ta gerçek IPC yolu `src-tauri/tests/ipc.rs` ile (sahte pencereyle) test edilir. Linux bir sürüm hedefi olursa tauri-driver yeniden değerlendirilir.
- WebView2'nin `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` ortam değişkeni işe yaramaz: Tauri WebView2'ye her zaman kendi tarayıcı argümanlarını verir ve WebView2 156 bu durumda ortam değişkenini yok sayar (CI'da süreç komut satırıyla görüldü, 6 Kasım 2026). Bu yüzden port, yalnızca test derlemesine giren `e2e-cdp` cargo özelliğiyle açılır: `VV_E2E_CDP_PORT` ayarlıysa pencereye `--remote-debugging-port` eklenir.
- Dağıtılan exe bu özellik olmadan derlenir; hata ayıklama portunu açacak kod içinde yoktur. Test exe'si ayrı bir dosyadır (`videovault-e2e.exe`) ve dağıtılmaz. Testler tek kullanımlık bir WebView2 profili kullanır.
- `npm run verify`, Windows dışında bu adımı “not run” olarak raporlar; geçti saymaz. CI'da Windows'ta zorunludur (`VV_REQUIRE_DESKTOP_E2E=1`).

**Kanıt.** `apps/desktop/e2e-desktop/desktop-shell.spec.ts`: gerçek durum rozeti, `ping` gidiş-dönüşü, yetki dışı komutların reddi ve CSP'nin uzak görseli engellemesi. İlk yeşil Windows çalışması: [CI #42271413057](https://github.com/cemekli/VideoDownloader/actions/runs/42271413057) (4/4).

---

## ADR-0002: SQLite erişimi, WAL ve migration stratejisi

SPEC DEC-06'yı karşılar. Kod: `crates/vv-persistence`.

**Bağlam.** Kütüphane verisi yerel SQLite'ta durur (SPEC §10.2). SPEC şunları ister: FTS5'in paketlenen SQLite'ta doğrulanması (§16.1), migration numarası ve checksum'ın izlenmesi, yarım şema bırakılmaması, migration öncesi tutarlı yedek (§11.7) ve çalışırken kör dosya kopyası yerine snapshot yedek (§15.3).

**Kütüphane.**

| Seçenek | Artı | Eksi |
|---|---|---|
| **rusqlite + `bundled`** | SQLite uygulamaya gömülü: test edilen sürüm, dağıtılan sürümdür; FTS5 derlemede açık; online backup API'si var; senkron ve basit | SQLite güncellemesi crate güncellemesiyle gelir |
| rusqlite + sistem SQLite'ı | Daha küçük exe | Windows'ta sürüm ve FTS5 makineye göre değişir; §16.1'i karşılamaz |
| sqlx | Async, derleme zamanı sorgu denetimi | Derleme zamanı denetimi için veritabanı gerekir; masaüstü tek kullanıcılı iş yükü için gereksiz karmaşıklık |

**Karar.**

1. `rusqlite =0.40.2`, `bundled` ve `backup` özellikleriyle. Gömülü SQLite: 3.53.2.
2. **Bağlantılar:** süreç başına tek yazıcı bağlantı, gerektikçe salt okunur okuyucu bağlantılar (`query_only`). Yazıcıyı paylaşan kod, onu kendi kilidiyle korur.
3. **PRAGMA'lar:**
   - `journal_mode = WAL`: okuyucular yazıcıyı engellemez. Dosya WAL'a geçemezse (ör. ağ sürücüsü) açılış hata verir, sessizce devam etmez.
   - `synchronous = FULL`: kullanıcının “kaydedildi” gördüğü bir kayıt elektrik kesintisinden sağ çıkar. Yazmalar küçük ve seyrek olduğu için maliyeti düşüktür.
   - `foreign_keys = ON`, `trusted_schema = OFF`, `busy_timeout = 5 sn`.
4. **Migration:**
   - Uygulamaya gömülü, numaralı SQL dosyaları (`migrations/NNNN_ad.up.sql` / `.down.sql`). Liste yalnızca sona eklenir; uygulanmış bir migration değiştirilmez.
   - `schema_migrations` tablosu sürümü, adı ve `up` SQL'inin SHA-256'sını tutar. Checksum değişmişse açılış durur.
   - Her migration kendi transaction'ında çalışır; hata olursa o migration tamamen geri alınır.
   - Uygulamanın bilmediği daha yeni bir şema sürümü bulunursa açılış reddedilir; veritabanına dokunulmaz.
   - Bekleyen migration varsa ve veritabanında veri varsa, önce `backups/pre-migration-vX-to-vY-<zaman>.sqlite` alınır ve bütünlük denetiminden geçer.
   - `down` betikleri tersinirliği test etmek içindir. Üretimde geri dönüş yolu: eski uygulama sürümü + migration öncesi yedek (SPEC §11.7).
5. **Yedek:** SQLite online backup API'si; yazma açıkken bile yalnızca commit edilmiş durumu alır.

**Sonuçlar.**

- FTS5'in varlığı hem Rust testinde hem de gerçek Windows exe'sinde (`get_diagnostics`, e2e-desktop) doğrulanır.
- Eşzamanlı yazma isteği hata vermek yerine `busy_timeout` kadar bekler. Uzun yazma transaction'larından kaçınılmalıdır.
- Kütüphane şeması (SPEC §11) F1.1'de, Türkçe arama davranışı F1.6'da eklenir.

**Kanıt.** `crates/vv-persistence/tests/migrations.rs` (11 test), `tests/fts5.rs` (2 test), `apps/desktop/e2e-desktop` FTS5 testi. İlk yeşil Windows çalışması: [CI #42275530245](https://github.com/cemekli/VideoDownloader/actions/runs/42275530245) (gerçek exe: SQLite 3.53.2, FTS5=true).

## ADR-0003: yt-dlp adaptörü: sabit bayraklar, progress protokolü ve hata eşleme

SPEC DL-05, §13.4, §14.1 ve §19'u karşılar. Kod: `crates/vv-ytdlp`. Süreç başlatma, iptal ve Job Object F0.10'dadır; bu crate yalnızca argümanları üretir ve çıktıyı okur.

**Bağlam.** yt-dlp'nin varsayılan çıktısı insan içindir: renkli ilerleme çubuğu, sürüme göre değişen metin. SPEC makine okunabilir progress ister, stdout/stderr'in sınırsız biriktirilmesini yasaklar ve ham çıktının log olarak saklanmasına izin vermez. Kullanıcı ve sistem yt-dlp config'i, eklenti dizinleri ve önbellek çalıştırmayı istem dışı etkilememelidir.

**Karar.**

1. **Sabit bayraklar** (`args.rs`, aynısı `tests/fixtures/ytdlp/args.json`; sapmayı bir test yakalar): her çalıştırmada `--ignore-config --no-plugin-dirs --no-cache-dir --color never`. Metadata için `--dump-single-json --no-playlist --flat-playlist`. İndirme için `--quiet --progress --newline --no-playlist` ve iki `--progress-template`. URL, çıktı dizini, çıktı şablonu ve format seçici backend tarafından ayrı argv öğeleri olarak eklenir.
2. **Progress protokolü:** indirme güncellemesi `vv-progress {...}`, post-processor güncellemesi `vv-postprocess {...}` satırıdır; alanlar `%(progress.x)j` ile JSON yazılır. Gözlenen iki davranış:
   - Eksik alanlar JSON olmayan çıplak `NA` olarak basılır. Ayrıştırıcı, string dışındaki `NA`, `NaN` ve `Infinity` değerlerini `null` sayar.
   - `--quiet` ile post-processor satırları stderr'e gider. Ayrıştırıcı iki akışı da okur.
3. **§13.4 kuralı:** toplam bilinmiyorsa yüzde ve ETA `null` olur. `total_bytes_estimate` ayrı tutulur, yüzdeye çevrilmez. Fragment'lı indirmede yüzde, tamamlanan fragment sayısından hesaplanır.
4. **Sınırlar:**
   - Satır başına en fazla 16 KiB; aşan kısım atılır ve satır "kesik" diye işaretlenir. `\r` de satır sonu sayılır.
   - stderr'den yalnızca redakte edilmiş son 20 satır ve son 8 `ERROR:` satırı tutulur.
   - Metadata belgesi en fazla 32 MiB, en fazla 1.000 format; string'ler kısaltılır, kontrol karakterleri atılır.
5. **Metadata allowlist'i:** format/manifest URL'leri, HTTP başlıkları (çerez taşıyabilir) ve açıklama ayrıştırmada atılır. Böylece imzalı medya URL'si DB'ye veya arayüze ulaşamaz (SPEC §13.4). Playlist URL'si genişletilmez; `PLAYLIST_UNSUPPORTED` olarak raporlanır.
6. **Hata eşleme:** son `ERROR:` satırı, öncelik sıralı bir ifade tablosuyla (`outcome.rs` `RULES`) SPEC §19 koduna eşlenir. İfadeler yalnızca yt-dlp'nin kendi metinlerinden alınır. Hiçbiri tutmazsa sonuç `EXTRACTOR_FAILED` olur; daha kesin bir neden uydurulmaz. Özel durumlar:
   - Bizim iptalimiz her şeyden önce gelir.
   - "Interrupted by user" `CANCELED` sayılır.
   - Çıkış kodu 2 (bayrak hatası, yani bizim hatamız) `INTERNAL_ERROR` olur.
   - "No supported JavaScript runtime" uyarısından sonra gelen belirsiz hata `TOOL_MISSING` olur.
   - Çıkış kodu 0 ama `ERROR:` satırı varsa sonuç başarı sayılmaz.
7. **Redaksiyon:** saklanan her stderr satırında şu adımlar uygulanır:
   - URL'lerden yalnızca `scheme://host[:port]` kalır.
   - `Cookie:`, `Set-Cookie:` ve `Authorization:` sonrası tamamen silinir.
   - `C:\Users\<ad>`, `/home/<ad>` ve `/Users/<ad>` içindeki kullanıcı adı değiştirilir.
   - Satır 500 karaktere kısaltılır.
8. **Fixture kaynağı:** `scripts/capture-ytdlp-fixtures.mjs`, sabitlenmiş yt-dlp'yi ffmpeg ile üretilmiş medyayı sunan yerel bir sunucuya karşı çalıştırır; makineden dışarı istek çıkmaz. Yerelde üretilemeyen hata metinleri (giriş, bölge, DRM, araç eksik, Windows ağ hataları), yt-dlp kaynak dosyasının adıyla birlikte `errors-source.json`'a yazılır.

**Sonuçlar ve açık noktalar.**

- yt-dlp sürümü değişince yakalama betiği yeniden çalıştırılır ve testler yeni çıktıyla geçmelidir. Sürüm değişikliği tek başına bir PR'dır.
- `MEDIA_URL_EXPIRED` henüz üretilmiyor: hiçbir yakalanmış çalışma, düz 403'ten ayırt edilebilen bir metin göstermedi. Tüm fragment'lar 403 döndüğünde yt-dlp yalnızca "The downloaded file is empty" diyor; bu `EXTRACTOR_FAILED` olarak kalır. F2'de yeniden değerlendirilir.
- Yakalama Linux'ta yapıldı. Windows'a özgü metinler (WinError, getaddrinfo) işletim sistemi metinlerinden yazıldı; gerçek Windows yakalaması F0.10'daki süreç testleriyle birlikte gelir.
- Video + ses ayrı indirilip birleştirildiğinde iki ayrı ilerleme dizisi gelir. Bunları tek iş ilerlemesinde birleştirmek F2 iş kuyruğunun konusudur.
- Canlı platform davranışı bu fixture'larla kanıtlanmaz (SPEC §21.2).

**Kanıt.** `crates/vv-ytdlp`: 17 birim testi + 14 fixture testi (15 yakalanmış çalışma, 20 kaynak metni vakası, CANARY redaksiyonu). Windows ve Linux'ta ilk yeşil çalışma: [CI #42281072226](https://github.com/cemekli/VideoDownloader/actions/runs/42281072226).

---

## ADR-0004: WebView2 oynatma matrisi ve yerel medya protokolü

SPEC DEC-04 ve §7.6 "Doğrulama kapısı"nı karşılar (STATE.md F0.8a). Kod: `apps/desktop/src-tauri/src/media_protocol.rs`, `apps/desktop/e2e-desktop/playback-matrix.spec.ts`, `scripts/make-playback-media.mjs`.

**Bağlam.** SPEC, MKV ve HEVC'nin WebView2'de oynayıp oynamadığını varsaymaz; karar gerçek dosyalarla ölçülen matrisle verilir. Sonuç runtime sürümüne, codec'e, MIME türüne, donanıma ve medyayı sunan protokole bağlıdır. Bu yüzden ölçüm, uygulamanın kendi protokolü üzerinden gerçek exe'de yapılır; tarayıcıda ya da `file://` ile yapılan bir ölçüm kanıt sayılmaz.

**Karar.**

1. **Test medyası:** 14 dosya, her biri 3 sn, 320x180, 24 fps, saniyede bir anahtar kare, 440 Hz ton. MP4, WebM, MKV ve MOV container'larında H.264, HEVC (`hvc1`), VP8, VP9 ve AV1; ses AAC, Opus ya da Vorbis. MP4 H.264'ün bir kopyası `moov` atomu sonda olacak şekilde yazılır (`faststart` yok); ffmpeg ile birleştirilen indirmeler böyle çıkabilir. Dosyalar `tests/fixtures/media/` altında depoya girer (toplam yaklaşık 1,1 MB). `manifest.json` her dosyanın sha256 özetini, ffprobe akışlarını ve `canPlayType` için MIME dizesini tutar.
2. **Yerel medya protokolü (prototip):** `vvmedia` şeması. Windows'ta adres `http://vvmedia.localhost/<ad>` olur.
   - Yalnızca `GET` ve `HEAD` kabul edilir. Tek bir `bytes=` aralığına `206` ve `Content-Range` ile cevap verilir. Açık uçlu aralıklar 4 MiB ile sınırlanır; oynatıcı devamını yeniden ister.
   - Birden çok aralık ya da dosya dışı aralık `416` ve `Content-Range: bytes */<boyut>` alır. `Range` başlığı olmayan istek dosya 64 MiB'tan küçükse `200` ile dosyanın tamamını alır.
   - Ad yalnızca `[A-Za-z0-9._-]` karakterlerinden oluşabilir. Ayraç, `..` ve yüzde kodlaması kabul edilmez. Yol, kanonik kök dizinin içinde kalmalı ve düzenli bir dosya olmalıdır; dışarı çıkan sembolik bağlantı `404` alır.
   - `X-Content-Type-Options: nosniff` ve `Cache-Control: no-store` başlıkları gönderilir.
   - Bugün protokol yalnızca E2E test derlemesinde (`e2e-cdp`) ve `VV_E2E_MEDIA_DIR` tanımlıyken kaydedilir. Bu derlemede CSP `media-src` değeri `tauri.e2e.conf.json` ile genişletilir. Dağıtılan exe'nin CSP'si değişmez (`media-src 'self'`).
3. **Ölçüm:** her dosya yeni bir `<video muted>` öğesinde açılır. Kaydedilenler: `canPlayType`, `loadeddata`, boyut, oynatma (`currentTime` > 0,5 sn), çözülen kare sayısı, çözülen ses baytı, 2. saniyeye seek ve seek sonrası oynatma, ilk kareye kadar geçen süre, hata ya da CSP engeli. Sonuç `test-artifacts/playback-matrix/matrix.{json,md}` dosyalarına yazılır. Sonuç şu üç değerden biridir:
   - `plays`: görüntü ve ses çözüldü, seek çalıştı.
   - `partial`: dosya açıldı ama görüntü, ses ya da seek eksik. Örnek: HEVC çözücü yoksa yalnızca ses çalar.
   - `fails`: dosya hiç açılmadı.
4. **Zorunlu olan:** yalnızca MP4 H.264/AAC'nin `plays` sonucu vermesi. Bu, MVP çıkış koşuludur (§7.6). Geri kalan satırlar ölçümdür, testi kırmaz. Sonuç runtime güncellemesiyle ve makineden makineye değişebilir (HEVC için Windows codec eklentisi ve GPU gerekir).

**Ölçüm sonuçları (7 Kasım 2026, Windows CI, WebView2 156, `windows-latest`, GPU ve HEVC eklentisi yok).** [CI #42361348985](https://github.com/cemekli/VideoDownloader/actions/runs/42361348985), `test-artifacts-windows-latest/playback-matrix/`. User-Agent dizesi kısaltılmış olduğu için yalnızca ana sürüm (156) okunabildi.

| Dosya | `canPlayType` | Sonuç | Not |
|---|---|---|---|
| mp4-h264-aac | probably | plays | MVP temel formatı |
| mp4-h264-aac-moov-end | probably | plays | `faststart` gerekmiyor; Range yeterli |
| mp4-hevc-aac | "" | partial | yalnızca ses, görüntü 0x0 |
| mp4-vp9-opus | probably | plays | |
| mp4-av1-opus | probably | plays | |
| webm-vp9-opus | probably | plays | |
| webm-vp8-vorbis | probably | plays | |
| webm-av1-opus | probably | plays | |
| mkv-h264-aac | probably | plays | |
| mkv-hevc-aac | "" | partial | yalnızca ses |
| mkv-vp9-opus | "" | plays | `canPlayType` yanılıyor |
| mkv-av1-opus | probably | plays | |
| mov-h264-aac | "" | plays | `canPlayType` yanılıyor |
| mov-hevc-aac | "" | partial | yalnızca ses |

Tüm `plays` satırlarında seek 2,0 sn'ye tam oturdu ve oynatma sürdü. İlk kare 11–234 ms arasında geldi; en yüksek değer, ilk dosyanın soğuk başlangıcıdır. CSP engeli ve medya hatası görülmedi.

**Bu ölçümden çıkan kararlar.**

- **Container sorun değil, codec sorun.** MKV ve MOV, H.264, VP9 ve AV1 ile oynuyor. PLAY-07 remux önizlemesi bu WebView2'de MKV için gerekmiyor; kapsamı daraltılır. Bu sonuç runtime sürümüne bağlı olduğu için matris her WebView2 ana sürümünde yeniden çalıştırılır.
- **HEVC oynatılamaz sayılır.** HEVC her üç container'da da yalnızca sesle oynadı. Remux bunu çözmez, çünkü sorun codec'tedir. Kalite önayarları HEVC dışındaki formatları tercih eder (yt-dlp format seçicisi F2'de). HEVC dosya içe aktarılırsa ya da başka format yoksa, dosya korunur ve kullanıcıya "uygulama içinde oynatılamayabilir" uyarısı ile harici oynatıcı seçeneği sunulur. Sessiz transcode yapılmaz (§7.5).
- **Varsayılan birleştirme hedefi:** H.264/AAC için MP4; VP9 ya da AV1 ile Opus için WebM. Bu, SPEC'in başlangıç adayıyla aynıdır. `faststart` gerekmez.
- **Oynatıcı, `canPlayType` sonucuna güvenmez.** Karar gerçek yüklemeye göre verilir. `partial` durumunu (`loadeddata` gelmiş ama `videoWidth = 0`) F2.4 oynatıcısı da algılamalı ve aynı uyarıyı göstermelidir.

**Kuru çalışma (kanıt sayılmaz).** Aynı ölçüm, Linux'ta Playwright Chromium'a karşı basit bir Range sunucusuyla çalıştırıldı; amaç ölçüm kodunu doğrulamaktı. Sonuç: H.264, VP8, VP9 ve AV1 dört container'da da `plays`. HEVC üç container'da da `partial`: ses çaldı, görüntü yoktu (`videoWidth = 0`). Bu, `partial` sınıfının gerçekten gerekli olduğunu gösterir. Sadece `canPlayType` ile karar verilemez: MKV VP9 için `""` döndüğü halde dosya oynadı.

**Sonuçlar ve açık noktalar.**

- F2.4 bu protokolü dağıtılan uygulamaya taşır. Orada istek bir dosya adı değil `MediaAsset` kimliği taşır; yol veritabanından çözülür ve storage root içinde doğrulanır. Büyük dosyalar parça parça okunur.
- Remux süresi (MKV'den MP4'e, `-c copy`) FFmpeg kararına (F0.8c, DEC-03) bağlıdır ve orada ölçülür.
- CI makinesi tek bir veri noktasıdır: GPU ve HEVC eklentisi yoktur. Geliştirici makinesinde (Windows 11, GPU'lu) ölçüm açık bir işdir. `npm run desktop:e2e-exe` ve ardından `npm run e2e:desktop` aynı tabloyu `test-artifacts/playback-matrix/` altına yazar. HEVC orada oynasa bile, kullanıcı makinelerinde garanti olmadığı için yukarıdaki HEVC kararı değişmez.

---

## ADR-0005: yt-dlp ve JS runtime dağıtımı ve güven modeli

SPEC DEC-02, §10.2, §14.1 ve §23.1'i karşılar (STATE.md F0.8b). Kod: `tools/manifest.json`, `crates/vv-tools`, `crates/vv-ytdlp/src/args.rs` (`JS_RUNTIME`), `scripts/fetch-tools.mjs`. Bu kayıt hukuki görüş değildir; lisans maddeleri dağıtım öncesinde ayrıca doğrulanır (SPEC §22, DEC-15).

**Bağlam.** yt-dlp, YouTube için harici bir JavaScript runtime'ı ister. SPEC'in istekleri:

- Runtime da yönetilen bir araç olmalı ve yalnızca manifestteki yoldan kullanılmalı; kullanıcının `PATH`'indeki bir runtime kullanılmamalı.
- Her aracın sürümü, kaynağı, hash'i, varsa imzası ve lisansı kayıtlı olmalı.
- Bilinmeyen bir internet konumundan executable indirilip çalıştırılmamalı.

**Doğrulanan gerçekler (7 Kasım 2026, birincil kaynaklar ve gerçek çalıştırma).**

- **yt-dlp 2026.10.26** (ADR-0003'te sabitlenen sürüm):
  - Kaynak kodu Unlicense. PyInstaller ile paketlenen `yt-dlp.exe`, GPLv3+ kod içerdiği için bütün olarak GPLv3+ sayılıyor (yt-dlp README).
  - Sürüm dosyaları `SHA2-256SUMS` ile yayımlanıyor ve bu dosya GPG ile imzalı. İmza anahtarı: Simon Sawicki, rsa4096, parmak izi `AC0CBBE6848D6A873464AF4E57CF65933B5A7581`. İmza bu oturumda doğrulandı ("Good signature").
  - Resmi exe, EJS çözücüsünü içinde taşıyor: `yt_dlp_ejs-0.8.0`, `-v` başlığında görülüyor. Çalışma anında bir şey indirmesi gerekmiyor.
- **Deno 2.9.8** (21 Ekim 2026): MIT lisanslı. Sürüm dosyalarının yanında yalnızca bir `.sha256sum` dosyası var; bağımsız bir imza bulamadım. `deno.exe` 98.081.568 bayt (zip olarak 42.917.472 bayt). `yt-dlp.exe` 23.575.744 bayt.
- **yt-dlp'nin Deno beklentisi:** en az 2.3.0 (`yt_dlp/utils/_jsruntime.py` içinde `DenoJsRuntime.MIN_SUPPORTED_VERSION`). Daha eski bir Deno `deno-1.46.3 (unsupported)` olarak raporlanıyor ve kullanılmıyor.
- **yt-dlp'nin varsayılanı SPEC'e aykırı.** Hiç bayrak verilmezse yt-dlp `PATH`'teki Deno'yu buluyor ve kullanıyor. `--no-js-runtimes --js-runtimes deno:<yol>` verildiğinde `PATH` yok sayılıyor; yol yoksa sonuç `none` oluyor. İki durum da gerçek çalıştırmayla kayda geçti: `tests/fixtures/tools/health-output.json`.
- **Deno korumalı çalışıyor.** yt-dlp, Deno'yu `run --no-remote --no-npm --cached-only --no-prompt ...` ile başlatıyor; hiçbir `--allow-*` bayrağı yok (`jsc/_builtin/deno.py`). Aynı bayraklarla çalıştırılan bir deneme betiğinde ağ, dosya okuma, dosya yazma, ortam değişkeni ve süreç başlatma girişimlerinin hepsi `NotCapable` hatası verdi. Bu, Node'a göre önemli bir artı: yt-dlp Node'u izin sınırı olmadan çalıştırır.

**Karar.**

1. **JS runtime Deno olacak.** Gerekçeler:
   - yt-dlp'nin varsayılan olarak açık tuttuğu tek runtime bu.
   - Yukarıda görüldüğü gibi izinsiz, korumalı çalışıyor.
   - Lisansı MIT.

   Bedeli boyut: kurulum dosyasına yaklaşık 41 MB sıkıştırılmış, 94 MiB açılmış veri ekliyor. QuickJS daha küçük ama yavaş; boyut sorun olursa yeniden değerlendirilir.
2. **Sürüm sabitlenir ve sabitleme `tools/manifest.json`'da tutulur.** Her araç için sürüm, kaynak, lisans, platform başına URL, indirilen dosyanın boyutu ve SHA-256'sı, ayrıca açılmış executable'ın SHA-256'sı yazılır. Güven çapaları JSON'da değil kodda durur (`vv-tools::manifest`):
   - İzinli URL ön ekleri; sürüm URL'nin içinde geçmek zorunda, yani "latest" bağlantısı kabul edilmez.
   - yt-dlp imza anahtarının parmak izi.

   JSON'da yapılan bir yönlendirme reddedilir; bunu 23 bozma vakası test ediyor.
3. **Uygulama executable indirmez.** M0–M2 boyunca araçlar kurulum paketinin içinde gelir. Araç güncellemesi yalnızca imzalı bir uygulama güncellemesiyle gelir; böylece araçların güven modeli uygulamanınkiyle aynı olur. Ayrı bir araç güncelleyici sonraya bırakıldı: yapılırsa aynı manifest doğrulamasını kullanacak ve kendi ADR'si olacak.
4. **Doğrulama zinciri:**
   - Derleme ve CI anında, `scripts/fetch-tools.mjs` ile:
     - indirilen dosyanın boyutu ve SHA-256'sı kontrol edilir;
     - yt-dlp için `SHA2-256SUMS` imzası depoya konmuş anahtarla (`tools/yt-dlp-public.key`) ve sabit parmak iziyle doğrulanır; imzalı listede manifestteki hash'in bulunduğu kontrol edilir;
     - executable'ın SHA-256'sı kontrol edilir;
     - `--version` çıktısı ve JS runtime yoklaması çalıştırılır.
   - Deno için imza yok. Güven, denoland'ın GitHub sürüm sayfası ve yayımlanan SHA-256'dır. Bu daha zayıf bir halka ve öyle olduğu kayıtlı.
   - Çalışma anında (F0.10): ilk çalıştırmadan önce boyut ve SHA-256 kontrol edilir (`vv_tools::verify_file`); açılışta sürüm sorgusu yapılır (`check_version`). Dosya eksikse `TOOL_MISSING` döner. Boyut ya da hash tutmuyorsa yeni hata kodu `CHECKSUM_MISMATCH` döner ve araç çalıştırılmaz (AC-25). Sürüm farklıysa ya da çıktı tanınmıyorsa da `TOOL_MISSING` döner.
5. **yt-dlp bayrakları** (`args::JS_RUNTIME`): `--no-js-runtimes --js-runtimes deno:<yönetilen yol> --no-remote-components`. Yol tek bir argv öğesi olarak geçer. Yönetilen Deno yoksa yalnızca `--no-js-runtimes --no-remote-components` verilir; YouTube bu durumda ADR-0003'teki kuralla `TOOL_MISSING` olur.
6. **Lisans yükümlülükleri (dağıtım kontrol listesine):**
   - `yt-dlp.exe` ayrı bir program olarak dağıtılır ve uygulamaya bağlanmaz. Kurulum paketi GPLv3 metnini ve ilgili sürümün kaynak koduna erişimi (etiketli kaynak arşivi) içermeli.
   - Deno için MIT telif bildirimi NOTICE dosyasına girer.

**Sonuçlar ve açık noktalar.**

- **F0.10 (süreç başlatma) için iki kural:**
  - yt-dlp, Deno'ya kendi ortam değişkenlerini olduğu gibi geçiriyor. Bu yüzden alt süreçler temizlenmiş bir ortamla başlatılır: `DENO_*` ve proxy değişkenleri kullanıcıdan alınmaz, `DENO_DIR` uygulamanın önbellek dizinine çevrilir (yt-dlp, Deno'nun analiz önbelleğinin kapatılamadığını not ediyor).
  - `yt-dlp -v` başlığı proxy haritasını ve ortamdan gelen değerleri yazıyor. `-v` çıktısı ham olarak saklanmaz; tanılamaya yalnızca ayrıştırılmış `JS runtimes` satırı girer.
- **Sürüm takibi:** stable kanal kullanılır. YouTube kırıldığında yeni yt-dlp sürümü tek bir PR olarak gelir: manifest güncellenir, fixture'lar yeniden yakalanır (ADR-0003), `fetch-tools` tekrar çalıştırılır. Deno sürümü yalnızca güvenlik düzeltmesi geldiğinde ya da yt-dlp'nin alt sınırı yükseldiğinde güncellenir.
- **Canlı YouTube doğrulanmadı.** Çözücünün gerçekten çalıştığı canlı platform matrisinde (F3.11) gösterilir. Bu ADR yalnızca runtime'ın bulunduğunu, sürümünün uyduğunu ve kısıtlı çalıştığını kanıtlar.
- **Doğrulama mekanizması (ayrı karar):** uygulama güncellemelerinin imzası ve code-signing (SPEC §24) bu ADR'nin dışındadır.

**Kanıt.**

- `vv-tools`: 8 test. Kapsam: gerçek manifest, 23 bozma vakası, boyut/hash/eksik dosya, FIPS SHA-256 test vektörü, gerçek sürüm çıktıları, 6 gerçek `JS runtimes` satırı, Deno izin kaydı.
- `vv-ytdlp` arg sapma testi `jsRuntime` dizisini de kapsıyor.
- `scripts/fetch-tools.mjs` Linux'ta uçtan uca geçti: indirme, GPG imzası, iki hash, sürüm, yönetilen Deno'nun kullanılması ve `PATH`'teki Deno'nun yok sayılması. Tek bayt değiştirilmiş bir `deno` dosyası reddedildi.
- Windows'ta aynı zincir gerçek `yt-dlp.exe` ve `deno.exe` ile geçti: indirme ve hash, `gpgv` ile imza (Git for Windows), `tar` ile zip açma, sürümler, yönetilen Deno'nun seçilmesi, `PATH`'teki Deno'nun yok sayılması. Linux'ta da geçti: [CI #42365085585](https://github.com/cemekli/VideoDownloader/actions/runs/42365085585).
- Windows'ta öğrenilen iki şey betiğe işlendi:
  - Git for Windows'un `gpg --import` komutu geçici dizinde başarısız oluyor; bu yüzden ajan gerektirmeyen `gpgv` kullanılıyor.
  - `gpgv`, `C:/...` gibi yolları URL sanıyor; bu yüzden göreli yol veriliyor.

---

## ADR-0006: FFmpeg build, lisans ve yerel dosya sınırı

SPEC DEC-03, §14.1, IMP-02, DL-09 ve AC-59'u karşılar (STATE.md F0.8c). Kod: `tools/manifest.json` (`ffmpeg`, `ffprobe`), `crates/vv-tools` (manifest kuralları, `FfmpegBanner`), yeni `crates/vv-ffmpeg` (sabit bayraklar, ffprobe sonucunun sınıflandırılması), `scripts/ffmpeg-probe.mjs`, `scripts/fetch-tools.mjs`. Bu kayıt hukuki görüş değildir; lisans ve patent maddeleri dağıtım öncesinde ayrıca doğrulanır (SPEC §22, DEC-15).

**Bağlam.** FFmpeg ve ffprobe; yerel import'ta biçim doğrulama (IMP-02), küçük resim, remux (PLAY-07) ve ileride ses çıkarma için gerekir. SPEC'in istekleri:

- Build'in lisansı, codec'leri ve yeniden dağıtım koşulları belli olmalı.
- İki araç ağsız çalışmalı: protokol izin listesi (yalnızca `file`) ve tek, doğrulanmış bir dosya yolu.
- Başka dosyaya ya da URL'ye referans veren girdiler (HLS/DASH manifesti, concat listesi, playlist) `INPUT_REJECTED` ile reddedilmeli. AC-59: böyle bir girdide FFmpeg/ffprobe ağa bağlanmamalı ve kök dışındaki bir dosyayı okumamalı.

**Seçenekler.**

| Seçenek | Artı | Eksi |
|---|---|---|
| **BtbN/FFmpeg-Builds, `lgpl` statik** | GitHub sürüm sayfası; tarihli etiketler; ay sonu build'leri 2 yıl saklanıyor; Windows ve Linux aynı yapılandırma; `checksums.sha256` | İmza yok; iki exe toplam 252 MiB |
| BtbN `lgpl-shared` | Zip 73 MiB (statik 161 MiB); exe'ler DLL'leri paylaşıyor | Artifact başına çok dosya: manifest şeması genişlemeli; DLL arama sırası dikkat ister |
| BtbN `gpl` | libx264/libx265 kodlayıcıları | GPL; ihtiyacımız olmayan kodlayıcılar için tüm araç GPL olur |
| gyan.dev | Yaygın | Yalnızca Windows; varsayılan build'ler GPL; sürüm başına kalıcı URL zayıf |
| Kendi build'imiz | Tam denetim, en küçük boyut | CI'da bakım yükü; M0 için gereksiz |

**Doğrulanan gerçekler (7 Kasım 2026, sabitlenen dosyalarla).**

- **Build:** FFmpeg `n9.0.2-39-g7adfbe40dc`, release/9.0 dalı, BtbN etiketi `autobuild-2026-10-31-12-59` (Ekim'in ay sonu build'i). `-version` çıktısı: `ffmpeg version n9.0.2-39-g7adfbe40dc-20261031`, gcc 16.2.0.
- **Lisans:** yapılandırmada `--enable-version3` var; `--enable-gpl` ve `--enable-nonfree` yok. Sonuç LGPL-3.0-or-later. libx264, libx265, libxvid, libfdk-aac kapalı. H.264 kodlayıcı olarak yalnızca libopenh264 var.
- **Codec'ler:** oynatma matrisindeki 14 dosyanın hepsi doğru okunuyor: MP4/MOV/MKV/WebM × H.264/HEVC/VP8/VP9/AV1, AAC/Opus/Vorbis. HEVC çözücüsü FFmpeg'in kendi (LGPL) çözücüsü.
- **Boyut:** `ffmpeg.exe` 132.190.720 bayt, `ffprobe.exe` 131.990.016 bayt; zip 168.715.328 bayt. Linux'ta `tar.xz` 136.027.168 bayt.
- **Ağ ve dosya erişimi** (`scripts/ffmpeg-probe.mjs`, yerel bir HTTP "kanarya" sunucusuyla; Linux'ta ek olarak `strace` ile):
  - Yalnızca `-protocol_whitelist file` **yetmiyor**. HLS listesi `../outside/secret.ts` satırını içerdiğinde ffprobe biçimi `hls` olarak açıyor ve kök dışındaki dosyayı okuyor (`strace`: 1 `openat`). HTTP satırı engelleniyor, yani yalnızca ağ korunmuş oluyor.
  - `-format_whitelist` eklenince HLS ve concat listeleri açılmadan reddediliyor: `[hls @ …] Format not on whitelist`. Kök dışı okuma yok (`strace`: 0), HTTP isteği yok (kanarya: 0).
  - Biçim uzantıdan değil içerikten belirleniyor: `.mkv` uzantılı bir concat listesi `concat` olarak tanınıyor ve reddediliyor.
  - Girdi olarak verilen bir URL `Protocol 'http' not on whitelist 'file'!` ile reddediliyor. Kontrol olarak kısıtsız ffprobe aynı URL'ye 1 istek atıyor; bu, kanaryanın gerçekten çalıştığını gösterir.
- **Remux:** MKV H.264/AAC → MP4 `-c copy` çalışıyor, sonuç H.264/AAC MP4. 1.212 sn'lik (31,8 MB) bir MKV, Linux CI kapsayıcısında yaklaşık 1 sn'de remux edildi (yaklaşık 30 MB/sn, gerçek zamanın yaklaşık 1.000 katı). Windows CI'da aynı dosya 609 ms sürdü (52 MB/sn, gerçek zamanın yaklaşık 2.000 katı). Bu düşük bit hızlı bir dosya; 1 saatlik 1080p bir video (yaklaşık 2–3 GB) için süreyi büyük ölçüde disk hızı belirler, birkaç on saniye ile bir iki dakika arası beklenir. PLAY-07 önizlemesi gerekirse arka plan işi olarak yapılır.
- **`-n` tuzağı:** çıktı dosyası varsa FFmpeg üzerine yazmıyor ama **çıkış kodu 0** dönüyor (6.1.1 ve 9.0.2'de aynı). Yani başarı yalnızca çıkış koduna bakılarak anlaşılamaz.

**Karar.**

1. **Build:** BtbN `lgpl` statik, FFmpeg 9.0.2 (`n9.0.2-39-g7adfbe40dc-20261031`). `ffmpeg` ve `ffprobe` manifestte iki ayrı araç olarak durur ama aynı arşivden gelir; manifest bunu zorunlu tutar (aynı URL, hash, boyut ve build).
2. **Sabitleme kodda:** FFmpeg URL'si sürüm dizini yerine tarihli BtbN etiketiyle sabitlenir. `vv-tools` şunları zorunlu tutar:
   - etiket `autobuild-YYYY-MM-DD-HH-MM` biçiminde ve tarihi build tarihiyle aynı;
   - dosya adı tam bu build'in adı ve platformu (`win64`/`linux64`);
   - yalnızca `-lgpl-` varyantı (`gpl`, `nonfree` ve `latest` reddedilir);
   - build bir release dalından (`n9.0.2-…`); `N-…` master build'leri reddedilir.

   Arşiv içindeki yol (`archivePath`) göreli, düz parçalardan oluşmalı ve executable adıyla bitmeli. Desteklenen arşivler `zip` ve `tar.xz`. 15 yeni bozma vakası bunları test ediyor.
3. **Lisans kontrolü çalışma anında da yapılır.** Açılış sağlık kontrolü (`FfmpegBanner::check`) şunları ister: doğru program, tam olarak sabitlenen build string'i ve LGPL yapılandırması. GPL ya da nonfree bir FFmpeg (örneğin dağıtımın kendi ffmpeg'i) sürümü tutsa bile kullanılmaz ve `TOOL_MISSING` olarak raporlanır. `fetch-tools` aynı kontrolü her CI çalışmasında gerçek dosyayla yapar.
4. **Her FFmpeg/ffprobe çağrısının sabit bayrakları** (`vv-ffmpeg::args`, `tests/fixtures/ffmpeg/args.json`):
   - Girdiden önce `-protocol_whitelist file -format_whitelist mov,mp4,m4a,3gp,3g2,mj2,matroska,webm,mp3,ogg,flac,wav,aac`. Listede başka dosyaya referans veren hiçbir demuxer yok. mov demuxer'ının harici veri referansları varsayılan olarak kapalı (`enable_drefs=0`).
   - Girdi ve çıktı `file:<mutlak yol>` olarak, tek argv öğesiyle verilir. Böylece `concat:` ya da `http:` ile başlayan bir dosya adı protokol olarak okunmaz. Göreli yol kabul edilmez.
   - ffprobe yalnızca okunan alanları JSON olarak ister: `format_name`, süre, boyut; akışların türü ve codec'i.
   - Remux: `-nostdin`, ilk video ve varsa ilk ses akışı, `-c copy`, `-movflags +faststart`, çıktıda da `-protocol_whitelist file`, `-n`.
   - `pipe` protokolü şimdilik açılmaz; gerekirse ayrı bir kararla eklenir.
5. **Hata eşleme** (`vv-ffmpeg::probe::classify`), iki yeni kod:
   - `INPUT_REJECTED`: ffprobe'un "not on whitelist" satırında adı geçen demuxer referans veren bir biçimse (`hls`, `dash`, `concat`, `imf`, `webm_dash_manifest`, `sdp`, `image2`) ya da reddedilen bir ağ protokolüyse.
   - `MEDIA_CORRUPT`: izin listesinde olmayan başka bir biçim (ör. AVI), okunamayan veri ya da hiç ses/görüntü akışı olmayan dosya.
   - Derinlemesine savunma: ffprobe bir şekilde izin listesi dışındaki bir `format_name` döndürürse bu da reddedilir.
6. **Remux sonucu doğrulanır.** Çağıran taraf her zaman var olmayan yeni bir geçici ad kullanır. Başarı için üç koşul gerekir: çıkış kodu 0, dosya var ve aynı kısıtlı ffprobe dosyayı beklenen biçim ve codec'lerle okuyor. Ancak bundan sonra dosya yerine taşınır.
7. **Dağıtım yükümlülükleri** (dağıtım kontrol listesine):
   - `ffmpeg.exe` ve `ffprobe.exe` ayrı programlardır ve süreç olarak çağrılır; uygulamaya bağlanmaz.
   - Kurulum paketi şunları içermeli: LGPLv3 metni (arşivdeki `LICENSE.txt`); FFmpeg'in tam kaynağına (commit `7adfbe40dc`) ve BtbN build betiklerine (etiketin commit'i) erişim; statik bağlanan kütüphanelerin lisans bildirimleri.
   - H.264, HEVC ve AAC gibi codec'lerin **patent** durumu lisanstan ayrı bir konudur; DEC-15 kapsamında değerlendirilir. libopenh264 statik bağlandığı için Cisco'nun kendi ikili dosyalarına tanıdığı patent lisansı bu build'i kapsamaz. Bu yüzden H.264 **kodlama** planlanmıyor; HEVC dosyaları dönüştürülmez, ADR-0004'teki gibi harici oynatıcı yolu sunulur.

**Sonuçlar ve açık noktalar.**

- **Boyut:** iki statik exe açılmış hâlde 252 MiB tutuyor. Kurulum paketi aşamasında (F4) `lgpl-shared` varyantına geçmek değerlendirilecek (zip 73 MiB). Bunun için manifestte artifact başına birden çok dosya gerekir; geçiş kendi PR'ında yapılır ve bu ADR güncellenir.
- **Güven halkası:** BtbN'nin `checksums.sha256` dosyası imzalı değil. Güven, GitHub sürüm sayfası ve depoya yazılmış hash'tir; Deno ile aynı düzeyde (ADR-0005).
- **Sürüm takibi:** güvenlik düzeltmesi geldiğinde ya da yeni bir ay sonu build'i gerektiğinde manifest güncellenir, `node scripts/ffmpeg-probe.mjs --update` ile fixture'lar yeniden yakalanır. BtbN ay sonu build'lerini 2 yıl tuttuğu için sabitlenen URL bu süre boyunca indirilebilir; kurulum paketi dosyaları zaten kendisi taşır.
- **F0.10 için:** FFmpeg süreçleri ADR-0005'teki temiz ortamla, süre ve kaynak sınırıyla (Job Object) başlatılır. `-loglevel error` dışında çıktı saklanmaz.
- **DL-09:** tanılama ekranı, manifestteki `build` ve `source` alanlarını gösterir.

**Kanıt.**

- `vv-ffmpeg`: 6 test. Kapsam: bayrakların `args.json` ile sapmaması, izin listesinde referans veren demuxer olmaması, yolların bölünmemesi ve mutlak olması, 14 matris dosyasının ffprobe çıktısının sınıflandırılması, 4 kısıtlı reddin `INPUT_REJECTED` olması ve kontrol vakası, diğer hataların `MEDIA_CORRUPT` olması.
- `vv-tools`: 9 test. 39 bozma vakası (15'i FFmpeg) ve gerçek `-version` çıktıları: sabitlenen build kabul ediliyor, GPL'li dağıtım build'i reddediliyor.
- `scripts/fetch-tools.mjs` Linux'ta gerçek dosyalarla uçtan uca geçti: indirme, hash'ler, `tar.xz` açma, build ve lisans kontrolü, 14 dosyanın ffprobe çıktısının fixture'larla aynı olması, AC-59 reddi (0 HTTP isteği) ve remux.
- Windows'ta aynı zincir gerçek `ffmpeg.exe` ve `ffprobe.exe` ile geçti: zip tek kez indirilip iki exe çıkarıldı, build ve LGPL kontrolü, 14 dosyanın fixture'larla aynı olması, 4 kısıtlı reddin 0 HTTP isteğiyle gerçekleşmesi, remux ve `-n` davranışı. Linux'ta da geçti: [CI #42367817432](https://github.com/cemekli/VideoDownloader/actions/runs/42367817432).
- Fixture'lar sabitlenen build ile yakalandı (`tests/fixtures/ffmpeg/`). İlk yakalama Ubuntu'nun ffmpeg 6.1.1'i ile yapılmıştı; tek fark 9.0'ın JSON'a eklediği boş `stream_groups` alanıydı.

---

## ADR-0007: Dış araçların ağ denetimi: yerel denetim proxy'si

SPEC DEC-05, §20.2 ve AC-29'u karşılar (STATE.md F0.9). Kod: `crates/vv-netguard`, `crates/vv-ytdlp/src/args.rs` (`NETWORK_GUARD`), `scripts/netguard-probe.mjs`. Tehdit modeli: `docs/threat-model.md`.

**Bağlam.** Kullanıcının verdiği URL'yi `vv-domain::url_policy` ile bir kez kontrol etmek yetmez. Kontrolden sonra yt-dlp kendi başına:

- yönlendirmeleri izler,
- sayfa, manifest ve playlist okur,
- segment indirir.

Bunların her biri loopback'e, özel ağa ya da bulut metadata adresine (169.254.169.254) gidebilir. Ayrıca DNS iki çözümlemede farklı cevap verebilir (DNS rebinding). SPEC'in kuralı: bağlantı başına politika garanti edilemiyorsa genel URL ağ çıkarımı kapalı kalır.

**Seçenekler.**

| Seçenek | Değerlendirme |
|---|---|
| URL'yi bir kez kontrol et | Yönlendirme, manifest ve rebinding açık kalır. SPEC açıkça yetersiz sayıyor. |
| İşletim sistemi güvenlik duvarı (Windows Filtering Platform, süreç başına kural) | En güçlüsü, ama yönetici yetkisi ister ve kurulumu ağırlaştırır. DNS adına göre karar veremez. |
| yt-dlp eklentisi ya da yamalı yt-dlp | Resmî imzalı exe'yi bırakmak gerekir (ADR-0005). Her yt-dlp sürümünde bakım yükü getirir. |
| **Uygulama içinde yerel denetim proxy'si** (`--proxy`) | Yönetici yetkisi gerekmez. Her bağlantıda karar verilir. Resmî exe değişmez. Yalnızca yt-dlp'nin proxy'ye uyduğu yollarda geçerlidir; bu yollar aşağıda gerçek çalıştırmayla doğrulandı. |

**Karar.**

1. **Her yt-dlp çalışması kendi proxy'siyle başlar** (`vv_netguard::Guard`). Proxy `127.0.0.1` üzerinde rastgele bir portta dinler. yt-dlp'ye `args::NETWORK_GUARD` verilir: `--proxy <url> --downloader native --abort-on-unavailable-fragments`.
   - `--downloader native`, yt-dlp'nin bir protokolü FFmpeg'e devretmesini önler; FFmpeg kendi bağlantısını açardı. Native indiricinin desteklemediği protokoller (RTMP, RTSP, MMS) indirilmez, hata verir.
   - `--abort-on-unavailable-fragments`: engellenen bir segment indirmeyi durdurur, boşluklu bir dosya oluşmaz.
2. **Bağlantı başına karar** (`Policy::vet`):
   - Port 80 ya da 443 olmalı. Bu kontrol DNS sorgusundan önce yapılır.
   - `localhost` ve `*.localhost` adları reddedilir.
   - Literal IP ya da DNS'ten dönen **tüm** adresler `is_public_ip` kontrolünden geçmeli. Tek bir özel adres bile adı engeller, çünkü hangi adresin seçileceğini saldırgan belirler. IPv4-mapped, NAT64 ve IPv6 özel aralıkları da kapsanır.
   - Proxy, kontrol ettiği adrese bağlanır ve adı ikinci kez çözümlemez. Böylece rebinding işe yaramaz.
   - HTTPS `CONNECT` tünelleri ve mutlak biçimli HTTP istekleri desteklenir. Mutlak biçimde `https://`, URL içinde kimlik bilgisi ya da başka bir şema `400` alır.
   - HTTP bağlantısı tek istekle sınırlıdır (`Connection: close`). Böylece yeniden kullanılan bir bağlantı ikinci bir hosta ulaşamaz.
3. **Erişim jetonu:** proxy her çalışmada 144 bitlik rastgele bir jeton üretir (`Proxy-Authorization: Basic`). Jetonu bilmeyen başka bir yerel süreç proxy'yi kullanamaz ve `407` alır. Jeton proxy URL'sinin içindedir: tek argv öğesi olarak geçer, loglanmaz.
4. **Kayıt:** her karar `Event` olarak tutulur. İçinde yalnızca yöntem, host, port ve karar vardır; yol, sorgu ve jeton yoktur (redaksiyon, SPEC §20.3).
5. **Hata eşleme:** proxy'de herhangi bir engel kaydı olan iş, yt-dlp ne yazmış olursa olsun `URL_BLOCKED` ile biter (`Guard::blocked`). yt-dlp'nin metni bunu güvenilir biçimde söylemez; gerçek çıktılar "HTTP Error 403: Forbidden", "Tunnel connection failed: 403" ve "fragment … unable to continue" gibi.
6. **Ortam:** `--proxy`, ortamdaki `HTTP(S)_PROXY`, `ALL_PROXY` ve `NO_PROXY=*` değerlerini geçersiz kılıyor; bu, gerçek çalıştırmayla doğrulandı. F0.10'da alt süreçler yine temiz ortamla başlatılır (ADR-0005); bu ikinci bir savunma katmanıdır.
7. **Kapsam sınırı:**
   - Deno, ağ izni olmadan çalışır (ADR-0005).
   - FFmpeg yalnızca `file` protokolünü kullanır (ADR-0006).
   - Uygulamadan dışarı giden tek araç trafiği yt-dlp'nin HTTP trafiğidir ve hepsi bu proxy'den geçer.

**Kanıt.**

- `vv-netguard`: 7 test. Politika: 16 engel vakası (rebinding cevabı, metadata, mapped IPv6, özel IPv6, `localhost.`, portlar) ve bir port engelinin DNS sorgusu yapmadığı. Proxy, yerel sunucularla şunları gösteriyor:
  - origin-form'a yeniden yazma ve `Proxy-Authorization`'ın silinmesi;
  - CONNECT ve HTTP için 403;
  - jetonsuz ve yanlış jetonlu istekler için 407;
  - yeniden kullanılan bağlantının ikinci hosta gitmemesi;
  - kayıtlarda yol, sorgu ve jeton olmaması.

  Kanarya sunucu hiçbir vakada bağlantı almadı.
- `scripts/netguard-probe.mjs`, sabitlenen yt-dlp 2026.10.26 ile, ortam değişkenleri kanaryayı gösterirken çalıştı. 12 vakanın hepsinde kanarya 0 bağlantı aldı:
  - doğrudan dosya indirmesi (izinli; başarılı);
  - kanaryaya, 127.0.0.1:80'e, `[::ffff:127.0.0.1]`'e, 169.254.169.254'e ve 10.0.0.1'e yönlendirme;
  - 127.0.0.1'e çözümlenen bir ada (`localtest.me`) yönlendirme;
  - segmenti kanaryada olan HLS;
  - bir segmenti izinli, bir segmenti yasak HLS (indirme durdu, dosya oluşmadı);
  - kanaryayı gömen sayfa;
  - `https://` kanarya (CONNECT engellendi);
  - `localhost`.

  Kontrol olarak proxy'siz yt-dlp aynı yönlendirmeyle kanaryaya 1 bağlantı açtı; bu, kanaryanın gerçekten çalıştığını gösterir. Windows ve Linux CI'da gerçek `yt-dlp.exe` ile geçti: [CI #42369808856](https://github.com/cemekli/VideoDownloader/actions/runs/42369808856). CI'da `localtest.me` gerçekten 127.0.0.1'e çözümlendi ve `non_public` olarak engellendi; DNS'i olmayan geliştirme kapsayıcısında aynı vaka `unresolved` olarak engellenmişti. Her CI çalışmasında tekrarlanır.

**Sonuçlar ve açık noktalar.**

- **İzin listesi hâlâ gerekli:** proxy ağ sınırını uygular, hangi sitelerin destekleneceğini belirlemez. Desteklenen konektörler F3'te platform matrisiyle doğrulanır.
- **curl_cffi (tarayıcı taklidi) ve websockets:** resmî exe'de bulunuyorlar ve yt-dlp ikisine de proxy'yi geçiriyor. Ancak burada yalnızca varsayılan HTTP yolu sınandı. Bu iki yolu kullanan bir konektör açılırsa (F3), aynı kanarya testi o yol için de eklenir.
- **QUIC/HTTP3 ve UDP:** yt-dlp kullanmıyor. Proxy'yi aşan bir yol bulunursa SPEC kuralı geçerli olur: o yol kapatılır, gerekirse genel URL çıkarımı kapalı kalır.
- **İşletim sistemi düzeyinde ek katman:** M2 öncesinde süreç başına WFP kuralı değerlendirilebilir. Yönetici yetkisi gerektirdiği için ayrı bir karar konusudur.
- **Port politikası:** yalnızca 80 ve 443. Başka porttan servis veren bir medya sunucusu desteklenmez. Gerekirse konektör bazında genişletilir.

---

## ADR-0008: Tek süreç, kütüphane kilidi ve alt süreç ağacı

SPEC INV-13, §13.2 ve AC-38'i karşılar (STATE.md F0.10). Kod: `crates/vv-persistence/src/lock.rs`, `crates/vv-process`, `apps/desktop/src-tauri/src/instance.rs`. Testler: `crates/vv-persistence/tests/lock.rs`, `crates/vv-process/tests/process.rs`, `apps/desktop/e2e-desktop/single-instance.spec.ts`.

**Bağlam.** İki sorun var:

- Aynı kütüphaneyi iki süreç açarsa iş kuyruğu iki kez işlenir ve aynı hedef dosyaya iki indirme yazabilir. SPEC bunu yasaklıyor (INV-13).
- yt-dlp, Deno'yu ve gerektiğinde FFmpeg'i kendisi başlatır. Uygulama çökerse ya da iş iptal edilirse bu torun süreçler yaşamaya devam edebilir. Sahipsiz bir süreç hedef dosyaya yazmayı sürdürür, kilitli tutar ya da ağ trafiği yapar.

Ayrıca ADR-0005 ve ADR-0007, araçların temiz bir ortamla başlatılmasını şart koşuyor: kullanıcının `HTTP_PROXY`, `DENO_*`, `PYTHONPATH` gibi değişkenleri araca geçmemeli.

**Seçenekler (tek süreç).**

| Seçenek | Değerlendirme |
|---|---|
| Yalnızca `tauri-plugin-single-instance` | Pencereyi öne getirir ve argümanı iletir. Ama tanıyıcıya (identifier) bağlı adlandırılmış mutex kullanır: başka bir derleme, taşınabilir kopya ya da başka bir oturum aynı klasörü açabilir. Tek başına INV-13'ü garanti etmez. |
| Yalnızca kilit dosyası | Veriyi korur, ama ikinci açılış pencereyi öne getirmez, URL iletilmez. |
| **İkisi birlikte** | Eklenti kullanıcı deneyimini sağlar, kilit veriyi korur. |

**Karar.**

1. **Katman 1, eklenti:** `tauri-plugin-single-instance` 2.5.5 (Tauri 2.12 ile uyumlu son 2.x; 3.0 alfa Tauri 3 içindir). Eklenti builder'a ilk eklenir. İkinci açılış, pencere ya da veritabanı oluşmadan argümanlarını çalışan sürece iletir ve 0 koduyla çıkar. Çalışan süreç pencereyi küçültülmüşse geri getirir, gösterir ve odaklar; sayaç `get_diagnostics.secondLaunches` ile okunur. Argümanlar (URL ya da dosya yolu) içe alma akışı yazılınca (F1) ona verilir. Yol içerebildikleri için loglanmaz.
2. **Katman 2, kütüphane kilidi:** veri dizinindeki `videovault.lock`, sürecin ömrü boyunca özel kilitle (`File::try_lock`, Windows'ta `LockFileEx`) tutulur. Kilit `setup` içinde, veritabanı ve worker'lardan önce alınır. Alınamazsa süreç **3** koduyla çıkar ve veritabanını açmaz (`EXIT_LIBRARY_LOCKED`). İşletim sistemi kilidi süreç bitince, çökme dahil, bırakır. Bu yüzden eski bir dosya hiçbir zaman engel olmaz. Dosyaya PID yazılır, ama yalnızca bilgi içindir. Windows kilitli aralığın okunmasını reddettiği için PID orada çoğu zaman okunamaz.
3. **Lease farklı bir iş görür:** SPEC'teki lease mekanizması tek süreç içindeki worker'lar ve çökme sonrası uzlaştırma içindir; çok süreçli erişimin yerine geçmez (§13.2).
4. **Alt süreçler `vv_process::ManagedCommand` ile başlatılır:**
   - **Temiz ortam:** `env_clear()` ve kısa bir izin listesi. Windows'ta `SystemRoot`, `SystemDrive`, `windir`, `TEMP`, `TMP`, `PATHEXT`, `NUMBER_OF_PROCESSORS`, `PROCESSOR_ARCHITECTURE` ve sistem dizinlerinden oluşan bir `PATH`. Unix'te `HOME`, `TMPDIR`, `LANG` ve `/usr/bin:/bin`. Gerisini (ör. `DENO_DIR`) çağıran açıkça verir.
   - **Windows Job Object:** çocuk `CREATE_SUSPENDED | CREATE_NO_WINDOW` ile oluşturulur, kendi job'ına alınır, ancak ondan sonra devam ettirilir. Böylece job'a girmeden bir torun başlatamaz. Job `KILL_ON_JOB_CLOSE` ve `DIE_ON_UNHANDLED_EXCEPTION` taşır. Uygulama çıkarsa ya da çökerse işletim sistemi job tanıtıcısını kapatır ve tüm ağaç ölür.
   - **İptal ve zaman aşımı** tüm ağacı öldürür (`TerminateJobObject`), sonra çocuğu toplar. Çocuk kendisi çıktığında da job'da kalanlar öldürülür. `Managed` düşürülürse (drop) ağaç yine öldürülür.
   - **Unix (yalnızca geliştirme):** çocuk kendi süreç grubunu yönetir; iptal ve zaman aşımı gruba `SIGKILL` gönderir. Uygulama çökerse grup yaşamaya devam eder. Bu kabul edildi, çünkü dağıtılan hedef Windows'tur.
5. **Test kancaları yalnızca test derlemelerinde:** `VV_DATA_DIR` (veri dizini) debug ve `e2e-cdp` derlemelerinde, `VV_E2E_NO_SINGLE_INSTANCE` (eklentiyi kapatır, kilit katmanını sınamak için) yalnızca `e2e-cdp` derlemesinde okunur. Dağıtılan exe ikisini de yok sayar.

**Kanıt.**

- `vv-persistence` kilit testleri (4): aynı süreçte ikinci kilit reddedilir; alt süreç kilidi miras almaz; başka bir süreç kilidi tutarken alınamaz, o süreç öldürülünce alınır ve dosya yerinde kalır.
- `vv-process` testleri: ortamda yalnızca izin listesi ve açık değişkenler var (`HTTP_PROXY`, `HTTPS_PROXY`, `NO_PROXY`, `DENO_DIR`, `PYTHONPATH` geçmiyor); iptal, zaman aşımı ve drop torunu da öldürüyor; normal çıkış raporlanıyor. Windows'a özgü test: "uygulamayı" oynayan yardımcı süreç `TerminateProcess` ile öldürülünce çocuğu ve torunu 5 saniye içinde ölüyor.
- Gerçek uygulama E2E (Windows): ikinci açılış 0 koduyla çıkıyor, ilk pencere açık kalıyor ve `secondLaunches` 1 oluyor. Eklenti kapalıyken ikinci süreç kilidi bulup 3 koduyla çıkıyor. İlk süreç bitince kilit yeniden alınabiliyor.
- CI: Windows ve Linux'ta geçti, [CI #42373404361](https://github.com/cemekli/VideoDownloader/actions/runs/42373404361). Windows'ta `vv-process` 6/6 (çökme testi dahil). Gerçek uygulama E2E 3/3: ikinci açılış 22 ms'de 0 koduyla çıktı; eklenti kapalıyken ikinci süreç 766 ms'de 3 koduyla çıktı.

**Sonuçlar ve açık noktalar.**

- Uygulama henüz alt süreç başlatmıyor. İlk kullanıcılar F2'deki indirme işleri olacak (yt-dlp, Deno, FFmpeg). yt-dlp'nin `--js-runtimes deno:<yol>` ile başlattığı Deno da aynı job'da kalır.
- İkinci açılışta kilit nedeniyle çıkan süreç kullanıcıya bir şey göstermez. Bu durum yalnızca eklentinin göremediği yollarla oluşur. Gerekirse F4'te yerel bir uyarı penceresi eklenir.
- ADR-0003'teki Windows'a özgü ağ hata metinlerinin gerçek yakalaması hâlâ açık. Bu iş, gerçek indirme işleri `vv-process` üzerinden çalışmaya başlayınca (F2) yapılacak.

---

## ADR-0009: Benchmark ortamı ve sürümlü veri üreticisi

SPEC §22 (NFR-01…07), §25.2 ve DEC-17'yi karşılar (STATE.md F0.12). Kod: `crates/vv-benchdata`, `scripts/bench.mjs`. Yöntem ve referans makine: `docs/testing.md` "Benchmark" bölümü.

**Bağlam.** SPEC'teki performans hedefleri ölçülmüş değerler değil, önerilen bütçelerdir. M0'da üç şey kesinleşmeli: ölçümün hangi makinede anlamlı olduğu, hangi veriyle yapıldığı ve bu verinin her seferinde aynı olduğu. Aynı seed farklı makinede ya da farklı bir derlemede başka veri üretirse iki ölçüm karşılaştırılamaz.

**Seçenekler (veri).**

| Seçenek | Değerlendirme |
|---|---|
| Depoya büyük, hazır bir veri dosyası koymak | 22 MB'lık JSONL depoyu şişirir; şema değişince elle yeniden üretmek gerekir. |
| `rand` gibi bir crate ile üretmek | Kolay, ama crate'in algoritması sürümler arasında değişebilir; aynı seed sessizce başka veri verir. |
| **Kendi PRNG'si olan sürümlü üretici + golden hash** | Birkaç satırlık SplitMix64, yalnızca tam sayılar, sıralı JSON. Çıktı değişirse test kırılır ve sürüm numarası artırılmak zorunda kalır. |

**Karar.**

1. **`vv-benchdata` crate'i** veri setini üretir. Varsayılan: seed 20261107, 10.000 öğe, 100.000 segment.
   - SplitMix64 crate'in içinde yazılı ve referans değerlerle test ediliyor. Kayan nokta yok, `HashMap` sırası yok, satır sonu her zaman `\n`.
   - Öğeler ve segmentler ayrı rastgele akışlardan gelir. Segment kuralları değişse de öğeler aynı kalır.
   - Üretilen baytları değiştiren her değişiklik `GENERATOR_VERSION`'ı ve `tests/fixtures/benchdata/golden.json`'daki SHA-256 değerlerini birlikte günceller. Test, golden değerleri Windows ve Linux'ta aynı şekilde doğrular.
   - İçerik SPEC §25.2'yi izler: üç öğe türü, TR/EN/karışık metin, çok uzun başlık, emoji, NFD yazım, Türkçe harfler, bilinmeyen süre, eksik küçük resim, `pending`/`failed` metadata, çoklu etiket ve koleksiyon. URL'ler gerçek alan adı değil, `.example` alan adlarıdır.
2. **Şema:** veri "bench-v1" adlı kendi şemasındadır, çünkü uygulama şeması F1.1'de gelecek. O zaman bir yükleyici bu satırları gerçek tablolara eşler.
3. **Benchmark'lar** (BM-01…03) bu veriyle, uygulamanınkiyle aynı biçimde açılan (`vv_persistence::Database`: WAL, `synchronous=FULL`) bir veritabanında, prototip `bench_*` tablolarında çalışır. BM-04 (başlangıç) ve BM-05 (bellek) uygulama kütüphaneyi açınca eklenecek.
4. **Referans makine:** Windows 11 x64, SSD, ≥ 8 GB RAM, ≥ 4 mantıksal CPU, CI dışı, prize takılı, güç modu "Dengeli" ya da daha yüksek. `scripts/bench.mjs` makineyi bu ölçütlerle sınıflar ve rapora yazar. Bütçe karşılaştırması yalnızca bu sınıfta anlam taşır.
5. **CI'da bilgi amaçlı:** CI her çalışmada benchmark'ı çalıştırır ve raporu `test-artifacts/bench/` altına koyar, ama bütçeyi kapı olarak kullanmaz. Paylaşılan CI makinelerinin süreleri dalgalanır; burada bir kapı ya sahte kırmızı ya da sahte güvence üretirdi. Bütçe kapısı F4.5'te, referans makinede ve gerçek şemada konur.

**Kanıt.**

- `vv-benchdata` testleri (7): SplitMix64 referans değerleri; varsayılan veri setinin golden hash'leri; aynı seed aynı bayt, başka seed başka bayt; segment sayısı öğeleri etkilemiyor; SPEC §25.2 içerik kuralları (sayılar, uzun başlık, NFD, emoji, Türkçe harfler, `.example`, sıralı ve çakışmasız segmentler); sorguların deterministik ve tırnaklı olması; küçük kütüphanede benchmark'ın satır döndürerek çalışması.
- Geliştirme kapsayıcısında ilk ölçüm (Linux, Xeon 2.3 GHz, 4 mantıksal CPU; referans sınıfı **değil**): BM-01 p95 0,50 ms, BM-02 p95 3,3 ms, BM-03 p95 8,4 ms. Üretim ~1 s, yükleme 10.000 öğe 130 ms, 100.000 segment 327 ms, veritabanı 25,8 MiB. Bunlar yöntemin çalıştığını gösterir; bütçenin karşılandığını değil.
- CI: Windows ve Linux'ta geçti, [CI #42376811094](https://github.com/cemekli/VideoDownloader/actions/runs/42376811094). Golden hash Windows'ta da aynı çıktı. CI ölçümleri (2 mantıksal CPU, referans sınıfı değil): Windows Server 2025 BM-01 p95 2,5 ms, BM-02 p95 4,3 ms, BM-03 p95 12,1 ms; Linux BM-01 p95 3,6 ms, BM-02 p95 2,7 ms, BM-03 p95 7,7 ms.
- **İlk referans sınıfı ölçüm**, 7 Kasım 2026, commit `bd00716`, `npm run bench` (Cuma'nın dizüstü bilgisayarı): Windows 11 Pro 10.0.26200, Intel Core i7-13700HX, 24 mantıksal CPU, 31,7 GiB RAM, NVMe SSD, güç modu Dengeli; rustc 1.99.1, Node 26.13.0, SQLite 3.53.2; veri seti v1, seed 20261107 (golden hash'lerle aynı). Yükleme: öğeler 158 ms, segmentler 349 ms.

  | Kimlik | p50 ms | p95 ms | max ms | Bütçe (p95) |
  |---|---|---|---|---|
  | BM-01 bookmark kaydı | 2,0 | 2,8 | 18,5 | ≤ 500 |
  | BM-02 lexical arama | 2,8 | 3,8 | 4,8 | ≤ 200 |
  | BM-03 döküm araması | 7,5 | 11,3 | 13,3 | ≤ 500 |

  Üçü de bütçenin en az 40 kat altında. Bu prototip tablolar ve yalnızca backend içindir; UI dahil görünür süre, başlangıç (BM-04) ve bellek (BM-05) henüz ölçülmedi.

**Sonuçlar ve açık noktalar.**

- Bütçe kapısı F4.5'te gerçek şemada konur; bu ölçüm prototip tablolardadır.
- FTS5 tokenizer'ı burada `unicode61 remove_diacritics 2`. Türkçe arama davranışının kararı F1.6'da Türkçe arama fixture'ıyla verilir; tokenizer değişirse benchmark da güncellenir.
- NFR-01'in "görünür sonuç" kısmı (UI dahil) ve NFR-02 (kullanım testi) bu ölçümde yok; F1.8'de ölçülür.

---

## ADR-0010: Sabitlenen sürümler ve güncelleme kuralı

SPEC DEC-01'i ("kesin Tauri/Rust/Node/React/SQLite sürümleri") karşılar. Sürümler F0.1–F0.10 boyunca seçildi ve kullanıldı; bu kayıt onları ve güncelleme kuralını tek yerde toplar.

**Bağlam.** SPEC §10.2 sürümlerin varsayımla değil, resmî kaynak, lockfile ve Windows build/test kanıtıyla sabitlenmesini istiyor. Bir bağımlılığın sessizce güncellenmesi test edilen ile dağıtılan arasında fark yaratır (ör. SQLite'ın FTS5 ile derlenip derlenmediği, Tauri'nin capability davranışı).

**Karar: sürümler.**

| Katman | Sürüm | Nerede sabit |
|---|---|---|
| Tauri (Rust) | `tauri` 2.12.4, `tauri-build` 2.7.3, `tauri-plugin-single-instance` 2.5.5 (wry 0.57.1) | `apps/desktop/src-tauri/Cargo.toml` (`=` ile) + `Cargo.lock` |
| Tauri (JS) | `@tauri-apps/api` 2.12.4, `@tauri-apps/cli` 2.12.3 | `apps/desktop/package.json` (tam sürüm) + `package-lock.json` |
| UI | React 19.3.1, TypeScript 7.0.2, Vite 8.3.6 | `package.json` + lockfile |
| Test | Vitest 5.1.0, Playwright 1.63.0 | `package.json` + lockfile |
| SQLite | 3.53.2, `rusqlite` 0.40.2 `bundled` (libsqlite3-sys 0.38.2), FTS5 dahil | `crates/vv-persistence/Cargo.toml` (`=`) + `Cargo.lock`; ADR-0002 |
| Node | 26 (LTS) | `.nvmrc`; CI `setup-node` bunu okur (son CI: 26.13.0) |
| Rust | stable, edition 2021 | CI `dtolnay/rust-toolchain@stable` (son CI: 1.99.1); geliştirme kapsayıcısı 1.97.0 |
| Dış araçlar | yt-dlp 2026.10.26, Deno 2.9.8, FFmpeg/ffprobe 9.0.2 LGPL | `tools/manifest.json`; ADR-0005, ADR-0006 |
| WebView2 | Sistem runtime'ı (CI'da 156) | Sabit değil; installer stratejisi F4.1 |

**Karar: güncelleme kuralı.**

1. Doğrudan bağımlılıklar tam sürümle yazılır (npm'de `^`/`~` yok; Rust'ta güvenliği ya da davranışı etkileyenler `=` ile). Geçişli bağımlılıkları lockfile'lar sabitler ve depoya girer.
2. Bir sürüm yalnızca kendi PR'ında yükseltilir. PR'da sürüm notu linki, `npm run verify` ve Windows CI sonucu bulunur. Sürüm bu tabloda güncellenir.
3. SQLite yükseltmesinde FTS5 testi (`vv-persistence`) ve paketlenen exe'deki `get_diagnostics` E2E kontrolü geçmelidir. Tauri yükseltmesinde IPC/ACL/CSP testleri (`tests/ipc.rs`, `e2e-desktop/desktop-shell.spec.ts`) geçmelidir.
4. Rust ve Node şimdilik "stable" ve "26" ile izlenir, tam sürüme bağlanmaz. Kod yeni dil özelliklerine dayanırsa (ör. `File::try_lock`, Rust 1.89+) bu not düşülür.

**Kanıt.** Bu sürümlerle Windows ve Linux CI'da tüm kapılar geçti; son çalışma [CI #42376811094](https://github.com/cemekli/VideoDownloader/actions/runs/42376811094). Paketlenen exe'de SQLite 3.53.2 ve FTS5, gerçek uygulama E2E testiyle doğrulandı (ADR-0002).

**Sonuçlar ve açık noktalar.**

- Rust ve Node'un tam sürüme bağlanmaması, iki CI çalışması arasında derleyicinin değişebileceği anlamına geliyor. Dağıtım derlemesi için `rust-toolchain.toml` ile tam sürüm F4.1'de sabitlenir.
- Geliştirme kapsayıcısında Node 22 vardı; `.nvmrc` 26 diyor. Testler iki sürümde de geçti, ama referans Node 26'dır.
- WebView2 sürümü kullanıcının makinesine bağlıdır; desteklenen en düşük sürüm F4.1'de belirlenir.

## ADR-0011: Kütüphane şeması, URL normalizasyonu ve bookmark kaydı

STATE.md F1.1–F1.3. SPEC §6, §7.1 (CAP-02…CAP-06, CAP-08), §11, §11.7.

**Bağlam.** M1'in ilk yarısı ağ olmadan URL kaydetmeyi istiyor. Kaydın kalıcı kimliği (kanonik URL, platform kimliği) bugün belirlenir ve sonradan değişen kurallarla yeniden hesaplanabilir olmalıdır. Aynı içerik iki kez kaydedilmemeli, eşzamanlı kayıt yarışı tutarlı bitmeli ve geçici sırlar diske yazılmamalıdır.

**Karar: şema (migration 0002).**

- Tablolar: `storage_root`, `library_item`, `media_asset`, `url_alias`, `duplicate_candidate` ve FTS5 `search_document`. Hepsi `STRICT`; kapalı değer kümeleri `CHECK (... IN (...))` ile sabit. Rust tarafındaki enum listeleri ile SQL listelerinin aynı olduğunu bir test iki yönlü denetler.
- `library_item.seq INTEGER PRIMARY KEY` yalnızca FTS satırının bağlandığı iç anahtardır (düz `rowid` VACUUM'da değişebilir); dışarıya `id` (UUID v4 metin) çıkar.
- Benzersizlik (§11.7): `(source_platform, source_id) WHERE source_id IS NOT NULL` ve `canonical_url WHERE canonical_url IS NOT NULL` kısmi unique indeksleri. Yerel öğe `kind = local_video` ⇔ `source_platform = local`; uzak öğede `canonical_url` zorunlu.
- §11.1 yayın tarihi: `published_precision` hangi alanın dolu olduğunu belirler; şema diğer birleşimleri reddeder.
- `media_asset.storage_ref` köke göre göreli yoldur; mutlak yol, UNC ve sürücü harfi şemada reddedilir. Türetilmiş varlık `derivation` olmadan, özgün varlık `derivation`/`source_asset_id` ile yazılamaz. Kaynak varlık silinince türevleri silinir (cache kapsamı); varlığı olan kök silinemez.
- `duplicate_candidate`: sırasız çift ve neden başına tek açık aday (ifade indeksi `min/max`). Karar verilince aynı çift yeniden önerilebilir.
- `search_document` belirteci geçici olarak `unicode61 remove_diacritics 2`; Türkçe kararı F1.6'da. Kaldırılmış öğe satırını korur, arama sorgusu `removed_at IS NULL` ile süzer. `tags` sütunu etiket tabloları gelene kadar boştur, böylece FTS düzeni o zaman değişmez.
- Platformdan gelmeyen başlık saklanmaz: metadata yokken UI ve arama belgesi bağlantıdan türetilen dürüst başlığı (`YouTube · <id>`, `example.com · son-parça`) kullanır, `title` null kalır (CAP-04).
- Güncellemeler beklenen `revision` ile gelir; uyuşmazlık `REVISION_CONFLICT` döner. `DUPLICATE_ITEM` ve `REVISION_CONFLICT` hata kodları eklendi; önerilen eylemler `open_existing` ve `reload`.

**Karar: normalizasyon (`vv-domain::normalize`, sürüm 1).**

- Bilinen platformda kanonik URL içerik kimliğinden **yeniden kurulur** (ör. `https://www.youtube.com/watch?v=<id>`, `https://vk.com/video<owner>_<id>`, `https://x.com/i/status/<id>`). Böylece takip ve imza parametreleri kanonik değere sızamaz.
- Genel URL'de yalnızca şema/host küçük harf, varsayılan port, host sonundaki nokta ve fragment normalize edilir; sorgu bayt bayt korunur (§7.1 "körlemesine silinmez").
- Kısa bağlantılar (`vm.tiktok.com`, `fb.watch`, `/share`) çevrimdışı çözülemez; olduğu gibi saklanır, M2 metadata'sı çözer. `youtu.be` kimliği yolda taşıdığı için çözülür.
- Playlist/kanal/profil URL'si tek bookmark olarak saklanır ve `PLAYLIST_UNSUPPORTED` uyarısı döner (§6).
- Sır parametreleri (`signature`, `token`, `access_key`, `expires`, `x-amz-*`…) `original_url`'de `REDACTED` olur. Kanonik biçimi olmayan URL sır taşıyorsa kayıt reddedilir (`URL_HAS_CREDENTIALS`, AC-52).
- Fixture `tests/fixtures/normalize.json` tek doğruluk kaynağıdır; fixture'daki `normalizationVersion` koddaki sabitle eşleşmezse test düşer. Kanonik değer tekrar normalize edildiğinde değişmez (sabit nokta testi).

**Karar: kayıt ve yarış.** `save_bookmark` politika → normalizasyon → `BEGIN IMMEDIATE` içinde eşleşme araması (kanonik URL, alias, platform kimliği) → yoksa öğe + arama belgesi aynı işlemde. Eşleşme varsa hiçbir şey yazılmaz, mevcut öğe ve açık duplicate adayları döner; öğe kaldırılmışsa `removed_at` dolu gelir (UI "Geri yükle/Vazgeç" sunar, AC-45). Yarışı kaçan bir yazıcı unique indekse çarparsa kazanan öğe döner (AC-03).

**Karar: yeniden normalizasyon (AC-44).** `library::renormalize::renormalize`, sürümü eski öğeleri `added_at` sırasıyla tek işlemde gezer. Değer boştaysa öğe alır; doluysa öğe eski değerinde kalır, `normalization_conflict = 1` olur ve sahibiyle `DuplicateCandidate(normalization_conflict)` açılır. Çakışmalı öğe çözülene kadar tekrar denenmez; geçiş yinelendiğinde iş kalmaz. Uygulama veritabanını açtıktan sonra bu geçişi çalıştırır (kabuk F1.8'de bağlanır).

**SPEC'e not (kenar durum).** §11.7 örneğinde B'nin eski değeri yeni değere eşitse "B eski değerinde kalır" ile "A yeni değeri alır" birlikte unique indeksi bozar. Bu durumda değeri zaten taşıyan B onu korur; A çakışmalı işaretlenir ve aday açılır (test: `stale_holder_with_the_new_value_already_keeps_it`). Kural, B'nin eski değeri farklı olduğunda SPEC'teki gibidir. Bu istisna ve düzeltilmiş örnek SPEC v1.5'te §11.7'ye eklendi.

**Sonuçlar ve açık noktalar.**

- CAP-01'in UI kısmı (Add URL, `Ctrl+V`) F1.8'de; bu PR servis ve veri katmanıdır.
- X bağlantısında `video` biçimi "içerik kimliği olan gönderi" anlamındadır; gönderide video olup olmadığı M2 metadata'sıyla öğrenilir.
- Etiket/koleksiyon tabloları (§11.4) ve `Tag` normalizasyonu F1.6/F1.7 ile gelir; `search_document.tags` o zaman dolar.

## ADR-0012: Yerel import, depolama kökü durumu ve arama

STATE.md F1.4–F1.6. SPEC §7.2 (IMP-01…07), §7.4 (SEA-01…05), §7.7 (DATA-09, DATA-10), §14.1, §14.2, §16.1.

**Bağlam.** M1 kütüphanesi ağ olmadan yerel dosya almalı, harici disk çıkarıldığında öğeleri bozmamalı ve Türkçe metinde makul arama yapmalı. Kararlar F1.7 (Library ekranı) ve F1.8 (Add URL) öncesinde veri katmanında verilir.

**Karar: yerel import (`crates/vv-import`).**

- Varsayılan dosyayı yerinde referanslar (`storage_kind = external_reference`); kopyalama, taşıma ya da kaynağın yanına yazma yok. "Kütüphaneye kopyala" seçeneği sonraya kalır.
- Seçilen dosya her zaman ffprobe'a gider. Seçilen klasörde yalnızca medya uzantılı dosyalar denenir; alt klasörler `recursive` ile. Symlink, junction ve diğer reparse point'ler izlenmez; OneDrive gibi yalnızca bulutta duran dosyalar okunmaz, ayrı raporlanır (DATA-10).
- Her dosyanın sırası: kısıtlı ffprobe (manifest/liste dosyası başka bir şey açılmadan reddedilir, `INPUT_REJECTED`) → tam yol duplicate kontrolü → isteğe bağlı SHA-256 → tek işlemde öğe + özgün varlık + arama belgesi. Her dosyanın kendi işlemi ve sonucu vardır; bozuk bir dosya diğerlerini geri almaz (IMP-04). İptal, bitmiş dosyaları korur.
- Aynı hash'e sahip başka öğe varsa `DuplicateCandidate(content_hash)` açılır; birleştirme yapılmaz (IMP-05).
- Kök seçimi: dosyayı içeren en özel `external_import` kökü yeniden kullanılır, yoksa seçilen klasör (tek dosyada dosyanın klasörü) yeni kök olur. Yollar canonical tutulur, Windows'un `\\?\` öneki mümkünse atılır. Köke göre yol bileşen bazında hesaplanır, string prefix kullanılmaz.
- ffprobe artık çözünürlüğü, ses kanallarını ve dil etiketini de istiyor (`tests/fixtures/ffmpeg/args.json`); probe fixture'ları sabit 9.0.2 build'iyle yeniden üretildi.
- Thumbnail import'tan sonra ayrı çalışır (IMP-06): ilk video akışından süre %10'u (en çok 5 s) noktasında 320 px JPEG, cache kökünde `thumbnails/<item>/<asset>.jpg`. Türetilmiş varlık olarak `derivation` (FFmpeg build'i, ayar sürümü) ve `source_asset_id` ile kaydedilir. Başarısızlık öğeyi ya da aramayı etkilemez.
- ffprobe/FFmpeg her zaman `vv-process` ile (temiz ortam, Job Object, zaman aşımı: probe 30 s, thumbnail 60 s) ve sabit `vv-ffmpeg` argümanlarıyla çalışır.

**Karar: depolama kökü durumu (F1.5).** `roots::check_storage` önce her kökün erişilebilirliğini ölçer: klasör yoksa `disconnected`, işletim sistemi reddediyorsa `inaccessible`. Erişilemeyen kökün varlıklarına dokunulmaz. Kök `available` ise her varlık kontrol edilir: dosya yoksa `missing`, boyut değiştiyse `modified`, geri geldiyse `verified`. `repo::set_asset_integrity` aynı kuralı SQL'de de uygular: kök `available` değilken `missing` yazılamaz (DATA-09). Kontrol uygulama açılışında ve periyodik çalıştırılır; disk geri takıldığında bir sonraki kontrol kökü ve varlıkları kendiliğinden doğrular. Relink (IMP-07) yalnızca varlığın konumunu değiştirir; hash kayıtlıysa yeni dosyanın hash'i aynı olmalıdır (kullanıcı açıkça kabul etmedikçe).

**Karar: arama (F1.6, `vv-persistence::library::search`).**

- Sorgu oluşturucu kullanıcı metnini FTS5 sözdizimine hiç geçirmez: her kelime tırnaklı dize olur, `AND`/`OR`/`NEAR`/`*`/`:` sıradan kelimedir, kullanıcının çift tırnakları ifade arar. Son kelime yazarken önek olarak eşleşir. Harf/rakam içermeyen girdi hata vermez; filtrelenmiş kütüphane ve `NoSearchableTerms` bildirimi döner (AC-31).
- Türkçe: `unicode61 remove_diacritics 2` büyük/küçük harfi, `İ`'yi, NFD biçimini ve `ç ğ ö ş ü`'yü zaten katlıyor (ölçüldü). Ayrı tuttuğu tek harf noktasız `ı`; indeks ve sorgu `ı → i` eşler. Eşleme karakter başına bire bir olduğu için snippet özgün metinden yeniden kurulur ve gerçek alt dizedir. Bilinen sınırlar fixture'da yazılı: kök bulma yok ("kitaplar" "kitap"ı bulmaz, "istanbulda" "İstanbul'da"yı bulmaz), `kirk` `kırk`ı da bulur.
- Sıralama: BM25 ağırlıkları başlık 10, not 5, etiket 5, açıklama 2, üretici 2; eşitlikte yeni eklenen önce, sonra `id`. Diğer sıralamalarda null değerler yönden bağımsız sona gider, tie-break `id`. Kaldırılmış ve (varsayılan olarak) arşivlenmiş öğeler sonuçta yok.
- Snippet ve vurgulama HTML değil `{text, highlight}` parçalarıdır; UI metin olarak çizer (SEA-03). Eşleşme kaynağı (hangi alan) her sonuçta döner.
- `repo::rebuild_search_index` arama indeksini otoritatif satırlardan yeniden kurar (§11.7).

**Sonuçlar ve açık noktalar.**

- Başlık sıralaması SQLite `lower()` ile yapılıyor (yalnızca ASCII); Türkçe locale sıralaması (ICU paketlenmiyor) F1.7'de UI tarafında ele alınacak.
- Disk değiştirme (aynı sürücü harfine başka disk) `volume_id` ile ayırt edilmiyor; F2'de değerlendirilecek.
- `modified` varlığın türevleri (thumbnail) henüz `stale` işaretlenmiyor; şemada alan yok, dökümler gelmeden önce eklenecek.
- Uygulama henüz bu servisleri çağırmıyor; import ekranı ve açılışta `check_storage` F1.7'de bağlanır.

---

## ADR-0013: Library ekranı: komutlar, thumbnail protokolü ve sanallaştırma

STATE.md F1.7. SPEC §9.2, §9.3, §9.4, §14.2, §18, §18.2; IMP-01, IMP-06, SEA-02…04, DATA-09, INV-02.

**Bağlam.** F1.4–F1.6 veri katmanını kurdu ama uygulama onu çağırmıyordu. Library ekranı veritabanını açmalı, arama/filtre/sıralamayı tek sorguyla sunmalı, import'u başlatabilmeli ve küçük resimleri CSP'yi gevşetmeden göstermelidir.

**Karar: kütüphanenin açılması.** `setup`, kütüphane kilidinden hemen sonra `<data>/db/library.sqlite`'ı açar (SPEC §14.2); migration öncesi yedek `<data>/backups/`. Tek yazıcı bir mutex arkasında; her okuma kendi salt okunur bağlantısını açar (`vv_persistence::open_reader`), böylece import yazıcıyı tutarken ekran cevap vermeye devam eder. Açılışta arka planda `check_storage` (DATA-09) ve eksik küçük resimler (en çok 200) çalışır.

**Karar: komutlar.**

- `list_items(query)`: görünüm (`all`, `favorites`, `recent` = son 30 gün, `archive`), platform/tür filtresi, metin, sıralama ve sayfa tek sorguda. Enum değerleri serde ile kapalı listedir; bilinmeyen alan ya da değer çalıştırılmadan reddedilir. Cevapta `localState` medya varlığından ve kökün durumundan türetilir, `kind`'dan değil (INV-02): hazır özgün dosya yoksa `bookmark_only`; kök erişilebilir ve dosya bulunduysa `local_ready`; kök erişilemezse `disconnected` (disk çıkarıldı, dosya silinmiş sayılmaz); kök erişilebilir ama dosya yoksa `missing`.
- `get_item(id)`: inspector alanları. Medya için yalnızca dosya adı ve kök etiketi döner; tam yol backend'de kalır.
- `import_media(source)`: yerel dosya diyaloğu Rust'ta açılır (`tauri-plugin-dialog` 2.8.4, yalnızca Rust tarafında kullanılır; webview'a `dialog:` izni verilmedi). Pencereye bırakılan dosyalar `WindowEvent::DragDrop` ile yine Rust'ta alınır ve sonuç `import-status` olayıyla bildirilir. Webview hiçbir zaman dosya yolu göndermez; bu yüzden yol alan bir komut yoktur. Webview'a eklenen tek çekirdek izin olay dinlemedir (`core:event:allow-listen`, `allow-unlisten`).
- ffprobe/FFmpeg şu sırayla aranır: `VV_TOOLS_DIR` (yalnızca debug/E2E derlemesi), exe'nin yanındaki `tools/`, debug derlemede deponun `tools/bin`'i. Bulunamazsa import `TOOL_MISSING` döner ve UI bunu söyler. Kurulumla birlikte araçları dağıtmak F4'ün işi.

**Karar: thumbnail protokolü.** `vvthumb://localhost/<asset-id>` (Windows'ta `http://vvthumb.localhost/...`). İstek bir yol değil, UUID biçiminde bir varlık kimliği taşır; yalnızca cache kökündeki `role = thumbnail` varlıklar çözülür, kullanıcının kendi videosu bu yoldan okunamaz. Dosya canonical yolu kökün içinde değilse ya da 4 MiB'tan büyükse 404. CSP `img-src` yalnızca `'self'` ve bu şemaya izin verir; uzak görsel yine engellidir.

**Karar: arayüz.**

- Sonuç alanı ARIA `listbox` (çoklu seçim) ve `option` kartlardan oluşur; roving tabindex. Ok tuşları odağı taşır ve seçer, Shift aralık seçer, Ctrl yalnızca odağı taşır, Ctrl+Space işaretler, Ctrl+A yüklü olanları seçer, Enter inspector'ı açar, Esc en üstteki yüzeyi kapatıp odağı geri verir, Ctrl+K aramaya gider (§9.4). Durumlar renkle birlikte metin ve sembolle gösterilir.
- Sanallaştırma: kart en az 232 px, 16:9 küçük resim, sabit satır yüksekliği; yalnızca görünen satırlar ve üçer satır pay çizilir. Sayfa 100 öğe; sona yaklaşınca sonraki sayfa gelir. `aria-setsize`/`aria-posinset` toplamı söyler. Sanallaştırma için paket eklenmedi; hesap birkaç satır ve birim testli (`geometry`).
- `<900 px`: gezinme menü düğmesiyle açılan çekmeceye, inspector modal çekmeceye dönüşür. Görünüm (grid/liste) ve inspector tercihi `localStorage`'da; okunamazsa varsayılanlar kullanılır.
- Henüz çalışmayan bölümler (Inbox, Downloads, Collections, Sessions, AI, Settings) gizli (§9.3). Toplu seçim sayısı ve temizleme var; toplu eylemler F2'de.
- Fixture istemcisi `?fixture=empty|many|error|slow` ile ilk kullanım, 2.000 öğe, hata ve yükleniyor durumlarını üretir; Playwright bu durumların 3 boyutta ekran görüntüsünü alır.

**Sonuçlar ve açık noktalar.**

- Gerçek uygulamada dosya diyaloğu ve sürükle-bırak otomatik test edilemiyor (yerel pencere). Windows'ta elle kontrol gerekiyor (MT-04); gerçek uygulama E2E'si kütüphanenin açıldığını ve `list_items`'ı doğruluyor.
- Import ilerleme yüzdesi ve iptal düğmesi yok; import API'si iptal bayrağını alıyor, UI bağlantısı F2'de.
- Başlık sıralaması hâlâ SQLite `lower()` (ASCII). Sıralama sayfalı ve backend'de olduğu için UI'da düzeltilemedi; Türkçe sıralama F2'ye kaldı.
- Panel genişliğini sürükleyerek değiştirme, hover önizleme (P1), açık tema ve çeviri anahtarları (F2.8) yok.


## ADR-0014: Add URL: komutlar, Ctrl+K ve Ctrl+V ile yakalama

STATE.md F1.8. SPEC §7.1 (CAP-01…CAP-06, CAP-08), §7.3 (kaldırılmış öğenin URL'si), §8.1, §9.3, §9.4; NFR-01, NFR-02; AC-01, AC-02, AC-45.

**Bağlam.** `save_bookmark` (F1.2) ve normalizasyon (F1.3) hazırdı ama arayüzden çağrılamıyordu. Yolculuk 1, hazır bir bağlantının en çok iki birincil eylemle ve yaklaşık beş saniyede kütüphanede görünmesini ister; ağ işi bu süreye girmez.

**Karar: komutlar.**

- `check_url(url)`: kullanıcı yazarken çalışır. Doğrulama ve normalizasyon çevrimdışıdır; platform, biçim (`video`, `collection`, `short_link`, `unknown`), kanonik URL, bağlantıdan türetilen başlık, uyarılar ve varsa aynı içeriği tutan öğe döner. Hiçbir şey yazmaz; okuma bağlantısında çalışır.
- `save_url(url)`: `save_bookmark`'ı tek yazıcıda çalıştırır. Sonuç `created` ya da `existing`'dir. `existing` hiçbir şey yazmaz ve öğeyi gösterir (CAP-06). Öğe Recently removed'daysa `removed: true` döner.
- `restore_item(id)`: `removed_at`'i temizler. Notlar, override'lar ve ilişkiler kaldırmada silinmediği için öğeyle birlikte geri gelir (AC-45). Canlı bir öğede hiçbir şey değiştirmez.
- Reddedilen girdi yalnızca §19 kodunu taşır (`INVALID_URL`, `URL_BLOCKED`, `URL_HAS_CREDENTIALS`); yazılan metin hata nesnesine, loga veya cevaba girmez. Arayüze yalnızca kanonik biçim döner (CAP-08). 8 KiB'tan uzun girdi `INVALID_URL`'dir.
- Üç komut da `capabilities/main.json`'a tek tek eklendi; yeni bir çekirdek ya da eklenti izni yok.

**Karar: arayüz.**

- Üst çubukta birincil düğme "Add URL"; Import ikincil oldu. Diyalog açılınca alan odaktadır, yapıştırılan bağlantı 120 ms sonra denetlenir. Save Bookmark denetim cevabını beklemez: yapıştır ve Enter hemen kaydeder, backend aynı denetimleri yapar (CAP-03).
- Diyalog durumları: önizleme (platform ve başlık), alan hatası, uyarı (`PLAYLIST_UNSUPPORTED`), "zaten kütüphanede" (birincil eylem Open item), "Recently removed içinde" (birincil eylem Restore, yanında Cancel). Çevrimdışıyken önizleme, kaydın hemen yapıldığını ve ayrıntıların bağlantı gelince beklediğini söyler. Inspector'da `pending` metadata çevrimdışıyken "Offline" görünür (CAP-04).
- Kayıttan sonra All Items açılır, arama ve filtreler temizlenir, yeni öğe en üstte seçili ve odakta görünür, bildirim çıkar. Mevcut ya da geri yüklenen öğe aynı şekilde gösterilir ve inspector'da açılır.
- `Ctrl+K` aramaya gider. Arama kutusundaki metin bir bağlantıysa (`http(s)://` ya da `www.` ile başlıyorsa) aranmaz; altında "Save this link as a bookmark" önerisi çıkar, Enter Add URL'yi bağlantıyla açar (§9.4).
- `Ctrl+V`: odak bir metin alanında değilken yapıştırılan bağlantı Add URL'yi açar; metin alanlarında normal yapıştırma korunur. Pano yalnızca kullanıcının yapıştırma olayından okunur, arka planda hiç okunmaz (CAP-01).
- Diyalog `role="dialog"`, `aria-modal`; Tab içinde döner, Esc kapatır ve odağı açan düğmeye geri verir.

**Ölçüm.** `src-tauri/tests/save_latency.rs`, 10.000 öğelik kütüphanede `save_url`'ün çekirdeğini (doğrulama, normalizasyon, eşleşme araması, öğe ve FTS commit'i, liste satırı) 300 kez ölçer ve `test-artifacts/nfr-01/report.json` yazar. Linux konteynerde debug derlemede p95 1,4 ms (NFR-01 bütçesi 500 ms). Arayüz tarafında fixture E2E ve gerçek uygulama E2E'si, yapıştırmadan kartın görünmesine kadar geçen süreyi ölçer ve 5 s altında olmasını ister.

**Sonuçlar ve açık noktalar.**

- Etiket ve koleksiyon seçimi (CAP-05) ve Inbox görünümü koleksiyonlarla birlikte F2'de gelir. Koleksiyon olmadan Inbox, All Items ile aynı listeyi gösterirdi; bu yüzden kayıt şimdilik All Items'ta görünür.
- Metadata çekme ve güvenli önizleme (CAP-07) M2'de. O zamana kadar başlık bağlantıdan türetilir ve öğe `pending` kalır.
- Fixture istemcisindeki URL tanıma (`src/ipc/fixtureUrl.ts`) yalnızca ekran durumlarını sürmek içindir; gerçek kurallar Rust'tadır ve `tests/fixtures/normalize.json` ile test edilir.
- Yönlendirilmiş kullanım testi (MT-17) ve arka planda pano okunmadığının elle kontrolü (MT-01) henüz yapılmadı.
