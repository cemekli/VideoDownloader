# Yol haritası

Kilometre taşları SPEC §3'te, çıkış kapıları §26'dadır. Bu belge her kilometre taşını [STATE.md](../STATE.md)'deki fazlara bağlar, bağımlılıkları ve kapsam değişikliklerini kaydeder. Alt görevlerin durumu ve kanıtı yalnızca STATE.md'de tutulur. SPEC §27'deki `TASKS.md`'nin görevini STATE.md üstlenir.

Son güncelleme: 7 Kasım 2026, F0.11.

## Kilometre taşları ve fazlar

| Kilometre taşı | Faz | Çıkış koşulu (SPEC §3) | Durum |
|---|---|---|---|
| M0 — Temel ve uygulanabilirlik | F0 | Uygulama açılır; tip/lint/test akışı çalışır; dış araç ve depolama kararlarının kanıtı kayıtlı | Tamamlandı: F0.1–F0.13 ✅, çıkış kapısı işaretli (STATE.md) |
| M1 — Çalışan Windows kütüphanesi | F1, F2 | Ağsız kaydet, düzenle, ara, oynat akışı yeniden başlatmada veriyi korur | Başlanmadı |
| M2 — İndirme ve Windows MVP | F3, F4 | En az bir izinli gerçek indirme uçtan uca; destek iddiaları platform matrisiyle sınırlı; §26.1 kapıları | Başlanmadı |
| M3 — Yetkili oturum PoC | F5 | Gerçek ve fixture testleri ayrı; özel VK başarısı yalnızca gerçek yetkili testle (§26.3) | Başlanmadı |
| M4 — AI V2, transkript araması | F6 | TR/EN fixture, iptal/yeniden başlatma, zaman koduna gitme | Başlanmadı |
| M5 — Anlamsal ve görsel AI | F7 | Sürümleme/reindex; konuşma ve görsel kanıt ayrı; kalite ölçülür | Başlanmadı |
| M6 — Android | F8 | Gerçek cihazda yetenek, izin, arka plan ve depolama testleri | Başlanmadı |

**Sürüm sınırı (SPEC §3):** Windows MVP, M0–M2 bitince çıkar. M3'ün canlı özel grup testi MVP'yi engellemez; doğrulanmamış oturum özelliği kapalı ya da açıkça deneysel kalır. M4 ayrı AI V2 sürümüdür.

## Bağımlılıklar

```text
F0 ──> F1 ──> F2 ──┐
  │                ├──> F4 (Windows MVP) ──> F5 (M3)
  └──> F3 ─────────┘                    └──> F6 (M4) ──> F7 (M5)
                                                       F8 (M6), F1–F2 sonrası araştırma
```

| Faz | Önce bitmesi gereken | Neden |
|---|---|---|
| F1 | F0.5 (SQLite), F0.7 (URL politikası) | Atomik kayıt ve FTS bu altyapıya dayanır |
| F2 | F1.1, F1.6 | Düzenleme ve export alan şemasını ve arama belgesini kullanır |
| F2.4 (oynatıcı) | F0.8a (oynatma matrisi, ADR-0004) | Hangi biçimin oynayacağı ölçüldü |
| F3 | F0.6, F0.8b/c, F0.9, F0.10 | yt-dlp ayrıştırma, araç manifesti, ağ denetimi, süreç denetimi hazır olmalı |
| F3.4 (uzak metadata) | F0.9 (DEC-05) | SPEC §3: ağ politikası kapısı geçilmeden uzak metadata açılmaz |
| F4.5 (performans) | F0.12, F1.1 | Benchmark ortamı ve veri üreticisi (ADR-0009); ölçümler gerçek şemada tekrarlanır |
| F5 | F4 | Oturum özelliği MVP kabuğunun üstüne kurulur; secret store F5.1 |
| F6, F7 | F1.6 (FTS), F3.1 (kuyruk) | Döküm ve embedding işleri kuyrukta çalışır, aramaya bağlanır |

## Kapsam değişiklikleri

| Tarih | Değişiklik | Gerekçe | Kayıt |
|---|---|---|---|
| 7 Kas 2026 | HEVC oynatılamaz sayılır; PLAY-07 remux önizlemesi yalnızca uyumlu codec'lerle oynamayan container'lar için | WebView2 156'da HEVC yalnızca ses verdi; MKV/MOV/WebM H.264/VP9/AV1 ile oynadı | ADR-0004 |
| 7 Kas 2026 | F0.8 üç alt göreve bölündü (oynatma matrisi, yt-dlp + Deno, FFmpeg) | Her biri ayrı karar ve kanıt gerektiriyor | STATE.md |
| 7 Kas 2026 | yt-dlp yalnızca native indiriciyle; RTMP/RTSP/MMS indirilmez | FFmpeg'e devredilen protokoller ağ denetimini aşardı | ADR-0007 |
| 7 Kas 2026 | Unix'te uygulama çökünce alt süreçler ölmeyebilir; yalnızca Windows garanti eder | Dağıtım hedefi Windows; Unix yalnızca geliştirme | ADR-0008 |
| 7 Kas 2026 | Benchmark bütçeleri CI'da kapı değil; kapı F4.5'te referans makinede | Paylaşılan CI makinelerinin süreleri güvenilir değil | ADR-0009 |
| 7 Kas 2026 | `TASKS.md` yerine `STATE.md` | Tek durum kaynağı; içerik SPEC §27'nin istediği sütunları taşıyor | Bu belge |

## Sonraki dilimler

SPEC §27.1'deki dikey dilimlerin sırası korunur: temel iskelet (M0, büyük ölçüde bitti) → offline bookmark (F1.1–F1.3, F1.6, F1.8) → yerel video (F1.4, F2.4) → düzenleme (F2.1, F2.2) → taşınabilirlik (F2.5, F2.6) → indirme (F3) → dağıtım (F4).
