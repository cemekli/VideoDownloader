# Devir notu — 7 Kasım 2026

Yarın buradan devam edilir. Projenin ayrıntılı durumu [STATE.md](./STATE.md) dosyasındadır. Bu dosya kaldığımız yeri ve ilk adımları özetler; bir sonraki oturum başlarken okunur, iş ilerledikçe güncellenir.

## Nerede kaldık

- **Tamamlananlar:** F0'ın tamamı, 13/74 (F0.11 ve F0.12 PR #11'de, CI yeşil: [CI #42376811094](https://github.com/cemekli/VideoDownloader/actions/runs/42376811094)). DEC-01 sürüm kaydı ADR-0010.
- **Açık PR #11:** F0.11 (zorunlu dokümanlar ve kapsama matrisi) ve F0.12 (benchmark). Dal: `claude/project-thread-t0tup8`.
  - Yeni: `docs/architecture.md`, `docs/roadmap.md`, `docs/platform-support.md`. `threat-model.md` artık SPEC §20.1'in her satırının durumunu gösteriyor.
  - Kapsama matrisi `docs/coverage.json`'da: 82 P0/INV/NFR satırının her biri bir AC'ye, kimlikli manuel teste (MT) ya da benchmark'a (BM) bağlı. `npm run coverage:write` tabloyu `docs/testing.md`'ye yazar; `verify` eksik bağı ve eskimiş tabloyu reddeder.
  - F0.12: `crates/vv-benchdata` aynı seed'den her işletim sisteminde aynı 10.000 öğe ve 100.000 segmenti üretir (golden hash testi). `npm run bench` BM-01…03'ü ölçer ve makineyi referans sınıfına göre etiketler (ADR-0009). CI bilgi amaçlı çalıştırır; bütçe kapısı F4.5'te.

## Sonraki ilk adımlar

1. Bu küçük PR'ı merge et, ardından `git checkout main && git pull`.
2. Sıradaki iş F1.1 (alan şeması ve repository'ler), ardından F1.2 (bookmark kaydı).

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
