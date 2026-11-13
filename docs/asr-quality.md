# Transkripsiyon kalite ve hız ölçümü (F6.7)

SPEC AI-A07, AC-21, DEC-11; yöntem ADR-0034. Bu dosya ölçümün kendisidir: hangi videoda, hangi makinede, neye karşı. Sayılar uygulama genelinde bir doğruluk ya da hız sözü değildir.

## Nasıl ölçülür (Windows, release exe)

1. Bir **Türkçe** ve bir **İngilizce** video seç. En iyisi yükleyicinin kendi altyazısı olan videolar (YouTube'da "Türkçe" yazan iz; "Türkçe (otomatik oluşturulan)" değil). Yoksa otomatik altyazı da olur, ama rapor o zaman doğruluk değil uyum ölçer.
2. Uygulamada her video için Details → Transcript → **Get site captions**.
3. Aynı videoyu her modelle yazıya dök: Tiny, Base, Small, Medium (dil: Auto ya da videonun dili). Her model ayrı döküm olarak saklanır; biri diğerini silmez. AI Processing → Processor use: **Full** (F6.2 süreleriyle karşılaştırılabilsin).
4. Medium çalışırken Görev Yöneticisi → Ayrıntılar'da `whisper-cli.exe`'nin en yüksek bellek kullanımını not et (aşağıdaki tabloya).
5. Depo klasöründe:

   ```
   git pull
   npm run asr:report
   ```

   Bu, yerel dökümü olan öğeleri listeler. Sonra iki videoyla:

   ```
   npm run asr:report -- --item "<TR başlığından bir parça>" --item "<EN başlığından bir parça>" --terms "Kadıköy,İstanbul"
   ```

   `--terms`'e videoda geçen özel isimleri yaz (kişi, yer, ürün). Rapor `test-artifacts\asr\report.md`'ye kaydedilir ve ekrana basılır.
6. Raporu bu dosyanın "Sonuçlar" bölümüne yapıştır (ya da Claude'a gönder).

İsteğe bağlı: **Light** ile bir kez daha Medium çalıştırıp süreyi Full ile karşılaştır (F6.6 testi). Rapor her model için en son işi gösterdiğinden Light ölçümünü raporu aldıktan sonra yap ve süresini aşağıya elle yaz.

## Okuma

- **WER**: 100 kelimede kaç kelime hatalı (yerine başka kelime, eksik ya da fazla). Düşük iyi.
- **WER, ç ğ ı ö ş ü hariç**: yalnız Türkçe harfi yanlış kelimeler hata sayılmazsa. WER'den farkı, Türkçe harf hatalarının payıdır (aramada `ışık` yazan biri `isik`'i bulamaz).
- **CER**: harf düzeyinde hata; `Kadıköy'de` / `Kadıköyden` gibi ek hataları WER kadar cezalandırmaz.
- **Noktalama / 100 kelime**: whisper noktalama yazar, otomatik altyazı yazmaz; not dışa aktarma ve okunabilirlik için önemli.
- **Gerçek zamanın katı**: 2× = 10 dakikalık video 5 dakikada yazıya döküldü.

## Daha önce ölçülenler

| Tarih | Makine | Video | Model | Süre | Gerçek zamanın katı | Not |
|---|---|---|---|---|---|---|
| 11 Kas 2026 | Cuma'nın PC'si, CPU (F6.2) | 21:44, dili kayda geçmedi | Tiny | 0:49 | ~27× | elle ölçüldü, kalite karşılaştırılmadı |
| 11 Kas 2026 | aynı | aynı | Base | 1:20 | ~16× | |
| 11 Kas 2026 | aynı | aynı | Small | 3:29 | ~6× | |
| 11 Kas 2026 | aynı | aynı | Medium | 10:29 | ~2× | |

## Bellek (elle, Görev Yöneticisi)

| Tarih | Model | `whisper-cli.exe` en yüksek bellek | Uygulamanın tahmini (ADR-0033) |
|---|---|---|---|
| 13 Kas 2026 | Medium (TR, 7:38) | ~2,5 GB; işlemci %33–34 (Full = 8 iş parçacığı / 24) | ~2,7 GB (tahmin tuttu) |

## Sonuçlar

### 13 Kasım 2026, Cuma'nın PC'si

Makine: i7-13700HX (24 mantıksal CPU), 31,7 GiB RAM, Windows 11 Pro, güç modu Balanced, Processor use: Full. Tam rapor: [`asr-reports/2026-11-13-cuma-pc.md`](asr-reports/2026-11-13-cuma-pc.md).

İki videoda da site altyazısı yoktu; referans **Medium**'un dökümü. Yani sayılar küçük modellerin Medium'dan ne kadar ayrıldığını gösterir, doğruluğu değil (Medium'un kendi hataları görünmez).

| Video | Model | Medium'dan fark (WER) | CER | Süre | Gerçek zamanın katı |
|---|---|---|---|---|---|
| TR, 7:38 | Tiny | 35,8 % | 13,2 % | 0:29 | 15,7× |
| TR | Base | 22,9 % | 8,3 % | 0:47 | 9,7× |
| TR | Small | 11,5 % | 4,1 % | 1:52 | 4,1× |
| TR | Medium | referans | — | 4:59 | 1,5× |
| EN, 17:31 | Tiny | 7,6 % | 4,1 % | 0:49 | 21,2× |
| EN | Base | 6,0 % | 3,2 % | 1:14 | 14,0× |
| EN | Small | 4,1 % | 2,2 % | 3:26 | 5,1× |
| EN | Medium | referans | — | 10:54 | 1,6× |

Bulgular:

- **Türkçe küçük modellerde çok bozuluyor**: Tiny her üç kelimeden birinde Medium'dan ayrılıyor, Base her beşte birde, Small her dokuzda birde. Hatalar çoğunlukla ek ve kelime kesimi (`farklı` → `fartla`, `karakteri` → `karakter`, `ikinci` → `ikinli`). Türkçe için Small en az, Medium tercih.
- **Türkçe harf (ç ğ ı ö ş ü) hatası yok denecek kadar az**: harfler hariç tutulunca WER yalnız Tiny'de 0,3 puan düşüyor. Sorun harf değil, kelimenin kendisi.
- **İngilizcede modeller birbirine yakın** (%4–8): İngilizce için Tiny ya da Base aramada yeterli olabilir; 4–14 kat daha hızlı.
- **Özel isimler küçük modellerin zayıf yeri**: "Claude" kelimesini Tiny 24 kez, Base 22 kez `cloud` yazdı; "Claude Code"u Medium 5, Small 4, Tiny ve Base 0 kez doğru yazdı. Small da 7 kez `clot` yazdı.
- **Noktalama** bütün modellerde benzer (TR 18–21, EN 11–12 / 100 kelime).
- **Hız** F6.2 ölçümüyle uyumlu (11 Kas: Tiny ~27×, Base ~16×, Small ~6×, Medium ~2×). Bu ölçüm biraz daha yavaş: Balanced güç modu ve Full = 8 iş parçacığı.
- **Bellek**: Medium ~2,5 GB, uygulamanın tahmini ~2,7 GB (ADR-0033) tuttu.

Bu ölçüm sırasında bulunan hata: whisper son pencerede sesin bittiği yerden sonra biten satır yazınca iş en sonda `segment N: ends after the media ends` ile düşüyordu (TR Small, Stanford videosu). PR #57 ile düzeltildi; Small sonra sorunsuz bitti.

Açık kalan: gerçek doğruluk için insan eliyle denetlenmiş (ya da yükleyicinin yazdığı) bir altyazı ile ölçüm (`--reference`). Öneri: whisper'a videonun başlığını ipucu (`--prompt`) olarak vermek özel isimleri düzeltebilir; ölçülmeden açılmamalı.
