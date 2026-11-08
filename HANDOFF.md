# Devir notu — 8 Kasım 2026 (akşam)

Yarın buradan devam edilir. Projenin ayrıntılı durumu [STATE.md](./STATE.md) dosyasındadır. Bu dosya kaldığımız yeri ve ilk adımları özetler; bir sonraki oturum başlarken okunur, iş ilerledikçe güncellenir.

## Nerede kaldık

- **Tamamlananlar:** F0'ın tamamı, F1.1–F1.3 (PR #13) ve F1.4–F1.6 (PR #14, ADR-0012), 19/74. Hepsi main'de, Windows + Linux CI yeşil.
- **F1.7 (Library ekranı) 🟨:** PR #16 ile merge edildi; Windows'ta elle kontrol (MT-04) 8 Kas 2026'da geçti. Yalnızca yeşil CI bekliyor (GitHub release indirmeleri 504 veriyordu). ADR-0013.
  - Uygulama açılışta kütüphaneyi `<data>/db/library.sqlite`'ta açıyor, arka planda `check_storage` ve eksik küçük resimleri çalıştırıyor.
  - Komutlar: `list_items` (görünüm, filtre, arama, sıralama, sayfa), `get_item` (inspector), `import_media` (dosya diyaloğu Rust'ta). Pencereye bırakılan dosyalar da Rust'ta import ediliyor; webview hiç dosya yolu göndermiyor.
  - Küçük resimler `vvthumb` protokolüyle geliyor; CSP yalnızca buna izin veriyor.
  - Ekran: gezinme, arama (`Ctrl+K`), filtre chip'leri, grid/liste (sanallaştırılmış), çoklu seçim, inspector, ilk kullanım/yükleniyor/hata/sonuç yok/çevrimdışı durumları. 3 boyutta ekran görüntüleri `test-artifacts/screenshots/library-*.png`.
- **F1.8 (Add URL) 🟨:** PR #17 ile merge edildi; Windows'ta elle kontrol geçti, yeşil CI bekliyor. ADR-0014.
  - Komutlar `check_url` (yazarken denetim, yazmaz), `save_url`, `restore_item` (Recently removed'dan geri alma).
  - Üst çubukta Add URL; yapıştır + Enter kaydeder. `Ctrl+K` aramasına bağlantı yapıştırılırsa "Save this link" önerisi çıkar. Liste odaktayken `Ctrl+V` ile bağlantı yapıştırmak Add URL'yi açar.
  - NFR-01: 10.000 öğede p95 1,4 ms (Linux debug). Gerçek uygulama E2E'si kaydet → kapat → aç → ara akışını Windows'ta dener.
- **F2.1–F2.3 (düzenleme) 🟨:** PR #18 ile merge edildi; Windows'ta elle kontrol geçti. ADR-0015.
- **F2.4–F2.9 🟨:** `claude/project-thread-yn7xg0` dalında, PR açık. ADR-0016 (oynatıcı, `vvmedia`, ayarlar, Offline Mode), ADR-0017 (JSON dışa/içe aktarma, DB yedeği), ADR-0018 (çeviri anahtarları, tema, ilk açılış, erişilebilirlik).
  - Inspector'da "Play": oynatıcı kayıtlı konumdan devam etmeyi sorar, konumu kaydeder; HEVC'de uyarı ve "Open in another player".
  - Settings (⚙ ya da `Ctrl+,`): tema, Offline Mode, güncelleme denetimi, medya klasörü, dışa/içe aktarma, yedek ve geri yükleme, About.
  - Yeni kütüphanede beş adımlı ilk açılış; "Skip" varsayılanları korur.
- **Çıkış kapıları:** Yolculuk 2 ve 5 E2E'leri `e2e/journeys.spec.ts`'te. Ağsız akışın gerçek uygulama testi `e2e-desktop/offline-flow.spec.ts`; yalnızca Windows'ta çalışır.

## Sonraki ilk adımlar

1. `git checkout main && git pull` (PR merge edildikten sonra).
2. Windows'ta `npm run verify`, sonra `npm run desktop:e2e-exe` ve `npm run e2e:desktop`. Gerçek uygulama E2E'si için önce `node scripts/fetch-tools.mjs` (import ffprobe ister).
3. Aşağıdaki elle kontrol listesi. Geçenler STATE.md'de ✅ olur.
4. Sonra F3 (indirme motoru).

## Windows elle kontrol listesi (F1/F2 çıkış kapıları)

Hepsi `npm run desktop` ile, mümkünse yeni bir kütüphaneyle (`VV_DATA_DIR` boş bir klasör) yapılır.

1. **İlk açılış:** beş adım görünür; yalnızca klavyeyle (Tab, Enter, ok tuşları) sona kadar gidilir. Açık temayı seç; uygulamayı kapatıp aç: tema korunur, ilk açılış bir daha çıkmaz.
2. **Ağsız akış (F1 kapısı):** Wi-Fi'yi kapat ya da kabloyu çıkar. Bağlantı kaydet, bir video import et, etiketle, ara, oynat. Uygulamayı kapatıp aç: hepsi duruyor, arama buluyor.
3. **Offline Mode:** Settings'te aç. Şerit görünür; bookmark'ta "Open source page" Offline Mode hatası verir; şeritteki "Turn off" kapatır. Kapatıp açınca ayar korunur.
4. **Oynatıcı:** bir MP4'ü yarısına kadar izle (en az 5 saniye), kapat, uygulamayı yeniden başlat, aynı öğede "Resume from …" doğru zamanı gösterir (AC-49). ←/→, J/L, M, F tuşları çalışır. Narrator oynatıcı düğmelerini okur.
5. **HEVC:** HEVC bir dosya (ör. telefon videosu) import edip oynat: uyarı ve "yalnızca ses"/"oynatılamadı" açıklaması, "Open in another player" varsayılan oynatıcıyı açar.
6. **Bağlı olmayan sürücü:** harici diskteki bir videoyu import et, diski çıkar, oynatmayı dene: neden söylenir; diski tak, "Locate file…" ile yeniden bağla.
7. **JSON dışa/içe aktarma:** Settings → Export (dosya yollarıyla ve yolsuz iki kez). Boş bir kütüphanede Import: önizleme sayıları doğru; kökü kendi klasörüne eşle; etiketler, koleksiyonlar, notlar ve oynatma konumları gelir; eşlenmeyen kök "bağlı değil" görünür.
8. **DB yedeği:** Back up now; birkaç öğeyi sil/etiketle; listeden Restore: özet doğru, geri yükleme sonrası eski durum gelir, listede "Before restore" yedeği belirir. Bozuk bir dosyayı "Restore from a file…" ile seç: reddedilir, kütüphane değişmez.
9. **Erişilebilirlik:** Windows ayarlarında metin ölçeğini %200 yap: yatay kaydırma yok, Add URL ve arama klavyeyle çalışır. Yüksek karşıtlık temasında odak görünür.

## Bilinmesi gerekenler

- **Çalışma düzeni:** Claude bir dalda PR açar (son dal: `claude/project-thread-yn7xg0`). Cuma merge eder ve `C:\Users\cumae\VideoDownloader` içinde `git pull` yapar. STATE.md'de bir alt görev yalnızca kanıtla ✅ olur: CI linki, test sonucu ya da elle kontrol. CI yalnızca hazır (taslak olmayan) PR'larda çalışır; CI çalışmadıysa Windows'ta Cuma'nın ve Linux'ta Claude'un `npm run verify` raporları birlikte kanıt sayılır (`docs/testing.md` "CI ne zaman çalışır").
- **Windows yerel komutları:**
  - `npm run verify`: tüm kapılar.
  - `npm run desktop`: uygulamayı geliştirme modunda açar. Import için önce `node scripts/fetch-tools.mjs` (ffprobe/FFmpeg `tools/bin`'e iner); yoksa import "media tools missing" der.
  - `npm run dev:fixture`, sonra tarayıcıda `?fixture=empty|many|error|slow`: Library ekranının durumları sahte veriyle.
  - `npm run desktop:e2e-exe` ve ardından `npm run e2e:desktop`: gerçek uygulama E2E.
- **yt-dlp fixture'larını yeniden yakalamak** yalnızca yt-dlp sürümü değişince gerekir: `VV_YTDLP=<yol> npm run fixtures:ytdlp`. ffmpeg PATH'te olmalıdır; ayrıntılar ADR-0003'te.
- **Açık noktalar (ADR-0003):**
  - `MEDIA_URL_EXPIRED` henüz üretilmiyor.
  - Windows'a özgü ağ hata metinleri işletim sistemi metinlerinden yazıldı; gerçek Windows yakalaması, indirme işleri `vv-process` üzerinden çalışınca (F2) yapılacak.
  - Video ve sesin ayrı indirildiği durumda ilerlemeyi birleştirme işi F2'de.
- **FFmpeg kurulum boyutu (ADR-0006):** iki statik exe 252 MiB tutuyor. F4'te `lgpl-shared` varyantına geçmek değerlendirilecek.
- **Engeller:** STATE.md "Engeller ve açık noktalar" bölümünde. Uygulama simgesi hâlâ geçici.
