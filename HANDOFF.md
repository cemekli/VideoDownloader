# Devir notu — 8 Kasım 2026

Yarın buradan devam edilir. Projenin ayrıntılı durumu [STATE.md](./STATE.md) dosyasındadır. Bu dosya kaldığımız yeri ve ilk adımları özetler; bir sonraki oturum başlarken okunur, iş ilerledikçe güncellenir.

## Nerede kaldık

- **Tamamlananlar:** F0, F1 ve F2'nin tamamı (30/74; M1 bitti). F1: PR #13, #14, #16, #17. F2: PR #18–#22; ADR-0015…0018. Kanıt: Cuma'nın Windows 11 `npm run verify` raporu (8 Kas 2026 15:12, gerçek uygulama E2E dahil) + Linux `verify` + Windows elle kontrol listesi (8 Kas 15:22).
- Elle kontrolde bulunan tek sorun: Play'e basınca video başlamıyordu; PR #22 ile düzeltildi.
- Uygulamada şu an: bookmark kaydetme, yerel import, arama, etiket/koleksiyon/toplu işlem/Undo, duplicate birleştirme, oynatıcı (kaldığı yerden devam), JSON dışa/içe aktarma, DB yedeği ve geri yükleme, Offline Mode, ilk açılış, açık/koyu tema. Ağ kullanan hiçbir özellik henüz yok.

## Sonraki ilk adımlar

1. `git checkout main && git pull`.
2. F3 (indirme motoru, M2'nin ilk yarısı). F3.1 kalıcı iş kuyruğu yazıldı (`crates/vv-persistence/src/jobs.rs`, ADR-0019; işçi çalışma zamanı henüz yok). F3.7 hata sınıflandırması yazıldı (`crates/vv-persistence/src/classify.rs`, ADR-0020). Kalan sıra: F3.2 araç sağlık kontrolü → F3.3 ağ politikası → F3.4 uzak metadata → F3.5/F3.6 format ve indirme → F3.8 Downloads ekranı. F3.11 izinli gerçek URL'ler ister; Cuma'nın seçeceği URL'lerle yapılır.

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
