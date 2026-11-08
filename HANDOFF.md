# Devir notu — 8 Kasım 2026

Yarın buradan devam edilir. Projenin ayrıntılı durumu [STATE.md](./STATE.md) dosyasındadır. Bu dosya kaldığımız yeri ve ilk adımları özetler; bir sonraki oturum başlarken okunur, iş ilerledikçe güncellenir.

## Nerede kaldık

- **Tamamlananlar:** F0'ın tamamı, F1.1–F1.3 (PR #13) ve F1.4–F1.6 (PR #14, ADR-0012), 19/74. Hepsi main'de, Windows + Linux CI yeşil.
- **F1.7 (Library ekranı) 🟨:** PR #16 ile merge edildi; Windows'ta elle kontrol (MT-04) 8 Kas 2026'da geçti. Yalnızca yeşil CI bekliyor (GitHub release indirmeleri 504 veriyordu). ADR-0013.
  - Uygulama açılışta kütüphaneyi `<data>/db/library.sqlite`'ta açıyor, arka planda `check_storage` ve eksik küçük resimleri çalıştırıyor.
  - Komutlar: `list_items` (görünüm, filtre, arama, sıralama, sayfa), `get_item` (inspector), `import_media` (dosya diyaloğu Rust'ta). Pencereye bırakılan dosyalar da Rust'ta import ediliyor; webview hiç dosya yolu göndermiyor.
  - Küçük resimler `vvthumb` protokolüyle geliyor; CSP yalnızca buna izin veriyor.
  - Ekran: gezinme, arama (`Ctrl+K`), filtre chip'leri, grid/liste (sanallaştırılmış), çoklu seçim, inspector, ilk kullanım/yükleniyor/hata/sonuç yok/çevrimdışı durumları. 3 boyutta ekran görüntüleri `test-artifacts/screenshots/library-*.png`.
- **F1.8 (Add URL) 🟨:** PR #17 ile merge edildi; Windows'ta elle kontrol geçti, yeşil CI bekliyor. ADR-0014.
  - Komutlar `check_url` (yazarken denetim, yazmaz), `save_url`, `restore_item` (Recently removed'dan geri alma).
  - Üst çubukta Add URL; yapıştır + Enter kaydeder. `Ctrl+K` aramasına bağlantı yapıştırılırsa "Save this link" önerisi çıkar. Liste odaktayken `Ctrl+V` ile bağlantı yapıştırmak Add URL'yi açar.
  - NFR-01: 10.000 öğede p95 1,4 ms (Linux debug). Gerçek uygulama E2E'si kaydet → kapat → aç → ara akışını Windows'ta dener.
- **F2.1–F2.3 (düzenleme) 🟨:** `claude/project-thread-yn7xg0` dalında, PR açık. ADR-0015.
  - Migration 0003: etiket, koleksiyon (ağaç), akıllı koleksiyon, Undo günlüğü, sahipsiz medya.
  - Kenar çubuğu: Inbox (sayaçlı), koleksiyon ağacı ("⋯" menüsü: yeniden adlandır, alt koleksiyon, taşı, sil), akıllı koleksiyonlar, etiketler (yeniden adlandır, birleştir, sil), Duplicates, Archive, Recently removed.
  - Seçim olunca toplu işlem çubuğu: Tag, Add to collection, Favorite, Archive, Remove. Bildirimde Undo; `Ctrl+Z` de geri alır. `Delete` seçili öğeleri Recently removed'a taşır.
  - Inspector'da favori, arşiv, etiketler, koleksiyonlar ve not düzenlenir.
  - Recently removed: Restore ve "Delete for good" (indirilmiş dosyaları silmek ayrı onay). Silinmeyen indirilmiş dosyalar "Orphaned media"da; oradan yeniden import edilebilir.
  - Duplicates: iki öğeden birini seç, önizlemeyi gör, birleştir; Undo geri alır.
  - Kaydedilen bookmark artık Inbox'ta açılır.

## Sonraki ilk adımlar

1. `git checkout main && git pull`.
2. GitHub indirmeleri düzelince CI'ı yeniden çalıştır; Windows + Linux yeşilse STATE.md F1.7 satırını ✅ yap ve CI linkini ekle.
3. F1.8 PR'ı Windows CI'da (gerçek uygulama E2E dahil) yeşilse F1.8'i ✅ yap. Elle: Add URL ile bir bağlantı kaydet, uygulamayı kapatıp aç, aramada bul; listede odak varken `Ctrl+V` ile bağlantı yapıştır (MT-01).
4. F2.1–F2.3 PR'ı: Windows CI (gerçek uygulama E2E'si `e2e-desktop/organize.spec.ts` dahil) yeşilse ve elle kontrol geçerse ✅. Elle: birkaç öğeyi seç, etiketle, Undo; koleksiyon oluştur ve alt koleksiyonlu silmeyi iki seçenekle dene; bir öğeyi kaldır, Recently removed'dan geri al; uygulamayı kapatıp açınca `Ctrl+Z`'nin "Nothing to undo" dediğini gör.
5. F1 çıkış kapısı: ağ kapalıyken kaydet/import/ara/yeniden başlat akışı Windows'ta; Yolculuk 2 (eski bookmark'ı bul) için E2E. Sonra F2.4 (oynatıcı).

## Bilinmesi gerekenler

- **Çalışma düzeni:** Claude bir dalda PR açar (son dal: `claude/project-thread-yn7xg0`). Cuma merge eder ve `C:\Users\cumae\VideoDownloader` içinde `git pull` yapar. STATE.md'de bir alt görev yalnızca kanıtla ✅ olur: CI linki, test sonucu ya da elle kontrol.
- **Windows yerel komutları:**
  - `npm run verify`: tüm kapılar.
  - `npm run desktop`: uygulamayı geliştirme modunda açar. Import için önce `node scripts/fetch-tools.mjs` (ffprobe/FFmpeg `tools/bin`'e iner); yoksa import "media tools missing" der.
  - `npm run dev:fixture`, sonra tarayıcıda `?fixture=empty|many|error|slow`: Library ekranının durumları sahte veriyle.
  - `npm run desktop:e2e-exe` ve ardından `npm run e2e:desktop`: gerçek uygulama E2E.
- **yt-dlp fixture'larını yeniden yakalamak** yalnızca yt-dlp sürümü değişince gerekir: `VV_YTDLP=<yol> npm run fixtures:ytdlp`. ffmpeg PATH'te olmalıdır; ayrıntılar ADR-0003'te.
- **Açık noktalar (ADR-0003):**
  - `MEDIA_URL_EXPIRED` henüz üretilmiyor.
  - Windows'a özgü ağ hata metinleri işletim sistemi metinlerinden yazıldı; gerçek Windows yakalaması, indirme işleri `vv-process` üzerinden çalışınca (F2) yapılacak.
  - Video ve sesin ayrı indirildiği durumda ilerlemeyi birleştirme işi F2'de.
- **FFmpeg kurulum boyutu (ADR-0006):** iki statik exe 252 MiB tutuyor. F4'te `lgpl-shared` varyantına geçmek değerlendirilecek.
- **Engeller:** STATE.md "Engeller ve açık noktalar" bölümünde. Uygulama simgesi hâlâ geçici.
