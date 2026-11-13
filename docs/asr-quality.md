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
| — | Medium | not run | ~2,7 GB |

## Sonuçlar

not run: `npm run asr:report` raporu bekleniyor.
