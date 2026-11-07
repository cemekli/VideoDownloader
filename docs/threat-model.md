# Tehdit modeli

SPEC §20.1 ve §27 bu dosyayı zorunlu tutar. Bu ilk sürüm, M0'da kararı verilen ağ ve dış araç sınırlarını kapsar; diğer satırlar (IPC, dosya yolu, oturum, export) ilgili alt görevlerde eklenecek (STATE.md F0.11).

## Dış araçlar ve ağ sınırı

| Bileşen | Ağa çıkabilir mi | Nasıl sınırlanıyor | Kanıt |
|---|---|---|---|
| yt-dlp | Evet: yalnızca HTTP(S) | Her bağlantı uygulamanın yerel proxy'sinden geçer (`vv-netguard`). Port 80/443; DNS sonrası tüm adresler public olmalı; kontrol edilen adrese bağlanılır (rebinding yok); jetonla erişim; `--downloader native`, `--abort-on-unavailable-fragments` | ADR-0007; `vv-netguard` testleri; `scripts/netguard-probe.mjs` (her CI çalışmasında) |
| Deno (yt-dlp'nin JS runtime'ı) | Hayır | `--allow-*` olmadan başlatılır; ağ, dosya, ortam ve süreç erişimi `NotCapable` | ADR-0005 |
| FFmpeg / ffprobe | Hayır | `-protocol_whitelist file`, `-format_whitelist` (referans veren biçimler yok), `file:<mutlak yol>` | ADR-0006; `scripts/ffmpeg-probe.mjs` (her CI çalışmasında) |
| Uygulama süreci | Henüz yok | Uzak metadata/küçük resim M2'de ve bu sınırın içinden | SPEC §5 |

Saldırgan modeli: kullanıcının yapıştırdığı URL'nin arkasındaki site kötü niyetli olabilir. Yönlendirme, manifest, playlist, segment adresi ve DNS cevabı saldırganın elindedir. Hedef: kullanıcının yerel ağına, loopback servislerine ya da bulut metadata adresine istek yaptırmak (SSRF).

Engellenen bir bağlantı işin `URL_BLOCKED` ile bitmesine yol açar. Bookmark kaydı korunur (SPEC §20.2).

## Açık noktalar

- curl_cffi ve websockets yolları ayrı sınanmadı (ADR-0007).
- İşletim sistemi düzeyinde süreç başına ağ kuralı (WFP) ikinci katman olarak değerlendirilecek.
- WebView2 trafiği NFR-11'e göre ayrıca ölçülür.
