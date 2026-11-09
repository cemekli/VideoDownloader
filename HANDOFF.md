# Devir notu — 9 Kasım 2026

Projenin ayrıntılı durumu [STATE.md](./STATE.md) dosyasındadır. Bu dosya kaldığımız yeri ve ilk adımları özetler; bir sonraki oturum başlarken okunur, iş ilerledikçe güncellenir.

## Nerede kaldık

- **Son durum (9 Kas, akşam):** F3 tamamlandı (PR #25–#30). Cuma Windows 11'de YouTube, Facebook, Instagram, TikTok, X, VK ve Twitch VOD ve Dailymotion indirdi; sonuçlar `docs/platform-support.md`'de. Reddit girişsiz engelli (yt-dlp tek başına da `Account authentication is required` diyor).
- **Tamamlananlar:** F0, F1, F2, F3 (41/74).
- **Sıra değişikliği:** F4 (installer, imzalı güncelleme) ertelendi; sıradaki faz **F5 yetkili oturum** (Cuma'nın kararı, 9 Kas).
- **Uygulamada şu an:** bookmark kaydedilince başlık, kanal, süre ve küçük resim gelir; Download ile dört önayar; Downloads ekranı; canlı yayınlar reddedilir; yerel import thumbnail'leri arka planda, en yeniden başlayarak, paralel yapılır. Bilinen platformlar: YouTube, Facebook, Instagram, TikTok, X, VK, Dailymotion, Twitch, Reddit.

## Sonraki ilk adımlar

1. F5.1: `AuthProfile`, izin akışı, işletim sistemi secret store (SPEC §11.6, §20.3, DEC-08). Önce bir ADR.
2. F5.2: `--cookies-from-browser`. Windows'ta Chrome çerezleri app-bound şifreli; önce Firefox, Chrome için F5.7 (uygulamaya ait tarayıcı profili) gerekebilir. İlk gerçek hedef: Reddit.

## Bilinmesi gerekenler

- **Çalışma düzeni:** Claude bir dalda PR açar, Cuma merge eder ve `C:\Users\cumae\VideoDownloader` içinde `git pull` yapar. STATE.md'de bir alt görev yalnızca kanıtla ✅ olur: CI linki, test sonucu ya da elle kontrol. CI yalnızca hazır (taslak olmayan) PR'larda çalışır.
- **Windows yerel komutları:**
  - `npm run verify`: tüm kapılar.
  - `npm run desktop`: uygulamayı geliştirme modunda açar. İndirme ve import için önce `node scripts/fetch-tools.mjs` (yt-dlp, Deno, FFmpeg, ffprobe `tools/bin`'e iner); yoksa Diagnostics "Missing" der ve Download düğmesi çalışmaz.
  - `npm run dev:fixture`, sonra tarayıcıda `?fixture=empty|many|error|slow|first-run|download-error`. `download-error`: her indirmenin ilk denemesi ağ hatasıyla düşer, Try again başarır (Yolculuk 6).
  - `npm run desktop:e2e-exe` ve ardından `npm run e2e:desktop`: gerçek uygulama E2E.
- **Testler:** motor `crates/vv-engine/tests` (sahte yt-dlp `src/bin/vv-fake-ytdlp.rs`; `real_ytdlp.rs` gerçek araçlarla yerel sunucuya karşı, araçlar yoksa yerelde atlanır, CI'da zorunlu).
- **Açık noktalar:** oturum gerektiren içerik `blocked`/`auth` olur (giriş M3'te). Pause motor tarafında var, arayüzde yok. `MEDIA_URL_EXPIRED` henüz üretilmiyor (ADR-0003). FFmpeg kurulum boyutu (ADR-0006) F4'te.
- **Engeller:** STATE.md "Engeller ve açık noktalar" bölümünde. Uygulama simgesi hâlâ geçici.
