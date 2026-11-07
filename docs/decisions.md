# Karar kayıtları (ADR)

SPEC §27 bu dosyayı zorunlu tutar. SPEC'teki DEC-01…DEC-17 kararları da kanıtlarıyla buraya yazılacak (STATE.md F0.8, F0.9, F0.11). Her kayıt: bağlam, seçenekler, karar, sonuçlar ve kanıt.

| ADR | Konu | Durum | Tarih |
|---|---|---|---|
| [ADR-0001](#adr-0001-gerçek-uygulama-e2e-aracı) | Gerçek uygulama E2E aracı | Kabul edildi | 6 Kasım 2026 |
| [ADR-0002](#adr-0002-sqlite-erişimi-wal-ve-migration-stratejisi) | SQLite erişimi, WAL ve migration stratejisi (SPEC DEC-06) | Kabul edildi | 6 Kasım 2026 |
| [ADR-0003](#adr-0003-yt-dlp-adaptörü-sabit-bayraklar-progress-protokolü-ve-hata-eşleme) | yt-dlp adaptörü: sabit bayraklar, progress protokolü, hata eşleme (SPEC DL-05, §14.1) | Kabul edildi | 6 Kasım 2026 |
| [ADR-0004](#adr-0004-webview2-oynatma-matrisi-ve-yerel-medya-protokolü) | WebView2 oynatma matrisi ve yerel medya protokolü (SPEC DEC-04) | Kabul edildi (CI ölçümü; geliştirici makinesi ölçümü açık) | 7 Kasım 2026 |
| [ADR-0005](#adr-0005-yt-dlp-ve-js-runtime-dağıtımı-ve-güven-modeli) | yt-dlp ve JS runtime (Deno) dağıtımı ve güven modeli (SPEC DEC-02) | Kabul edildi | 7 Kasım 2026 |
| [ADR-0006](#adr-0006-ffmpeg-build-lisans-ve-yerel-dosya-sınırı) | FFmpeg build'i, lisansı ve yerel dosya sınırı (SPEC DEC-03, §14.1, AC-59) | Kabul edildi | 7 Kasım 2026 |
| [ADR-0007](#adr-0007-dış-araçların-ağ-denetimi-yerel-denetim-proxysi) | Dış araçların ağ denetimi: yerel denetim proxy'si (SPEC DEC-05, §20.2, AC-29) | Kabul edildi | 7 Kasım 2026 |

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
