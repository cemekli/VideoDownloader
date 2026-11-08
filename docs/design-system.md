# Tasarım sistemi

SPEC §9.1, §9.2, §9.4, §9.5. Bu belge `apps/desktop/src/styles.css`'teki token'ları ve kuralları anlatır; ikisi birlikte değişir. İlk sürüm F1.7'de (Library ekranı) yazıldı. Açık tema ve çeviri anahtarları F2.8'de eklenecek.

## Renk (koyu tema)

| Token | Değer | Kullanım |
|---|---|---|
| `--bg` | `#121316` | Uygulama zemini, üst çubuk, inspector |
| `--surface` | `#1b1d22` | Kart, giriş alanı |
| `--surface-2` | `#23262c` | Hover, düğme |
| `--surface-3` | `#2c3037` | Etkin öğe, menü |
| `--border` | `#2f333a` | Ayraç ve çerçeve |
| `--text` | `#e8e9ec` | Ana metin |
| `--muted` | `#a3a7b0` | İkincil metin (zeminde AA) |
| `--accent` | `#6aa3ff` | Tek vurgu rengi: birincil düğme, seçim, etkin gezinme |
| `--focus` | `#9cc2ff` | Odak halkası |
| `--ok` / `--warn` / `--danger` | `#5fcf8f` / `#e8b85a` / `#f08a73` | Durum rozetleri; her zaman metin ve sembolle birlikte |

Durum yalnızca renkle anlatılmaz (§9.4): `✓ Downloaded`, `🔖 Bookmark only`, `⏏ Drive disconnected`, `! File missing`.

## Aralık, yarıçap, tipografi

- Aralık: 4 / 8 / 12 / 16 / 24 / 32 px (`--s1` … `--s6`).
- Yarıçap: 8 px kart ve panel, 6 px düğme ve giriş, 999 px chip.
- Font: sistem fontu (`system-ui`, Windows'ta Segoe UI). Paketli ya da uzak font yok.
- Roller: sayfa başlığı 1.25 rem / 600; kart başlığı 0.95 rem / 600, en çok iki satır; meta 0.8 rem `--muted`; inspector bölüm başlığı 0.8 rem büyük harf.

## Yerleşim

| Bölge | Ölçü |
|---|---|
| Üst çubuk | 60 px |
| Sol gezinme | 232 px; `<900 px`'te menü düğmesiyle açılan çekmece |
| Kart | en az 232 px, 16:9 küçük resim, aralık 12 px |
| Liste satırı | 64 px |
| Inspector | 340 px; `<900 px`'te sağdan modal çekmece |

Breakpoint'ler: `<900 px` kompakt, `900–1199 px` dar masaüstü, `≥1200 px` tam masaüstü (§9.2). 1366×768'de inspector kapalıyken 4, açıkken 3 sütun; 1920×1080'de 5–6 sütun.

## Etkileşim durumları

- Odak: 2 px `--focus` halkası, her etkileşimli öğede görünür (`:focus-visible`).
- Hover: bir üst yüzey tonu. Seçili kart: `--accent` çerçeve ve %12 vurgu zemini.
- Devre dışı: %55 opaklık, imleç normal.
- Yükleniyor: metin durumu (`role=status`) ve iskelet kartlar; yüklenme görünümü hiçbir zaman tamamlanmış sonuca benzemez.
- Hata: `role=alert`, ne olduğunu ve verinin güvende olduğunu söyleyen metin, hata kodu ve "Try again".

## Hareket

Yalnızca renk geçişleri, 120 ms. `prefers-reduced-motion` açıkken geçiş yok.

## Kurallar

- Runtime'da CDN'den font, ikon veya görsel yüklenmez; CSP `img-src` yalnızca `'self'` ve yerel thumbnail protokolü.
- Çalışmayan özellik düğmesi gösterilmez (§9.3).
- Uzun başlık kartı büyütmez: iki satırda kesilir, tamamı `title` niteliğinde ve inspector'da.
