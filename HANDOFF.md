# Devir notu — 9 Kasım 2026

Projenin ayrıntılı durumu [STATE.md](./STATE.md) dosyasındadır. Bu dosya kaldığımız yeri ve ilk adımları özetler; bir sonraki oturum başlarken okunur, iş ilerledikçe güncellenir.

## Nerede kaldık

- **Son durum (9 Kas):** F3'ün kalanı tek PR'da yazıldı (dal `claude/project-thread-qp6onk`; ADR-0021). F3.2–F3.6 ve F3.8–F3.10 🟨: kod ve otomatik testler hazır, Linux `npm run verify` geçti. ✅ için CI ve Windows'ta gerçek bir indirme gerekiyor.
- **Tamamlananlar:** F0, F1, F2 (30/74; M1 bitti), F3.1 (PR #23) ve F3.7 (PR #24): 32/74.
- **Uygulamada şu an:** öncekilere ek olarak bookmark kaydedilince arka planda başlık, kanal, süre ve küçük resim gelir. Inspector'da "Download…" ile dört önayardan biri seçilir (Best, 1080p, 720p, Audio). Downloads ekranı faz, ilerleme, hata cümlesi, Try again / Cancel / Delete partial file gösterir. Settings'te depolama kullanımı, kota, "Move media folder…" ve araç kontrolü (Diagnostics) var.

## Sonraki ilk adımlar

1. PR'ı incele, CI'ın yeşil olduğunu gör, merge et; sonra `git checkout main && git pull`.
2. Windows'ta `node scripts/fetch-tools.mjs`, sonra `npm run desktop`. Settings → Diagnostics'te dört araç "OK" olmalı.
3. İzinli bir gerçek URL kaydet, metadata gelsin, Download → 720p, Downloads ekranında bitmesini izle, Play ile oynat. Bu F3 çıkış kapısının "izinli gerçek indirme uçtan uca" maddesidir.
4. F3.4'ün gerçek URL testi ve F3.11 platform matrisi: Cuma'nın seçeceği izinli URL'lerle; sonuç tarihli olarak `docs/platform-support.md`'ye.
5. Sonra F3 ✅ ve F4 (Windows dağıtımı).

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
