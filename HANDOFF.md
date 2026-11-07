# Devir notu — 7 Kasım 2026

Yarın buradan devam edilir. Projenin ayrıntılı durumu [STATE.md](./STATE.md) dosyasındadır. Bu dosya kaldığımız yeri ve ilk adımları özetler; bir sonraki oturum başlarken okunur, iş ilerledikçe güncellenir.

## Nerede kaldık

- **Tamamlananlar:** F0.1–F0.10 ve F0.13, toplam 11/74. Tek süreç ve alt süreç yönetimi (F0.10, ADR-0008) PR #10 ile main'de.
- **Açık PR:** F0.11, yani zorunlu dokümanlar ve kapsama matrisi. Dal: `claude/project-thread-t0tup8`.
  - Yeni: `docs/architecture.md`, `docs/roadmap.md`, `docs/platform-support.md`. `threat-model.md` artık SPEC §20.1'in her satırının durumunu gösteriyor.
  - Kapsama matrisi `docs/coverage.json`'da: 82 P0/INV/NFR satırının her biri bir AC'ye, kimlikli manuel teste (MT) ya da benchmark'a (BM) bağlı. `npm run coverage:write` tabloyu `docs/testing.md`'ye yazar; `verify` eksik bağı ve eskimiş tabloyu reddeder.
- **Sıradaki:** F0.12 (benchmark ortamı ve sürümlü veri üreticisi).

## Sonraki ilk adımlar

1. F0.11 PR'ını merge et, ardından `git checkout main && git pull`.
2. F0.12 bitince F0 çıkış kapısı kontrol edilir ve F1'e (çevrimdışı kütüphane çekirdeği) geçilir.

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
