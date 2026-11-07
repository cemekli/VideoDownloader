# Devir notu — 7 Kasım 2026

Yarın buradan devam edilir. Projenin ayrıntılı durumu [STATE.md](./STATE.md) dosyasındadır. Bu dosya kaldığımız yeri ve ilk adımları özetler; bir sonraki oturum başlarken okunur, iş ilerledikçe güncellenir.

## Nerede kaldık

- **Tamamlananlar:** F0.1–F0.9 ve F0.13, toplam 11/74 (F0.10 PR'da, CI yeşil). Ağ denetimi (F0.9, ADR-0007) PR #9 ile main'de.
- **Açık PR:** F0.10, yani tek süreç ve alt süreç yönetimi (ADR-0008). Dal: `claude/project-thread-t0tup8`.
  - İkinci açılış `tauri-plugin-single-instance` ile çalışan pencereye devredilir ve 0 koduyla çıkar.
  - Veri dizinindeki `videovault.lock` ikinci katmandır: kilidi alamayan süreç 3 koduyla çıkar, DB'yi açmaz.
  - Yeni `vv-process` crate'i: alt süreçler temiz ortamla, Windows'ta Job Object içinde başlar. İptal, zaman aşımı ve uygulama çökmesi tüm ağacı (yt-dlp → Deno/FFmpeg) öldürür.
  - `get_diagnostics` artık `libraryLock` ve `secondLaunches` döndürüyor.
  - Windows ve Linux CI'da geçti: [CI #42373404361](https://github.com/cemekli/VideoDownloader/actions/runs/42373404361).

## Sonraki ilk adımlar

1. F0.10 PR'ını merge et, ardından `git checkout main && git pull`.
2. Sıradaki iş: F0.11 (zorunlu dokümanlar ve kapsama matrisi) ya da F0.12 (benchmark veri üreticisi). F0 bitince F1'e geçilir.

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
