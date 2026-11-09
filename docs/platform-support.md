# Platform desteği

SPEC §6 ve §25.4'ün istediği tarihli destek matrisi. **Bu belgede "destekleniyor" yazan bir satır yalnızca tarihli, gerçek bir testin sonucudur.** Fixture ile geçen bir test canlı platform başarısı sayılmaz (INV-10, SPEC §25.1). İlk gerçek platform testleri F3.11'de yapıldı (9 Kasım 2026, izinli gerçek URL'ler); oturumlu erişim F5.6'da eklenecek.

Son güncelleme: 9 Kasım 2026, F3.11.

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
| YouTube | supported | supported | supported | — | 9 Kas 2026 | yt-dlp 2026.10.26 + Deno | Cuma, Windows 11, release exe ve geliştirme derlemesi; girişsiz (çerez yok), `vv-netguard` proxy arkasında. 720p indirildi ve uygulamada oynadı (F3 çıkış kapısı). |
| Facebook | supported | supported | supported (herkese açık video) | untested | 9 Kas 2026 | yt-dlp 2026.10.26 | Cuma, Windows 11, release exe ve geliştirme derlemesi; girişsiz (çerez yok), `vv-netguard` proxy arkasında. |
| Instagram | supported | supported | supported (herkese açık video) | untested | 9 Kas 2026 | yt-dlp 2026.10.26 | Cuma, Windows 11, release exe ve geliştirme derlemesi; girişsiz (çerez yok), `vv-netguard` proxy arkasında. |
| TikTok | supported | supported | supported | — | 9 Kas 2026 | yt-dlp 2026.10.26 | Cuma, Windows 11, release exe ve geliştirme derlemesi; girişsiz (çerez yok), `vv-netguard` proxy arkasında. |
| X | supported | supported | supported (herkese açık video) | untested | 9 Kas 2026 | yt-dlp 2026.10.26 | Cuma, Windows 11, release exe ve geliştirme derlemesi; girişsiz (çerez yok), `vv-netguard` proxy arkasında. Bir video sessiz indi: ses kodeği bildirilmeyen format atılıyordu, PR #26'da düzeltildi; düzeltmeden sonra yeniden denenmedi. |
| VK | supported | supported | supported (herkese açık video) | untested | 9 Kas 2026 | yt-dlp 2026.10.26 | Cuma, Windows 11, release exe ve geliştirme derlemesi; girişsiz (çerez yok), `vv-netguard` proxy arkasında. Oturumlu erişim F5.6. |
| Dailymotion | supported | supported | supported | — | 9 Kas 2026 | yt-dlp 2026.10.26 | Cuma, Windows 11, release exe ve geliştirme derlemesi; girişsiz (çerez yok), `vv-netguard` proxy arkasında. Önce genel URL sayılıyordu ve format listesi gelmiyordu; PR #26 ile ayrı platform oldu, sonra indirme çalıştı. |
| Twitch | supported | supported | conditional: VOD ve klip indirilir; canlı yayın reddedilir (SPEC §6) | — | 9 Kas 2026 | yt-dlp 2026.10.26 | Cuma, Windows 11, release exe ve geliştirme derlemesi; girişsiz (çerez yok), `vv-netguard` proxy arkasında. Eski bir VOD indirildi. Canlı kanal önce sonsuz bekliyordu; PR #28'den beri `INPUT_REJECTED` (`live-stream`). |
| Reddit | supported | conditional: Firefox oturumuyla (F5.2) | conditional: Firefox oturumuyla (F5.2) | supported (Firefox oturumu, F5.2) | 9 Kas 2026 | yt-dlp 2026.10.26 | Cuma, Windows 11, release exe ve geliştirme derlemesi; girişsiz (çerez yok), `vv-netguard` proxy arkasında. Paylaşım linki (`/r/<sub>/s/<kod>`) doğru gönderiye yönlendi; Reddit gönderi JSON'unu girişsiz vermedi: yt-dlp `Account authentication is required`. Aynı sonuç yt-dlp proxy'siz, tek başına çalıştırıldığında da alındı, yani engel uygulamada değil. Video dosyasının sağ tıkla kopyalanan adresi (`packaged-media.redd.it`) geçici, imzalı bir medya adresidir, gönderi değildir (PR #30). 9 Kas 2026 akşam (PR #35 sonrası, release exe): Settings → Sessions'ta Firefox profili bağlandı, gönderi indirildi ve oynatıldı (Cuma). 9 Kas 2026 22:52 (PR #36 sonrası): Sessions ekranında Check now geçti, yani §12.1 (a) kanıtı ve `session_available` (Cuma). |
| Genel URL (bookmark) | untested | — | — | — | — | — | Sözdizimsel URL politikası test edildi (F0.7), kayıt akışı F1.2'de |

Yukarıdaki satırlar elle yapılmış tek seferlik testlerdir; platformlar değiştikçe yeniden denenmelidir. Otomatik testler gerçek yt-dlp'yi yalnızca yerel test sunucusuna karşı çalıştırır (ADR-0003, ADR-0007); onlar platform desteği kanıtı değildir.

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

**Oturum doğrulama (§12.1, ADR-0024).** Platforma özgü, yalnızca oturum açıkken geçen bir kontrol (kanıt (b)) hiçbir platform için yok: uygulamanın istekleri yalnızca yt-dlp üzerinden gider ve yt-dlp böyle bir kontrol sunmaz. Bu yüzden her platformda oturum yalnızca (a) ile doğrulanır: giriş gerektirmiş bir öğe profilsiz reddedilir, profille açılır. Sessions ekranındaki "Check now" bunu kaydedilmiş en yeni böyle öğeyle yapar; öyle bir öğe yoksa doğrulama yapılamaz.

**Yöntemler.** Firefox profili (F5.2, Reddit'te 9 Kas 2026'da çalıştı) ve dışa aktarılmış çerez dosyası (F5.3, yalnızca Windows; gerçek platformla henüz denenmedi, VK-10). Chrome ve Edge Windows'ta doğrudan okunamaz (app-bound şifreleme); bunlardaki oturum çerez dosyası olarak dışa aktarılarak kullanılabilir.

## Kayıt kuralı

Yeni bir satır ya da durum değişikliği için: tarih, uygulama commit'i, araç sürümleri, işletim sistemi, kullanılan izinli URL'nin türü (gerçek URL ve çerezler yazılmaz), adımlar ve kanıt yolu (`test-artifacts/...` ya da CI linki). Başarısız sonuçlar da yazılır (SPEC §25.4).
