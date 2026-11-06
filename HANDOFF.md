# Devir notu — 6 Kasım 2026 akşamı

Yarın buradan devam edilir. Projenin ayrıntılı durumu [STATE.md](./STATE.md) dosyasındadır. Bu dosya kaldığımız yeri ve ilk adımları özetler; bir sonraki oturum başlarken okunur, iş ilerledikçe güncellenir.

## Nerede kaldık

- **Tamamlananlar:** F0.1, F0.2, F0.3, F0.4, F0.5, F0.6, F0.7, F0.13 (8/74). F0.6 dışındakiler main'de; F0.6 PR #5'in merge'ini bekliyor.
  - Tauri 2 kabuğu ve gerçek IPC.
  - Windows'ta gerçek uygulama E2E (Playwright + WebView2 CDP, ADR-0001).
  - SQLite, migration altyapısı ve FTS5 (ADR-0002).
- **Açık PR:** [#5 F0.6: yt-dlp fixture adaptörü](https://github.com/cemekli/VideoDownloader/pull/5), dal `feat/f0.6-ytdlp-fixtures`.
  - Linux'ta `npm run verify` 8/8 geçti.
  - Yeni crate `vv-ytdlp`: sabit bayraklar, progress şablonu, metadata/progress ayrıştırma, sınırlı satır okuma, stderr redaksiyonu ve SPEC §19 hata eşleme (ADR-0003).
  - Fixture'lar `tests/fixtures/ytdlp/` altında: 15 gerçek yt-dlp 2026.10.26 çalışması ve kaynak metinlerinden 20 hata vakası.
  - Windows ve Linux CI yeşil: [CI #42281072226](https://github.com/cemekli/VideoDownloader/actions/runs/42281072226). STATE.md'de F0.6 ✅.

## Yarın ilk adımlar

1. PR #5 henüz merge edilmediyse merge et, sonra yerelde `git checkout main && git pull` çalıştır.
2. Sıradaki alt görevi seç:
   - **F0.8: araç ve lisans araştırması** (önerilen). yt-dlp + JS runtime (Deno), FFmpeg build'i ve WebView2 codec/container matrisi (DEC-02/03/04). F0.6'nın sabitlediği yt-dlp sürümünün dağıtım ve lisans tarafını kapatır.
   - F0.9: dış araçların ağ denetimi (DEC-05).
   - F0.10: single-instance ve Windows Job Object ile alt süreç yönetimi. `vv-ytdlp` burada gerçek süreçle bağlanır.

## Bilinmesi gerekenler

- **Çalışma düzeni:** Claude, `feat/...` dalında PR açar. Cuma merge eder ve `C:\Users\cumae\VideoDownloader` içinde `git pull` yapar. STATE.md'de bir alt görev yalnızca kanıtla ✅ olur: CI linki, test sonucu ya da elle kontrol.
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
