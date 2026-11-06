# Test rehberi

Durum takibi [STATE.md](../STATE.md) içindedir. Bu belge testlerin nasıl çalıştırılacağını ve yeni testin nereye ekleneceğini anlatır.

## Kurulum (bir kez)

Gerekenler: Rust (stable, `rustfmt` ve `clippy` ile), Node 26 (LTS, sürüm `.nvmrc` dosyasında).

- Windows: WebView2 (Windows 11'de yüklü gelir) ve Visual Studio C++ Build Tools.
- Linux: Tauri sistem kütüphaneleri: `sudo apt-get install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev`

```bash
npm run setup        # web bağımlılıkları + Playwright Chromium
```

## Çalıştırma

| Komut | Ne yapar |
|---|---|
| `npm run verify` | Tüm kapılar: fixture sır taraması, Rust fmt/clippy/test, TS tip kontrolü, Vitest, Vite build, Playwright E2E |
| `npm run verify:fast` | E2E ve build hariç hızlı kontrol |
| `node scripts/verify.mjs --only rust` | Yalnızca bir grup (`repo`, `rust`, `web`, `e2e`) veya adım kimliği |
| `npm run test:rust` | Rust testleri |
| `npm run test:web` | Vitest |
| `npm run e2e` | Playwright (fixture IPC, 3 görünüm) |
| `npm run dev:fixture` | Arayüzü fixture verisiyle tarayıcıda açar |
| `npm run desktop` | Masaüstü uygulamasını geliştirme modunda açar (Tauri + Vite) |
| `npm run desktop:build` | Release derlemesi ve Windows installer |

`verify` sonucu `test-artifacts/verify/report.md` ve `report.json` dosyalarına yazılır. Her adım “automated pass”, “automated fail” veya “not run” olarak raporlanır (SPEC §25.4). JUnit çıktıları `test-artifacts/vitest/` ve `test-artifacts/e2e/`, ekran görüntüleri `test-artifacts/screenshots/` altındadır.

Önceden kurulu bir Chromium kullanmak için `PW_CHROMIUM_EXECUTABLE` ortam değişkenine tarayıcının yolunu verin.

## Yeni test nereye eklenir

| Ne test ediliyor | Nereye | Not |
|---|---|---|
| Saf iş kuralı (URL, durum makinesi, hata eşleme) | `crates/vv-domain/src/*.rs` içinde `#[cfg(test)]` | IO yok, her işletim sisteminde çalışır |
| Çok vakalı kural | `tests/fixtures/<ad>.json` + `crates/*/tests/<ad>_fixture.rs` | Beklenen sonuç fixture'da yazılır; tek doğruluk kaynağı fixture'dır |
| IPC komutu (yeni Tauri komutu) | `apps/desktop/src-tauri/tests/ipc.rs` + `tests/fixtures/ipc-contract.json` | Komut `build.rs` listesine ve `capabilities/main.json`'a da eklenir; aksi halde ACL reddeder |
| UI bileşeni | `apps/desktop/src/**/<Bileşen>.test.tsx` | Fixture istemcisiyle |
| Kullanıcı yolculuğu | `apps/desktop/e2e/*.spec.ts` | 3 görünümde çalışır |

Kurallar:

- Test adı veya yorumu ilgili SPEC kimliğini taşır (ör. `AC-29`, `INV-02`).
- Fixture'lara gerçek URL'ler, çerezler, token'lar veya kişisel dosyalar konmaz. Redaksiyon testlerindeki sahte değerler `CANARY` kelimesini içerir. `verify` bunu denetler.
- Fixture ile geçen test, canlı platform başarısı sayılmaz.
- Fixture IPC ile geçen E2E testi, gerçek Tauri IPC'nin çalıştığını kanıtlamaz. `tests/ipc.rs` gerçek IPC yolunu sahte pencereyle test eder; gerçek pencerede E2E F0.4'tedir.

## Fixture'lar

| Dosya | İçerik | Kullanan |
|---|---|---|
| `tests/fixtures/url-policy.json` | 33 URL vakası ve beklenen sonuç | `vv-domain` fixture testi |
| `tests/fixtures/ipc-contract.json` | IPC komutlarının ve `DomainError`'ın tel biçimi (sözleşme v1) | `vv-desktop` IPC testleri, Vitest |
| `tests/fixtures/library-small.json` | 6 sentetik öğe: karışık platform, uzun başlık, Türkçe, emoji, bilinmeyen süre, eksik dosya | Arayüz fixture istemcisi, Vitest, Playwright |

## Kapsama matrisi

Her P0 gereksinim en az bir AC'ye veya kimlikli bir manuel teste bağlanır (SPEC §25.3). Matris F0.11 kapsamında bu bölüme eklenecek.
