# Devir notu — 7 Kasım 2026

Yarın buradan devam edilir. Projenin ayrıntılı durumu [STATE.md](./STATE.md) dosyasındadır. Bu dosya kaldığımız yeri ve ilk adımları özetler; bir sonraki oturum başlarken okunur, iş ilerledikçe güncellenir.

## Nerede kaldık

- **Tamamlananlar:** F0'ın tamamı, F1.1–F1.3 (PR #13) ve F1.4–F1.6 (PR #14, ADR-0012), 19/74. Hepsi main'de, Windows + Linux CI yeşil. 7 Kasım akşamı burada durduk; açık PR yok.
  - `crates/vv-import`: dosya/klasör import (yerinde referans, kısıtlı ffprobe, SHA-256, duplicate adayı), thumbnail, relink ve `roots::check_storage` (çıkarılan disk `disconnected`, kaybolan dosya `missing`).
  - Arama: `vv-persistence::library::search`; güvenli sorgu oluşturucu, filtre, sıralama, sayfa, snippet. Türkçe davranış `tests/fixtures/search-tr.json`'da.
  - Import testleri gerçek ffprobe/FFmpeg ister: yerelde `node scripts/fetch-tools.mjs` çalışmadıysa atlanır, CI'da zorunludur.
  - Uygulama bu servisleri henüz çağırmıyor; Library ekranı (F1.7) ve Add URL (F1.8) bağlayacak.

## Sonraki ilk adımlar

1. Bu devir notunun PR'ı merge edildiyse `git checkout main && git pull`.
2. F1.7: Library ekranı (grid/liste, inspector, filtreler, boş ve hata durumları); arama ve import servislerini Tauri komutlarına bağlar.
3. Ardından F1.8: Add URL (`Ctrl+K`, `save_bookmark`).

## Bilinmesi gerekenler

- **Çalışma düzeni:** Claude bir dalda PR açar (son PR: `claude/project-thread-t0tup8`). Cuma merge eder ve `C:\Users\cumae\VideoDownloader` içinde `git pull` yapar. STATE.md'de bir alt görev yalnızca kanıtla ✅ olur: CI linki, test sonucu ya da elle kontrol.
- **Windows yerel komutları:**
  - `npm run verify`: tüm kapılar.
  - `npm run desktop`: uygulamayı geliştirme modunda açar.
  - `npm run desktop:e2e-exe` ve ardından `npm run e2e:desktop`: gerçek uygulama E2E.
- **yt-dlp fixture'larını yeniden yakalamak** yalnızca yt-dlp sürümü değişince gerekir: `VV_YTDLP=<yol> npm run fixtures:ytdlp`. ffmpeg PATH'te olmalıdır; ayrıntılar ADR-0003'te.
- **Açık noktalar (ADR-0003):**
  - `MEDIA_URL_EXPIRED` henüz üretilmiyor.
  - Windows'a özgü ağ hata metinleri işletim sistemi metinlerinden yazıldı; gerçek Windows yakalaması, indirme işleri `vv-process` üzerinden çalışınca (F2) yapılacak.
  - Video ve sesin ayrı indirildiği durumda ilerlemeyi birleştirme işi F2'de.
- **FFmpeg kurulum boyutu (ADR-0006):** iki statik exe 252 MiB tutuyor. F4'te `lgpl-shared` varyantına geçmek değerlendirilecek.
- **Engeller:** STATE.md "Engeller ve açık noktalar" bölümünde. Uygulama simgesi hâlâ geçici.
