# Devir notu — 7 Kasım 2026

Yarın buradan devam edilir. Projenin ayrıntılı durumu [STATE.md](./STATE.md) dosyasındadır. Bu dosya kaldığımız yeri ve ilk adımları özetler; bir sonraki oturum başlarken okunur, iş ilerledikçe güncellenir.

## Nerede kaldık

- **Tamamlananlar:** F0.1–F0.9 ve F0.13, toplam 10/74 (F0.9 PR'da, CI yeşil). FFmpeg (F0.8c, ADR-0006) PR #8 ile main'de.
- **Açık PR:** F0.9, yani dış araçların ağ denetimi (ADR-0007). Dal: `claude/project-thread-t0tup8`.
  - Yeni `vv-netguard` crate'i: yt-dlp'nin her bağlantısı uygulamanın yerel proxy'sinden geçiyor. Kurallar: port 80/443, DNS sonrası tüm adresler public, kontrol edilen adrese bağlanma (rebinding yok), çalışma başına jeton.
  - yt-dlp bayrakları: `--proxy <url> --downloader native --abort-on-unavailable-fragments` (`args::NETWORK_GUARD`).
  - `scripts/netguard-probe.mjs` gerçek yt-dlp ile 12 vaka çalıştırıyor (yönlendirme, HLS segmenti, gömülü video, CONNECT, localhost). Kanarya sunucu hiçbirinde bağlantı almıyor; Windows ve Linux CI'da geçti: [CI #42369808856](https://github.com/cemekli/VideoDownloader/actions/runs/42369808856).
  - `docs/threat-model.md` ağ bölümüyle başladı.

## Sonraki ilk adımlar

1. F0.9 PR'ını merge et, ardından `git checkout main && git pull`.
2. Sıradaki iş: F0.10 (single-instance, Job Object, alt süreçlerin temiz ortamla başlatılması; ADR-0005 ve ADR-0007'deki kurallar burada uygulanacak).

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
