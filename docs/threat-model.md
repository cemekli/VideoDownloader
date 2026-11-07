# Tehdit modeli

SPEC §20.1 ve §27 bu dosyayı zorunlu tutar. Aşağıda önce M0'da kararı verilen ağ, dış araç ve süreç sınırları, ardından SPEC §20.1'deki her satırın bugünkü durumu yer alır. Mimari ve güven sınırlarının genel resmi: [architecture.md](./architecture.md).

## Dış araçlar ve ağ sınırı

| Bileşen | Ağa çıkabilir mi | Nasıl sınırlanıyor | Kanıt |
|---|---|---|---|
| yt-dlp | Evet: yalnızca HTTP(S) | Her bağlantı uygulamanın yerel proxy'sinden geçer (`vv-netguard`). Port 80/443; DNS sonrası tüm adresler public olmalı; kontrol edilen adrese bağlanılır (rebinding yok); jetonla erişim; `--downloader native`, `--abort-on-unavailable-fragments` | ADR-0007; `vv-netguard` testleri; `scripts/netguard-probe.mjs` (her CI çalışmasında) |
| Deno (yt-dlp'nin JS runtime'ı) | Hayır | `--allow-*` olmadan başlatılır; ağ, dosya, ortam ve süreç erişimi `NotCapable` | ADR-0005 |
| FFmpeg / ffprobe | Hayır | `-protocol_whitelist file`, `-format_whitelist` (referans veren biçimler yok), `file:<mutlak yol>` | ADR-0006; `scripts/ffmpeg-probe.mjs` (her CI çalışmasında) |
| Uygulama süreci | Henüz yok | Uzak metadata/küçük resim M2'de ve bu sınırın içinden | SPEC §5 |

Saldırgan modeli: kullanıcının yapıştırdığı URL'nin arkasındaki site kötü niyetli olabilir. Yönlendirme, manifest, playlist, segment adresi ve DNS cevabı saldırganın elindedir. Hedef: kullanıcının yerel ağına, loopback servislerine ya da bulut metadata adresine istek yaptırmak (SSRF).

Engellenen bir bağlantı işin `URL_BLOCKED` ile bitmesine yol açar. Bookmark kaydı korunur (SPEC §20.2).

## Süreç sınırı

| Tehdit | Önlem | Kanıt |
|---|---|---|
| Kullanıcı ortamındaki değişkenler araca geçer (`HTTP_PROXY`, `DENO_*`, `PYTHONPATH`) | `vv_process::ManagedCommand`: boş ortam ve kısa izin listesi | ADR-0008; `vv-process` testleri |
| Uygulama çöker, yt-dlp/Deno/FFmpeg sahipsiz kalıp yazmaya ya da ağa çıkmaya devam eder | Windows Job Object (`KILL_ON_JOB_CLOSE`), çocuk job'a girmeden çalışmaz | ADR-0008; Windows çökme testi |
| İki uygulama süreci aynı kütüphaneyi açar | single-instance eklentisi + veri dizininde özel kilit; kilitsiz süreç DB'yi açmaz | ADR-0008; AC-38 E2E |

## SPEC §20.1 kontrollerinin durumu

Her satır SPEC §20.1'deki bir varlık/girişe karşılık gelir. "Uygulandı" yalnızca kodda olan ve testle gösterilen kontrol içindir.

| Varlık / giriş | Uygulandı | Açık | Kanıt / plan |
|---|---|---|---|
| URL ve redirect | Yakalamada şema, kimlik bilgisi ve yerel adres reddi; araç trafiğinde bağlantı başına port, DNS sonrası IP ve rebinding denetimi | Uygulamanın kendi HTTP istemcisi yok (M2'de uzak küçük resim aynı sınırdan geçecek) | F0.7, ADR-0007; F3.3 |
| Metadata / altyazı | — | Güvenli render, alan ve çıktı limitleri | F3.4, AC-28 |
| Dosya adı / yolu | FFmpeg'e yalnızca `file:<mutlak yol>`; medya protokolü kök dışına çıkmıyor (test derlemesi); import'ta yollar canonical, kök göreli yol bileşen bazlı (string prefix değil), klasörde symlink/junction izlenmiyor, bulut yer tutucu dosyalar okunmuyor | Güvenli create/rename (indirme hedefi) | ADR-0004, ADR-0006, ADR-0012; F3.6, AC-30 |
| CLI girdisi | Argüman dizisi (kabuk yok), sabit bayraklar, temiz ortam | Kötü niyetli metadata ile metakarakter testleri | ADR-0003, ADR-0008; F4.4 |
| Cookie / profile | — | İzin akışı, OS secret store, redaksiyon | F5.1, F3.10, AC-18, AC-55 |
| Dış binary / model | Sürüm pin, SHA-256, yt-dlp GPG imzası, FFmpeg lisans kontrolü | İmzalı uygulama güncellemesi, model checksum | ADR-0005, ADR-0006; F4.2, F6.1, AC-25 |
| Medya parser | Protokol ve biçim izin listesi, referanslı girdi reddi (import'ta gerçek ffprobe ile AC-59 testi); iptal ve zaman aşımında süreç ağacı ölür; ffprobe 30 s, thumbnail 60 s sınırı | Kaynak (bellek/CPU) sınırı | ADR-0006, ADR-0008, ADR-0012; F3.1 |
| Arama sorgusu | Kullanıcı metni FTS5 sözdizimine dönüşmüyor: her terim tırnaklı dize, operatörler kelime; parametreli SQL; vurgulama HTML değil parça listesi | — | ADR-0012, AC-31 |
| IPC | Least privilege capability, CSP, `freezePrototype`; capability dışı komutlar reddediliyor | Navigasyon/origin negatif testlerinin genişletilmesi | ADR-0001; `tests/ipc.rs`, `e2e-desktop/desktop-shell.spec.ts`; F4.4 |
| Export / import | — | Alan izin listesi, önizleme, şema doğrulama | F2.5, AC-15, AC-16, AC-30 |
| Tek süreç | İkinci süreç DB'yi açmıyor | — | ADR-0008, AC-38 |

## Açık noktalar

- curl_cffi ve websockets yolları ayrı sınanmadı (ADR-0007).
- İşletim sistemi düzeyinde süreç başına ağ kuralı (WFP) ikinci katman olarak değerlendirilecek.
- WebView2 trafiği NFR-11'e göre ayrıca ölçülür.
