# Devir notu — 7 Kasım 2026

Yarın buradan devam edilir. Projenin ayrıntılı durumu [STATE.md](./STATE.md) dosyasındadır. Bu dosya kaldığımız yeri ve ilk adımları özetler; bir sonraki oturum başlarken okunur, iş ilerledikçe güncellenir.

## Nerede kaldık

- **Tamamlananlar:** F0'ın tamamı, 13/74. DEC-01 sürüm kaydı ADR-0010.
- **Açık PR #13** (dal `claude/project-thread-t0tup8`): F1.1–F1.3 (ADR-0011). Benchmark kaydı PR #12 ile merge edildi.
  - Şema: migration 0002 ile `storage_root`, `library_item`, `media_asset`, `url_alias`, `duplicate_candidate` ve FTS5 `search_document`. Repository'ler `crates/vv-persistence/src/library/repo.rs`.
  - Kayıt: `library::bookmark::save_bookmark` ağsız çalışır; aynı içerik ikinci kez kaydedilince mevcut öğe döner, eşzamanlı kayıt tek öğede biter, imza/sır değerleri diske yazılmaz.
  - Normalizasyon: `vv-domain::normalize` (sürüm 1), fixture `tests/fixtures/normalize.json`. Kural değişince sürüm artar; `library::renormalize` eski kayıtları yeniden hesaplar (AC-44).
  - Uygulama henüz veritabanını açmıyor; `save_bookmark` komutu ve Add URL ekranı F1.8'de bağlanacak.

## Sonraki ilk adımlar

1. PR #13'ü merge et, ardından `git checkout main && git pull`.
2. Sıradaki iş F1.4 (yerel import: kısıtlı ffprobe, hash, duplicate adayı, thumbnail).
3. SPEC §11.7'deki kenar durum (ADR-0011 "SPEC'e not") için SPEC'e bir cümle eklenip eklenmeyeceğine karar ver.

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
