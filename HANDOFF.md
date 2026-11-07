# Devir notu — 7 Kasım 2026

Yarın buradan devam edilir. Projenin ayrıntılı durumu [STATE.md](./STATE.md) dosyasındadır. Bu dosya kaldığımız yeri ve ilk adımları özetler; bir sonraki oturum başlarken okunur, iş ilerledikçe güncellenir.

## Nerede kaldık

- **Tamamlananlar:** F0.1–F0.7 ve F0.13, toplam 8/74. Hepsi main'de. F0.8 üç parçaya bölündü ve yarım (🟨).
- **Açık PR:** [#6 F0.8a: WebView2 oynatma matrisi](https://github.com/cemekli/VideoDownloader/pull/6), dal `claude/project-thread-t0tup8`. Windows ve Linux CI yeşil: [CI #42361348985](https://github.com/cemekli/VideoDownloader/actions/runs/42361348985).
  - 14 test videosu `tests/fixtures/media/` altında. Range destekli `vvmedia` protokolü prototipi var; yalnızca test derlemesinde.
  - Matris WebView2 156'da ölçüldü (ADR-0004). MP4, WebM, MKV ve MOV; H.264, VP8, VP9 ve AV1 ile oynuyor. HEVC her container'da yalnızca sesle oynuyor.
  - Kararlar: HEVC oynatılamaz sayılır; PLAY-07 remux kapsamı daralır; varsayılan hedef H.264 için MP4, VP9/AV1 için WebM.

## Sonraki ilk adımlar

1. PR #6'yı merge et, ardından `git checkout main && git pull`.
2. İstersen kendi makinende matrisi ölç: `npm run desktop:e2e-exe`, sonra `npm run e2e:desktop`. Tablo `test-artifacts/playback-matrix/matrix.md` dosyasına yazılır; HEVC'nin GPU'lu makinedeki durumunu gösterir.
3. Sıradaki alt görev:
   - **F0.8b:** yt-dlp + JS runtime (Deno) dağıtımı, güncelleme güven modeli ve lisans (DEC-02).
   - **F0.8c:** FFmpeg build'i, lisansı ve MKV'den MP4'e remux süresi (DEC-03).
   - Alternatifler: F0.9 (ağ denetimi), F0.10 (single-instance ve Job Object).

## Bilinmesi gerekenler

- **Çalışma düzeni:** Claude bir dalda PR açar (son PR: `claude/project-thread-t0tup8`). Cuma merge eder ve `C:\Users\cumae\VideoDownloader` içinde `git pull` yapar. STATE.md'de bir alt görev yalnızca kanıtla ✅ olur: CI linki, test sonucu ya da elle kontrol.
- **Windows yerel komutları:**
  - `npm run verify`: tüm kapılar.
  - `npm run desktop`: uygulamayı geliştirme modunda açar.
  - `npm run desktop:e2e-exe` ve ardından `npm run e2e:desktop`: gerçek uygulama E2E.
- **yt-dlp fixture'larını yeniden yakalamak** yalnızca yt-dlp sürümü değişince gerekir: `VV_YTDLP=<yol> npm run fixtures:ytdlp`. ffmpeg PATH'te olmalıdır; ayrıntılar ADR-0003'te.
- **Açık noktalar (ADR-0003):**
  - `MEDIA_URL_EXPIRED` henüz üretilmiyor.
  - Windows'a özgü ağ hata metinleri işletim sistemi metinlerinden yazıldı; gerçek Windows yakalaması F0.10'da yapılacak.
  - Video ve sesin ayrı indirildiği durumda ilerlemeyi birleştirme işi F2'de.
- **Engeller:** STATE.md "Engeller ve açık noktalar" bölümünde. Uygulama simgesi hâlâ geçici.
