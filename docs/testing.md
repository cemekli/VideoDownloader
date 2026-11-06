# Test rehberi

Durum takibi [STATE.md](../STATE.md) içindedir. Bu belge testlerin nasıl çalıştırılacağını ve yeni testin nereye ekleneceğini anlatır.

## Kurulum (bir kez)

Gerekenler: Rust (stable, `rustfmt` ve `clippy` ile), Node 22.

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

`verify` sonucu `test-artifacts/verify/report.md` ve `report.json` dosyalarına yazılır. Her adım “automated pass”, “automated fail” veya “not run” olarak raporlanır (SPEC §25.4). JUnit çıktıları `test-artifacts/vitest/` ve `test-artifacts/e2e/`, ekran görüntüleri `test-artifacts/screenshots/` altındadır.

Önceden kurulu bir Chromium kullanmak için `PW_CHROMIUM_EXECUTABLE` ortam değişkenine tarayıcının yolunu verin.

## Yeni test nereye eklenir

| Ne test ediliyor | Nereye | Not |
|---|---|---|
| Saf iş kuralı (URL, durum makinesi, hata eşleme) | `crates/vv-domain/src/*.rs` içinde `#[cfg(test)]` | IO yok, her işletim sisteminde çalışır |
| Çok vakalı kural | `tests/fixtures/<ad>.json` + `crates/*/tests/<ad>_fixture.rs` | Beklenen sonuç fixture'da yazılır; tek doğruluk kaynağı fixture'dır |
| UI bileşeni | `apps/desktop/src/**/<Bileşen>.test.tsx` | Fixture istemcisiyle |
| Kullanıcı yolculuğu | `apps/desktop/e2e/*.spec.ts` | 3 görünümde çalışır |

Kurallar:

- Test adı veya yorumu ilgili SPEC kimliğini taşır (ör. `AC-29`, `INV-02`).
- Fixture'lara gerçek URL'ler, çerezler, token'lar veya kişisel dosyalar konmaz. Redaksiyon testlerindeki sahte değerler `CANARY` kelimesini içerir. `verify` bunu denetler.
- Fixture ile geçen test, canlı platform başarısı sayılmaz.
- Fixture IPC ile geçen E2E testi, gerçek Tauri IPC'nin çalıştığını kanıtlamaz (F0.4 bunu kapatır).

## Fixture'lar

| Dosya | İçerik | Kullanan |
|---|---|---|
| `tests/fixtures/url-policy.json` | 33 URL vakası ve beklenen sonuç | `vv-domain` fixture testi |
| `tests/fixtures/library-small.json` | 6 sentetik öğe: karışık platform, uzun başlık, Türkçe, emoji, bilinmeyen süre, eksik dosya | Arayüz fixture istemcisi, Vitest, Playwright |

## Kapsama matrisi

Her P0 gereksinim en az bir AC'ye veya kimlikli bir manuel teste bağlanır (SPEC §25.3). Matris F0.11 kapsamında bu bölüme eklenecek.
