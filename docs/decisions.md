# Karar kayıtları (ADR)

SPEC §27 bu dosyayı zorunlu tutar. SPEC'teki DEC-01…DEC-17 kararları da kanıtlarıyla buraya yazılacak (STATE.md F0.8, F0.9, F0.11). Her kayıt: bağlam, seçenekler, karar, sonuçlar ve kanıt.

| ADR | Konu | Durum | Tarih |
|---|---|---|---|
| [ADR-0001](#adr-0001-gerçek-uygulama-e2e-aracı) | Gerçek uygulama E2E aracı | Kabul edildi | 6 Kasım 2026 |

---

## ADR-0001: Gerçek uygulama E2E aracı

**Bağlam.** SPEC §25.1, fixture IPC ile geçen E2E testinin gerçek Tauri IPC'nin çalıştığını kanıtlamadığını söyler. STATE.md F0.4, gerçek uygulama penceresinde çalışan bir E2E altyapısı ister. Sürüm hedefi Windows 11 ve WebView2'dir (SPEC §3). Arayüz testleri zaten Playwright ile yazılıyor.

**Seçenekler.**

| | Playwright + WebView2 CDP | tauri-driver + WebDriver |
|---|---|---|
| Nasıl | Uygulama `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=N` ile açılır, Playwright `connectOverCDP` ile bağlanır | `tauri-driver`, Windows'ta `msedgedriver`'ı, Linux'ta `WebKitWebDriver`'ı sarar; testler WebdriverIO/Selenium ile yazılır |
| Platform | Yalnızca Windows (WebView2) | Windows ve Linux; macOS yok |
| Ek araç | Yok; mevcut Playwright | `tauri-driver` (cargo) + WebView2 sürümüyle eşleşen `msedgedriver` + WebdriverIO |
| Test dili | Mevcut Playwright testleriyle aynı API ve assertion'lar | Ayrı bir test yığını |
| Kırılganlık | WebView2'nin belgelenmiş ortam değişkenine bağlı | Driver sürümü WebView2 runtime sürümüyle uyumsuz kalınca kırılır |

**Karar.** Gerçek uygulama E2E testleri Playwright + WebView2 CDP ile yazılır ve Windows'ta çalışır (`apps/desktop/e2e-desktop/`).

**Gerekçe.** Sürüm hedefi Windows'tur ve orada tek test yığınıyla (Playwright) hem fixture hem gerçek uygulama testleri yazılabilir. Sürüm eşleştirmesi gereken ek driver yoktur. Microsoft, WebView2 uygulamalarının Playwright ile CDP üzerinden test edilmesini belgeler.

**Sonuçlar.**

- Linux'ta gerçek pencere E2E'si yoktur. Linux'ta gerçek IPC yolu `src-tauri/tests/ipc.rs` ile (sahte pencereyle) test edilir. Linux bir sürüm hedefi olursa tauri-driver yeniden değerlendirilir.
- Hata ayıklama portu yalnızca test sürecinin ortam değişkeniyle, `127.0.0.1` üzerinde ve tek kullanımlık bir WebView2 profiliyle açılır. Uygulama bu portu kendisi asla açmaz. Kullanıcı ortamında bu değişkenin ayarlanması tehdit modelinde ele alınır (F0.11).
- `npm run verify`, Windows dışında bu adımı “not run” olarak raporlar; geçti saymaz. CI'da Windows'ta zorunludur (`VV_REQUIRE_DESKTOP_E2E=1`).

**Kanıt.** `apps/desktop/e2e-desktop/desktop-shell.spec.ts`: gerçek durum rozeti, `ping` gidiş-dönüşü, yetki dışı komutların reddi ve CSP'nin uzak görseli engellemesi. CI çalışması STATE.md F0.4 satırında.
