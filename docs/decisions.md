# Karar kayıtları (ADR)

SPEC §27 bu dosyayı zorunlu tutar. SPEC'teki DEC-01…DEC-17 kararları da kanıtlarıyla buraya yazılacak (STATE.md F0.8, F0.9, F0.11). Her kayıt: bağlam, seçenekler, karar, sonuçlar ve kanıt.

| ADR | Konu | Durum | Tarih |
|---|---|---|---|
| [ADR-0001](#adr-0001-gerçek-uygulama-e2e-aracı) | Gerçek uygulama E2E aracı | Kabul edildi | 6 Kasım 2026 |
| [ADR-0002](#adr-0002-sqlite-erişimi-wal-ve-migration-stratejisi) | SQLite erişimi, WAL ve migration stratejisi (SPEC DEC-06) | Kabul edildi | 6 Kasım 2026 |

---

## ADR-0001: Gerçek uygulama E2E aracı

**Bağlam.** SPEC §25.1, fixture IPC ile geçen E2E testinin gerçek Tauri IPC'nin çalıştığını kanıtlamadığını söyler. STATE.md F0.4, gerçek uygulama penceresinde çalışan bir E2E altyapısı ister. Sürüm hedefi Windows 11 ve WebView2'dir (SPEC §3). Arayüz testleri zaten Playwright ile yazılıyor.

**Seçenekler.**

| | Playwright + WebView2 CDP | tauri-driver + WebDriver |
|---|---|---|
| Nasıl | Test derlemesi WebView2'yi `--remote-debugging-port=N` ile başlatır, Playwright `connectOverCDP` ile bağlanır | `tauri-driver`, Windows'ta `msedgedriver`'ı, Linux'ta `WebKitWebDriver`'ı sarar; testler WebdriverIO/Selenium ile yazılır |
| Platform | Yalnızca Windows (WebView2) | Windows ve Linux; macOS yok |
| Ek araç | Yok; mevcut Playwright | `tauri-driver` (cargo) + WebView2 sürümüyle eşleşen `msedgedriver` + WebdriverIO |
| Test dili | Mevcut Playwright testleriyle aynı API ve assertion'lar | Ayrı bir test yığını |
| Kırılganlık | Ayrı bir test derlemesi gerekir (aşağıya bakın) | Driver sürümü WebView2 runtime sürümüyle uyumsuz kalınca kırılır |

**Karar.** Gerçek uygulama E2E testleri Playwright + WebView2 CDP ile yazılır ve Windows'ta çalışır (`apps/desktop/e2e-desktop/`).

**Gerekçe.** Sürüm hedefi Windows'tur ve orada tek test yığınıyla (Playwright) hem fixture hem gerçek uygulama testleri yazılabilir. Sürüm eşleştirmesi gereken ek driver yoktur. Microsoft, WebView2 uygulamalarının Playwright ile CDP üzerinden test edilmesini belgeler.

**Sonuçlar.**

- Linux'ta gerçek pencere E2E'si yoktur. Linux'ta gerçek IPC yolu `src-tauri/tests/ipc.rs` ile (sahte pencereyle) test edilir. Linux bir sürüm hedefi olursa tauri-driver yeniden değerlendirilir.
- WebView2'nin `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` ortam değişkeni işe yaramaz: Tauri WebView2'ye her zaman kendi tarayıcı argümanlarını verir ve WebView2 156 bu durumda ortam değişkenini yok sayar (CI'da süreç komut satırıyla görüldü, 6 Kasım 2026). Bu yüzden port, yalnızca test derlemesine giren `e2e-cdp` cargo özelliğiyle açılır: `VV_E2E_CDP_PORT` ayarlıysa pencereye `--remote-debugging-port` eklenir.
- Dağıtılan exe bu özellik olmadan derlenir; hata ayıklama portunu açacak kod içinde yoktur. Test exe'si ayrı bir dosyadır (`videovault-e2e.exe`) ve dağıtılmaz. Testler tek kullanımlık bir WebView2 profili kullanır.
- `npm run verify`, Windows dışında bu adımı “not run” olarak raporlar; geçti saymaz. CI'da Windows'ta zorunludur (`VV_REQUIRE_DESKTOP_E2E=1`).

**Kanıt.** `apps/desktop/e2e-desktop/desktop-shell.spec.ts`: gerçek durum rozeti, `ping` gidiş-dönüşü, yetki dışı komutların reddi ve CSP'nin uzak görseli engellemesi. İlk yeşil Windows çalışması: [CI #42271413057](https://github.com/cemekli/VideoDownloader/actions/runs/42271413057) (4/4).

---

## ADR-0002: SQLite erişimi, WAL ve migration stratejisi

SPEC DEC-06'yı karşılar. Kod: `crates/vv-persistence`.

**Bağlam.** Kütüphane verisi yerel SQLite'ta durur (SPEC §10.2). SPEC şunları ister: FTS5'in paketlenen SQLite'ta doğrulanması (§16.1), migration numarası ve checksum'ın izlenmesi, yarım şema bırakılmaması, migration öncesi tutarlı yedek (§11.7) ve çalışırken kör dosya kopyası yerine snapshot yedek (§15.3).

**Kütüphane.**

| Seçenek | Artı | Eksi |
|---|---|---|
| **rusqlite + `bundled`** | SQLite uygulamaya gömülü: test edilen sürüm, dağıtılan sürümdür; FTS5 derlemede açık; online backup API'si var; senkron ve basit | SQLite güncellemesi crate güncellemesiyle gelir |
| rusqlite + sistem SQLite'ı | Daha küçük exe | Windows'ta sürüm ve FTS5 makineye göre değişir; §16.1'i karşılamaz |
| sqlx | Async, derleme zamanı sorgu denetimi | Derleme zamanı denetimi için veritabanı gerekir; masaüstü tek kullanıcılı iş yükü için gereksiz karmaşıklık |

**Karar.**

1. `rusqlite =0.40.2`, `bundled` ve `backup` özellikleriyle. Gömülü SQLite: 3.53.2.
2. **Bağlantılar:** süreç başına tek yazıcı bağlantı, gerektikçe salt okunur okuyucu bağlantılar (`query_only`). Yazıcıyı paylaşan kod, onu kendi kilidiyle korur.
3. **PRAGMA'lar:**
   - `journal_mode = WAL`: okuyucular yazıcıyı engellemez. Dosya WAL'a geçemezse (ör. ağ sürücüsü) açılış hata verir, sessizce devam etmez.
   - `synchronous = FULL`: kullanıcının “kaydedildi” gördüğü bir kayıt elektrik kesintisinden sağ çıkar. Yazmalar küçük ve seyrek olduğu için maliyeti düşüktür.
   - `foreign_keys = ON`, `trusted_schema = OFF`, `busy_timeout = 5 sn`.
4. **Migration:**
   - Uygulamaya gömülü, numaralı SQL dosyaları (`migrations/NNNN_ad.up.sql` / `.down.sql`). Liste yalnızca sona eklenir; uygulanmış bir migration değiştirilmez.
   - `schema_migrations` tablosu sürümü, adı ve `up` SQL'inin SHA-256'sını tutar. Checksum değişmişse açılış durur.
   - Her migration kendi transaction'ında çalışır; hata olursa o migration tamamen geri alınır.
   - Uygulamanın bilmediği daha yeni bir şema sürümü bulunursa açılış reddedilir; veritabanına dokunulmaz.
   - Bekleyen migration varsa ve veritabanında veri varsa, önce `backups/pre-migration-vX-to-vY-<zaman>.sqlite` alınır ve bütünlük denetiminden geçer.
   - `down` betikleri tersinirliği test etmek içindir. Üretimde geri dönüş yolu: eski uygulama sürümü + migration öncesi yedek (SPEC §11.7).
5. **Yedek:** SQLite online backup API'si; yazma açıkken bile yalnızca commit edilmiş durumu alır.

**Sonuçlar.**

- FTS5'in varlığı hem Rust testinde hem de gerçek Windows exe'sinde (`get_diagnostics`, e2e-desktop) doğrulanır.
- Eşzamanlı yazma isteği hata vermek yerine `busy_timeout` kadar bekler. Uzun yazma transaction'larından kaçınılmalıdır.
- Kütüphane şeması (SPEC §11) F1.1'de, Türkçe arama davranışı F1.6'da eklenir.

**Kanıt.** `crates/vv-persistence/tests/migrations.rs` (11 test), `tests/fts5.rs` (2 test), `apps/desktop/e2e-desktop` FTS5 testi. CI çalışması STATE.md F0.5 satırında.
