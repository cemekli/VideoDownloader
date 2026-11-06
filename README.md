# VideoVault

Yerel öncelikli kişisel video kütüphanesi. Ürün ve teknik sözleşme: [SPEC.md](./SPEC.md). Proje durumu ve faz planı: [STATE.md](./STATE.md). Testler: [docs/testing.md](./docs/testing.md).

```bash
npm run setup     # bir kez
npm run verify    # tüm testler ve kontroller
npm run desktop   # masaüstü uygulamasını aç (geliştirme modu)
```

## Yapı

```text
apps/desktop/          React + TypeScript + Vite arayüzü, Vitest ve Playwright testleri
apps/desktop/src-tauri/ Tauri 2 kabuğu: IPC komutları, yetkiler (capabilities), CSP
crates/vv-domain/      Saf Rust iş kuralları (IO yok)
tests/fixtures/        Rust ve arayüz testlerinin paylaştığı deterministik fixture'lar
scripts/verify.mjs     Tek komutla doğrulama ve rapor
.github/workflows/     CI (Windows + Linux)
```
