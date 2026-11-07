# Karar kayıtları (ADR)

SPEC §27 bu dosyayı zorunlu tutar. SPEC'teki DEC-01…DEC-17 kararları da kanıtlarıyla buraya yazılacak (STATE.md F0.8, F0.9, F0.11). Her kayıt: bağlam, seçenekler, karar, sonuçlar ve kanıt.

| ADR | Konu | Durum | Tarih |
|---|---|---|---|
| [ADR-0001](#adr-0001-gerçek-uygulama-e2e-aracı) | Gerçek uygulama E2E aracı | Kabul edildi | 6 Kasım 2026 |
| [ADR-0002](#adr-0002-sqlite-erişimi-wal-ve-migration-stratejisi) | SQLite erişimi, WAL ve migration stratejisi (SPEC DEC-06) | Kabul edildi | 6 Kasım 2026 |
| [ADR-0003](#adr-0003-yt-dlp-adaptörü-sabit-bayraklar-progress-protokolü-ve-hata-eşleme) | yt-dlp adaptörü: sabit bayraklar, progress protokolü, hata eşleme (SPEC DL-05, §14.1) | Kabul edildi | 6 Kasım 2026 |
| [ADR-0004](#adr-0004-webview2-oynatma-matrisi-ve-yerel-medya-protokolü) | WebView2 oynatma matrisi ve yerel medya protokolü (SPEC DEC-04) | Kabul edildi (CI ölçümü; geliştirici makinesi ölçümü açık) | 7 Kasım 2026 |

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

