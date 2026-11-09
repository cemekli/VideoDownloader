# Platform desteği

SPEC §6 ve §25.4'ün istediği tarihli destek matrisi. **Bu belgede "destekleniyor" yazan bir satır yalnızca tarihli, gerçek bir testin sonucudur.** Fixture ile geçen bir test canlı platform başarısı sayılmaz (INV-10, SPEC §25.1). Henüz gerçek platform testi yapılmadı; ilk kayıtlar F3.11'de (izinli gerçek URL'lerle) ve F5.6'da (VK) eklenecek.

Son güncelleme: 7 Kasım 2026, F0.11.

Durum değerleri: `supported` (tarihli kanıt var), `conditional` (koşul yazılı), `unsupported` (denendi, çalışmıyor), `untested` (denenmedi).

## İşletim sistemi ve çalışma ortamı

| Hedef | Durum | Tarih | Ortam | Kanıt |
|---|---|---|---|---|
| Windows 11 x64: build, açılış, gerçek IPC | supported (geliştirme derlemesi) | 6 Kas 2026 | Cuma'nın dizüstü bilgisayarı, Windows 11 | F0.3 elle doğrulandı (STATE.md) |
| Windows Server (GitHub Actions `windows-latest`), WebView2 156: E2E, oynatma matrisi, araçlar, tek süreç | supported (CI) | 7 Kas 2026 | GPU'suz CI makinesi | [CI #42373404361](https://github.com/cemekli/VideoDownloader/actions/runs/42373404361) |
| Temiz Windows 11'de installer, Python/Node/Rust olmadan | untested | — | — | MT-20, MT-21; installer F4.1'de |
| Windows 10, ARM64 | untested | — | — | NFR-14: ayrı doğrulama |
| Linux x64 | Yalnızca geliştirme ve CI testleri; dağıtım hedefi değil | 7 Kas 2026 | `ubuntu-latest` | aynı CI |
| macOS, Android | untested | — | — | Android M6 |

## Platform (URL kaynağı) matrisi

| Platform | Tanıma | Public metadata | İndirme | Oturumlu erişim | Son test | Araç sürümü | Kanıt |
|---|---|---|---|---|---|---|---|
| YouTube | untested | untested | untested | — | — | yt-dlp 2026.10.26 | — |
| Facebook | untested | untested | untested | untested | — | yt-dlp 2026.10.26 | — |
| Instagram | untested | untested | untested | untested | — | yt-dlp 2026.10.26 | — |
| TikTok | untested | untested | untested | — | — | yt-dlp 2026.10.26 | — |
| X | untested | untested | untested | untested | — | yt-dlp 2026.10.26 | — |
| VK | untested | untested | untested | untested | — | yt-dlp 2026.10.26 | — |
| Dailymotion | untested | untested | untested | — | — | yt-dlp 2026.10.26 | — |
| Twitch | untested | untested | untested | — | — | yt-dlp 2026.10.26 | — |
| Reddit | untested | untested | untested | — | — | yt-dlp 2026.10.26 | — |
| Genel URL (bookmark) | untested | — | — | — | — | — | Sözdizimsel URL politikası test edildi (F0.7), kayıt akışı F1.2'de |

Bugüne kadar gerçek yt-dlp yalnızca yerel test sunucusuna karşı çalıştırıldı (fixture yakalama, ADR-0003; ağ denetimi, ADR-0007). Bunlar platform desteği kanıtı değildir.

## Medya biçimi (WebView2 oynatma)

Ölçüm: [ADR-0004](./decisions.md), `apps/desktop/e2e-desktop/playback-matrix.spec.ts`, 7 Kasım 2026, WebView2 156, GPU'suz CI.

| Container | H.264 | VP8 | VP9 | AV1 | HEVC |
|---|---|---|---|---|---|
| MP4 | supported (zorunlu test; `moov` sonda da) | — | ölçüldü: oynuyor | ölçüldü: oynuyor | unsupported (yalnızca ses) |
| WebM | — | ölçüldü: oynuyor | ölçüldü: oynuyor | ölçüldü: oynuyor | — |
| MKV | ölçüldü: oynuyor | — | ölçüldü: oynuyor (`canPlayType` boş dönse de) | ölçüldü: oynuyor | unsupported (yalnızca ses) |
| MOV | ölçüldü: oynuyor (`canPlayType` boş dönse de) | — | — | — | unsupported (yalnızca ses) |

"Ölçüldü: oynuyor" bir CI ölçümüdür, her sürümde zorunlu tutulan bir test değildir. Yalnızca MP4 H.264/AAC her çalışmada zorunludur.

Ayrıntılı sonuç her CI çalışmasında `test-artifacts/playback-matrix/` altına yazılır. GPU'lu bir Windows 11 makinesinde ölçüm hâlâ açık (STATE.md engeller tablosu).

## Kimlik doğrulama (M3)

VK-01…VK-10 canlı test matrisi SPEC §21.2'de tanımlıdır. Hiçbiri çalıştırılmadı. Sonuçlar F5.6'da buraya tarihli yazılır.

## Kayıt kuralı

Yeni bir satır ya da durum değişikliği için: tarih, uygulama commit'i, araç sürümleri, işletim sistemi, kullanılan izinli URL'nin türü (gerçek URL ve çerezler yazılmaz), adımlar ve kanıt yolu (`test-artifacts/...` ya da CI linki). Başarısız sonuçlar da yazılır (SPEC §25.4).
