# Devir notu — 7 Kasım 2026

Yarın buradan devam edilir. Projenin ayrıntılı durumu [STATE.md](./STATE.md) dosyasındadır. Bu dosya kaldığımız yeri ve ilk adımları özetler; bir sonraki oturum başlarken okunur, iş ilerledikçe güncellenir.

## Nerede kaldık

- **Tamamlananlar:** F0.1–F0.7 ve F0.13, toplam 8/74. F0.8a (oynatma matrisi, ADR-0004) PR #6 ile main'de. F0.8 hâlâ yarım (🟨).
- **Açık PR:** F0.8b, yani yt-dlp + Deno dağıtımı (ADR-0005). Dal: `claude/project-thread-t0tup8`.
  - `tools/manifest.json` sürümleri sabitliyor: yt-dlp 2026.10.26 (GPG imzası doğrulandı) ve Deno 2.9.8.
  - Yeni `vv-tools` crate'i: manifesti doğruluyor, hash kontrolü yapıyor, araçların sürüm ve JS runtime çıktısını okuyor.
  - Yeni hata kodu `CHECKSUM_MISMATCH`.
  - yt-dlp artık yalnızca yönetilen Deno'yu kullanıyor; `PATH`'teki Deno yok sayılıyor ve bu gerçek çalıştırmayla kanıtlandı.
  - `npm run tools:fetch` araçları `tools/bin/` altına indirip doğruluyor. CI'da da çalışıyor.

## Sonraki ilk adımlar

1. F0.8b PR'ını merge et, ardından `git checkout main && git pull`.
2. Kendi makinende `npm run tools:fetch` çalıştır (Git for Windows içindeki gpg yeterli).
3. **F0.8c:** FFmpeg build'i, lisansı ve MKV'den MP4'e remux süresi (DEC-03).
4. Alternatifler: F0.9 (ağ denetimi), F0.10 (single-instance, Job Object; ADR-0005'teki ortam temizleme kuralı burada uygulanacak).

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
