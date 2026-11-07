# Devir notu — 7 Kasım 2026

Yarın buradan devam edilir. Projenin ayrıntılı durumu [STATE.md](./STATE.md) dosyasındadır. Bu dosya kaldığımız yeri ve ilk adımları özetler; bir sonraki oturum başlarken okunur, iş ilerledikçe güncellenir.

## Nerede kaldık

- **Tamamlananlar:** F0.1–F0.8 ve F0.13, toplam 9/74. F0.8a (oynatma matrisi, ADR-0004) ve F0.8b (yt-dlp + Deno, ADR-0005) main'de; F0.8c PR'da, CI yeşil.
- **Açık PR:** F0.8c, yani FFmpeg (ADR-0006). Dal: `claude/project-thread-t0tup8`.
  - `tools/manifest.json` artık FFmpeg ve ffprobe 9.0.2'yi de sabitliyor: BtbN LGPL build'i, tarihli etiket (`autobuild-2026-10-31-12-59`).
  - Yeni `vv-ffmpeg` crate'i: her çağrının sabit bayrakları ve ffprobe sonucunun sınıflandırılması.
  - Önemli bulgu: yalnızca protokol izin listesi yetmiyor; HLS listesi kök dışındaki dosyayı okuyabiliyor. `-format_whitelist` ile bu kapandı.
  - Yeni hata kodları `INPUT_REJECTED` ve `MEDIA_CORRUPT`.
  - `npm run tools:fetch` artık FFmpeg'i de indirip doğruluyor, AC-59 yoklamasını ve remux ölçümünü yapıyor. Windows ve Linux CI'da gerçek exe'lerle geçti: [CI #42367817432](https://github.com/cemekli/VideoDownloader/actions/runs/42367817432). 20 dakikalık MKV, Windows'ta 609 ms'de MP4'e remux edildi.

## Sonraki ilk adımlar

1. F0.8c PR'ını merge et, ardından `git checkout main && git pull`.
2. Kendi makinende `npm run tools:fetch` çalıştır. İlk çalıştırma yaklaşık 210 MB indirir.
3. Sıradaki iş: F0.9 (ağ denetimi, DEC-05) ya da F0.10 (single-instance, Job Object; ADR-0005'teki ortam temizleme kuralı burada uygulanacak).

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
- **FFmpeg kurulum boyutu (ADR-0006):** iki statik exe 252 MiB tutuyor. F4'te `lgpl-shared` varyantına geçmek değerlendirilecek.
- **Engeller:** STATE.md "Engeller ve açık noktalar" bölümünde. Uygulama simgesi hâlâ geçici.
