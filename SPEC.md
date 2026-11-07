# VideoVault — Ayrıntılı Ürün ve Teknik Spesifikasyon

| Belge bilgisi | Değer |
|---|---|
| Sürüm | 1.5 — normalizasyon çakışmasında kenar durum netleştirildi (bkz. §31) |
| Tarih | 7 Kasım 2026 |
| Kaynak | [VIDEO_LIBRARY_MASTER_PROMPT.md](./VIDEO_LIBRARY_MASTER_PROMPT.md) |
| İlk hedef | Windows 11 üzerinde çalışan, yerel öncelikli kişisel video kütüphanesi |
| Sonraki hedefler | İzinli oturum kullanımı, yerel transkripsiyon, anlamsal/görsel arama, Android |
| Ürün adı | **VideoVault**, geçici ad; marka uygunluğu doğrulanmış değildir |
| Belgenin dili | Türkçe; ürünün ilk arayüz dili İngilizce, Türkçe yerelleştirmeye hazır |
| Doğrulama durumu | Kaynak prompt incelendi. Uygulama, bağımlılık sürümleri, dış siteler, cihazlar ve platform indirmeleri bu belge hazırlanırken test edilmedi. |

## 1. Belgenin amacı ve yorumlama kuralları

Bu belge, kaynak prompttaki ürün hedeflerini geliştirilebilir gereksinimlere, veri sözleşmelerine, iş akışlarına, aşama sınırlarına ve ölçülebilir kabul kriterlerine dönüştürür. Bu çalışmanın teslimatı proje kökündeki `SPEC.md` dosyasıdır. Kaynaktaki “hemen kodlamaya başla”, Git deposu başlat, uygulama geliştir, test çalıştır ve commit oluştur talimatları, gelecekteki uygulama sürecinin girdisi olarak değerlendirilmiştir; bu belge onların yerine getirildiği anlamına gelmez.

Belgede üç tür bilgi ayrılır:

- **Zorunlu gereksinim:** Kaynaktan türetilen ürün veya güvenlik şartı. “Zorunlu”, “yapılmalıdır”, “yapılamaz” ifadeleri bağlayıcıdır.
- **Önerilen başlangıç kararı:** Kaynağın ayrıntılandırmadığı bir alanı somutlaştıran mühendislik varsayımı. İlgili bölümde **Öneri** olarak işaretlenir; uygulama sırasında ADR ile değiştirilebilir.
- **Doğrulama kapısı:** Güncel doküman, lisans, canlı platform, gerçek cihaz veya performans ölçümü gerektiren karar. Sonuç alınana kadar destek/başarı iddiası üretilemez.

Gereksinim öncelikleri:

| Öncelik | Anlam |
|---|---|
| P0 | İlgili kilometre taşının tamamlanması veya sürümün yayımlanması için zorunlu |
| P1 | İlgili aşamadaki planlı kapsam; ertelenirse gerekçe ve yeni aşama kaydedilmeli |
| P2 | Araştırma veya sonraki sürüm adayı; çalışan özellik gibi gösterilemez |

Güvenlik, veri kaybını önleme, erişim yetkilerine uyma ve doğrulanmamış başarı iddiasından kaçınma koşulları tüm aşamalarda P0'dır. “Gelecek özellik” ifadesi bu koşulları ertelemez.

## 2. Ürün vizyonu

Kullanıcı; farklı platformlardaki video bağlantılarını ve kendi yerel dosyalarını tek, aranabilir, çevrimdışı kullanılabilir bir kütüphanede saklayabilmelidir. Önce başlık, açıklama, not ve etiketlerden; ilerleyen aşamalarda konuşma dökümlerinden, ekran yazılarından ve görsel sahnelerden içeriği yeniden bulabilmelidir.

Ürün ücretsiz ve açık kaynak olmalıdır. Zorunlu hesap, abonelik, ücret duvarı, izleme veya tescilli bir bulut hizmetine bağımlılık bulunamaz. Bağış özelliği ancak ayrı hukuki/vergi değerlendirmesinden sonra ele alınabilir; MVP'de görünmez ve devre dışıdır.

Temel dayanıklılık kuralı: Bir platformun çıkarıcısı bozulduğunda kullanıcı bağlantısını, notlarını, koleksiyonlarını, mevcut dosyasını ve indeksini kullanmaya devam eder. İndirme başarısı, kütüphanenin çalışmasının önkoşulu değildir.

### 2.1 Kullanıcıya verilen temel sözler

1. Bağlantı kaydı, indirme ve AI işleminden bağımsız olarak saklanabilir ve aranabilir.
2. Yerel dosyalar ve indirilen dosyalar ağ bağlantısı olmadan kullanılabilir.
3. Notlar, etiketler ve koleksiyonlar platformdaki içerik silinse de korunur.
4. Oturum veya erişim hatası açıkça gösterilir; üyelik yetkisi, oturum varlığı ve çıkarıcı desteği birbirine karıştırılmaz.
5. Kullanıcının dosyaları, özel bağlantıları, çerezleri veya dökümleri varsayılan olarak üçüncü taraf AI hizmetlerine gönderilmez.
6. Her AI sonucu gerçek bir işleme ve kaynağa dayanır; zaman kodu, özet veya güven puanı uydurulamaz.
7. Dışa aktarma ve yedekten geri yükleme, kullanıcıyı uygulamaya kilitlemeden çalışır.

### 2.2 Kapsam dışı davranışlar

- DRM, ücret duvarı, CAPTCHA, MFA veya başka erişim kontrollerini aşmak.
- Gizli otomasyon, anti-bot kaçınması, özel içerik için yetki yükseltme.
- Desteklenen belgeli yöntem ve açık izin olmadan özel favori/kayıt listelerini topluca taramak.
- Her platformda, her özel videoda veya her bölgede indirme garantisi vermek.
- MVP için merkezi sunucu, zorunlu senkronizasyon, ekip hesabı veya sosyal ağ kurmak.
- MVP'de eklenti pazarı, kapsamlı bilgi grafiği, TV odaklı medya sunucusu veya ödeme sistemi geliştirmek.
- Kullanıcının şifrelerini veya MFA sırlarını almak; Android'in Chrome'un özel verilerine erişebildiğini varsaymak.

## 3. Sürüm ve kilometre taşı kapsamı

| Aşama | Teslim edilecek sonuç | Aşama çıkış koşulu |
|---|---|---|
| M0 — Temel ve uygulanabilirlik | Tauri/React/Rust iskeleti, CI, mimari kararlar, SQLite FTS5 denemesi, yt-dlp fixture adaptörü, paketleme/lisans araştırması | Temel uygulama açılır; tip/lint/test akışı çalışır; dış araç ve depolama kararlarının kanıtı kaydedilir |
| M1 — Çalışan Windows kütüphanesi | URL bookmark, yerel import, yerel dosya metadata'sı (ffprobe) ve küçük resmi, not/etiket/koleksiyon, favori/arşiv, FTS arama, oynatıcı, JSON dışa/içe aktarma | Ağsız kaydet–düzenle–ara–oynat akışı yeniden başlatma sonrasında veriyi korur |
| M2 — İndirme ve Windows MVP | İzinli indirme, format seçimi, kalıcı kuyruk, hata/progress, iptal/yeniden deneme, FFmpeg, kurulum ve güncelleme akışı | En az bir izinli gerçek indirme uçtan uca doğrulanır; destek iddiaları platform matrisiyle sınırlıdır; MVP kapıları geçer |
| M3 — Yetkili oturum PoC | Açık izinle tarayıcı çerezi kullanımı, VK test matrisi, ayrıştırılmış oturum/erişim hataları | Gerçek ve fixture testleri ayrılır; özel VK başarısı yalnızca gerçek yetkili testten sonra kaydedilir; “araştırma tamamlandı” ile “özellik destekleniyor” sonuçları ayrıdır (§26.3) |
| M4 — AI V2 / transkript araması | İsteğe bağlı yerel model, döküm, zaman kodlu FTS, işlem merkezi, kaynak sınırları | Türkçe/İngilizce fixture, iptal/yeniden başlatma ve zaman koduna gitme doğrulanır |
| M5 — Anlamsal ve görsel AI | Çok dilli embedding, hibrit sıralama, konu önerileri, OCR, seyrek kare araması | Sürümleme/reindex çalışır; sonuçlar konuşma/görsel kanıtını ayırır; kalite ölçülür |
| M6 — Android | Paylaş menüsü, mobil kütüphane, scoped storage, yerel köprüler, çevrimdışı oynatma | Gerçek Android cihazda yetenek, izin, arka plan ve depolama testleri tamamlanır |

**Öneri — sürüm sınırı:** Windows MVP, M0–M2 tamamlanınca çıkarılabilir. M3'ün canlı özel grup testi, temel bookmark kütüphanesinin çıkışını engellemez; doğrulanmamış oturum özelliği dağıtımda kapalı veya açıkça deneysel kalır. M4 ayrı AI V2 sürümüdür. M5, anlamsal sınıflandırma ve görsel işleme alt dilimlerine ayrılabilir.

**Karar — uzak metadata:** URL'ler için uzak metadata ve küçük resim çekme (yt-dlp üzerinden) ağ politikası kapısı (DEC-05) geçildikten sonra M2'de açılır. M1'de URL bookmark'ları URL'den türetilen başlık ve `metadata_status = pending` ile kaydedilir (CAP-04). M2'de bekleyen öğeler için metadata işi kuyruğa alınır.

Platformlara özgü indirme, GPU, Android ve uygulamaya ait tarayıcı profili desteği birer doğrulama kapısıdır. Yol haritasında bulunmaları, yayımlanmış sürümde var oldukları anlamına gelmez.

## 4. Kullanıcı rolleri ve kullanım koşulları

| Rol / durum | İhtiyaç |
|---|---|
| Tek cihaz kullanıcısı | Hesap açmadan bağlantı ve dosya arşivleme |
| Büyük Inbox sahibi | Çoklu seçim, toplu etiketleme, favori, arşiv ve hızlı filtre |
| İçerik araştırmacısı | Notlar, kaynak bilgisi, zaman kodlu arama ve kaynaklı dışa aktarma |
| Sınırlı donanım kullanıcısı | AI kapalıyken tam temel işlev; CPU odaklı seçenekler ve kaynak uyarıları |
| Yetkili özel içerik izleyicisi | İzinli oturum kullanımı, doğrulanabilir erişim ve güvenli hata teşhisi |
| Çevrimdışı kullanıcı | Yerel dosya, mevcut metadata, indeks ve dökümlere erişim |

MVP tek işletim sistemi kullanıcısının kişisel kütüphanesine odaklanır. Çok kullanıcılı yetki modeli ve eşzamanlı ağ paylaşımı kapsam dışıdır. İşletim sistemi hesabının bütünüyle ele geçirilmesine karşı mutlak koruma iddia edilmez.

## 5. Ürün değişmezleri

| Kimlik | Değişmez |
|---|---|
| INV-01 | Başarısız metadata, indirme veya AI işi kayıtlı bookmark'ı silmez. |
| INV-02 | `downloaded` görünümü yalnızca kullanılabilir yerel medya kaydından türetilir. Kuyrukta olmak yeterli değildir. |
| INV-03 | Bir `AuthProfile` kaydının varlığı “erişim doğrulandı” anlamına gelmez. |
| INV-04 | Ağ ve uzun medya işlemleri UI iş parçacığında yürütülmez. |
| INV-05 | Kuyruk ve alan verisi, tutarlı SQLite işlemleriyle kalıcılaştırılır. |
| INV-06 | Yerel import kaynak dosyasını varsayılan olarak değiştirmez veya silmez. |
| INV-07 | Arama kaynağı ile sonuç açıklaması tutarlıdır; transkript yokken transkript eşleşmesi gösterilmez. |
| INV-08 | Webview doğrudan shell, dosya sistemi veya gizli verilere erişmez. |
| INV-09 | Çerezler/kimlik bilgileri SQLite, uygulama logları ve taşınabilir yedekte bulunmaz. |
| INV-10 | Sahte indirme sayısı, yapay AI puanı, uydurma test veya destek iddiası gösterilemez. |
| INV-11 | Kütüphane kaydını kaldırmak ile fiziksel dosyayı silmek ayrı işlemlerdir. |
| INV-12 | Kullanıcı notları ve düzeltmeleri, tekrar metadata çekme veya AI çalıştırma ile ezilmez. |
| INV-13 | Aynı kütüphane veritabanını ve iş kuyruğunu aynı anda yalnızca tek bir uygulama süreci açar. |

## 6. Platform tanıma ve yetenek modeli

YouTube, Facebook, Instagram, TikTok, X, VK ve genel URL adaptörü bulunmalıdır. Bir alan adını tanımak, metadata çekebilmek, indirmek ve yetkili özel içeriğe erişmek ayrı yeteneklerdir.

Her adaptör aşağıdaki sözleşmeyi karşılar:

```text
recognize(url) -> PlatformMatch | NoMatch
normalize(url) -> CanonicalIdentity
capabilities(context) -> ConnectorCapabilities
fetch_metadata(item, access_context) -> MetadataResult
list_formats(item, access_context) -> FormatList
enqueue_download(item, selection, access_context) -> JobReference
map_error(tool_result) -> DomainError
```

`ConnectorCapabilities` en az `recognition`, `public_metadata`, `download`, `browser_cookie_auth`, `captions`, `pause`, `resume` alanlarını içerir. Her alan `unknown | supported | conditional | unsupported | disabled` değerlerinden birini, açıklamasını, araç sürümünü ve varsa son test zamanını taşır. `pause`/`resume` gerektiğinde seçilen indirme biçimine göre ayrıca hesaplanır.

| Platform | URL tanıma hedefi | İndirme/özel erişim durumu |
|---|---|---|
| YouTube | Kaydedilebilir video bağlantıları ve bilinen URL varyantları | Kullanılan yt-dlp sürümü ve gerçek test ile belirlenir |
| Facebook | Desteklenen video bağlantısı biçimleri | Kamusal/oturumlu erişim ayrı test edilir |
| Instagram | Desteklenen video/Reels bağlantısı biçimleri | Kamusal/oturumlu erişim ayrı test edilir |
| TikTok | Video bağlantıları ve güvenli kısa bağlantı çözümü | Bölge, ağ ve çıkarıcı durumuna bağlı |
| X | Video içeren paylaşım bağlantıları | Metadata/indirme ayrı doğrulanır |
| VK | Genel ve özel grup video bağlantıları | Grup üyeliği, oturum, medya URL süresi ve çıkarıcı ayrı değerlendirilir |
| Genel URL | Geçerli HTTP(S) bağlantısını bookmark olarak saklama | Güvenli ağ erişimi ve çıkarıcı desteği doğrulanırsa etkinleştirilir |

**Doğrulama kapısı:** Bu tablo güncel platform desteği beyanı değildir. Gerçek sonuçlar `docs/platform-support.md` içinde araç sürümü, test tarihi ve kanıtla tutulmalıdır.

**Öneri:** MVP'de bir yakalama işlemi tek video/öğe oluşturur. Playlist, kanal veya profil URL'si otomatik toplu indirmeyi başlatmaz; desteklenmiyorsa açıklayıcı uyarıyla bookmark saklanır. Kısa URL çözülemese de özgün bağlantı korunur ve sonradan yeniden denenebilir.

## 7. İşlevsel gereksinimler

### 7.1 URL yakalama ve bookmark

| Kimlik | Gereksinim | Aşama / öncelik |
|---|---|---|
| CAP-01 | Global Add URL eylemi ve uygun odakta `Ctrl+V` ile URL girişi sağlanmalı. Pano arka planda sürekli okunmamalı. | M1 / P0 |
| CAP-02 | URL şeması doğrulanmalı; tanıma ve yerel normalizasyon ağdan bağımsız çalışmalı. | M1 / P0 |
| CAP-03 | Save Bookmark, metadata isteği bitmeden kullanılabilmeli; kayıt ve arama belgesi atomik saklanmalı. | M1 / P0 |
| CAP-04 | Metadata gelmiyorsa bağlantıdan türetilen dürüst bir başlık ve `pending`/`failed` durumu gösterilmeli. “Offline” kalıcı bir `metadata_status` değeri değildir: `metadata_status = pending` iken Offline Mode açıksa veya ağ yoksa UI bunu “Offline” olarak gösterir. Offline Mode kapandığında veya ağ geri geldiğinde bekleyen metadata işleri `blocked → queued` geçişiyle yeniden kuyruğa girer. | M1 / P0 |
| CAP-05 | Etiket/koleksiyon seçimi isteğe bağlı olmalı; varsayılan kayıt Inbox'a gitmeli. | M1 / P0 |
| CAP-06 | Platform+source ID veya normalize URL eşleşmesi aynı öğeye yönlendirmeli; mevcut notlar korunmalı. Eşleşen öğe “Recently removed” içindeyse §7.3'teki yeniden kayıt dalı uygulanır. | M1 / P0 |
| CAP-07 | Güvenli metadata önizlemesi varsa gösterilmeli; Download ayrı ve isteğe bağlı eylem olmalı. | M2 / P0 |
| CAP-08 | Geçici oturumlu medya URL'leri kalıcı kaynak URL yerine kullanılmamalı; hassas URL sorguları log/diagnostics'e yazılmamalı. | M1 / P0 |
| CAP-09 | Tarayıcıdan hızlı yakalama için Windows URL protocol handler (ör. `videovault://add?url=`) veya Windows Share hedefi araştırılmalı. Gelen URL normal capture doğrulamasından geçmeli ve kullanıcı onayı olmadan kaydedilmemeli. | M2+ / P2 |

Kaynak URL, platformun kalıcı içerik bağlantısı olmalıdır. Genel URL sorgu parametreleri körlemesine silinmez; yalnızca platforma özgü doğrulanmış takip parametreleri normalizasyonda çıkarılır. Kullanıcı adı/şifre içeren URL'ler reddedilir. Açıkça geçici kimlik doğrulama sırrı taşıyan bağlantı güvenli bir kaynak bağlantısına dönüştürülemiyorsa kalıcı kayıttan önce kullanıcıya düzeltme gereksinimi gösterilir.

### 7.2 Yerel dosya ve klasör içe aktarma

| Kimlik | Gereksinim | Aşama / öncelik |
|---|---|---|
| IMP-01 | Dosya seçici, klasör seçici ve erişilebilir alternatifi olan sürükle-bırak desteklenmeli. | M1 / P0 |
| IMP-02 | MP4, MKV, WebM, MOV ve seçilen araçların desteklediği ses dosyaları `ffprobe` ile doğrulanmalı. Uzantı tek başına yeterli değildir. | M1 / P0 |
| IMP-03 | Süre, container, codec, boyut, çözünürlük ve ses parçası bilgisi mevcut olduğu ölçüde çıkarılmalı. | M1 / P0 |
| IMP-04 | Hatalı/okunamayan dosyalar dosya bazında raporlanmalı; bir hata bütün toplu importu geri almamalı. | M1 / P0 |
| IMP-05 | Tam yol eşleşmesi ve isteğe bağlı içerik hash'i yinelenen dosyayı belirlemeli. Hash eşleşmesinde yıkıcı otomatik birleştirme yapılmamalı. | M1 / P0 |
| IMP-06 | Küçük resim üretimi arka planda yapılmalı; eksik küçük resim kaydı veya aramayı engellememeli. | M1 / P0 |
| IMP-07 | Kaynak dosya taşınır/silinirse öğe korunmalı, `missing` gösterilmeli ve yeniden bağlama sunulmalı. | M1 / P0 |

**Öneri:** Varsayılan import, dosyayı özgün konumunda referanslar; “kütüphaneye kopyala” seçeneği depolama etkisiyle ayrıca sunulur. Klasör importunda alt klasörler kullanıcı tarafından seçilir; junction/symlink döngüleri takip edilmez. İptal, başarıyla alınmış öğeleri korur ve henüz işlenmemişleri bırakır.

### 7.3 Düzenleme, koleksiyonlar ve toplu işlemler

| Kimlik | Gereksinim | Aşama / öncelik |
|---|---|---|
| ORG-01 | Manuel koleksiyon oluşturma, yeniden adlandırma ve silme desteklenmeli; koleksiyon silme öğeleri/dosyaları silmemeli. | M1 / P0 |
| ORG-02 | Bir öğe birden çok koleksiyona ve etikete bağlanabilmeli. | M1 / P0 |
| ORG-03 | Kullanıcı notu, favori ve arşiv durumları kalıcı olmalı. | M1 / P0 |
| ORG-04 | Çoklu seçimle etiket/koleksiyon atama, favorileme, arşivleme ve kayıttan kaldırma sağlanmalı. | M1 / P0 |
| ORG-05 | Toplu işlemler başarılı/başarısız öğe sayısını doğru göstermeli; geri alma mümkün olan işlemlerde sunulmalı. | M1 / P0 |
| ORG-06 | Platform, koleksiyon, yerel durum, iş durumu, süre, dil ve tarihe göre filtreleme sağlanmalı. | M1 / P0 |
| ORG-07 | Akıllı koleksiyonlar, kayıtlı filtre kurallarını dinamik olarak uygulamalı. | M1 / P1 |
| ORG-08 | Koleksiyon ağacı, yeniden boyutlandırma ve erişilebilir taşıma alternatifi sunmalı. | M1 / P1 |
| ORG-09 | Etiket yeniden adlandırma, silme ve birleştirme desteklenmeli. Etkilenen öğelerin `SearchDocument` kayıtları aynı işlemde veya kalıcı bir reindex işiyle güncellenmeli; SEA-05 gereği işlem tamamlandıktan sonra aramada görünmeli. Birleştirme Undo kapsamındadır. | M1 / P0 |
| ORG-10 | Alt koleksiyonu olan bir koleksiyon silinirken seçenek sunulmalı: alt koleksiyonları üst seviyeye taşı (varsayılan) veya birlikte sil. Her iki durumda da öğeler ve dosyalar silinmez. ORG-08 ile aynı aşama ve öncelikte yürür; ORG-08 ertelenirse M1 düz koleksiyonla çıkar ve ORG-10 onunla birlikte ertelenir. | M1 / P1 |
| ORG-11 | Kullanıcı bir duplicate adayındaki iki öğeyi önizlemeli birleştirebilmeli: hangi not, etiket, koleksiyon, medya ve dökümün hedefe geçeceği önceden gösterilmeli; kaynak öğe soft-delete edilmeli ve işlem Undo kapsamında olmalı. | M1 / P1 |

**Önerilen semantik:** Inbox, arşivlenmemiş ve hiçbir manuel koleksiyona atanmamış öğeleri gösterir. Etiket veya favori eklemek Inbox'tan çıkarmaz. “Koleksiyona ekle” diğer üyelikleri korur; “koleksiyondan taşı” yalnızca belirtilen kaynak üyeliğini kaldırır. Arşiv, öğeyi normal listeden gizler; arama filtresiyle erişilebilir. Akıllı koleksiyon üyeliği elle düzenlenmez.

**Kaldırma semantiği:** “Kütüphaneden kaldır” soft-delete'tir. Öğe `removed_at` ile işaretlenir, tüm listelerden ve aramadan çıkar, “Recently removed” görünümünde saklama süresi boyunca (öneri: 30 gün, ayarlanabilir) durur ve buradan geri yüklenebilir. Süre dolduğunda veya kullanıcı “Kalıcı olarak sil” dediğinde kayıt, ilişkiler, dökümler ve türetilmiş dosyalar silinir. Kaldırma onayı ve sonrasında gösterilen bildirim “30 gün sonra kalıcı olarak silinir” bilgisini açıkça verir; süre sonundaki otomatik temizlik bu onayın kapsamındadır. Otomatik temizlik managed özgün medyayı hiçbir zaman silmez, her zaman `OrphanMedia`'ya düşürür. Özgün medyanın fiziksel silinmesi yalnızca kullanıcının açık “Kalıcı olarak sil ve medyayı da sil” eylemiyle olur. JSON import veya DB restore ile gelen `removed_at` dolu öğelerin saklama sayacı import/restore anından yeniden başlar; önizleme kaç öğenin “Recently removed”a gireceğini gösterir. Eski bir yedek, öğelerin hemen kalıcı silinmesine yol açmaz. Managed özgün medya (`storage_kind = managed`) kalıcı silmede ayrı bir onay kutusuyla sorulur ve varsayılan olarak korunur. Korunan dosya, Settings → Storage altındaki “Sahipsiz medya” listesinde görünür; oradan yeniden import edilebilir veya silinebilir. `external_reference` dosyalara hiçbir koşulda dokunulmaz. Öğe kaldırıldığında ona bağlı kuyruktaki ve çalışan işler iptal edilir; geri yüklemede bu işler otomatik yeniden başlamaz, kullanıcı isterse yeniden başlatır.

**Kaldırılmış öğenin URL'si yeniden kaydedilirse:** Ayrı yeni kayıt oluşturulmaz (unique kuralı). Kullanıcıya “Bu öğe Recently removed içinde” mesajı ve iki eylem sunulur: **Geri yükle** (varsayılan) ve **Vazgeç**. Geri yüklemede öğe tüm notları, override'ları ve ilişkileriyle geri gelir; Add URL ekranında yeni seçilen etiket/koleksiyonlar mevcutlara eklenir. Vazgeçilirse hiçbir şey değişmez. Kalıcı olarak silinmiş öğenin URL'si normal yeni kayıt oluşturur.

**Sahipsiz medya:** Kalıcı silmede korunan managed medya için `OrphanMedia` kaydı oluşturulur; liste dosya sistemi taramasıyla değil bu kayıtlarla üretilir. Kayıt dosyanın kökünü, yolunu, boyutunu ve varsa hash'ini taşır. Listeden “Yeniden import” yeni bir `local_video` öğesi oluşturur ve dosyayı taşımadan managed varlık olarak bağlar; “Sil” ayrı onay ister. Kökü `disconnected` olan kayıtlar erişilemez olarak gösterilir, silinmez. Ayrıca kullanıcı isteğe bağlı “Medya kökünü tara” eylemiyle hiçbir kayda bağlı olmayan dosyaları listeleyebilir; bu tarama otomatik çalışmaz.

**Undo semantiği:** Etiket/koleksiyon ekleme-çıkarma, etiket yeniden adlandırma ve birleştirme, öğe birleştirme (ORG-11), favori, arşiv ve kütüphaneden kaldırma işlemlerinin (tekli ve toplu) son işlemi geri alınabilir. Undo kaydı SQLite'ta işlem günlüğü olarak tutulur. Kullanıcının başlattığı yeni bir Undo kapsamındaki işleme veya uygulama yeniden başlayana kadar geçerlidir. Arka plan işleri (metadata, thumbnail, indeksleme, AI) Undo kaydını geçersiz kılmaz. Undo yalnızca işlemin değiştirdiği alanları geri yazar; arada arka plan işinin güncellediği başka alanlar korunur. Bir kaydın aynı alanı bu arada kullanıcı tarafından değiştirilmişse (`revision` kontrolü) Undo o kayıt için atlanır ve atlanan kayıt sayısı gösterilir. Kalıcı silme, fiziksel dosya silme, import ve metadata yenileme Undo kapsamında değildir. Kullanıcının başlattığı bu işlemler önceden açık onay ister; saklama süresi sonundaki otomatik temizlik ise kaldırma anındaki onaya dayanır. Etiket birleştirmenin Undo'su, işlem günlüğünde saklanan eski etiket kimliğini, adını ve tüm `LibraryItemTag` ilişkilerini geri getirir.

**Öneri:** M1 akıllı koleksiyon kural dili, sürümlü bir AND koşulları listesidir; ORG-06 filtreleriyle aynı alan kümesini (platform, etiket, manuel koleksiyon, favori, arşiv, tarih, süre, dil, yerel durum ve iş durumu) destekler. Kuralın referans verdiği etiket veya koleksiyon silinirse kural `invalid` işaretlenir; akıllı koleksiyon sessizce boş gösterilmez, hangi koşulun bozulduğu açıklanır. Ham SQL veya çalıştırılabilir ifade saklanmaz. Alt koleksiyonlarda döngüye izin verilmez.

### 7.4 Arama ve yeniden keşif

| Kimlik | Gereksinim | Aşama / öncelik |
|---|---|---|
| SEA-01 | SQLite FTS5 ile başlık, açıklama, kullanıcı notu, etiket ve kaynak/üretici metinleri aranmalı. | M1 / P0 |
| SEA-02 | Metin sorgusu ve yapılandırılmış filtre birlikte uygulanmalı; seçili filtreler kaldırılabilir chip olarak görünmeli. | M1 / P0 |
| SEA-03 | Sonuçlar başlık, ilgili snippet ve eşleşme kaynağını göstermeli; eşleşen metin güvenli biçimde vurgulanmalı. | M1 / P0 |
| SEA-04 | Boş sorgu filtrelenmiş kütüphane gibi çalışmalı; bozuk FTS sözdizimi anlaşılır biçimde ele alınmalı. | M1 / P0 |
| SEA-05 | Kaydetme, not ve etiket değişiklikleri tamamlandıktan sonra yeni aramada görünmeli. | M1 / P0 |
| SEA-06 | Transkript sonuçları öğe altında gruplanmalı; gerçek segment zamanına açılmalı. | M4 / P0 |
| SEA-07 | Çok dilli embedding ve lexical sonuçlar açıklanabilir hibrit sıralama ile birleşmeli. | M5 / P0 |
| SEA-08 | OCR ve görsel eşleşmeler konuşma eşleşmesinden ayrı etiketlenmeli. | M5 / P0 |

MVP doğal dil sorgusunu metin sorgusu olarak kabul eder; anlamsal eşleşme garantisi vermez. “PostgreSQL replikasyonunu anlatan konuşma” gibi bir sorgunun farklı kelimeli dökümü bulması M5 kalite hedefidir. Hiç dökümü olmayan içerik için zaman kodu üretilemez.

### 7.5 İndirme ve kuyruk

| Kimlik | Gereksinim | Aşama / öncelik |
|---|---|---|
| DL-01 | Kullanıcı izinli içeriğin formatlarını inceleyip anlaşılır kalite önayarlarıyla indirme başlatabilmeli. | M2 / P0 |
| DL-02 | Format seçimi gerçek format verisine dayanmalı; bilinmeyen boyut/süre tahmini kesin değer gibi sunulmamalı. | M2 / P0 |
| DL-03 | Metadata, download, merge, thumbnail ve indexing işleri ayrılmalı; bağımlılıkları ve durumları izlenmeli. | M2 / P0 |
| DL-04 | Kuyruk SQLite'da kalıcı olmalı; yeniden başlatmada yarım işler güvenli biçimde toparlanmalı. | M2 / P0 |
| DL-05 | Makine tarafından okunabilir JSON/progress çıktısı kullanılmalı; stdout/stderr sınırsız biriktirilmemeli. | M2 / P0 |
| DL-06 | İptal, yeniden deneme ve desteklenen işlerde devam ettirme bulunmalı. Pause yalnızca doğrulanmış teknik destek varsa etkin olmalı. | M2 / P0 |
| DL-07 | Dosya tamlığı doğrulanmadan tamamlandı denmemeli; birleştirme sürerken %100 başarı görünmemeli. | M2 / P0 |
| DL-08 | Disk dolması, oturum, bölge, silinmiş içerik ve çıkarıcı hataları ayrı aksiyonlarla gösterilmeli. | M2 / P0 |
| DL-09 | Yönetilen yt-dlp/FFmpeg/ffprobe sürümü ve kaynağı diagnostics'te görünmeli. | M2 / P0 |
| DL-10 | Dosya yolu, klasörde göster ve güvenli temizleme seçenekleri sunulmalı. | M2 / P0 |

**Öneri:** Varsayılan kalite “Best available”, alternatifler “Up to 1080p”, “Up to 720p” ve uygunsa “Audio only” olabilir. Her önayar, uygulama içinde oynatılabilir bir çıktı hedefler: birleştirme/remux hedefi, oynatma matrisinde (DEC-04) doğrulanmış container'dır (başlangıç adayı: H.264/AAC için MP4, VP9-AV1/Opus için WebM). Kaynak formatlar bu hedefe kayıpsız remux edilemiyorsa kullanıcıya “uygulama içinde oynatılamayabilir” uyarısı gösterilir; sessiz transcode yapılmaz. Bu değerler ürün önayarıdır; gerçek yt-dlp argümanları sürüme göre doğrulanır ve kullanıcıdan serbest komut satırı alınmaz. İşin indirme aşaması tamamlanıp AI indekslemesi sürüyorsa medya hazır, AI bekliyor olarak ayrı gösterilir.

### 7.6 Oynatma

| Kimlik | Gereksinim | Aşama / öncelik |
|---|---|---|
| PLAY-01 | Yerel dosya oynatma, ağdan bağımsız çalışmalı; seek, play/pause ve ses denetimleri erişilebilir olmalı. | M1 / P0 |
| PLAY-02 | Son izleme konumu ve devam etme eylemi kalıcı olmalı. | M1 / P0 |
| PLAY-03 | Kaynak URL ile yerel dosya durumu açıkça ayrılmalı. Yerel dosyası olmayan öğe için kaynakta aç eylemi sunulmalı. | M1 / P0 |
| PLAY-04 | Desteklenmeyen codec/container açıklanmalı; import başarısı oynatılabilirlik garantisi sayılmamalı. | M1 / P0 |
| PLAY-05 | Bölüm/filmstrip yalnızca veri veya üretilmiş önizleme varsa gösterilmeli. | M2 / P1 |
| PLAY-06 | Transkript tıklaması ilgili milisaniye ofsetine seek isteği göndermeli. | M4 / P0 |
| PLAY-07 | Uyumlu codec'lere sahip ama desteklenmeyen container'daki dosyalar (M0 oynatma matrisinde oynatılamadığı görülen container'lar; ilk aday MKV) için isteğe bağlı, kayıpsız remux ile türetilmiş oynatma önizlemesi (`MediaAsset.role = preview`) üretilebilmeli. Özgün dosya değişmez; önizleme cache kapsamındadır ve disk etkisi gösterilir. | M1 / P0 |

**Doğrulama kapısı:** Windows WebView2 ve seçilecek oynatma bileşeninin codec/container matrisi gerçek dosyalarla ölçülmelidir. WebView2'nin MKV ve HEVC desteği; runtime sürümüne, codec'e, MIME türüne, donanıma ve kullanılan yerel medya protokolüne bağlıdır. Bu belge MKV'nin oynatılamadığını varsaymaz; karar M0 oynatma matrisiyle verilir (DEC-04). Matriste oynatılamadığı görülen container/codec kombinasyonları için PLAY-07 remux önizlemesi kullanılır; hepsi oynatılabiliyorsa PLAY-07'nin kapsamı daraltılır. Desteklenmeyen durumda özgün dosya korunur ve güvenli harici oynatıcıya açma seçeneği sunulur. Uygulama içi oynatmanın temel test formatında çalışması MVP çıkış koşuludur.

### 7.7 Taşınabilirlik, yedek ve çevrimdışı kullanım

| Kimlik | Gereksinim | Aşama / öncelik |
|---|---|---|
| DATA-01 | Sürümlü JSON ile metadata dışa/içe aktarılmalı; kimlik bilgileri ve çerezler dışarı çıkmamalı. | M1 / P0 |
| DATA-02 | Kullanıcı notları, etiketler, koleksiyonlar, kaynaklar ve medya referansları geri yüklenebilmeli. | M1 / P0 |
| DATA-03 | Eksik medya dosyaları yeniden bağlanabilmeli; portable metadata export, medya yedeği diye sunulmamalı. | M1 / P0 |
| DATA-04 | Yerel arama, düzenleme, mevcut indeksler ve yerel oynatma ağ olmadan kullanılabilmeli. | M1 / P0 |
| DATA-05 | Açık Offline Mode, metadata/indirme/model/güncelleme ağ isteklerini ve backend'in uzak küçük resim indirmesini durdurmalı. | M1–M4 / P0 |
| DATA-06 | Hatalı/gelecek şemalı import mevcut veriyi bozmamalı; ön doğrulama ve sonuç raporu bulunmalı. | M1 / P0 |
| DATA-07 | Depolama kotası, boş alan uyarısı, model ve türetilmiş dosya kullanımı görünür olmalı. | M2 / P0 |
| DATA-08 | Medya kökü ayarlardan değiştirildiğinde managed dosyalar için “taşı” veya “yalnızca yeni indirmelerde kullan” seçeneği sunulmalı. “Yalnızca yeni indirmelerde” seçilirse yeni bir `StorageRoot` eklenir ve `active_download_root_id` değişir; eski varlıklar eski kökte kalır. “Taşı” seçilirse §13.1'deki `storage_move` işi varlık bazında yürütülür: hiçbir anda doğrulanmış tek kopya kaybolmaz, kesinti sonrası iş kaldığı yerden idempotent biçimde devam eder. Toplu taşımanın tamamı tek atomik işlem değildir; kesinti anında bazı varlıklar yeni, bazıları eski kökte olabilir ve her ikisi de doğru çözülür. | M2 / P0 |
| DATA-09 | Medya kökü veya dış referansların bulunduğu sürücü erişilemezse (ör. çıkarılmış harici disk) öğeler tek tek `missing` yapılmamalı; kök düzeyinde “bağlı değil” durumu (`StorageRoot.status = disconnected`) gösterilmeli ve sürücü geri geldiğinde otomatik doğrulanmalı. Bir varlığın `integrity_state` değeri yalnızca kökü `available` iken `missing` yapılabilir. | M1 / P0 |
| DATA-10 | OneDrive gibi bulut senkron klasörlerindeki yalnızca-bulutta (placeholder) dosyalar, kullanıcı onayı olmadan ffprobe/hash için indirilmeye zorlanmamalı; durumları ayrı gösterilmeli. | M1 / P1 |
| DATA-11 | Bir öğe için not, kaynak URL ve seçili transkript zaman kodlarını içeren Markdown not dışa aktarımı sağlanmalı; zaman kodları gerçek segmentlere dayanmalı (§8.4 adım 6). | M4 / P1 |

Offline Mode'un kullanıcıya verdiği söz şudur: VideoVault ve yönettiği araçlar ağa çıkmaz. Windows'un ve WebView2 runtime'ının kendi servisleri bu sözün dışındadır ve Settings → Privacy'de açıklanır. Offline Mode'da yeni bookmark yerel olarak kaydedilir; ağ işi bekletilir. Kaynakta aç gibi dış ağ eylemleri açıklamayla devre dışıdır. İnternetin kopması ile kullanıcının bilinçli Offline Mode seçimi farklı nedenler olarak gösterilir.

## 8. Kritik kullanıcı akışları

### 8.1 Beş saniyede kaydet

1. Kullanıcı URL'yi kopyalar, Add URL'yi açar veya `Ctrl+K` içindeki yakalama eylemini seçer.
2. URL yerelde doğrulanır ve platform tahmini hemen görünür.
3. Metadata isteği paralel ilerleyebilir; Save Bookmark etkin kalır.
4. Save işlemi öğeyi ve arama belgesini atomik oluşturur ve Inbox'ta gösterir. Kayıt sırasında manuel koleksiyon seçildiyse öğe Inbox yerine o koleksiyonda görünür ve sonuç bildirimi bunu belirtir.
5. Sonradan gelen metadata kullanıcı alanlarına dokunmadan öğeyi zenginleştirir.

Başarısızlık dalları: geçersiz şema → düzeltilebilir alan hatası; yinelenen URL → mevcut öğeye git/etiket ekle; çevrimdışı → pending metadata; sunucu hatası → bookmark korunur, yeniden dene. Yapıştırma sonrasında kaydetme en fazla iki birincil eylem gerektirir. Beş saniyelik hedef, kullanıcının URL'yi hazır ettiği andan yerel kaydın görünmesine kadardır; uzaktaki metadata süresini içermez.

### 8.2 Eski bookmark'ı yeniden bul

1. Global aramaya başlık/konu/etiket yazılır.
2. Başlık, açıklama ve not eşleşmeleri hemen döner.
3. Kullanıcı platform, tarih, koleksiyon veya yerel dosya filtresini ekler.
4. Döküm mevcutsa zaman kodlu alt sonuçlar açılır.
5. Öğe veya gerçek transkript zamanı açılır.

Hiç sonuç yoksa filtreleri temizleme ve sorguyu değiştirme sunulur. Transkript üretilmemişse açık durum ve isteğe bağlı oluşturma eylemi vardır; anlamsal mod yokken varmış gibi davranılmaz.

### 8.3 Yetkili özel VK videosu

1. Kullanıcı kalıcı video bağlantısını kaydeder.
2. İzinli metadata/indirme denenir.
3. Oturum gerekiyorsa hangi tarayıcı/profilin hangi amaçla okunacağı açıklanır.
4. Açık izinle desteklenen oturum yöntemi kullanılır.
5. Oturum doğrulaması ve içerik erişim sonucu ayrı kaydedilir.
6. Başarılı indirme doğrulanır veya kullanıcıya doğru hata/sonraki adım sunulur.

Çerez okuma hatası, grup üyeliği iptali, süresi dolmuş oturum, geçici medya URL süresi ve çıkarıcı bozulması ayrı dallardır. Her dalda bookmark ve kullanıcı metadata'sı korunur. Gerçek özel video testi yapılmadan başarı beyanı verilemez.

### 8.4 Videodan aranabilir bilgi üret

1. Kullanıcı tamamlanmış dosya veya yerel import üzerinde Transcribe seçer.
2. İzinli mevcut altyazı varsa kaynağı gösterilerek önerilir.
3. Yerel model gerekiyorsa boyut, lisans, beklenen kaynak ihtiyacı ve indirme onayı gösterilir.
4. Kuyruk ses hazırlama, transkripsiyon ve indeksleme ilerlemesini gösterir.
5. Tamamlanan döküm dil/kaynak/model bilgisiyle açılır.
6. Zaman koduna tıklanır, etiket/koleksiyon eklenir veya kaynak ve zaman kodu içeren not dışa aktarılır.

Eksik model, yetersiz RAM/disk, desteklenmeyen ses ve iptal durumlarında medya kaybolmaz. İşin devam ettirilebilir checkpoint'i varsa yeniden kullanılır; aksi durumda yeniden başlama açıkça belirtilir.

### 8.5 Büyük Inbox'ı düzenle

1. Kullanıcı 20 kartı klavye veya işaretleme ile seçer.
2. Toplu etiket, koleksiyon, favori veya arşiv eylemi uygular.
3. İşlem sonucu ve mümkünse Undo gösterilir.
4. Liste konumu ve seçim bağlamı korunur.

Benzer URL veya dosya eşleşmesi uyarı olarak sunulur. Otomatik yıkıcı birleştirme yapılmaz. Filtre nedeniyle öğeler listeden çıkarsa işlem sonucu kullanıcıya açıklanır.

### 8.6 Hata sonrasında toparlan

1. Ağ kesilir, disk dolar, oturum sona erer veya uygulama işlem sırasında kapanır.
2. Kuyruk tutarlı hata/bekleme durumuna geçer.
3. Yeniden başlatmada çalışan işler lease/checkpoint üzerinden uzlaştırılır.
4. Kullanıcı disk açar, oturumu yeniler veya yeniden denemeyi seçer.
5. Destek varsa kısmi dosyadan devam edilir; yoksa açıkça yeniden başlatılır.

Başarı koşulu; kaybolan bookmark olmaması, tamamlanmamış dosyanın hazır görünmemesi, iki worker'ın aynı çıktıyı yayımlamaması ve gizli verinin loglara sızmamasıdır.

## 9. Bilgi mimarisi ve görsel tasarım

### 9.1 Tasarım yönü

Sakin, modern, içerik odaklı bir masaüstü arayüzü hedeflenir. İlk açılışta koyu tema, alternatif açık tema ve son seçimin korunması bulunmalıdır. Nötr yüzeyler, tek vurgu rengi, güçlü metin hiyerarşisi, belirgin odak/hover, sınırlı hareket ve ölçülü yuvarlatma kullanılmalıdır. Büyük tanıtım başlıkları, yoğun gradyanlar ve aşırı cam efekti kullanılmamalıdır.

Tasarım sentezindeki Eagle %50, Raindrop %30 ve Recall %20 oranları ekran alanı veya geliştirme bütçesi değildir; sırasıyla düzenleme, hızlı yakalama ve AI keşfi üzerindeki nitel ağırlığı ifade eder.

### 9.2 Yerleşim ölçüleri

| Bölge | Şart |
|---|---|
| Sol gezinme | 220–280 px; daraltılabilir, yeniden boyutlandırılabilir; Inbox, All Items, Favorites, Downloads, Recently Added, Collections, Smart Collections, katlanabilir platform grubu, Archive ve Recently removed; alt bölümde Sessions (M3), AI Processing (M4) ve Settings. Promptun ana gezinmesindeki Search, üst çubuktaki global arama ve Search ekranıyla karşılanır. Promptta geçen “All Videos” burada “All Items” olarak adlandırılır, çünkü ses dosyalarını da kapsar |
| Üst komut çubuğu | 56–68 px; global arama, Add URL, gerekirse kuyruk göstergesi, görünüm ve sıralama |
| İçerik | Yaklaşık 220–280 px minimum kart genişliği; düzenli grid ve liste/tablo; küçük resimler 16:9 |
| Inspector | İsteğe bağlı 300–380 px; metadata, etiketler, koleksiyonlar, notlar, kaynak, medya ve AI durumu |
| Dar pencere | Inspector kapanır/çekmeceye dönüşür; gezinme daralır; ana iş akışları yatay taşmaya bağımlı olmaz |
| Büyük kütüphane | Sanallaştırılmış liste/grid; geri dönüşte scroll ve seçim korunur |

1366×768'de inspector kapalıyken en az iki kullanılabilir kart sütunu; 1920×1080'de panel genişliklerine bağlı olarak dört veya daha fazla sütun hedeflenir. Ultrawide görünüm metinleri okunamayacak genişliğe yaymamalıdır. Masonry, kart durumlarını gizlemeyen ve klavye sırasını koruyan alternatif olarak P1'dir.

Kartlarda süre, kaynak platformu, bookmark/yerel dosya durumu ve mevcut AI durumu sürekli ayırt edilebilir olmalıdır. Koleksiyon sayaçları yalnızca doğru ve düşük maliyetle hesaplanabiliyorsa gösterilir. Panel genişlikleri/görünüm tercihleri yerelde saklanır. Hızlı hover preview P1'dir; isteğe bağlı türetilmiş medya kullanır, ağ isteğini veya sesli otomatik oynatmayı tetiklemez ve klavye ile erişilebilir alternatifi bulunur.

**Önerilen breakpoint'ler:** `<900 px` kompakt görünüm, `900–1199 px` dar masaüstü, `≥1200 px` tam masaüstü. 200% metin ölçeğinde panel davranışı kullanılabilir içerik alanına göre yeniden hesaplanır; bu değerler görsel testten sonra kesinleşir.

### 9.3 Ekran sözleşmeleri

| Ekran | Temel içerik ve eylemler | Zorunlu durumlar | Aşama |
|---|---|---|---|
| Library | Grid/list, sıralama, platform/koleksiyon filtreleri, toplu seçim, inspector | İlk kullanım, yükleniyor, çok öğe, eksik thumbnail, çevrimdışı, boş filtre | M1 |
| Add URL | Yapıştır alanı, platform, önizleme, Save Bookmark, isteğe bağlı etiket/koleksiyon/Download | Geçersiz URL, duplicate, pending metadata, offline, unsupported download | M1–M2 |
| Downloads | Aktif/tamamlanan/hatalı işler, faz, gerçek ilerleme, retry/cancel ve desteklenirse pause | Ağ/disk/auth hatası, indeterminate progress, yeniden başlatma, kısmi dosya | M2 |
| Video Detail | Oynatıcı veya kaynak kartı, metadata; Transcript, Notes, Chapters, Related alanları | Bookmark only, dosya hazır/eksik/bozuk, unsupported codec, AI yok/işleniyor | M1–M5 |
| Search | Tek sorgu, filtre chip'leri, öğeye göre gruplanmış snippet ve zaman kodları | Boş sorgu, sonuç yok, lexical, transcript, gelecekte hybrid/visual | M1–M5 |
| Collections | Manuel ağaç, akıllı kurallar, toplu atama ve düzenleme | Boş, kurala uygun öğe yok, geçersiz kural, silme/geri alma | M1 |
| Sessions | Platform bazlı durum, doğrulama zamanı, clear/reconnect, güvenli yönlendirme | Not connected, Session available, Expired, Needs verification, Unsupported | M3 |
| AI Processing | Model kataloğu/yerel durum, indirme izni, disk/RAM bilgisi, kuyruk, cancel | Model yok, indirme bekliyor, doğrulama hatası, CPU/GPU kullanılabilirliği | M4–M5 |
| Settings & Privacy | Yollar, kota, concurrency, dil, tema, export/import, offline, update, veri silme | Geçersiz yol, yetersiz disk, yeniden bağlama, onay bekleyen model | M1–M4 |

Gelecek aşama kontrolleri üretimde ya gizlenir ya da açık açıklamayla devre dışıdır. Çalışmayan bir düğme işlevselmiş gibi gösterilemez. Detay ekranındaki AI kategorileri ve özetleri yalnızca ilgili özellik gerçekten işlendiğinde görünür.

### 9.4 Klavye, erişilebilirlik ve durum geri bildirimi

- `Ctrl+K`: arama/komut paletini açar; URL yapıştırılmışsa Add URL önerisi sunabilir. Odak davranışı ekranlar arasında tutarlı olmalıdır.
- `Ctrl+V`: uygun yakalama bağlamında URL yapıştırır; not/metin alanlarında normal yapıştırmayı korur.
- `Esc`: modal/palet/inspector gibi en üst geçici yüzeyi kapatır, odağı çağırana döndürür.
- `Enter`: seçili sonucu veya birincil eylemi açar; çok satırlı not alanının davranışını bozmaz.
- `Space`: yalnızca oynatıcı odak bağlamında play/pause; metin girişi veya kart seçimiyle çatışmaz.
- Arama sonuçlarında yukarı/aşağı gezinme ve Enter zorunludur. Çoklu seçim ve sürükle-bırak klavye alternatifi taşımalıdır.
- Temel metin ve denetimlerde WCAG AA kontrast hedefi, anlamlı erişilebilir ad, görünür odak, ekran okuyucuya durum duyurusu ve 200% metin ölçeği desteklenmelidir. 200% ölçek temel akışlarda (kaydet, ara, oynat, ayarlar) zorunludur (AC-27); yoğun grid gibi yüzeylerde yeniden düzen kabul edilir, işlev kaybı kabul edilmez.
- Yalnızca renge dayalı durum ayrımı yapılamaz. Yüklenme animasyonu tamamlanmış AI sonucuna benzeyemez.
- 120–200 ms geçişler yararlıysa kullanılabilir; reduced-motion tercihinde kaldırılır veya azaltılır.
- Boş kütüphane, sonuç yok, indirilmemiş bookmark, döküm yok, internet yok, geçersiz URL, süresi dolmuş oturum ve yasak/kullanılamayan indirme için ayrı metin ve eylem bulunmalıdır.

### 9.5 Tasarım sistemi ve referans araştırması

`docs/design-system.md`; renk token'ları, 4/8/12/16/24/32 spacing, tipografi rolleri, yüzeyler, breakpoint'ler, focus/hover/disabled/error/loading durumları ve hareket kurallarını tanımlamalıdır. Sistem fontu veya lisansı doğrulanmış paketli font kullanılmalıdır. Runtime'da font, ikon veya UI asset'i için üçüncü taraf CDN çağrısı yapılmaz.

| Referans | Resmî adres | Kaynağın önerdiği inceleme odağı | Uyarlama hedefi | Kopyalanmayacak unsurlar |
|---|---|---|---|---|
| Eagle | https://www.eagle.cool/ | Yoğun varlık kütüphanesi, düzenleme, inspector | Koleksiyon ağacı, toplu etiket, grid, akıllı filtre | Piksel düzeni, marka, ikon, asset ve metin |
| Raindrop.io | https://raindrop.io/ | Bookmark yakalama ve koleksiyonlar | Inbox-first ve indirmeden kayıt | Ürün kimliği ve özel arayüz/iş akışı varlıkları |
| Recall | https://www.recall.it/ | Bilgi yakalama ve AI keşfi | Kanıt snippet'i, zaman kodu, kaynaklı ilişkiler | Bulut zorunluluğu ve doğrulanmamış AI iddiaları |
| Stacher | https://stacher.io/ | Sade indirme etkileşimi | Kalite önayarı ve anlaşılır kuyruk | Markası, kaynak kodu veya varsayılan CLI karmaşası |
| Jellyfin | https://jellyfin.org/ | Medya ayrıntısı ve oynatma | Büyük oynatıcı ve kaldığı yerden devam | TV/streaming odaklı ürünün bütünü |
| Obsidian | https://obsidian.md/ | Yerel bilgi ve klavye gezinmesi | Komut paleti, hızlı geçiş, dışa aktarma | MVP'ye kapsamlı eklenti sistemi/graf eklemek |

Bu tablo kaynak promptun araştırma brifidir; rakip ürünlerin özelliklerinin bağımsız gözlemle doğrulandığı anlamına gelmez. UI uygulanmadan önce yalnızca kamuya açık resmî kaynaklar incelenmeli; `docs/design-references.md` içinde gerçek gözlem, kaynak URL, inceleme tarihi, çözülen kullanıcı sorunu, uyarlama önerisi ve kopyalanmayacak öğeler ayrılmalıdır. Erişim yoksa bu kısıt yazılmalıdır.

İlk wireframe'ler Library, Add URL, Downloads, Video Detail ve Search için kaynak kontrolünde tutulmalıdır. Component showcase gerçek uygulama durumlarını kullanmalıdır. 1366×768, 1920×1080 ve dar pencere ekran görüntüleri hassas olmayan fixture'larla alınmalıdır.

### 9.6 Yerelleştirme ve metin davranışı

İlk yayımlanan arayüz İngilizcedir; tüm kullanıcı metinleri çeviri anahtarlarıyla yönetilir ve Türkçe çeviri için hazırlanır. Backend hata kodu döndürür; çevrilebilir mesaj UI katmanında çözülür. Tarih, saat, sayı, dosya boyutu ve çoğul biçimleri seçili locale'e göre biçimlenir. Platformdan gelen özgün başlık ve kullanıcı notu otomatik çevrilmez veya değiştirilmez.

Uzun başlıklar grid boyutlarını bozmaz; tam metin detayda ve erişilebilir ad/tooltip üzerinden okunabilir. Context menu, tooltip ve quick switcher davranışları klavye/focus kurallarıyla tutarlıdır. Tema başlatma tercihi ilk render öncesinde uygulanır; yanlış temanın kısa süreli parlaması önlenir.

### 9.7 İlk açılış

İlk açılış kısa ve atlanabilir olmalıdır:

1. Karşılama: uygulamanın yerel çalıştığı ve hesap gerektirmediği söylenir.
2. Yasal bildirim: kullanıcı içeriği edinme hakkından/izninden sorumludur; uygulama DRM veya erişim kontrollerini aşmaz. Bu, okunabilir bir bilgilendirme ve “Anladım” eylemidir; Settings → About'tan tekrar açılabilir. İlk indirmeden önce kısa bir hatırlatma gösterilir.
3. Medya kökü seçimi: önerilen varsayılan konum ve boş alan bilgisi. OneDrive gibi senkron klasör seçilirse uyarı gösterilir (DATA-10).
4. Güncelleme tercihi (varsayılan: elle kontrol) ve Offline Mode'un ne yaptığına dair kısa bilgi.
5. Tema (varsayılan koyu).

İlk açılış sırasında bağış, AI model indirme veya oturum bağlama istenmez. Adımlar atlanırsa belgelenmiş varsayılanlar uygulanır.

## 10. Sistem mimarisi

### 10.1 Ana bileşenler

```text
React + TypeScript + Vite UI
        |
        | Tipli Tauri command / event sınırı
        v
Rust uygulama servisleri ve domain kuralları
        |
        +-- SQLite repositories + migrations + FTS5
        +-- Connector registry + URL / network policy
        +-- Durable scheduler + process supervisor
        |       +-- yt-dlp adapter
        |       +-- FFmpeg / ffprobe adapter
        |       +-- Local AI provider (M4+)
        +-- Managed storage + import/export + integrity
        +-- OS-protected secret/profile references
        +-- Diagnostics + update verification
```

MVP'de merkezi backend yoktur. UI, domain veya depolama kurallarını kendi başına aşamaz. HTTP istekleri, dosya erişimi, süreç başlatma ve kimlik bilgisi kullanımı backend üzerinden yürütülür. Keyfi uzak sayfalar ayrıcalıklı Tauri komutlarına erişemez.

### 10.2 Teknoloji kararları

| Katman | Kaynakta belirlenen yön | Uygulama öncesi doğrulama |
|---|---|---|
| Desktop | Tauri 2, Rust | Windows 11, WebView2, installer ve capability ayarları |
| UI | React, TypeScript, Vite | Birbiriyle uyumlu sabit sürümler ve test zinciri |
| Veri | SQLite, migrations, FTS5 | Dağıtılan SQLite derlemesinde FTS5 mevcut mu? |
| İndirme | Yönetilen yt-dlp executable | Güvenilir dağıtım, lisans, hash/imza, JSON/progress sözleşmesi |
| Medya | FFmpeg ve ffprobe | Kullanılan build'in lisansı, codec'ler, yeniden dağıtım koşulları |
| JS runtime | yt-dlp'nin ihtiyaç duyduğu harici JavaScript runtime (ör. Deno) | yt-dlp'nin güncel gereksinimi, desteklenen runtime sürümü, lisans, boyut ve güncelleme politikası |
| Transkripsiyon | Yerel Whisper uyumlu provider | Windows paketleme, model lisansı, RAM/CPU/GPU gereksinimi |
| Vector | Soyut repository arayüzü | sqlite-vec veya alternatifin bakım/uyumluluk/lisans durumu; M5 |
| Android | Paylaşılan UI/domain, gerektiğinde native köprü | Tauri Android plugin, ABI, izinler ve store kuralları; M6 |

Paket, model, CLI bayrağı ve sürüm numaraları varsayımla sabitlenmez. M0/M4 karar kayıtlarında resmî doküman, seçilen sürüm, uyumluluk ve test kanıtı bulunmalıdır. Büyük bir UI bağımlılığı, gereksinime dayalı gerekçe olmadan eklenmemelidir.

### 10.3 Önerilen modül sınırları

| Modül | Sorumluluk |
|---|---|
| `domain` | Kimlikler, durumlar, değişmezler, hata sınıfları |
| `application` | Capture/import/organize/search/playback use case'leri |
| `persistence` | Repository implementasyonları, transaction, migration ve FTS |
| `connectors` | Platform tanıma, yetenekler, metadata ve erişim eşleme |
| `jobs` | Scheduler, lease, bağımlılık, retry, checkpoint, cancellation |
| `media` | ffprobe, thumbnail, oynatma varlığı, FFmpeg işlemleri |
| `ai` | Model registry, transcript/OCR/embedding provider'ları |
| `security` | URL/path policy, capabilities, secret reference, redaction |
| `portability` | JSON schema, export/import, backup ve relink |
| `diagnostics` | Sürüm/sağlık bilgisi ve gizli veri içermeyen olaylar |

UI'da karşılık gelen feature modülleri kullanılabilir. Bu isimler öneridir; kesin klasör yapısı M0'da ADR ile kaydedilir. Domain kuralları platforma özgü executable yollarına bağımlı olmamalıdır.

## 11. Veri modeli

### 11.1 Ortak veri kuralları

**Önerilen başlangıç kararları:**

- Kimlikler rastgele UUID olarak metin biçiminde saklanır. Export/import kimlik çakışması açık eşleme ile çözülür.
- Zaman damgaları veritabanında UTC Unix milisaniyesi, JSON'da ISO 8601 UTC olarak tutulur. UI yerel saat diliminde gösterir.
- Süre ve medya ofsetleri tam sayı milisaniyedir. Bilinmeyen değer `null` olur; `0` ile temsil edilmez.
- Dil kodları BCP 47 etiketidir (ör. `tr`, `en-US`); bilinmiyorsa `null`. `und` yalnızca kaynak dili açıkça belirsiz bildirdiğinde kullanılır.
- Yayın tarihi iki biçimde saklanır. `published_precision = datetime` ise `published_at` UTC Unix milisaniyesidir. `published_precision = date` ise `published_date` saat dilimsiz takvim tarihidir (`YYYY-MM-DD` metni) ve `published_at` null kalır. Tarih-only değer saat dilimine dönüştürülmeden gösterilir; böylece gün kayması olmaz. Sıralama ve filtrede datetime değerler kullanıcının yerel tarihine indirgenerek tarih-only değerlerle karşılaştırılır. JSON'da da aynı ayrım korunur (`published_at` ISO 8601 UTC, `published_date` `YYYY-MM-DD`).
- SQLite foreign keys açık olmalıdır. Unique constraint'ler uygulama katmanındaki duplicate kontrolünü tamamlar.
- Kullanıcının değiştirdiği alan ile platformdan gelen değer ayrılır; metadata yenilemesi kullanıcı düzeltmesini ezmez.
- Metinler düz veri olarak saklanır. HTML/Markdown render edilecekse güvenli izin listesi uygulanır.
- Arama/vector indeksleri türetilmiş ve yeniden üretilebilir veridir. Notlar, ilişkiler ve özgün dökümler otoritatif veridir.

### 11.2 `LibraryItem`

| Alan | Tür / kural |
|---|---|
| `id` | UUID, primary key |
| `kind` | `bookmark`, `remote_video`, `local_video` |
| `canonical_url` | Uzak öğelerde kalıcı kaynak URL; yerel öğede nullable |
| `original_url`, `normalization_version` | Kullanıcının girdiği özgün URL (hassas sorgu değerleri CAP-08'e göre redakte) ve `canonical_url`'yi üreten normalizasyon kural sürümü |
| `source_platform` | `youtube`, `facebook`, `instagram`, `tiktok`, `x`, `vk`, `generic`, `local` |
| `source_id` | Platform kimliği, bilinmiyorsa null |
| `title`, `description`, `creator` | Platformdan gelen güvenli metin; metadata yenilemesi yalnızca bu alanları günceller |
| `user_overrides` | Kullanıcının düzelttiği alanlar: `title`, `description`, `creator`, `language` ve tek tipli değer olarak `published_at: { value, precision }`. Sürümlü, tipli yapı; UI'da görünen değer = override varsa override, yoksa platform değeri. Override temizlenince platform değerine dönülür |
| `published_at`, `published_date`, `published_precision`, `added_at`, `updated_at` | Yayın zamanı (UTC ms) veya tarih-only değer (`YYYY-MM-DD`) ve hassasiyeti (§11.1), nullable; ekleme/güncelleme zorunlu |
| `duration_ms`, `language` | Bilinmiyorsa null; dil mümkünse standardize kod |
| `visibility` | `unknown`, `public`, `unlisted`, `private`; yalnızca eldeki kanıt kadar |
| `remote_availability` | `unknown`, `available`, `unavailable`, `deleted`, `region_restricted`; silinme ve bölge bilgisinin tek kaynağı |
| `access_state`, `access_profile_id?`, `access_checked_at?` | §12.1 öğe katmanı erişim durumu ve son denemenin profili/zamanı; oturum geçerliliği burada tutulmaz |
| `metadata_status` | `pending`, `fetching`, `ready`, `partial`, `failed` |
| `thumbnail_asset_id` | Yerel türetilmiş varlık referansı, nullable |
| `user_notes` | Kullanıcı metni; platform metadata'sından bağımsız |
| `is_favorite`, `archived_at` | Favori ve arşiv; kaynak UX gereksinimlerini tamamlayan alanlar |
| `removed_at` | Soft-delete zamanı (§7.3); null değilse öğe listelerden ve aramadan çıkar |
| `last_checked_at` | Son uzak doğrulama; başarının kapsamı ayrıca kaydedilir |
| `metadata_provenance` | Extractor/adapter ve kaynak sürümü; gizli verisiz seçilmiş alanlar |
| `revision` | **Zorunlu** (AC-13 buna dayanır): her yazımda artan sürüm; güncelleme komutları beklenen `revision` ile gelir, uyuşmazsa çakışma döner |

**Önerilen `kind` semantiği:** URL ile ilk kayıt `bookmark` olur; doğrulanmış ilk uzak medya ediniminden sonra `remote_video` olur. Yerel import `local_video` olur. Daha sonra dosyanın eksilmesi `kind` değerini değiştirmez. “Yerelde mevcut” bilgisi her zaman `MediaAsset` kayıtlarından türetilir; `kind` tek başına indirilmiş durumunu göstermez. Ses dosyası kabul edildiğinde bu üçlü kaynak türü korunur, medya türü ayrıca varlıkta `audio` olarak tutulur; UI ses kaydını yanlışlıkla video diye etiketlemez.

### 11.3 `MediaAsset`

| Alan | Tür / kural |
|---|---|
| `id`, `library_item_id` | UUID ve foreign key |
| `role` | **Öneri:** `original`, `thumbnail`, `preview`, `waveform`, `extracted_audio`, `sampled_frame` |
| `media_kind` | `video`, `audio`, `image`, `other` |
| `storage_kind` | `managed`, `external_reference` |
| `storage_root_id` | Varlığın bulunduğu `StorageRoot` (§11.6). Managed medya, türetilmiş dosyalar ve dış referanslar için zorunludur |
| `storage_ref` | `storage_root_id` köküne göre relative yol. Kökün absolute yolu yalnızca `StorageRoot.path`'te tutulur; böylece kök taşınınca veya yeniden bağlanınca varlık satırları değişmez |
| `content_hash`, `hash_algorithm` | İsteğe bağlı tam içerik hash'i; **öneri:** SHA-256 |
| `size_bytes`, `mime_type`, `container` | ffprobe/dosya gözleminden doğrulanmış metadata |
| `video_codec`, `width`, `height` | Varsa video özellikleri |
| `audio_tracks` | Dil/codec/kanal gibi çoklu parça bilgisi; ayrı tablo veya sürümlü yapı |
| `duration_ms`, `acquired_at` | Medya süresi ve edinim zamanı |
| `acquisition_status` | `pending`, `acquiring`, `ready`, `failed`, `canceled` |
| `integrity_state` | `unknown`, `verified`, `missing`, `corrupt`, `modified` |
| `source_metadata` | Secret içermeyen allowlist; ham extractor JSON'u varsayılan saklanmaz |
| `source_fingerprint` | Türetilmiş AI/preview işlerinin hangi sürüm girdiden oluştuğunu belirler |
| `source_asset_id?` | Türetilmiş varlıklarda (`role` ≠ `original`) zorunlu: üretimin yapıldığı kaynak `MediaAsset`. Uzaktan indirilen thumbnail için null olur ve `source_metadata` kaynak URL'nin redakte biçimini taşır |
| `derivation` | Türetilmiş varlıklarda zorunlu: `generator` (ör. ffmpeg, provider), `generator_version`, `config` (tipli, sürümlü üretim ayarları), `created_at` |

`ready` edinim sonucu, dosyanın hâlen mevcut olduğunu tek başına garanti etmez; oynatma öncesi erişilebilirlik kontrol edilir. Özgün medya değişirse eski transkript/vector indeksleri sessizce yeni dosyaya bağlanmaz; `stale` işaretlenir.

**Türev geçersizleştirme:** Bir varlığın `integrity_state` değeri `modified` olursa veya öğeye yeni bir `original` varlık edinilirse, `source_asset_id` ile eski varlığa bağlı tüm türevler (preview, waveform, extracted_audio, sampled_frame, üretilmiş thumbnail) ve `asset_id` ile bağlı dökümler `stale` işaretlenir. Stale türev oynatma veya arama için kullanılmaz, ancak kullanıcı verisi olan dökümler silinmez. Kaynak varlık silinirse türevleri de silinir (cache kapsamı); dökümler korunur ve `asset_id` null'a çekilir.

### 11.4 Düzenleme tabloları

| Tablo | Alanlar / kurallar |
|---|---|
| `Collection` | `id`, `name`, `parent_id?`, `sort_order`, `created_at`, `updated_at`; parent döngüsü yasak |
| `Tag` | `id`, `name`, `normalized_name`, `created_at`; normalizasyona göre unique |
| `LibraryItemTag` | `(item_id, tag_id)` bileşik primary key; `origin` (`user` \| `ai_accepted` \| `import`), `proposal_id?` |
| `LibraryItemCollection` | `(item_id, collection_id)` bileşik primary key |
| `SmartCollection` | `id`, `name`, `rule_schema_version`, doğrulanmış `rules`, `sort`, zamanlar |
| `PlaybackState` | `item_id`, `asset_id`, `position_ms`, `last_played_at`, isteğe bağlı `completed_at` |
| `UrlAlias` | `id`, `item_id`, `url` (normalize edilmiş), `normalization_version`, `created_at`; `url` unique. Capture, eşleşme ararken `canonical_url`'nin yanında alias'lara da bakar |
| `DuplicateCandidate` | `id`, `item_a_id`, `item_b_id`, `reason` (`normalization_conflict` \| `source_id_conflict` \| `content_hash` \| `similar_url`), `status` (`open` \| `merged` \| `dismissed`), `created_at`, `decided_at?`. IMP-05 hash eşleşmeleri de bu tabloyu kullanır |
| `OrphanMedia` | `id`, `storage_root_id`, `storage_ref`, `size_bytes`, `content_hash?`, `former_item_title?`, `orphaned_at`. Kalıcı silmede korunan managed medya için oluşturulur (§7.3) |
| `AIProposal` | `id`, `item_id`, `type` (`tag` \| `topic` \| `collection`), `value`, `status` (`pending` \| `accepted` \| `rejected`), `provider`, `model_id`, `model_version`, `source_fingerprint`, `created_at`, `decided_at?` (AI-B02) |

Son iki tablo kaynak promptun akıllı koleksiyon ve kaldığı yerden devam etme gereksinimlerini somutlaştırır. Notlar için MVP'de öğe başına tek metin yeterlidir; bağımsız not/backlink sistemi sonraki karar olabilir.

### 11.5 Döküm ve arama

| Tablo | Alanlar / kurallar |
|---|---|
| `Transcript` | `id`, `item_id`, `asset_id?`, `language`, `provider`, `model_id?`, `model_version?`, `provenance`, `status`, `source_fingerprint`, zamanlar |
| `TranscriptSegment` | `id`, `transcript_id`, `ordinal`, `start_ms`, `end_ms`, `text`, `confidence?`, `speaker?` |
| `SearchChunk` | `id`, `item_id`, `transcript_id?`, `start_segment_id?`, `end_segment_id?`, `start_ms?`, `end_ms?`, `text`, `source_type`, `source_revision`, `producer_provider?`, `producer_model_id?`, `producer_model_version?`, `embedding_model_id?`, `embedding_version?`, `vector_ref?` |
| `SearchDocument` | **Öneri:** öğe metadata'sının FTS'ye hazır başlık/açıklama/not/etiket/üretici alanları |
| `ModelInstallation` | **M4 önerisi:** model/provider kimliği, sürüm, checksum, lisans, path, boyut, install/consent durumu |

`Transcript.provenance`: `source_captions | generated_local | imported`. `generated_local` dökümlerde `asset_id` zorunludur; `source_captions` ve `imported` dökümlerde null olabilir. `status`: `pending | processing | partial | ready | failed | canceled | stale`. Aynı öğenin farklı dillerde veya farklı modellerle üretilmiş dökümleri ayırt edilebilmelidir.

Segmentlerde `0 <= start_ms < end_ms` zorunludur. Bilinen medya süresini aşan zamanlar doğrulama hatasıdır veya belgeli toleransla düzeltilir; sessizce uydurulmaz. Çakışan konuşmacı/altyazı zamanları desteklenebilir. `confidence` ve `speaker` provider üretmiyorsa null olur.

`source_type`: `metadata | transcript | ocr | visual_description | user_note`. Metadata/not için zaman aralığı zorunlu değildir.

Lexical (FTS5) aramada öğe metadata'sı, notlar ve etiketlerin tek kaynağı `SearchDocument`'tır. `SearchChunk` zaman aralıklı içerik (transkript, OCR, görsel açıklama) ve M5 embedding'leri içindir. `metadata` ve `user_note` türündeki chunk'lar yalnızca embedding üretimi için oluşturulur; `SearchDocument` ile aynı transaction'da yazılır ve `source_revision` ile senkron tutulur. Görsel embedding hit'inin metinsel açıklaması mevcut değilse metin kanıtı uydurulmaz; ilgili kare ve zaman gösterilir.

### 11.6 İş, oturum ve ayarlar

| Tablo | Zorunlu alanlar |
|---|---|
| `Job` | `id`, `type`, `item_id?`, `status`, `attempts`, `created_at`, `updated_at`, `progress?`, `error_code?`, `redacted_error_message?`, `cancel_requested`, `next_retry_at?` |
| `Job` ek önerileri | `payload_version`, tipli/gizli verisiz `payload`, `parent_job_id?`, `idempotency_key`, `priority`, `phase`, `bytes_done?`, `bytes_total?`, `checkpoint_ref?`, `lease_owner?`, `lease_expires_at?`, `generation`, `blocked_reason?`, `cancel_reason?` (`user` \| `dependency_failed` \| `item_removed` \| `shutdown`) |
| `JobDependency` | `job_id`, `depends_on_job_id`; döngü yasak |
| `AuthProfile` | `id`, `provider`, `label`, `secure_local_ref?`, `browser_kind?`, `profile_locator?`, `session_state`, `consent_scope`, `consent_at?`, `last_verified_at?` |
| `StorageRoot` | `id`, `kind` (`managed_media` \| `models` \| `cache` \| `external_import`), `label`, `path`, `volume_id?`, `status` (`unknown` \| `available` \| `disconnected` \| `inaccessible`), `created_at`, `last_verified_at?`. Dış import'ta seçilen üst klasör (tek dosyada dosyanın klasörü) `external_import` kökü olarak kaydedilir; aynı klasör veya onu kapsayan bir kök varsa yeniden kullanılır |
| `AppSettings` | `schema_version`, `active_download_root_id`, concurrency, kota/uyarı, model seçimi, dil, tema, privacy, offline, update tercihleri |

`Job.payload`, çerez, bearer token, geçici authenticated medya URL'si veya ham araç argüman dump'ı içeremez. Yeniden başlatmada erişim bağlamı güvenli profil referansından tekrar oluşturulur. `AuthProfile.secure_local_ref`, işletim sistemi korumalı store'a referanstır; ham secret değildir.

### 11.7 Unique constraint, indeks ve migration

- Bilinen uzak kimlik için `(source_platform, source_id)` unique; null kaynak kimliği ayrı ele alınır.
- Uzak öğelerde normalize edilmiş kalıcı URL üzerinde unique constraint bulunur. Eşzamanlı capture yarışı mevcut öğeye döner.
- Normalizasyon kuralları değiştiğinde migration, `normalization_version` eski olan öğelerin `canonical_url` değerini yeniden hesaplar. Yeni değer başka bir öğeyle çakışırsa otomatik birleştirme yapılmaz. Kural şöyledir: çakışan öğelerden `added_at` değeri en eski olan yeni değeri alır. Diğer öğe eski `canonical_url` ve eski `normalization_version` değeriyle kalır, `normalization_conflict = true` işaretlenir ve `DuplicateCandidate(reason = normalization_conflict)` kaydı oluşur. Eski değer farklı olduğu için unique indeks ihlali oluşmaz; migration çakışan satırlar dışında tamamlanmış sayılır. **İstisna:** Değeri zaten taşıyan öğenin saklı değeri yeni değere eşitse (yeni kural onu değiştirmiyorsa) o öğe değeri korur, daha eski olsa bile diğer öğe `normalization_conflict` ile işaretlenir ve duplicate adayı olur; aksi hâlde iki öğe aynı değere düşerdi (ADR-0011).
  - Örnek: A (2025, `…watch?v=X&si=1`) ve B (2026, `youtu.be/X`) yeni kuralla aynı değere (`…watch?v=X`) düşer. A yeni değeri alır; B eski değerinde kalır ve A ile duplicate adayı olur.
  - Çözülmemiş çakışmada yeni capture yeni değeri taşıyan öğeye (A) yönlendirilir ve duplicate adayı da gösterilir. Metadata yenileme ve export iki öğe için ayrı ayrı çalışır.
  - Kullanıcı birleştirmesi (ORG-11) önizlemelidir. Birleştirme sonrası kaynak öğe soft-delete edilir; böylece Undo ve “Recently removed” ile geri alınabilir.
- Aynı içerik hash'i ayrı notları olan iki öğeyi zorla birleştirmez; duplicate adayı üretir.
- **Kimliğin sonradan öğrenilmesi:** Örneğin çevrimdışıyken kısa ve uzun URL ayrı öğeler olarak kaydedilmiş, metadata çözümlemesi ikisini aynı `(source_platform, source_id)` değerine götürmüş olabilir. Bu durumda unique hatası kullanıcıya veya işe yansımaz: ikinci öğenin `source_id` alanı yazılmaz (diğer metadata yazılır), öğe `identity_conflict` ile işaretlenir ve `DuplicateCandidate(reason = source_id_conflict)` oluşur. Kullanıcı ORG-11 ile birleştirdiğinde kaynak öğenin URL'si hedef öğenin `UrlAlias` kaydı olur. İki öğenin notları ve kullanıcı metadata'sı birleştirme kararına kadar ayrı kalır.
- `Job(status, next_retry_at, priority)`, item ekleme tarihi, platform, favori/arşiv ve ilişki foreign key'leri indekslenir.
- Transkript segmentleri `(transcript_id, ordinal)` ve zaman aralığı ile erişilebilir olmalıdır.
- Schema migration numarası ve checksum'ı izlenir; başarısız migration yarım şema bırakmaz.
- Migration öncesi tutarlı yedek alınır. Pratikte geri alınabilir migration'lar test edilir; geri alınamayan değişimde desteklenen geri dönüş, eski sürüm + uyumlu yedek restore akışıdır.
- FTS veya vector bozulması, otoritatif kayıtlardan yeniden indekslenebilir. Veritabanı bozulması hâlinde otomatik sıfırlama yapılmaz.

## 12. Bağımsız durum makineleri

Bir öğenin uzak erişimi, yerel dosyası, oturumu ve AI işi tek bir `status` alanına sıkıştırılamaz. Örneğin kaynak silinmişken yerel dosya oynatılabilir; oturum sona ermişken eski döküm aranabilir.

### 12.1 İçerik erişimi

Kaynak promptun durum listesi (unauthenticated, login required, login verified, access denied, expired session, extraction failed, unsupported/DRM) iki ayrı katmana bölünür, çünkü bir kısmı öğeye, bir kısmı oturum profiline aittir:

- **Öğe katmanı** (`LibraryItem.access_state`): belirli bir içeriğe son denemede ne olduğu.
- **Profil katmanı** (`AuthProfile.session_state`, §12.2): oturumun kendisinin geçerli olup olmadığı.

Oturum geçerliliği yalnızca profilde tutulur; öğe üzerinde kopyalanmaz. Böylece oturum yenilendiğinde yüzlerce öğeyi tek tek güncellemek gerekmez ve iki kaynak birbirinden sapmaz. UI iki katmanı birleştirerek gösterir (ör. “Login required · Session expired”). Silinmiş, kullanılamaz veya bölge kısıtlı içerik bilgisi yalnızca `remote_availability` alanında tutulur.

| Öğe durumu | Anlam | Sonraki güvenli eylem |
|---|---|---|
| `unknown` | İçerik erişimi henüz test edilmedi | Metadata kontrolü veya bookmark olarak kullanma |
| `public_ok` | Oturumsuz erişim son denemede başarılı oldu | Normal kullanım |
| `login_required` | İçerik için oturum gerekli olduğuna dair yeterli sinyal var | Açık izinle desteklenen profil seçimi; profil durumu §12.2'den okunur |
| `authorized` | Belirli bir profil ile son denemede erişim başarılı (`access_profile_id`, `access_checked_at`) | Gelecekteki erişim garanti değildir; hata olursa yeniden sınıflandırılır |
| `access_denied` | Oturumdan bağımsız olarak içerik yetkisi reddedildi | Üyelik/yetki durumunu kullanıcının kontrol etmesi |
| `extraction_failed` | İçerik çıkarımı başarısız; nedeni kesin olmayabilir | Redakte tanı, sürüm kontrolü, sınırlı retry |
| `unsupported` | Mevcut adaptör/araç bu içeriği desteklemiyor | Bookmark ve kaynak bağlantısını koruma |
| `drm_protected` | Desteklenmeyen koruma tespit edildi | İndirmeyi durdurma; aşma yöntemi sunmama |

Promptun adlarıyla eşleme: `unauthenticated` → profil yok (`not_connected`); `login verified` → `session_available`; `expired session` → `expired` (profil katmanı); diğerleri öğe katmanındadır.

**Hata kodu → durum eşlemesi.** “–” alanın değişmediği anlamına gelir.

| Hata kodu (§19) | `access_state` | `remote_availability` | `AuthProfile.session_state` |
|---|---|---|---|
| Başarı, oturumsuz veya oturum kanıtı olmadan | `public_ok` | `available` | – (profil kullanılmış olsa bile `last_verified_at` güncellenmez) |
| Başarı, kimlik doğrulanmış erişim kanıtıyla | `authorized` | `available` | `session_available`, `last_verified_at` güncellenir |
| `LOGIN_REQUIRED` | `login_required` | – | – |
| `SESSION_EXPIRED` | `login_required` | – | `expired` |
| `COOKIE_ACCESS_FAILED` | – | – | `needs_verification` (oturumun geçersiz olduğu kanıtlanmadı) |
| `ACCESS_DENIED` | `access_denied` | – | – (oturum geçerli olabilir) |
| `MEDIA_URL_EXPIRED` | – | – | – (oturum süresi dolması değildir) |
| `CONTENT_UNAVAILABLE` | – | Kanıta göre `deleted` veya `unavailable` | – |
| `REGION_RESTRICTED` | – | `region_restricted` | – |
| `DRM_UNSUPPORTED` | `drm_protected` | – | – |
| `EXTRACTOR_UNSUPPORTED` | `unsupported` | – | – |
| `EXTRACTOR_FAILED` | `extraction_failed` | – | – |
| `NETWORK_UNAVAILABLE`, `RATE_LIMITED`, `TIMEOUT` | – | – | – |

**Kimlik doğrulanmış erişim kanıtı.** Profil ile istek atılması tek başına kanıt değildir: süresi dolmuş çerezle yapılan istek, herkese açık bir videoda anonim olarak başarılı olabilir. Kanıt yalnızca şu iki durumdan biridir: (a) aynı içerik, aynı araç sürümüyle oturumsuz denemede `LOGIN_REQUIRED` veya `ACCESS_DENIED` verdi ve profil ile başarılı oldu (gerekirse oturumsuz deneme ayrıca yapılır); (b) `verify_session` komutunun platforma özgü, yalnızca oturum açıkken başarılı olan doğrulama kontrolü geçti. Platform için güvenilir bir (b) kontrolü yoksa bu durum `docs/platform-support.md`'de yazılır ve profil yalnızca (a) ile doğrulanabilir.

Hata sınıflandırması yeterli kanıt yokken `access_denied` veya `expired` sonucu üretmemelidir. Böyle durumlarda `extraction_failed` ve “neden doğrulanamadı” açıklaması kullanılır. Geçici medya URL süresi dolması oturumun süresinin dolmasıyla aynı olay değildir.

### 12.2 Profil/oturum durumu

| UI durumu | Kalıcı durum | Gösterim kuralı |
|---|---|---|
| Not connected | `not_connected` | Kullanılacak izinli profil tanımlanmamış |
| Needs verification | `needs_verification` | Profil referansı var, canlı doğrulama yok veya eskimiş |
| Session available | `session_available` | Son doğrulama zamanı ve kapsamıyla birlikte gösterilir |
| Expired | `expired` | Kanıtlı geçersiz oturum; reconnect sunulur |
| Unsupported | `unsupported` | Seçili yöntem/ortam desteklenmiyor |

Bu tablo, kaynak promptun oturuma ait durumlarının (§12.1'deki eşleme) tek kaynağıdır. Stored cookie/profile referansı tek başına yeşil onay işareti üretmez. “Session available”, bütün özel gruplara erişim veya gelecekteki her isteğin başarılı olacağı sözü değildir. Clear işlemi uygulamanın güvenli referansını/uygulamaya ait secret'ını temizler; kullanıcının Chrome çerezlerini veya dış hesap oturumunu silmez. Uzak oturumu iptal etme ancak platformun belgeli yeteneği varsa ayrı kapsamdır.

### 12.3 İş durumları ve geçişler

**Önerilen ortak durum kümesi:** `queued | blocked | running | retry_wait | pausing | paused | canceling | canceled | succeeded | failed`.

| Başlangıç | Hedef | Koşul |
|---|---|---|
| `queued` | `running` | Bağımlılıklar, izinler ve kaynak sınırı uygun; lease atomik alındı |
| `queued/running` | `blocked` | Offline Mode, eksik model/izin, disk veya auth gibi kullanıcı/ortam eylemi gerekiyor |
| `blocked` | `queued` | Engel giderildi; tekrar doğrulandı |
| `running` | `succeeded` | Çıktı doğrulandı, dosya finalize edildi, domain kaydı commit edildi |
| `running` | `retry_wait` | Geçici hata ve deneme bütçesi mevcut |
| `retry_wait` | `queued` | `next_retry_at` geldi |
| `running` | `failed` | Kalıcı hata veya retry bütçesi bitti |
| `running` | `pausing → paused` | İş türü güvenli pause/checkpoint destekliyor |
| `paused` | `queued` | Kullanıcı devam ettirdi; giriş/checkpoint tekrar doğrulandı |
| `queued/blocked/retry_wait/paused` | `canceled` | Kullanıcı iptali; uygun kısmi dosya politikası uygulandı |
| `running/pausing` | `canceling → canceled` | Alt süreç durduruldu ve lease/finalizasyon güvenli kapatıldı |
| `failed/canceled` | `queued` | Kullanıcı yeniden denedi; attempt geçmişi korunur, generation artar |
| `canceled` (`dependency_failed`) | `queued` | Ebeveyn iş yeniden kuyruğa girdi; `cancel_reason` temizlenir |
| `retry_wait/paused` | `blocked` | Bekleme sırasında Offline Mode açıldı veya kaynak/izin engeli oluştu |
| `blocked` | `canceled` | Bağımlı olunan iş kalıcı `failed`/`canceled` oldu (§13.1) |

`running → blocked` geçişinde (ör. Offline Mode açıldı) alt süreç güvenli biçimde durdurulur, destekleniyorsa checkpoint korunur; bu geçiş başarısız deneme sayılmaz ve `attempts` artmaz.

Tamamlanan iş yeniden çalıştırılacaksa yeni job veya yeni sürümlü işlem oluşturulur. “Pause desteklenmiyor” durumunda düğme gizlenir ya da açıklamayla devre dışı bırakılır; cancel işlemi pause olarak etiketlenmez.

## 13. Kalıcı iş kuyruğu ve işlem denetimi

### 13.1 İş türleri ve bağımlılıklar

`metadata`, `probe_media`, `hash_media`, `download`, `merge_media`, `thumbnail`, `extract_audio`, `fetch_captions`, `transcription`, `fts_index`, `ocr`, `embedding`, `visual_embedding`, `model_download`, `reindex`, `storage_move` iş türleri ayrı handler'larla yürütülür. Bazıları yalnızca ilgili aşamada açılır.

**`storage_move` sözleşmesi.** Payload: `source_root_id`, `target_root_id` ve iş başladığında alınan varlık kimliği listesi. Her varlık `pending → copied → verified → switched → cleaned` adımlarından geçer ve adım durumu kalıcı checkpoint'tir:

1. Dosya hedef kökteki staging konumuna kopyalanır (`copied`).
2. Boyut ve tam içerik hash'i doğrulanır; hash yoksa önce kaynakta hesaplanır (`verified`).
3. Tek bir SQLite transaction'ında varlığın `storage_root_id`/`storage_ref` değeri yeni konuma çevrilir (`switched`). Bu andan önce DB eski kopyayı, sonra yeni kopyayı gösterir.
4. Eski kopya ancak commit'ten sonra silinir (`cleaned`). Silme başarısız olursa varlık `cleanup_pending` olarak işaretlenir ve sonraki açılışta yeniden denenir.

Yeniden başlatmada `copied` adımındaki varlık yeniden doğrulanır, `switched` adımındaki varlığın eski kopyası temizlenir. İptalde `switched` olmuş varlıklar yeni kökte kalır, diğerleri eski kökte kalır; iki kök de kayıtlı kalır. Taşıma sürerken oynatma DB'nin gösterdiği kopyadan yapılır; yeni indirmeler taşıma bitene kadar eski aktif köke yazılır; taşınmakta olan varlık silinmek istenirse önce o varlığın adımı güvenli biçimde bitirilir veya geri alınır; export, DB'nin o anki referanslarını yazar.

Örnek bağımlılık zincirleri:

```text
URL save ──► yerel metadata FTS güncelleme
    └──────► metadata fetch ──► FTS güncelleme

Download ──► merge (gerekliyse) ──► probe/integrity ──► media ready
                                                   ├──► thumbnail
                                                   └──► transcription (kullanıcı seçerse)

Transcription ──► segment validation ──► transcript FTS
                                      └──► embeddings (ayrıca etkinse)
```

Kaydetme ve temel metadata FTS güncellemesi küçük bir transaction içinde yapılır; sıradan bookmark araması uzun kuyruk işine bağımlı olmaz. Büyük transkript/reindex işleri arka planda yürütülür. Bir bağımlılık hatası ilgisiz işleri durdurmaz. Başarısız bağımlılığın alt işi, nedeni açık bir `blocked` durumuna alınır. Ebeveyn kalıcı olarak `failed` kalırsa veya iptal edilirse alt iş `canceled` olur ve `cancel_reason = dependency_failed` kaydedilir. Ebeveyn yeniden denenip kuyruğa döndüğünde yalnızca `blocked` durumdaki ve `cancel_reason = dependency_failed` olan alt işler yeniden kuyruğa girer. Kullanıcının kendisinin iptal ettiği (`user`) alt işler iptal kalır. `item_removed` iptalleri öğe geri yüklense de otomatik dönmez (§7.3). `shutdown` nedeniyle kesilen işler açılıştaki uzlaştırmada değerlendirilir (§13.2).

### 13.2 Crash recovery ve idempotency

- Uygulama tek örnek (single-instance) çalışır. İkinci örnek açılırsa mevcut pencereye odaklanır ve varsa URL/dosya argümanını ona iletip kapanır. Buna ek olarak veritabanı dizininde süreç kilidi tutulur; kilit alınamazsa ikinci süreç DB'yi açmaz (INV-13). Lease mekanizması tek süreç içindeki worker'lar ve çökme sonrası uzlaştırma içindir, çok süreçli erişimin yerine geçmez.
- Worker bir işi transaction ile sahiplenir. Lease sahibi, süre sonu ve generation kaydedilir.
- **Öneri:** Lease 30 saniye, heartbeat 10 saniye; gerçek ortamda ayarlanır. Süresi dolan lease, alt süreç/çıktı uzlaştırması yapılmadan doğrudan tamamlandı sayılmaz.
- Uygulama açılışında eski `running/pausing/canceling` işleri incelenir; terminal sonuç veya geçerli checkpoint yoksa yeniden kuyruklanır ya da açıklayıcı engel gösterilir.
- İşlem semantiği “en az bir kez yürütme + idempotent çıktı”dır. Tam olarak bir kez çalışan alt süreç garantisi verilmez.
- `(job type, item/input fingerprint, configuration/model version)` temelli idempotency key aynı işlemin yanlışlıkla eşzamanlı çoğalmasını önler.
- Çıktıların sonlandırılması, geçerli worker generation'ı ile yapılır; eski worker iptalden sonra yeni çıktıyı yayımlayamaz.
- Uygulama kapanırken çocuk süreçlerin yaşam döngüsü yönetilir. Yetim süreçlerin aynı hedefe yazmasını engelleyen Windows süreç grubu/Job Object yaklaşımı M0'da doğrulanır.
- Atomik dosya taşıma ile DB commit arasındaki kesinti için başlangıç uzlaştırması vardır: sahipsiz tamamlanmış dosya, staging çıktı ve DB referansı kontrol edilir. Dosya+SQLite için tek atomik transaction varsayılmaz.

### 13.3 Retry ve kaynak sınırları

**Önerilen başlangıç politikası:**

| Alan | Başlangıç değeri / davranış |
|---|---|
| Eşzamanlı download | 2; ayarlardan sınırlı aralıkta değiştirilebilir |
| Eşzamanlı metadata | 3; provider başına ayrıca hız sınırlaması |
| Transkripsiyon/OCR/embedding | Başlangıçta toplam 1 ağır AI işi |
| ffprobe/thumbnail | En çok 2; düşük disk/CPU önceliği |
| Otomatik retry | İlk deneme + en çok 3 ek deneme |
| Backoff | 5 s, 30 s, 120 s; jitter ve varsa geçerli Retry-After dikkate alınır |
| Progress event | İş başına en çok yaklaşık 4/s; terminal durum hemen gönderilir |
| Kısmi dosya temizliği | Otomatik yıkıcı silme yok; kullanıcıya alan ve sonuç gösterilir |

Ağ kopması, timeout, geçici sunucu hatası ve uygun rate limit retry adayıdır. DRM, unsupported, yetki reddi ve bozuk girdi otomatik sonsuz retry'ye giremez. Disk dolması ve login gereksinimi kullanıcı müdahalesi bekler. Offline Mode'da retry zamanlayıcısı ağ isteği başlatmaz.

### 13.4 Progress ve iptal

İlerleme `phase`, `bytes_done`, `bytes_total?`, `units_done?`, `units_total?`, `percentage?` ve `eta?` alanlarıyla raporlanır. Toplam bilinmiyorsa yüzde/ETA null olur. Fazlar arası gerçekçi ağırlık yoksa tek toplam yüzde uydurulmaz.

İptal bayrağı kalıcı kaydedilir, worker'a iletilir ve süreç ağacı kapatılır. Kapanma sırasında dosyanın hazır görünmesi engellenir. Kısmi dosyanın tutulacağı/silineceği açık politika ve kullanıcı eylemine bağlıdır. Yeniden denemede imzalı/geçici medya URL'si diskten geri okunmaz; kaynak yeniden çözülür.

## 14. Dış araç, dosya ve depolama sözleşmesi

### 14.1 yt-dlp ve FFmpeg çalıştırma

- yt-dlp Windows'ta yönetilen executable olarak kullanılır; son kullanıcıdan Python kurulumu istenmez.
- yt-dlp'nin güncel sürümleri bazı platformlar (özellikle YouTube) için harici bir JavaScript runtime'ı (ör. Deno) gerektirir. Bu runtime da yönetilen araç olarak manifestte yer alır; yt-dlp'ye yalnızca manifestteki yol açıkça verilir, kullanıcının PATH'indeki rastgele bir runtime kullanılmaz. Runtime yoksa veya uyumsuzsa `TOOL_MISSING` döner ve bookmark korunur.
- Yürütülebilir dosya yolu uygulamanın onaylı manifestinden gelir. URL, metadata, dosya adı veya export içeriği executable seçemez.
- Argümanlar tipli/izinli seçeneklerden dizi olarak üretilir. Shell string birleştirme ve `cmd /c` üzerinden kullanıcı girdisi çalıştırma yasaktır.
- Çıktı şablonları backend tarafından üretilir; metadata'nın dosya yolu, argüman veya yt-dlp bayrağı belirlemesine izin verilmez.
- Araçların kullanıcı/global config dosyalarını ve ortam değişkenlerini istem dışı devralması engellenmeli veya açıkça denetlenmelidir. Kesin bayraklar güncel resmî dokümanla doğrulanır.
- `--cookies-from-browser` yalnızca desteklenen, kullanıcı tarafından izin verilmiş profille M3 kapsamında kullanılır.
- stdout/progress için makine okunabilir protokol, stderr için sınırlı ve redakte hata işleme kullanılır. Ham çıktı varsayılan log olarak saklanmaz.
- ffprobe/FFmpeg hatalı medya için süre, çıktı boyutu ve süreç kaynak sınırlarıyla çalıştırılır. Araç komutu veya medya içeriği çalıştırılabilir program olarak açılmaz.
- Yerel import, probe, thumbnail, remux ve ses çıkarma çağrıları ağsız çalışır. FFmpeg/ffprobe bir protokol izin listesiyle (yalnızca `file`, gerekiyorsa `pipe`) çağrılır ve girdi doğrulanmış tek bir dosya yolu olarak verilir. Concat listeleri, HLS/DASH manifestleri, playlist'ler ve başka dosya ya da URL'ye referans veren girdi biçimleri yerel import'ta reddedilir (`INPUT_REJECTED`); biçim, işlemden önce aynı kısıtlı ffprobe çağrısıyla belirlenir. Kesin bayraklar M0'da seçilen FFmpeg sürümünün dokümanıyla doğrulanır.
- Araç versiyonu, checksum, provenance, lisans ve son doğrulama tarihi kullanıcıya gösterilebilir.

### 14.2 Önerilen depolama düzeni

```text
App data root/
  db/library.sqlite
  cache/thumbnails/
  cache/previews/
  staging/
  models/
  logs/                 # yalnızca redakte uygulama olayları
  backups/              # kullanıcının etkinleştirdiği metadata/DB yedekleri

User-selected media root/
  media/<item-id>/<asset-id>.<container>
```

Kesin Windows dizinleri Tauri/OS app-data API'leri ile belirlenir; sabit kullanıcı adı içermez. DB'nin uygulama alanında, büyük medya deposunun kullanıcının seçtiği konumda tutulması önerilir. Modeller varsayılan olarak yukarıdaki `models/` dizinindedir; büyük modeller için ayarlardan farklı bir konum seçilebilir ve bu durumda `ModelInstallation.path` o konumu gösterir. Taşınabilir medya referansı kullanıcı köküne göre çözülür; absolute dış import referansı ayrıca işaretlenir.

Dosya adında Windows reserved name, yol ayracı, denetim karakteri, sondaki nokta/boşluk, aşırı uzunluk ve çakışma test edilmelidir. Görünen başlık fiziksel dosya adından bağımsızdır. Kullanıcı kontrollü metadata dosyanın hedef kök dışına çıkmasına neden olamaz. Junction/symlink ve gerçek resolved path kontrolü yalnızca string prefix kontrolüyle geçiştirilemez.

Thumbnail, preview, waveform ve örnek kareler ihtiyaç oldukça üretilir; bütün medya için peşinen pahalı türevler oluşturulmaz. Türetilmiş dosyalar kaynak fingerprint'i ve üretim ayarlarıyla ilişkilendirilir. Cache limiti ve temizleme, özgün medya ile kullanıcı dökümlerini kapsam dışında tutar.

### 14.3 Tamamlama ve bütünlük

1. Hedef disk, yazma izni ve tahmini boş alan kontrol edilir.
2. Kısmi dosya kontrollü staging/partial konuma yazılır.
3. Gerekirse birleştirme yapılır; özgün medya politikası ve ara dosyalar ayrılır.
4. ffprobe ile container/akış kontrolü, beklenen boyut ve varsa checksum doğrulanır.
5. Aynı volume'da atomik rename ile final konuma taşınır. Volume değişiminde kopyala–doğrula–sonlandır uygulanır.
6. `MediaAsset` hazır olarak kaydedilir; thumbnail ve isteğe bağlı AI işleri kuyruğa alınır.

Boş alan yarış koşulunda azalırsa süreç anlaşılır hatayla durur; kullanıcı dosyaları kendiliğinden temizlenmez. “Özgün dosyayı koruma”, yalnızca birleştirme için gerekli geçici parçalardan farklıdır; alınan ana medya kullanıcıya haber verilmeden transcode edilmez veya değiştirilmez.

### 14.4 Temizleme ve silme

| Eylem | Etkisi |
|---|---|
| Koleksiyondan çıkar | Sadece ilişkiyi kaldırır |
| Arşivle | Kayıt korunur; varsayılan listelerden gizlenir |
| Kütüphaneden kaldır | Soft-delete; “Recently removed” görünümünde saklama süresince geri yüklenebilir; medya korunur (§7.3) |
| Kalıcı olarak sil | Kayıt, ilişkiler, dökümler ve türetilmiş dosyalar silinir; managed özgün medya yalnızca ayrı onayla silinir, aksi halde “Sahipsiz medya” listesine düşer |
| Yerel dosyayı sil | Ayrı açık eylem; dosya listesi/boyutu ve geri alınabilirlik belirtilir |
| Cache temizle | Yeniden üretilebilir thumbnail/preview/waveform; özgün dosya korunur |
| Modeli kaldır | Seçilen model dosyaları; mevcut dökümler/indeksler korunur |
| Uygulamayı kaldır | Kullanıcı medya kütüphanesini sessizce silmez |

Dışarıdan referanslanan kaynak dosyalar sıradan uygulama temizliğine dahil edilmez. Aktif işin kullandığı model veya medya dosyası eşzamanlı silinmez; önce işin durumu güvenli çözülür. Türetilmiş dosya silme, ilgili oynatma/arama yeteneğinin yeniden üretim gerektireceğini gösterir.

## 15. JSON taşınabilirlik ve yedekleme

### 15.1 Dışa aktarma şeması

**Önerilen üst düzey sözleşme:**

```json
{
  "schema_version": 1,
  "exported_at": "2026-11-06T00:00:00Z",
  "app_version": "<release-version>",
  "items": [],
  "collections": [],
  "smart_collections": [],
  "tags": [],
  "item_tags": [],
  "item_collections": [],
  "media_references": [],
  "transcripts": [],
  "playback_states": [],
  "storage_roots": [],
  "duplicate_candidates": []
}
```

Örnek tarih ve sürüm placeholder'ı veri sözleşmesini anlatır; gerçek export sonucu değildir. `items` her öğenin favori, arşiv, `removed_at`, not ve `user_overrides` alanlarını içerir. `smart_collections` kurallarındaki etiket/koleksiyon referansları export içi kimliklerle yazılır; import sırasında yeni kimliklere yeniden eşlenir, eşlenemeyen referans kuralı `invalid` yapar. Dökümler kullanıcı seçimine göre eklenebilir; boyut etkisi gösterilir. FTS tabloları, vector binary formatları, çalışan job lease'leri, çerezler, token'lar ve `AuthProfile` secret referansları portable export'a girmez. Notlar, özel kaynak bağlantıları ve dökümler kişisel veri olabilir; export içeriği kullanıcıya önceden açıklanır.

Medya dosyaları JSON'a gömülmez. Relative path, dosya boyutu ve varsa hash ile ilişki kurulabilir. Absolute kaynak yolları dışa aktarmadan önce görünür seçenek olmalıdır; varsayılan taşınabilir export makineye özgü yolları dışarıda bırakır. Bu durumda her `media_references` girdisi `asset_id`, kök etiketi (`storage_roots` içindeki kayıt, yolsuz), köke göre relative yol, `size_bytes`, varsa `content_hash`, `container` ve `duration_ms` taşır. Restore önizlemesi her referansı “otomatik relink edilebilir” (kök eşlendi, boyut/hash tuttu) veya “manuel relink gerekli” olarak ayırır.

### 15.2 İçe aktarma işlemi

1. Dosya boyutu, JSON yapısı, şema sürümü ve alan limitleri doğrulanır.
2. URL/path alanları normal uygulama girdisi gibi güvenlik denetiminden geçer.
3. Önizlemede yeni kayıt, olası eşleşme, eksik medya ve desteklenmeyen alan sayıları gösterilir.
4. Mevcut kullanıcı alanları varsayılan olarak korunur; çakışmalar kullanıcı tarafından seçilen politikayla çözülür.
5. Yeni kimlik/ilişki eşlemeleri transaction içinde uygulanır; büyük import parçalara ayrılıyorsa checkpoint ve tekrar çalıştırma güvenliği vardır.
6. Medya kökü eşleme veya tek dosya relink yapılır; boyut/hash/probe ile doğrulanır.
7. FTS yeniden üretilir; yeni model uyumsuzluğu varsa vector indeksleri sonradan oluşturulur.

İçe aktarma ağ isteği, executable indirme veya dosya silme başlatmaz. Kaynak URL'lerin metadata yenilemesi ayrı kullanıcı/uygulama politikasıyla kuyruğa alınır. Gelecek şema sürümü anlaşılır hata ile durur; mevcut kütüphane etkilenmez.

### 15.3 Yedekleme ve geri yükleme

Portable JSON ile tam DB yedeği ayrı adlandırılır. SQLite dosyası çalışırken kör dosya kopyalama yerine tutarlı snapshot/backup mekanizması kullanılmalıdır. Restore öncesi mevcut durum yedeklenir; schema uyumu doğrulanır. Medya ayrı yedeklenmediyse “metadata geri geldi, medya eksik” açıkça gösterilir. Kayıp dosya referansları korunur ve yeniden bağlama sağlanır.

## 16. Arama ve indeksleme ayrıntıları

### 16.1 Lexical arama

- FTS5 kullanılabilirliği paketlenen SQLite ile doğrulanmalıdır; geliştirme makinesindeki başka SQLite sonucu yeterli değildir.
- Başlık, açıklama, not, etiket ve üretici metinleri ayrı ağırlıklandırılabilir alanlardır.
- **Öneri:** İlk sıralama başlık > etiket/not > açıklama/üretici ağırlıklı BM25 ve kararlı tie-break olarak eklenme tarihi/ID kullanır. Sayısal ağırlıklar fixture ile seçilir.
- Kullanıcı sorgusu raw SQL'e birleştirilmez. Parametreli SQL ve güvenli FTS query builder gerekir.
- Türkçe `I/İ/ı/i`, aksan, Unicode normalizasyonu, çok sözcüklü sorgu, tırnak ve noktalama davranışı §25.2'deki Türkçe arama fixture'ıyla test edilip belgelenir. Tokenizer'ın Türkçe kök bulma veya kusursuz dilsel eşleşme yaptığı varsayılmaz.
- Snippet'ler kaynak metnin gerçek alt parçalarıdır. UI vurgulaması güvenli render edilir; HTML metadata çalıştırılamaz.
- Sayfalama kararlı olmalı; büyük kütüphaneler tek seferde belleğe yüklenmemelidir.
- Sıralama alanları: `added_at` (varsayılan, azalan), `published_at`, `title` (locale'e duyarlı), `duration_ms`, `updated_at` ve sorgu varken `relevance`. Null değerler yönden bağımsız olarak en sona gelir; tie-break her zaman `id`'dir. Seçili sıralama görünüm bazında yerelde saklanır.

### 16.2 Döküm indeksleme

Segmentler ve arama chunk'ları ayrı tutulur. Chunk komşu segmentleri birleştirebilir fakat `start_segment_id`/`end_segment_id` ile ilk/son segmente ve gerçek zaman aralığına referans vermelidir. İki segment aynı dökümde olmalı ve başlangıç ordinal'i bitişten büyük olmamalıdır. Bir dökümün segmentleri yeniden üretilirse o dökümün chunk'ları aynı transaction'da silinip yeniden oluşturulur. OCR ve görsel chunk'larda `producer_*` alanları zorunludur. Sonuçlarda tekrar eden yakın eşleşmeler aynı öğe altında gruplanır. Kısmi transkript aranıyorsa işlenmiş aralık açıkça gösterilir; tamamlanmış dökümle karıştırılmaz.

Metadata düzenlemesi mevcut transkript indeksini gereksiz yere silmez. Kaynak medya değişikliği veya yeni model sürümü ilgili index generation'ını geçersiz kılar. Arama sonucuna tıklama, o anki dosya ve zamanın hâlâ geçerli olduğunu kontrol eder.

### 16.3 Hibrit ve görsel arama

M5'te `EmbeddingProvider` ve `VectorRepository` arayüzleri eklenir. Vektörler provider/model/sürüm/boyut ve kaynak fingerprint ile saklanır; farklı model vektörleri doğrudan karşılaştırılmaz. Basit reciprocal rank fusion başlangıç adayıdır; kalitesi ölçülmeden kesin sonuç garantisi verilmez.

Reindex eski kullanılabilir indeksin yanında oluşturulup tamamlanınca aktif generation değiştirilebilir. İptal yarım indeksin varsayılan arama sonucu olmasına neden olmaz. Embedding modeli kaldırılırsa lexical arama çalışmaya devam eder; mevcut vektörün sorgulanabilirliği model gereksinimine göre açıklanır.

OCR sonucu kare zamanı ve gerçek çıkarılan metni; görsel benzerlik sonucu kare zamanını ve eşleşme türünü gösterir. Model skorları kalibre edilmemişse kullanıcıya “% doğruluk” olarak sunulmaz. Kaynak türü etiketleri zorunludur; confidence yalnızca anlamı açıklanabiliyorsa gösterilir.

## 17. AI sağlayıcıları ve kaynak yönetimi

### 17.1 Ortak sağlayıcı sözleşmesi

```text
capabilities() -> supported_languages / devices / operations
estimate_resources(input, model) -> disk / memory / work estimate
prepare(model, consent) -> local verified installation
run(input_reference, configuration, checkpoint, cancellation) -> typed results
checkpoint() -> resumable state when supported
dispose() -> release process and device resources
```

Model seçimi ve provider implementasyonu UI'dan ayrılmalıdır. Model indirmesi açık onaya dayanır; dosya boyutu, disk ihtiyacı, lisans/kaynak, CPU/GPU beklentisi ve ilerleme gösterilir. Checksum/imza mekanizması doğrulanmadan model hazır sayılmaz. Model güncellemesi eski onayın kapsamını aşan yeni indirme ise yeniden açık seçim gerektirir.

### 17.2 AI A — transkripsiyon

| Kimlik | Gereksinim |
|---|---|
| AI-A01 | İzinli mevcut altyazı varsa kaynak/dil bilgisiyle tercih edilebilmeli; kaynak ve üretilen döküm ayrılmalı. |
| AI-A02 | Türkçe ve İngilizce örneklerde çalışan yerel Whisper implementasyonu Windows gereksinimleriyle seçilmeli. |
| AI-A03 | Uzun ses kontrollü parçalara ayrılmalı; parça overlap'i tekrar eden metin ve kaymış zaman kodu üretmemeli. |
| AI-A04 | İptal ve yeniden başlatma desteklenmeli; checkpoint hangi model/girdi sürümüne ait olduğunu belirtmeli. |
| AI-A05 | RAM/disk yetersizliği önceden uyarılmalı; düşük bellekli cihazda büyük model sessizce yüklenmemeli. |
| AI-A06 | Sonuç paneli model/sürüm, dil, kaynak ve işlem durumunu göstermeli. |
| AI-A07 | Kalite ve hız gerçek fixture ile ölçülmeli; sabit doğruluk/hız sözü verilmemeli. |

**Öneri:** Transkripsiyon varsayılan olarak kapalıdır ve öğe/batch bazında seçilir. CPU yolu temel destek hedefidir; GPU ancak gerçek ortam testi ve desteklenen sürücü/runtime ile açılır. Eksik GPU koşulunda kullanıcıya CPU'ya dönme seçeneği gösterilir. Ses parçalama boyutu provider performans ölçümünden sonra belirlenir.

### 17.3 AI B — sınıflandırma ve semantic search

- **AI-B01:** Başlık, açıklama ve gerçek dökümden kullanıcı tarafından düzenlenebilir konu/etiket önerileri üretilebilir.
- **AI-B02:** AI önerileri kullanıcı etiketlerini sessizce değiştirmez. Kabul/ret ve provenance ayırt edilir. Öneriler `AIProposal` olarak tutulur; kabul edilen öneri `LibraryItemTag(origin = ai_accepted, proposal_id)` ilişkisine dönüşür. Reddedilen bir değer aynı öğe için, model sürümü değişse bile, varsayılan olarak tekrar önerilmez.
- **AI-B03:** Çok dilli embedding, Türkçe/İngilizce sorgu–içerik eşleşmeleriyle değerlendirilir.
- **AI-B04:** Aynı kaynak/model/config fingerprint'i tekrar gereksiz işlemeye yol açmamalıdır.
- **AI-B05:** Model değişiminde reindex maliyeti, disk alanı ve ilerleme gösterilir.
- **AI-B06:** Related items yalnızca tanımlı etiket/koleksiyon ilişkisi veya gerçek hesaplanmış benzerlikle sunulur; açıklama kaynağı belirtilir.

### 17.4 AI C — görsel anlama

- **AI-C01:** Sahne değişimi ve seyrek örnekleme birlikte değerlendirilmeli; tüm kareleri sınırsız işleme yaklaşımı kullanılmamalıdır.
- **AI-C02:** Süre/öğe başına kare, disk, CPU/GPU ve iş sayısı bütçeleri tanımlanmalıdır. Kesin sayılar M5 benchmark kararıdır.
- **AI-C03:** OCR, kare zamanına bağlı metin kaydeder. Görsel embedding ayrı opt-in ile etkinleşir.
- **AI-C04:** Döküm, OCR ve görsel kanıt aynı sonuçta birleşebilir; her katkı kaynak türüyle görünür olmalıdır.
- **AI-C05:** Yerel LLM özet/Q&A isteğe bağlıdır; iddialar gerçek video zamanlarına referans verir, kanıt yetersizse yanıt üretmekten kaçınır.
- **AI-C06:** Video metni/metadata içindeki komutlar AI sistem talimatı sayılmaz; yalnızca incelenecek içeriktir.
- **AI-C07:** Harici AI API, açık kullanıcı yapılandırması ve gönderilecek veri kapsamı belirtilmeden kullanılamaz; MVP/M4 için gerekli değildir.

## 18. Frontend–backend sözleşmeleri

### 18.1 Temel komut yüzeyi

Aşağıdaki adlar önerilen domain sözleşmesidir; mevcut Tauri API'si veya yazılmış kod iddiası değildir. Tablo, bu belgedeki zorunlu kullanıcı işlemlerinin tamamını kapsamalıdır; yeni zorunlu işlem eklendiğinde buraya da eklenir.

| Komut grubu | Örnek komutlar | Önemli sınır |
|---|---|---|
| Capture | `preview_url`, `save_bookmark`, `refresh_metadata` | Bookmark kaydı metadata çağrısından bağımsız |
| Library | `list_items`, `get_item`, `update_item`, `bulk_update_items`, `remove_items`, `restore_items`, `purge_items`, `undo_last_operation` | Tipli filtre, pagination, revision doğrulama |
| Duplicates | `list_duplicate_candidates`, `preview_merge`, `merge_items`, `dismiss_duplicate` | Önizlemesiz birleştirme yok |
| Import | `select_import_sources`, `start_import`, `relink_asset` | Native seçimden gelen izinli referanslar |
| Organize | `create_collection`, `rename_collection`, `move_collection`, `delete_collection`, `set_tags`, `rename_tag`, `merge_tags`, `delete_tag`, `set_memberships`, `save_smart_collection` | Ham SQL veya kod kabul edilmez |
| Search | `search_library`, `get_match_context` | Kaynak türü ve gerçek zaman aralığı |
| Download | `list_formats`, `enqueue_download`, `retry_job`, `cancel_job`, `pause_job` | Capability kontrolü backend'de |
| Playback | `get_playback_source`, `save_playback_position`, `open_source` | Arbitrary filesystem/remote scheme erişimi yok |
| AI | `list_models`, `approve_model_download`, `enqueue_transcription`, `reindex` | Onay ve kaynak denetimi backend'de |
| Sessions | `list_auth_profiles`, `verify_session`, `clear_session_reference` | Gizli değer UI'a dönmez |
| Portability | `export_library`, `validate_import`, `import_library`, `backup_library` | Seçilen hedef ve şema doğrulaması |
| Settings | `get_settings`, `update_settings`, `get_storage_usage`, `get_diagnostics` | Ayarların tipi, aralığı ve erişimi denetlenir |
| Storage | `list_storage_roots`, `add_storage_root`, `start_storage_move`, `list_orphan_media`, `reimport_orphan`, `delete_orphan`, `scan_storage_root` | Kök dışına yazma yok; silme ayrı onay ister |
| AI proposals | `list_ai_proposals`, `decide_ai_proposal` | Kabul edilmeden kullanıcı verisi değişmez |

### 18.2 DTO ve olay kuralları

- Komut girişleri backend'de tekrar doğrulanır; UI validasyonu güvenlik sınırı değildir.
- Sonuçlar kararlı DTO döner; DB satırı veya ham subprocess çıktısı doğrudan UI'a verilmez.
- Önerilen olaylar: `library_item_changed`, `job_progress`, `job_state_changed`, `storage_warning`, `model_state_changed`, `session_state_changed`.
- Olaylar iş/öğe kimliği, artan sequence/revision ve zaman taşır. UI eski olayın yeni durumu ezmesini engeller.
- UI yeniden bağlanınca SQLite-backed snapshot sorgular; kaçırılmış event nedeniyle kalıcı olarak yanlış kalmaz.
- Abonelikler sayfa kapanınca temizlenir; progress olayları render fırtınası oluşturmaz.
- Dosya oynatımı izin verilen local asset/protocol aracılığıyla yapılır. Webview'a tüm diski okuma yetkisi verilmez.
- Webview uzak kaynaktan görsel, medya, font veya script yüklemez (CSP `img-src`/`media-src` yalnızca yerel asset protocol). Uzak küçük resimler backend tarafından ağ politikası (§20.2) altında indirilir ve `cache/thumbnails/` altında `MediaAsset(role = thumbnail)` olarak saklanır; UI yalnızca bu yerel kopyayı gösterir.

### 18.3 Ortak hata biçimi

```text
DomainError {
  code: stable enum,
  message_key: localized key,
  retryable: boolean,
  suggested_action: optional enum,
  diagnostic_id: opaque id,
  safe_details: bounded allowlisted object
}
```

Normal UI raw stack trace, shell komutu, token, cookie veya tam authenticated URL göstermez. Teknik diagnostics görünümü de aynı redaction sınırına tabidir; “advanced” olması gizli veri açığa çıkarma izni değildir.

## 19. Hata sınıflandırması

| Kod | Kullanıcıya açıklanacak durum | İş davranışı / eylem |
|---|---|---|
| `INVALID_URL` | Geçersiz veya desteklenmeyen şema | Alanı düzelt; iş başlatma |
| `URL_BLOCKED` | Ağ/hedef güvenlik politikasına aykırı URL | Ağ isteği yapma; gerekçeyi göster |
| `DUPLICATE_ITEM` | Zaten kayıtlı içerik | Mevcut öğeye yönlendir; metadata'yı koru |
| `NETWORK_UNAVAILABLE` | Ağ erişilemiyor | Backoff veya offline bekleme |
| `RATE_LIMITED` | Kaynak geçici olarak istek sınırı uyguluyor | Retry-After/backoff; gizli kaçınma yok |
| `LOGIN_REQUIRED` | Oturum gerekli | Açık izinli oturum akışına yönlendir |
| `SESSION_EXPIRED` | Doğrulanmış oturum artık geçersiz | Reconnect bekle |
| `COOKIE_ACCESS_FAILED` | Tarayıcı profilinden izinli oturum okunamadı | Kilit/OS şifreleme/izin için güvenli açıklama |
| `ACCESS_DENIED` | İçeriğe erişim yetkisi yok | Otomatik tekrar yok; yetki kontrolü |
| `MEDIA_URL_EXPIRED` | Geçici medya adresi artık geçerli değil | Kalıcı kaynaktan sınırlı yeniden çözümleme |
| `CONTENT_UNAVAILABLE` | Silinmiş veya artık erişilemeyen içerik | Bookmark ve varsa yerel dosyayı koru |
| `REGION_RESTRICTED` | Kaynak bölge erişimine izin vermiyor | Kısıtı açıkla; aşma akışı sunma |
| `DRM_UNSUPPORTED` | Korunan medya desteklenmiyor | İndirmeyi durdur |
| `EXTRACTOR_UNSUPPORTED` | Bu kaynak biçimi desteklenmiyor | Bookmark fallback |
| `EXTRACTOR_FAILED` | Çıkarma başarısız, neden kesin olmayabilir | Redakte tanı; güvenli sürüm/retry eylemi |
| `DISK_FULL` / `QUOTA_EXCEEDED` | Alan yetersiz veya kota dolu | Kullanıcı müdahalesi bekle |
| `PATH_NOT_WRITABLE` | Hedefe yazılamıyor | Yeni hedef seç |
| `FILE_MISSING` / `MEDIA_CORRUPT` | Dosya yok veya doğrulanamıyor | Relink veya yeniden edinim |
| `CODEC_UNSUPPORTED` | Dosya mevcut, uygulama oynatamıyor | Desteklenen oynatma seçeneğini açıkla |
| `MODEL_MISSING` / `RESOURCE_LIMIT` | AI modeli veya kaynak yetersiz | Model/kaynak ayarını düzelt |
| `CHECKSUM_MISMATCH` / `SIGNATURE_INVALID` | Araç/model/update güvenilirliği doğrulanamadı | Kullanma/çalıştırma; güvenli hata |
| `IMPORT_SCHEMA_UNSUPPORTED` | Yedek sürümü desteklenmiyor | Mevcut kütüphaneyi değiştirme |
| `MIGRATION_FAILED` | Veri şeması yükseltilemedi | Güvenli duruş ve yedek kurtarma |
| `INPUT_REJECTED` | Dosya, başka dosya veya ağ kaynağına referans veren bir biçimde (playlist, manifest, concat) | Import etme; nedenini açıkla |
| `STORAGE_ROOT_UNAVAILABLE` | Varlığın kökü bağlı değil veya erişilemiyor | Kökü bağlama/yeniden konumlandırma; varlığı `missing` yapma |
| `URL_HAS_CREDENTIALS` | URL kullanıcı adı/şifre veya geçici sır içeriyor | Kaydetme; güvenli biçime düzeltme iste |
| `PLAYLIST_UNSUPPORTED` | Playlist/kanal/profil URL'si toplu işlenmiyor | Tek bookmark olarak kaydet; uyarı göster (§6) |
| `OFFLINE_MODE` | Kullanıcı Offline Mode'u açtı | İşi `blocked` tut; ağ isteği yapma |
| `TIMEOUT` | İşlem süre sınırını aştı | Retry adayı; tekrarlarsa redakte tanı |
| `CANCELED` | Kullanıcı iptal etti | Terminal durum; hata gibi gösterme |
| `DEPENDENCY_FAILED` | Bağımlı olunan iş başarısız | Ebeveyn işi yeniden deneme eylemi |
| `TOOL_MISSING` | Gerekli araç (yt-dlp, JS runtime, FFmpeg/ffprobe) yok veya uyumsuz | Bookmark'ı koru; aracı onarma/yeniden kurma yolu |
| `TOOL_BLOCKED` | Araç antivirüs/SmartScreen veya OS politikası nedeniyle çalıştırılamadı | Güvenli açıklama ve araç doğrulama bilgisi; güvenlik yazılımını kapatmayı önermez |
| `REVISION_CONFLICT` | Kayıt başka bir işlemle değişti | Güncel veriyi yükle; değişikliği yeniden uygula |
| `INTERNAL_ERROR` | Beklenmeyen hata | Diagnostic id göster; ayrıntı uydurma |

Yerelleştirme metni hata kodundan türetilir. Bir provider yalnızca belirsiz hata döndürüyorsa daha kesin kullanıcı açıklaması uydurulmaz. Loglarda arama sorgusu, kullanıcı notu ve özel kaynak başlığı varsayılan olarak tutulmaz.

## 20. Güvenlik ve gizlilik

### 20.1 Güven sınırları ve tehdit modeli

| Varlık / giriş | Tehdit | Zorunlu kontrol | Doğrulama |
|---|---|---|---|
| URL ve redirect | Yerel/özel ağa erişim, tehlikeli şema, DNS rebinding | HTTP(S) allowlist, hedef/IP politikası, her redirect ve bağlantıda yeniden denetim | Loopback/private/link-local/IPv6/redirect fixture'ları |
| Metadata/altyazı | XSS, prompt injection, dev payload | Güvenli render, alan/çıktı limitleri, içerik–talimat ayrımı | HTML/script/uzun veri testleri |
| Dosya adı/yolu | Traversal, junction kaçışı, üzerine yazma | Resolved path sınırı, isim güvenliği, güvenli create/rename | Windows yol ve symlink/junction testleri |
| CLI girdisi | Komut/argüman injection, yerel config etkisi | Argüman dizisi, typed allowlist, kontrollü ortam/config | Metacharacter ve kötü niyetli metadata testleri |
| Cookie/profile | Gizli verinin log, DB, export veya process output'a sızması | Açık izin, OS koruması, allowlist/redaction, ham dump yok | Canary secret ile DB/log/export/process-output testi |
| Dış binary/model | Tedarik zinciri saldırısı | Güvenilir manifest, hash/imza, sürüm pin, güvenli update | Bozuk/uyuşmayan artifact reddi |
| Medya parser | Bozuk medya, kaynak tüketimi; manifest/concat girdisiyle ağa veya izinsiz dosyaya erişim | Güncel sabit araç, timeout, process/resource sınırı, protokol izin listesi, referanslı girdi biçimlerinin reddi | Bozuk/kötü biçimli fixture, iptal, ağa/başka dosyaya işaret eden manifest fixture'ı |
| IPC | Yetkisiz uzak içerikten ayrıcalıklı çağrı | Tauri least privilege, CSP, origin/navigation kontrolü | Capability ve origin negatif testleri |
| Export/import | Veri sızıntısı, path/şema injection | Alan allowlist, önizleme, limits, şema doğrulama | Round-trip ve kötü niyetli JSON |

### 20.2 Ağ politikası

Yalnızca HTTP(S) kaynak URL'leri kabul edilir. `file:`, `javascript:`, `data:` ve işletim sistemi komut şemaları capture girdisi olarak reddedilir. URL userinfo ve credential taşıyan girdiler reddedilir. Loopback, özel ağ, link-local, cloud metadata endpoint'leri ve IPv6 karşılıkları uzak içerik yakalamada engellenmelidir. IPv4-mapped IPv6 ve normalizasyon varyantları test kapsamındadır.

URL'yi bir kez çözümleyip ardından kontrolsüz dış araca vermek tam SSRF koruması sayılmaz. yt-dlp/FFmpeg'in iç redirect, manifest ve alt kaynak isteklerinin nasıl denetleneceği M0 güvenlik uygulanabilirlik kapısıdır. Gerekirse yalnızca doğrulanmış konektörler açılır; bağlantı başına politika garanti edilemiyorsa genel URL ağ çıkarımı kapalı kalır, bookmark kaydı sürer. Seçilen ağ denetimi yaklaşımı `docs/threat-model.md` ve ADR'de kanıtlanmalıdır.

### 20.3 Oturum, tanı ve hassas veri

- OS korumalı credential store/DPAPI benzeri mekanizmanın uygun kullanımı doğrulanmalıdır. Secret değeri React state'ine veya SQLite'a girmez.
- Kullanıcının tüm tarayıcı çerez deposu kalıcı plaintext'e kopyalanamaz. İlk aşama izinli, desteklenen doğrudan tarayıcı okuma yoludur.
- Kullanıcı hangi tarayıcı/profilin okunacağını, hangi platform işlemi için kullanılacağını ve uygulamanın saklayacağı referansı görür. İzin kapsamı kaydedilir ve kaldırılabilir.
- Araç çıktısı redaction işleminden önce kalıcı loga yazılmaz. İmzalı medya URL, cookie header, Authorization, token/sensitive query parametreleri ve özel path'ler için test vardır.
- Crash report/diagnostic otomatik yüklenmez. Uygulama bilinçli olarak secret içeren dump üretmez; child process ve OS dump davranışı dağıtım öncesi incelenir. İşletim sistemi yönetici dump'ları üzerinde mutlak garanti iddia edilmez.
- **Öneri:** Diagnostics varsayılan olarak yerel, dönen ve toplam boyutu sınırlı log tutar; export kullanıcı önizlemesiyle yapılır. Kesin süre/boyut M0'da belgelenir.
- UI'da kullanıcının kendi kaynak bağlantısını açabilmesi, bu bağlantının loga veya AI servisine gönderilebileceği anlamına gelmez.

### 20.4 Gizlilik ve uygunluk ilkeleri

MVP telemetry/analytics içermez. Gelecekte eklenirse ayrı opt-in ve veri minimizasyonu gerekir. Zorunlu hesap veya arka planda dış AI gönderimi bulunamaz. Uygulama, yerel DB/not/transkript içeriğinin tamamının otomatik olarak şifreli olduğunu iddia etmemelidir; secret koruması ile tüm kütüphane şifrelemesi farklı kararlardır. Kullanıcı OS dosya izinleri ve disk şifrelemesinden yararlanabilir; uygulama içi tam veri şifreleme MVP zorunluluğu değildir.

Kullanıcıya içeriği edinme hakkı/izni ve erişim kısıtlarına uyma sorumluluğu açıklanır. Bu belge hukuki uygunluk onayı değildir. Dağıtım öncesinde ürün lisansı, yt-dlp, FFmpeg build'i, Tauri, modeller, font/ikonlar ve geçişli bağımlılıkların yeniden dağıtım koşulları ayrı doğrulanmalıdır. Platform koşulları ve mağaza kuralları hedef sürüm tarihinde kontrol edilir.

Bağış UI'ı, ödeme uçları, kripto adresi, cüzdan toplama veya ülke bazlı hukuki geçiş yöntemleri MVP'de bulunamaz. Hukuki/vergi incelemesi tamamlanana kadar görünür bağış düğmesi de etkinleştirilmez.

## 21. VK ve tarayıcı oturumu doğrulama planı

Kullanıcının geçmiş gözlemi, özel VK videosunun Chrome çerezleriyle ve VK sekmesi açıkken indirilebildiğidir. Bu gözlem, yt-dlp'nin sekmenin açık olmasına teknik olarak ihtiyaç duyduğunu kanıtlamaz.

### 21.1 Aşama 1 — tarayıcı çerezi kullanımı

Desteklenen `--cookies-from-browser` yöntemi açık izinle değerlendirilir. OS şifreleme, Chrome sürümü, profil kilidi ve dosya izinleri başarısızlık nedenleri olabilir; çözümlerin güncel ortamda geçerliliği test edilmelidir.

**Bilinen risk — Chrome on Windows:** Chrome 127 ve sonrası Windows'ta çerezleri *app-bound encryption* ile korur; bu nedenle `--cookies-from-browser chrome` Windows'ta yaygın biçimde başarısız olur. Chrome açıkken çerez veritabanı ayrıca kilitli olabilir. Bu nedenle DEC-08'in başlangıç varsayımı “Chrome çerezleri Windows'ta doğrudan okunamayabilir” şeklindedir. Test matrisi Chrome'a ek olarak şu alternatifleri kapsar:

- Firefox profili üzerinden `--cookies-from-browser firefox` (açık izinle, yalnızca ilgili platform için kullanılır).
- Kullanıcının kendi tarayıcısından açık eylemle dışa aktardığı Netscape biçimli çerez dosyası (`--cookies`). yt-dlp'nin resmî dokümanına göre `--cookies FILE` dosyayı okumakla kalmaz, çerez deposunu aynı dosyaya geri yazar. Bu nedenle kullanıcının dosyası yt-dlp'ye hiçbir zaman doğrudan verilmez:
  - Uygulama dosyadan yalnızca ilgili platformun alan adlarına ait çerezleri, iş başına ayrı bir çalışma kopyasına (`staging/` altında) yazar. Kopyanın erişim izni yalnızca mevcut Windows kullanıcısıyla sınırlanır.
  - yt-dlp'ye bu çalışma kopyası verilir. İş başarıyla bitse de, başarısız olsa da, iptal edilse de kopya iş sonunda silinir. Çökme sonrası açılışta kalan çalışma kopyaları uzlaştırma adımında silinir.
  - Kullanıcının özgün dosyası değiştirilmez. Uygulama bu dosyayı saklamaz ve kullanıcıya işi bitince silmesini önerir.
  - Çalışma kopyasının yolu veya içeriği `Job.payload`'a, loga, diagnostics'e ve export'a girmez; payload yalnızca opak bir referans taşır.
- Aşama 2'deki uygulamaya ait profil (§21.3).

Güncel durum M3 başlangıcında yt-dlp ve Chrome sürüm notlarından yeniden doğrulanır; burada yazılan risk canlı test sonucu değildir. Tam profile veya ilgisiz site çerezlerine kalıcı kopyalama yapılmaz. Tarayıcıyı zorla kapatmak veya kullanıcının profil ayarlarını değiştirmek otomatik davranış değildir.

### 21.2 Canlı test matrisi

Aynı izinli VK videosu ve mümkün olduğunca aynı profil/oturumla çalışılır. Yeniden login/oturum yenileme gibi koşul değişiklikleri kayıt altına alınır. Oturumun süresi veya medya URL yenilenmesi kontrol edilmeden sonuçlar sekme durumuna bağlanmaz.

| Test | Koşul | İncelenen soru |
|---|---|---|
| VK-01 | Chrome açık, VK sekmesi açık, login mevcut, izinli browser cookies | Temel yetkili erişim/indirme çalışıyor mu? |
| VK-02 | Chrome açık, VK sekmesi kapalı, aynı profil/oturum | Sekme açık olma koşulu fark yaratıyor mu? |
| VK-03 | Chrome tamamen kapalı, aynı profil/oturum | Profil kilidi veya tarayıcı durumu etkili mi? |
| VK-04 | Uygulamaya ait profil, kullanıcı interaktif login yapmış | Oturum deposu yt-dlp ile gerçekten uyumlu mu? |
| VK-05 | Profil/çerez erişimi başarısız | Doğru cookie-access hatası mı veriliyor? |
| VK-06 | Grup üyeliği kaldırılmış veya içerik yetkisi yok | Oturumdan ayrı access-denied teşhisi çalışıyor mu? |
| VK-07 | Oturum süresi dolmuş | Reconnect ve bookmark koruma çalışıyor mu? |
| VK-08 | Video silinmiş/kullanılamaz | Yerel metadata ve varsa dosya korunuyor mu? |
| VK-09 | Firefox profili, VK login mevcut, izinli browser cookies | Chrome app-bound şifreleme engeline karşı uygulanabilir yol mu? |
| VK-10 | Kullanıcının dışa aktardığı çerez dosyası, çalışma kopyası yöntemiyle | Dosya tabanlı yöntem çalışıyor mu; özgün dosya değişmeden kalıyor mu; çalışma kopyası iş sonunda siliniyor mu? |

Her test için OS build'i, tarayıcı ve yt-dlp sürümü, test zamanı, anonim fixture/test kimliği, exit code, sanitize hata kategorisi, sonuç ve gözlem kaydedilir. Çerez değerleri, authenticated medya URL'leri ve özel başlıklar rapora girmez. Gerçek kullanıcı erişimi gerektiren koşullar yoksa test **not run** olarak işaretlenir; fixture sonucu canlı başarı sayılmaz. Üyelik kaldırma gibi hesap durumunu değiştiren testler yalnızca kullanıcı tarafından izin verilen test düzeninde yapılır.

### 21.3 Aşama 2 — uygulamaya ait tarayıcı PoC

WebView2, Playwright Chromium ve Chrome cookie/profile depolarının birbirinin yerine kullanılabildiği varsayılmaz. Kullanıcı login'i kendisi etkileşimli yapar. Gerekirse yalnızca ilgili platforma daraltılmış, şifreli, yerel ve kısa ömürlü session bridge tasarlanır; şifre, MFA sırrı ve ilgisiz site çerezleri alınmaz. Bu mekanizma tasarım/güvenlik/lisans ve gerçek çalışma kanıtı olmadan üretime açılmaz.

Tarayıcı uzantısı ayrıca izin isteyen alternatif bir araştırmadır; MVP bağımlılığı değildir. PoC başarısızlığı temel bookmark/import/arama işlevlerini etkilemez.

## 22. İşlevsel olmayan gereksinimler

Aşağıdaki sayısal hedefler **önerilen mühendislik kabul bütçeleridir**; ölçülmüş performans iddiası değildir. M0'da referans cihaz ve benchmark yöntemi kesinleştirilip `docs/testing.md` içine yazılmalıdır.

| Kimlik | Alan | Hedef / ölçüm |
|---|---|---|
| NFR-01 | Bookmark kaydı | Metadata beklemeden yerel commit ve görünür sonuç için p95 ≤ 500 ms; 10.000 öğeli fixture |
| NFR-02 | Kullanıcı akışı | Hazır URL yapıştırıldıktan sonra ≤ 2 birincil eylem; yönlendirilmiş kullanım testinde yaklaşık 5 s kayıt hedefi |
| NFR-03 | Lexical arama | Sıcak indekste 10.000 öğe için backend p95 ≤ 200 ms; 100 ms debounce dahil görünür sonuç hedefi ≤ 400 ms |
| NFR-04 | Döküm araması | M4'te 100.000 segmentli fixture için p95 ≤ 500 ms başlangıç hedefi; ölçüm sonucu bütçe gözden geçirilir |
| NFR-05 | UI tepkisi | Ağ/medya/AI işi sırasında gezinme ve iptal denetimleri yanıt verir; büyük listeler sanallaştırılır |
| NFR-06 | Başlangıç | Referans cihazda 10.000 öğeli kütüphanenin temel etkileşime hazır olması için ≤ 3 s hedef; soğuk/sıcak ölçüm ayrılır |
| NFR-07 | Bellek | AI kapalı temel kütüphane için ölçülen idle/scroll bellek değeri raporlanır; **başlangıç hedefi:** toplam uygulama süreçlerinde ≤ 500 MB idle |
| NFR-08 | Veri dayanıklılığı | Başarılı commit edilmiş bookmark/not/ilişkiler force-close sonrasında korunur; doğrulanmamış medya hazır görünmez |
| NFR-09 | İptal | İptal talebi UI'da hemen görünür; normal koşullarda süreç duruşu ≤ 5 s hedef, zorunlu kapatma sonrası tutarlılık kontrolü |
| NFR-10 | Seek | Backend doğru segment ofsetini gönderir; uyumlu test medyasında oynatıcı hedefi ±500 ms içinde doğrulanır; codec sınırlamaları ayrıca kaydedilir |
| NFR-11 | Gizlilik | Offline Mode testinde uygulama ve yönettiği süreçlerden dış bağlantı yok; log/DB/export canary secret içermiyor. Ölçüm sınırı: uygulama süreci ve yönettiği alt süreçler (yt-dlp, JS runtime, FFmpeg, AI provider) için sıfır dış bağlantı kesin şarttır. WebView2/Edge runtime süreçlerinin trafiği ayrıca ölçülür; kapatılabilenler kapatılır, kapatılamayanlar `docs/privacy.md`'de belgelenmiş istisna olarak listelenir. Uygulamadan bağımsız işletim sistemi servisleri kapsam dışıdır |
| NFR-12 | Erişilebilirlik | Temel akışlar klavyeyle tamamlanır; kritik otomatik erişilebilirlik bulgusu yok; ekran okuyucu ve kontrast manuel kontrol edilir |
| NFR-13 | Paketleme | Temiz Windows 11 hedefinde son kullanıcı Python/Node/Rust kurmadan kurulum ve temel akışlar çalışır |
| NFR-14 | Uyumluluk | Windows x64 ilk paket hedefi önerilir; ARM64/Windows 10/macOS/Linux desteği ayrı doğrulamaya tabidir |

**Önerilen benchmark ortamı:** Windows 11 x64, SSD, 8 GB RAM, 4 mantıksal CPU veya üzeri; CPU modeli, OS build'i, araç sürümleri ve güç modu rapora yazılır. AI hızı bu bütçelere dahil değildir; model bazında real-time factor, tepe RAM, disk ve varsa GPU kullanımı ölçülür. Hedef karşılanmazsa sonuç gizlenmez; optimizasyon veya belgeli kapsam/bütçe kararı gerekir.

## 23. Windows dağıtımı ve güncelleme

### 23.1 Paket kapsamı

Installer; uygulama, gerekli runtime politikası, SQLite/FTS5, onaylı dış araçlar veya bunları güvenli hazırlayan yönetici ve lisans bildirimlerini içermelidir. İlk kurulumun ağ/izin gereksinimi açık olmalıdır. WebView2 runtime varlığı ve offline/online kurulum stratejisi M0'da doğrulanır.

yt-dlp, JS runtime (ör. Deno), FFmpeg ve ffprobe için binary manifest; ad, sürüm, kaynak, hash, mevcutsa imza, lisans ve uyumluluk bilgilerini taşır. Uygulama açılışında ve araç güncellemesinden sonra her araç için hafif bir sağlık kontrolü (sürüm sorgusu) yapılır; antivirüs karantinası veya eksik dosya `TOOL_MISSING`/`TOOL_BLOCKED` olarak gösterilir. Bilinmeyen internet konumundan executable indirip çalıştırılamaz. Model ve araç güncellemeleri uygulama güncellemesinden ayrı yönetilebilir ancak aynı güven doğrulama politikasına tabidir.

### 23.2 Güncelleme davranışı

- Uygulama update paketi imzalı olmalı; installer'ın code-signing/provenance stratejisi dağıtım öncesi kesinleşmelidir.
- Kullanıcı güncelleme denetimini ve kurulumu kontrol eder. **Öneri:** MVP'de açık “Check for updates” ve onaylı kurulum; otomatik kontrol isteğe bağlı ayardır.
- Offline Mode güncelleme ağına da uygulanır.
- İmza/hash doğrulaması başarısız artifact kullanılmaz. Önceki çalışan sürüm, güvenli rollback veya yeniden kurulum yolu korunur.
- Araç güncellemesi aktif indirmede executable'ı değiştirmez; yeni sürüm sonraki işe uygulanır.
- Veritabanı migration'ı olan uygulama güncellemesinde geri dönüşün veri uyumluluğu açıkça yönetilir.
- Sürüm notları, veri değişikliği, uyumluluk ve bilinen kısıtlar gösterilir.

### 23.3 Temiz kurulum ve kaldırma

Temiz Windows 11 üzerinde kurulum, ilk açılış, dosya izinleri, boş kütüphane, bookmark/import, gerçek izinli download ve oynatma doğrulanmalıdır. Installer başarıyla üretilmesi gerçek kurulum testiyle aynı değildir. Uninstall, medya kütüphanesini ve kullanıcı yedeklerini sessizce silmez; uygulama verisi temizliği ayrı ve açık seçenek olmalıdır.

## 24. Android ve gelecekteki senkronizasyon

M6'da paylaşılan domain sözleşmeleri ve UI mantığı kullanılabilir; Windows executable/subprocess yolu Android'e doğrudan taşınamaz. Tauri 2 Android yetenekleri, plugin bakımı, ABI paketleme, indirme motoru, local AI runtime ve store koşulları güncel resmî kaynak ve cihaz testiyle doğrulanmalıdır.

Android gereksinimleri:

- Android Share → Save akışı; URL kaydı indirmeden bağımsızdır.
- Masaüstü sidebar'ını küçültmek yerine mobil gezinme ve bottom navigation.
- Scoped storage/Storage Access Framework; masaüstü absolute path yerine platforma uygun erişim referansı.
- Arka plan çalışma, bildirim izni, batarya ve ağ kısıtları için native köprüler ve gerçek cihaz testi.
- Chrome veya başka uygulamaların özel cookie deposuna erişim varsayımı yoktur.
- Çevrimdışı yerel oynatma ve hazır indeks araması temel hedeftir. AI işleme masaüstünde hazırlanmış veriyle sınırlanabilir.
- Masaüstü GPU özelliklerinin telefonda aynı biçimde bulunacağı varsayılmaz.

Senkronizasyon MVP/M6'nın zorunlu önkoşulu değildir. Gelecekte opt-in olmalı; uygun yerde uçtan uca şifreleme, anahtar/kurtarma, çakışma, silme yayılımı, medya transfer bütçesi ve gizli profil verisinin hariç tutulması ayrı SPEC/ADR gerektirir.

## 25. Test stratejisi

### 25.1 Test katmanları

| Katman | Zorunlu kapsam |
|---|---|
| Unit | URL normalizasyon/tanıma, güvenli dosya adı, model invariants, state transitions, retry/error mapping, timecode, arama query builder ve ranking |
| Integration | SQLite migration/backup/restore, FTS güncelleme, ffprobe import, duplicate yarışları, yt-dlp JSON/progress/error fixture, cancel/retry, crash recovery |
| E2E | URL save, local import, arama, etiket/koleksiyon, offline kütüphane, download queue; kritik altı akış için deterministic fixture |
| Security | URL/redirect/path/command injection, untrusted metadata, secret redaction, import schema, update checksum/imza negatif senaryoları |
| Visual | 1366×768, 1920×1080, dar pencere; koyu/açık, uzun başlıklar, eksik görseller, çok dil, loading/error/empty |
| Accessibility | Klavye ve focus, label, screen reader duyurusu, kontrast, zoom, reduced-motion |
| Manual device/platform | Temiz Windows 11, gerçek izinli platform URL'leri, gerçek VK oturumu, installer/update, gelecekte Android/GPU |

Playwright ile deterministik fixture arayüzünde altı kritik yolculuğun her biri için en az bir E2E test bulunmalıdır. UI testindeki mocked IPC başarısı gerçek Tauri IPC veya Windows subprocess entegrasyonunun kanıtı değildir. Native/backend integration ve gerçek Windows smoke testleri ayrıca yapılır; seçilecek desktop E2E harness M0'da doğrulanır.

### 25.2 Deterministik fixture seti

- Karma platformlu, bazıları bookmark-only bazıları local-ready en az 50 gerçekçi sentetik öğe.
- Çok uzun başlık, Türkçe/İngilizce metin, emoji, eksik thumbnail, bilinmeyen süre, çoklu etiket ve koleksiyon.
- §16.1 Türkçe arama davranışı için ayrı bir arama fixture'ı: `I/İ/ı/i` büyük-küçük harf eşleşmeleri (ör. “İstanbul” ↔ “istanbul”, “ISIK” ↔ “ışık”), `ç/ğ/ö/ş/ü` ve aksansız yazımları (ör. “Gunes” ↔ “Güneş”), NFC ve NFD biçimli aynı metin, birleşik nokta işaretli “i̇” ve tırnak/noktalama içeren çok sözcüklü sorgular. Her vaka için beklenen sonuç (eşleşir / eşleşmez / belgelenmiş kısıt) fixture içinde yazılıdır.
- Sabit yt-dlp metadata, format listesi, aşamalı progress, bilinmeyen toplam, hata ve cancellation çıktıları.
- Lisansı/izni uygun kısa MP4/MKV/WebM/MOV ve desteklenen ses örnekleri; bozuk/eksik dosya örnekleri.
- En az bir Türkçe ve bir İngilizce ses/video; elle doğrulanmış kısa referans döküm ve zaman aralıkları.
- Duplicate URL varyantları, hash eşleşmeleri, geniş klasör ağacı ve import hata örnekleri.
- Güvenlik için gerçek secret yerine canary cookie/token ve kötü niyetli URL/filename/metadata.
- Performans için ayrı 10.000 öğe ve 100.000 transkript segmenti üreten sürümlü veri üreticisi.

Gerçek özel VK URL'leri, kullanıcı çerezleri ve kişisel dosyalar repository/CI fixture'ı olamaz. Fixture sonuçları veri ve seed sürümüyle tekrarlanabilir olmalıdır.

### 25.3 Kabul senaryoları

| Kimlik | Koşul ve eylem | Beklenen sonuç |
|---|---|---|
| AC-01 | Ağ kapalıyken geçerli URL kaydet | Bookmark ve FTS kaydı oluşur; pending metadata; restart sonrasında görünür |
| AC-02 | Aynı platform/source ID veya normalize URL iki kez kaydet | Tek öğe; mevcut not/etiket kaybolmaz; mevcut öğeye yönlendirilir |
| AC-03 | Aynı URL'yi eşzamanlı iki capture isteğiyle kaydet | Unique constraint sonucu tutarlı tek öğe; anlaşılır duplicate sonucu |
| AC-04 | Geçerli ve bozuk dosyaları aynı klasörden import et | Geçerli dosyalar kayıtlı; hatalılar ayrı raporlu; kaynaklar değişmemiş |
| AC-05 | Etiket/not ekle, uygulamayı kapat/aç ve ara | Son düzenleme kalıcı; yeni metin aramada görünür |
| AC-06 | Koleksiyonu sil | Öğeler ve medya korunur; üyelikler temizlenir |
| AC-07 | 20 öğeye toplu etiket/arşiv uygula, Undo kullan | Doğru kayıtlar değişir ve geri alınır; seçim/scroll bağlamı tutarlı |
| AC-08 | İzinli indirme + gerekiyorsa merge | Progress ve fazlar doğru; doğrulanmış yerel dosya açılır |
| AC-09 | Toplam byte bilinmeyen indirme | Indeterminate gösterge; uydurma yüzde/ETA yok |
| AC-10 | İndirmeyi iptal et | İş terminal canceled; süreç durmuş; kısmi dosya hazır görünmüyor |
| AC-11 | İndirmede ağ kes, retry veya restart yap | Destek varsa devam; değilse açıklanmış restart; tek final medya referansı |
| AC-12 | Disk dolsun veya hedef yazılamasın | Anlaşılır engel; bookmark korunur; ilgisiz dosya silinmez |
| AC-13 | Metadata refresh ve kullanıcı not düzenlemesi yarışsın | Kullanıcı notu/override'ı korunur; eski revision yeni değişikliği ezmez |
| AC-14 | Kaynak video silinsin, yerel dosya kalsın | Kaynak unavailable; yerel oynatma ve arama çalışır |
| AC-15 | JSON export/import ile boş kütüphaneye geri yükle | Not/etiket/koleksiyon korunur; medya relink; FTS yeniden oluşur |
| AC-16 | Kötü veya gelecek sürümlü JSON import et | Mevcut kütüphane değişmez; doğru şema hatası |
| AC-17 | Offline Mode etkin, uzak thumbnail/model/update işi mevcut | Uygulama süreci ve yönettiği alt süreçlerden dış ağ çağrısı yapılmaz; işler nedenleriyle bekler; yerel işlevler çalışır. WebView2 runtime trafiği NFR-11'e göre ayrı ölçülür ve belgelenmiş istisna listesiyle karşılaştırılır |
| AC-18 | Canary secret içeren araç hatası üret | UI/log/SQLite/export/diagnostics içinde canary yok |
| AC-19 | M3 oturum var fakat üyelik yok | Session ile access-denied ayrı gösterilir; bookmark korunur |
| AC-20 | Model indirme izni verilmedi | Model indirilmez; temel kütüphane çalışır |
| AC-21 | Türkçe ve İngilizce fixture transcribe et | Gerçek döküm/zamanlar üretilir; kalite ve hız raporlanır |
| AC-22 | Arama transkript hit'ini aç | Doğru medya ve segment ofseti; oynatma toleransı ölçülür |
| AC-23 | Transkripsiyon sırasında restart/iptal | Checkpoint politikası uygulanır; duplicate segment yok; partial açıkça etiketli |
| AC-24 | Model sürümünü değiştir ve reindex yap | Eski/yeni indeks karışmaz; iptal eski kullanılabilir indeksi bozmaz |
| AC-25 | Update/model hash veya imzası hatalı | Artifact çalıştırılmaz/yüklenmez; önceki çalışan durum korunur |
| AC-26 | Temiz Windows 11'e kur, temel akışları çalıştır, kaldır | Geliştirme SDK'sı gerekmez; kullanıcı medyası uninstall'da korunur |
| AC-27 | Dar görünüm ve 200% metin ölçeğinde klavye ile kaydet/ara | Odak görünür; temel eylemler erişilebilir; önemli bilgi kırpılıp kaybolmaz |
| AC-28 | Uzak metadata içinde script ve kötü amaçlı dosya adı kullan | Kod çalışmaz; hedef kök dışına yazılamaz; UI güvenli metin gösterir |
| AC-29 | `http://127.0.0.1`, `http://169.254.169.254`, `http://[::ffff:127.0.0.1]` ve özel ağa redirect eden URL ile capture/metadata dene | `URL_BLOCKED`; uygulamadan ve yönettiği araçlardan hedefe bağlantı yapılmaz |
| AC-30 | JSON import içinde `..\` veya junction ile medya kökü dışına çıkan, kullanıcının onaylamadığı referans kullan | Import reddedilir; mevcut kütüphane değişmez; kök dışına yazma yok |
| AC-31 | Bozuk FTS sözdizimiyle ara (ör. tek tırnak, `AND` ile biten sorgu) | Çökme yok; anlaşılır mesaj veya güvenli literal arama (SEA-04) |
| AC-32 | Desteklenmeyen container/codec'li dosyayı import edip oynat | Import başarılı; `CODEC_UNSUPPORTED` açıklaması; uygunsa remux önizlemesi veya harici oynatıcı seçeneği (PLAY-04, PLAY-07) |
| AC-33 | Düşük boş alan ve kota sınırıyla indirme başlat | Önceden uyarı; kota aşılınca iş `blocked`; kullanım ekranı doğru (DATA-07) |
| AC-34 | Aynı içerikli iki dosyayı farklı yollardan import et | Duplicate adayı gösterilir; otomatik birleştirme yok (IMP-05) |
| AC-35 | Import edilmiş dosyayı diskte taşı, uygulamayı aç ve yeniden bağla | Öğe `missing`; relink sonrası not ve dökümler korunur (IMP-07) |
| AC-36 | İmza parametresi platform kuralıyla çıkarılabilen bir URL kaydet ve hata üret (fixture beklenen canonical değeri tanımlar) | Kayıt imzasız canonical URL ile oluşur; `original_url` redakte; log, diagnostics ve export'ta parametre değeri yok (CAP-08) |
| AC-37 | Öğeyi kaldır, Undo kullan; tekrar kaldır ve saklama süresinin dolmasını simüle et | Undo öğeyi geri getirir; süre sonunda kalıcı silinir; managed medya onaysız silinmez ve `OrphanMedia` kaydı oluşur (§7.3) |
| AC-38 | Uygulama açıkken ikinci örneği başlat | İkinci süreç DB'yi açmaz; mevcut pencere öne gelir (INV-13) |
| AC-39 | Etiketi yeniden adlandır, ardından Undo kullan | Yeni ad aramada görünür ve eski ad sonuç vermez; Undo sonrası eski ad geri gelir ve aramada görünür (ORG-09) |
| AC-46 | İki etiketi birleştir, ardından Undo kullan | Birleştirmede tüm ilişkiler hedef etikete geçer; Undo sonrası eski etiket aynı kimlik ve ilişkilerle geri gelir (ORG-09) |
| AC-40 | M5: Aynı konuyu farklı kelimelerle anlatan TR/EN fixture'da anlamsal sorgu yap | Hibrit sonuç kaynak türüyle döner; kalite ölçümü raporlanır; embedding modeli kaldırılınca lexical arama çalışır |
| AC-41 | M5: Ekranda metin olan fixture videoda bu metni ara | Sonuç kare zamanı ve OCR etiketiyle, konuşma eşleşmesinden ayrı gösterilir |
| AC-42 | M6: Android'de Share → Save yap, uygulamayı kapat, çevrimdışı aç | Bookmark kalıcı; çevrimdışı arama ve yerel oynatma çalışır; izin ve arka plan sınırları raporlanır |
| AC-43 | Medya kökünü “Taşı” ile değiştir; taşıma ortasında uygulamayı zorla kapat ve yeniden aç | İş kaldığı yerden devam eder; her varlık eski veya yeni kökte doğru çözülür; doğrulanmış tek kopya kaybolmaz; eski kopyalar yalnızca commit sonrası silinir (DATA-08) |
| AC-44 | Normalizasyon kuralı değişikliğiyle aynı canonical değere düşen iki öğe oluştur | En eski öğe yeni değeri alır; diğeri eski değerle kalır ve duplicate adayı olur; migration tamamlanır; yeni capture yeni değerli öğeye gider (§11.7) |
| AC-45 | Kaldırılmış bir öğenin URL'sini yeniden kaydet | Ayrı kayıt oluşmaz; “Geri yükle” seçilince öğe notları ve ilişkileriyle geri gelir; “Vazgeç” seçilince hiçbir şey değişmez (§7.3) |
| AC-47 | Harici diskteki medya kökünü çıkar, uygulamayı aç, diski geri tak | Kök “bağlı değil” görünür; varlıklar tek tek `missing` olmaz; disk gelince kök otomatik doğrulanır (DATA-09) |
| AC-48 | Alt koleksiyonu olan koleksiyonu her iki seçenekle sil | “Üst seviyeye taşı”da alt koleksiyonlar kalır; “Birlikte sil”de alt koleksiyonlar silinir; iki durumda da öğeler ve dosyalar korunur (ORG-10) |
| AC-49 | Bir videoyu ortasına kadar izle, uygulamayı kapat ve aç | Kaldığı yerden devam etme eylemi doğru konumu gösterir (PLAY-02) |
| AC-50 | Tam DB yedeği al, veriyi değiştir, yedekten geri yükle; ayrıca migration öncesi otomatik yedekten dönüşü dene | Geri yükleme öncesi mevcut durum yedeklenir; şema uyumu doğrulanır; veri yedekteki duruma döner (§15.3, §11.7) |
| AC-51 | Kullanıcının açıkça onayladığı, kök dışındaki geçerli bir dosyayı JSON import ile dış referans olarak al | Dosya kendi `external_import` köküyle dış referans olarak kaydedilir; dosyaya yazma yapılmaz |
| AC-52 | Güvenli canonical biçime dönüştürülemeyen, açık sır taşıyan URL kaydetmeyi dene | `URL_HAS_CREDENTIALS`; DB'de kayıt oluşmaz; değer log ve diagnostics'te yok (§7.1) |
| AC-53 | Kalıcı silmede managed medyayı koru, “Sahipsiz medya” listesinden yeniden import et | Dosya listede görünür; yeniden import yeni `local_video` öğesi oluşturur ve dosya taşınmaz (§7.3) |
| AC-54 | Süresi dolmuş/geçersiz çerezli profille herkese açık bir videoya eriş | İndirme başarılı olabilir; öğe `public_ok` olur; profil `session_available` sayılmaz ve `last_verified_at` değişmez (§12.1) |
| AC-55 | Çerez dosyası yöntemiyle işi başarıyla bitir, iptal et ve işlem ortasında uygulamayı zorla kapat | Özgün dosya değişmez; çalışma kopyası üç durumda da silinir (çökmede açılış uzlaştırmasında); kopya yolu/içeriği log, DB ve export'ta yok (§21.1) |
| AC-56 | Çevrimdışıyken aynı videonun kısa ve uzun URL'sini ayrı kaydet, sonra metadata çözümlensin | Unique hatası görünmez; ikinci öğe `identity_conflict` ve duplicate adayı olur; birleştirmede URL alias'a dönüşür ve notlar korunur (§11.7) |
| AC-57 | `removed_at` değeri aylar önce olan öğeler içeren eski bir yedeği geri yükle | Öğeler hemen kalıcı silinmez; saklama sayacı restore anından başlar; önizleme etkilenen öğe sayısını gösterir (§7.3) |
| AC-58 | Bir alt işi kullanıcı olarak iptal et, ebeveynin başarısız olmasıyla bir diğerinin iptal olmasını sağla, ebeveyni yeniden dene | Bağımlılık nedeniyle iptal edilen alt iş yeniden kuyruğa girer; kullanıcının iptal ettiği alt iş iptal kalır (§13.1) |
| AC-59 | Ağ URL'sine ve kök dışı bir dosyaya işaret eden yerel HLS/concat manifestini import et | `INPUT_REJECTED`; FFmpeg/ffprobe'dan ağ bağlantısı veya kök dışı dosya okuması yok (§14.1) |
| AC-60 | Toplu etiketleme yap, arka planda metadata güncellemesi tamamlansın, ardından Undo kullan | Undo hâlâ kullanılabilir; yalnızca etiket değişikliği geri alınır; metadata güncellemesi korunur (§7.3) |

Her P0 gereksinim en az bir AC'ye veya kimlikli bir manuel teste bağlanır. Gereksinim–test kapsama matrisi `docs/testing.md` içinde tutulur ve sürüm çıkış kapısında kontrol edilir.

### 25.4 Manuel matris ve kanıt biçimi

Windows testleri; temiz kurulum, Python'sız kullanım, public URL/platform, yetkili özel VK, expired login, deleted video, region unavailable, no network, disk full, corrupt media ve download sırasında restart senaryolarını kapsar. Public URL testleri yalnızca izinli ve mevcut içerikle yapılır. Kullanılamayan platform **unsupported/untested** olarak raporlanabilir; bütün platformların başarılı olduğu varsayılamaz.

Her raporda `automated pass | automated fail | manual verified | not run | blocked` durumu, commit/sürüm, ortam, komut veya adımlar ve kanıt yolu bulunmalıdır. “Test edildi” ifadesi yalnızca gerçekten çalıştırılmış test için kullanılabilir. Başarısızlıklar ve ortam kısıtları release notlarına yansıtılır. Ekran görüntüleri ve loglar hassas olmayan test artifact dizininde saklanır.

## 26. Sürüm çıkış kapıları

### 26.1 Windows MVP — M0–M2

- [ ] Windows 11'de build, launch ve gerçek installer smoke testi başarılı.
- [ ] Ağ ve indirmeden bağımsız URL bookmark kaydı çalışıyor.
- [ ] Yerel import ve metadata kalıcılığı restart ile doğrulandı.
- [ ] İzinli gerçek video indiriliyor, progress/error doğru, final dosya oynatılıyor.
- [ ] Desteklenen platform iddiaları tarihli kanıta dayanıyor; başarısız çıkarıcı bookmark'ı bozmuyor.
- [ ] Not, etiket, manuel koleksiyon, favori/arşiv, lexical arama ve filtreler çalışıyor.
- [ ] Kuyruk cancel/retry/crash recovery ve disk/network hata testleri geçiyor.
- [ ] JSON round-trip, tutarlı yedek ve medya relink doğrulandı (AC-15, AC-50).
- [ ] Her P0 gereksinim kapsama matrisinde en az bir AC'ye veya manuel teste bağlı.
- [ ] Cookie/credential plaintext kalıcılığı veya log sızıntısı bulunmuyor.
- [ ] URL/path/IPC/command/update güvenlik kapıları açık bulgu bırakmıyor; AC-29, AC-30, AC-38, AC-51 ve AC-52 geçti.
- [ ] Temel klavye/erişilebilirlik ve hedef çözünürlük görsel kontrolleri tamamlandı veya eksikleri sürümü engelleyecek şekilde kaydedildi.
- [ ] Lisanslar, binary provenance, imzalı update ve temiz uninstall politikası doğrulandı.
- [ ] Testler, zorunlu dokümanlar, build/paketleme talimatları ve bilinen kısıtlar mevcut.
- [ ] Bağış ve gelecekteki AI/özel oturum özellikleri yanlışlıkla aktif/görünür değil.

### 26.2 AI V2 — M4

- [ ] Model indirme açık izin, boyut/kaynak bilgisi ve bütünlük doğrulamasıyla çalışıyor.
- [ ] Türkçe ve İngilizce test dosyalarında yerel transkripsiyon fiilen çalıştırıldı.
- [ ] Transkript provenance/model/dil bilgisi görünür ve indeksle tutarlı.
- [ ] Zaman kodlu arama doğru dosyada doğrulanmış ofsete açılıyor.
- [ ] İndeksleme/transkripsiyon iptal ve restart sonrası güvenli devam davranışı gösteriyor.
- [ ] CPU/RAM/disk ve varsa doğrulanmış GPU kullanımı görülebilir ve sınırlanabilir.
- [ ] Model yok/bozuk/yetersiz kaynak durumlarında temel kütüphane çalışıyor.
- [ ] Gerçek kalite/hız ölçümü raporlandı; ölçülmemiş doğruluk veya performans iddiası yok.

### 26.3 Yetkili oturum — M3

- [ ] Oturum özelliği açık izinle çalışıyor; izin kapsamı görünür ve kaldırılabilir.
- [ ] VK-01…VK-10 sonuçları tarihli kanıtla `docs/platform-support.md` içinde; çalıştırılamayanlar **not run** olarak işaretli.
- [ ] Session ile access-denied ayrımı (AC-19) ve §12.1 hata eşlemesi testlerle doğrulandı.
- [ ] Canary secret DB, log, export ve process output'ta yok (AC-18).
- [ ] Doğrulanmamış oturum yöntemleri dağıtımda kapalı veya açıkça deneysel.

M3 iki farklı sonuçla kapanabilir. **Araştırma tamamlandı:** tüm VK testleri çalıştırıldı ya da gerekçesiyle not run/blocked işaretlendi; özellik `unsupported` veya deneysel olarak kayıt altına alındı. **Özellik destekleniyor:** en az bir yöntemle gerçek yetkili VK metadata ve indirme başarısı `manual verified` kanıtıyla kaydedildi. Ürün yalnızca ikinci sonuçta “desteklenen” etiketini kullanabilir.

### 26.4 Anlamsal ve görsel AI — M5

- [ ] Embedding/vector, OCR ve görsel model kararları (DEC-12, DEC-13) kanıtla kayıtlı.
- [ ] Reindex generation geçişi ve iptal güvenliği (AC-24) doğrulandı.
- [ ] AC-40 ve AC-41 geçti; kalite ölçümü raporlandı.
- [ ] Sonuçlar konuşma, OCR ve görsel kanıtı ayrı etiketliyor; kalibre edilmemiş skor yüzde olarak gösterilmiyor.
- [ ] Kare ve kaynak bütçeleri uygulanıyor ve ayarlanabilir.

### 26.5 Android — M6

- [ ] DEC-14 kararları güncel resmî kaynak ve gerçek cihaz testiyle kayıtlı.
- [ ] AC-42 gerçek Android cihazda geçti.
- [ ] Scoped storage/SAF, bildirim izni, arka plan ve batarya sınırları test edildi.
- [ ] Chrome veya başka bir uygulamanın çerez deposuna erişim varsayımı yok.
- [ ] Mağaza politikası ve lisans kontrolü tamamlandı.

## 27. Geliştirme teslimatları ve çalışma protokolü

Uygulama aşamasında çalışma küçük uçtan uca dilimlerle ilerlemelidir. Her dilim için kapsam, kabul kriteri, yapılan değişiklik, çalıştırılan komut ve test sonucu kaydedilir. Kullanıcının commit edilmemiş çalışması ezilmez. Kaynak promptun küçük ve anlamlı commit yaklaşımı uygulanırken gerçek repository/izin bağlamı dikkate alınır.

| Dosya | Beklenen içerik | En geç |
|---|---|---|
| `SPEC.md` | Bu ürün/teknik sözleşme ve değişiklik takibi | Uygulama öncesi |
| `TASKS.md` | Checkbox, requirement/AC bağı, durum, engel ve sonraki küçük dilim | M0'dan itibaren |
| `docs/architecture.md` | Bileşenler, trust boundary, veri/iş akışı, paketleme | M0 |
| `docs/decisions.md` | ADR'ler, alternatifler, gerekçe, sürüm/kanıt | M0'dan itibaren |
| `docs/threat-model.md` | Tehditler, kontroller, açık güvenlik kapıları | M0 |
| `docs/roadmap.md` | M0–M6, bağımlılıklar ve kapsam değişimleri | M0 |
| `docs/platform-support.md` | Platform/format/auth bazlı tarihli test ve support matrisi | M0–M3 |
| `docs/testing.md` | Otomasyon, fixture, benchmark, manuel matris, kanıt biçimi | M0 |
| `docs/design-references.md` | Resmî kamu kaynakları, tarih, gözlem/öneri ayrımı | UI implementasyonu öncesi |
| `docs/ux-flows.md` | Altı kritik yolculuğun başarı/hata dalları | UI polish öncesi |
| `docs/ui-screen-specs.md` | Ekran anatomisi, durumlar, etkileşimler | UI polish öncesi |
| `docs/design-system.md` | Token, tipografi, spacing, component states, accessibility | UI polish öncesi |

**Önerilen ek teslimatlar:** `docs/release.md` (kurulum/update/rollback), `docs/data-format.md` (JSON şema), `docs/privacy.md` (veri akışı), lisans/NOTICE dosyaları ve secret içermeyen test artifact klasörü. Bu dosyalar bu SPEC ile oluşturulmuş sayılmaz; uygulama teslimatlarıdır.

### 27.1 Önerilen ilk dikey dilimler

1. **Temel iskelet:** Windows shell açılır; tipli ping/status komutu; SQLite migration ve fixture testleri çalışır.
2. **Offline bookmark:** Add URL → transaction → Inbox → restart → FTS arama uçtan uca.
3. **Yerel video:** Dosya seç → ffprobe → kayıt/thumbnail → yerel oynatma → restart.
4. **Düzenleme:** Not/etiket/koleksiyon/favori/arşiv → bulk/undo → anlık arama güncellemesi.
5. **Taşınabilirlik:** JSON export → temiz DB import → medya relink → arama.
6. **İndirme:** Fixture queue → izinli gerçek URL → merge/verify → playback → crash/cancel/retry.
7. **Dağıtım:** Installer, provenance/update, fresh-machine smoke ve uninstall.
8. **Sonraki aşamalar:** M3 auth, M4 transcript, M5 semantic/visual, M6 Android.

Her dilimin sonunda “uygulandı”, “otomatik test edildi”, “manuel doğrulandı” ve “henüz doğrulanmadı” ayrımı yapılır. Canlı erişim veya cihaz engeli varsa fixture testi ve uygulanabilir manuel adımlar hazırlanır; fiilî başarı iddiası üretilmez.

## 28. Açık kararlar ve doğrulama kayıtları

| Kimlik | Karar / belirsizlik | Çözüm kanıtı | Hedef aşama |
|---|---|---|---|
| DEC-01 | Kesin Tauri/Rust/Node/React/SQLite sürümleri | Resmî doküman, lockfile, Windows build/test | M0 |
| DEC-02 | yt-dlp ve JS runtime dağıtımı, güncelleme güven modeli | Binary provenance, checksum/imza, lisans, runtime uyumluluğu ve fixture parsing | M0–M2 |
| DEC-03 | FFmpeg build'i ve codec/lisans seçimi | Build konfigürasyonu, dağıtım yükümlülüğü ve format testleri | M0–M2 |
| DEC-04 | Tauri/WebView2 oynatma, güvenli local protocol, varsayılan indirme container'ı ve MKV remux önizlemesi | Container/codec matrisi (MKV/MP4/WebM/MOV, H.264/HEVC/VP9/AV1), remux süresi, seek ve origin/capability testleri | M0–M1 |
| DEC-05 | Dış araç alt isteklerinde SSRF/ağ politika enforcement | Redirect/manifest/DNS testleri ve documented network boundary | M0–M2 |
| DEC-06 | SQLite WAL/connection ve migration stratejisi | Crash/backup/concurrency testleri; FTS5 paket testi | M0 |
| DEC-07 | Dosya referansı/kopyalama ve disk kota varsayılanları | UX kararı, relink/cleanup ve sınırlı disk testleri | M1–M2 |
| DEC-08 | Oturum erişimi ve OS secret store yöntemi; Chrome app-bound encryption nedeniyle Firefox/çerez dosyası alternatifleri | Güncel browser/OS testi (VK-01…VK-10), redaction ve açık izin akışı | M3 |
| DEC-09 | VK sekmesinin açık olmasının etkisi | Aynı erişimli video ile VK-01/02/03 kontrollü gözlemleri | M3 |
| DEC-10 | Uygulamaya ait browser/session bridge uygulanabilirliği | Depo uyumluluğu, dar kapsam ve gerçek PoC | M3 araştırma |
| DEC-11 | Whisper implementation/model ve kaynak limitleri | Windows paketleme, model lisansı, TR/EN kalite/hız/RAM ölçümü | M4 |
| DEC-12 | Embedding/vector storage ve rank fusion | Bakım/lisans/uyumluluk ve retrieval benchmark | M5 |
| DEC-13 | OCR/görsel model ve kare örnekleme bütçesi | Gerçek medya kalite/kaynak değerlendirmesi | M5 |
| DEC-14 | Android bridge, background ve mağaza uygunluğu | Güncel resmî kaynak + gerçek cihaz/build testi | M6 |
| DEC-15 | Ürün lisansı/marka ve bağış hukuki durumu | Yetkili değerlendirme; bağış için ayrı açma kararı | Dağıtım / sonrası |
| DEC-16 | Installer signing ve updater altyapısı | Anahtar yönetimi, doğrulama/rollback, temiz kurulum | M2 |
| DEC-17 | Sayısal performans bütçeleri ve desteklenen cihaz tabanı | Sabit benchmark ortamı ve ölçüm raporu | M0–M4 |

Bu kayıtlar kullanıcıdan şimdi alınması gereken onay listesi değildir. Uygulama aşamasında teknik araştırmayla çözülebilecek kararlar belgelendirilir; özel hesaba erişim, model indirme, dış hizmet yapılandırması veya hukuki kabul gereken noktada ilgili gerçek izin alınır.

## 29. Başlıca riskler ve azaltma yaklaşımı

| Risk | Ürün etkisi | Azaltma |
|---|---|---|
| Platform extractor değişimi | Metadata/indirme geçici bozulur | Bookmark-first, yerel dosyalar, versioned adapter, dürüst support matrisi |
| Browser cookie koruma değişimi | Yetkili oturum okunamaz | Belgeli yöntem, açık teşhis, PoC gate, bookmark fallback |
| Chrome app-bound encryption (Windows) | `--cookies-from-browser chrome` Windows'ta çalışmayabilir; VK planının ana yolu kapanır | Firefox ve kullanıcı tarafından dışa aktarılan çerez dosyası alternatifleri, VK-09/VK-10 testleri, `COOKIE_ACCESS_FAILED` ile dürüst teşhis |
| yt-dlp'nin JS runtime ihtiyacı | YouTube metadata/indirme runtime olmadan başarısız olabilir | Manifestte yönetilen JS runtime, sürüm uyumluluk testi, `TOOL_MISSING` hatası |
| Antivirüs/SmartScreen engeli | `yt-dlp.exe` veya diğer araçlar karantinaya alınır ya da çalıştırılamaz | İmzalı/kaynağı belgeli binary, başlangıçta araç sağlık kontrolü, `TOOL_BLOCKED` hatası ve kullanıcıya güvenli açıklama |
| Yanlış auth teşhisi | Kullanıcı gereksiz login yapar veya erişim var sanır | Belirsizliği koruyan hata eşleme, session/access ayrımı |
| Dış araç ağ kontrolü eksikliği | SSRF ve özel ağa istek | M0 enforcement kapısı; doğrulanmayan genel extractor'ı kapalı tutma |
| Codec/container farkları | Dosya import edilir ama oynatılamaz | Gerçek playback matrisi, özgün dosya koruma, açık unsupported durumu |
| Crash/çift worker | Kısmi veya duplicate çıktı, veri tutarsızlığı | Lease/generation, idempotency, staging ve başlangıç uzlaştırması |
| Disk/RAM tüketimi | Kullanıcı işi kesilir, cihaz yavaşlar | Kota/uyarı, concurrency, opt-in AI, kare/model bütçeleri |
| Veri taşıma ve cleanup hatası | Not veya medya kaybı | Ayrı silme kapsamları, round-trip, backup/restore, relink |
| AI yanlış eşleşme/özet | Kullanıcı güveni zedelenir | Kaynak/zaman kanıtı, model sürümü, edit edilebilir öneri, yetersiz kanıtta çekimserlik |
| Kapsam büyümesi | Temel ürün gecikir | M0–M2 çıkış sınırı, gerçek dikey dilimler, M3–M6 ayrı kapılar |
| Lisans/paketleme belirsizliği | Dağıtım engellenir | M0 araştırma, manifest/NOTICE, dağıtım öncesi kontrol |

## 30. Kaynak izlenebilirliği

| Kaynak prompt bölümü | Bu SPEC'teki karşılık |
|---|---|
| §0 Vizyon ve temel ilkeler | §§2–5, 20, 26 |
| §1 Çalışma biçimi | §§1, 25, 27–28 |
| §2 Mimari ve teknoloji | §§6, 10, 13–18, 23–24 |
| §3 Domain model | §§11–12, 15–16 |
| §4 MVP yolculukları | §§7–8, 25–26 |
| §5 Authentication/VK | §§12, 19–21, DEC-08/09/10 |
| §6 AI roadmap | §§16–17, 22, 26.2 |
| §7 UI/UX | §9, §§23–24 |
| §7A Tasarım araştırması ve ekranlar | §§8–9, 25, 27 |
| §8 Güvenlik/gizlilik/uygunluk | §§14–15, 19–21, 23 |
| §9 Test/kabul | §§22, 25–26 |
| §10 Execution phases | §§3, 27–28 |
| §11 Başlangıç ve raporlama | §1 (bu çalışmanın sınırı), §27 (uygulama teslim protokolü) |

SPEC değişikliklerinde kapsam, etkilenen gereksinim/kabul kriteri ve ADR bağı güncellenmelidir. Kaynakta olmayan önerilerin ürün kararı hâline gelmesi `docs/decisions.md` içinde kaydedilir; doğrulama bekleyen özellikler kanıt olmadan “destekleniyor” durumuna taşınmaz.

## 31. Değişiklik geçmişi

| Sürüm | Tarih | Değişiklik |
|---|---|---|
| 1.0 | 6 Kasım 2026 | İlk spesifikasyon |
| 1.1 | 6 Kasım 2026 | Review bulguları işlendi. Chrome app-bound şifreleme riski ve VK alternatifleri (§21.1, VK-09/VK-10); yt-dlp JS runtime bağımlılığı (§10.2, §14.1, §23.1); oynatılabilir varsayılan indirme formatı ve MKV remux önizlemesi (PLAY-07); erişim durumunun öğe ve profil katmanlarına ayrılması, hata kodu eşleme tablosu (§12.1); soft-delete ve Undo semantiği (§7.3, §14.4); single-instance (INV-13, §13.2); uzak metadata'nın M2'ye bağlanması (§3); `user_overrides`, `removed_at`, `normalization_version`, `published_precision` alanları; ilk açılış akışı (§9.7); yeni gereksinimler ORG-09/10, CAP-09, DATA-08…11; AI-B ve AI-C kimlikleri; yeni hata kodları; AC-29…AC-42; M3, M5 ve M6 çıkış kapıları. |
| 1.2 | 6 Kasım 2026 | `SPEC_ANALIZ_RAPORU.md` bulguları işlendi. `StorageRoot` modeli ve `storage_root_id` (Y-01); `storage_move` iş sözleşmesi ve adım bazlı kurtarma (Y-02); normalizasyon çakışma kuralı, `DuplicateCandidate`, ORG-11 öğe birleştirme (Y-03); kaldırılmış öğenin URL'sinin yeniden kaydı ve kaldırmada işlerin iptali (Y-04); türev varlıklar için `source_asset_id`, `derivation` ve geçersizleştirme kuralları (Y-05); `published_precision` (O-01); CAP-04 offline netleştirmesi (O-02); Undo kapsamına etiket rename/merge (O-03); `OrphanMedia` (O-04); `start_segment_id`/`end_segment_id` (O-05); `AIProposal` ve `LibraryItemTag.origin` (O-06); ORG-10 önceliği P1'e hizalandı (O-07); kapsama matrisi kuralı ve AC-43…AC-53 (O-08); AC-30/AC-36 ayrıştırıldı (O-09, O-10); M3 kapanış sonuçları (O-11); sıralama sözleşmesi, export referans biçimi, 200% ölçek zorunluluğu ve komut yüzeyi (D-01…D-04). |
| 1.3 | 6 Kasım 2026 | `SPEC_REVIEW_ASTRA.md` bulguları işlendi (rapor v1.1'i incelemişti; v1.2'de kapanmış olanlar dışındakiler). Kimlik doğrulanmış erişim kanıtı kuralı ve eşleme tablosunun düzeltilmesi (#1); `--cookies` için iş başına çalışma kopyası ve temizlik (#3); sonradan öğrenilen `source_id` çakışması, `UrlAlias` (#4); MKV hükmünün M0 testine bağlanması (#5); otomatik temizlik ile onay kuralının ayrımı ve restore sonrası saklama sayacı (#6); `cancel_reason` ve bağımlı iş yeniden deneme kuralı (#7); FFmpeg/ffprobe protokol izin listesi ve referanslı girdilerin reddi, `INPUT_REJECTED` (#8); Offline Mode ölçüm sınırı ve AC-17 hizalaması (#9); tarih-only yayın değeri için `published_date` (#10); Undo geçerlilik kuralı (#11); AC-54…AC-60. |
| 1.4 | 6 Kasım 2026 | `SPEC_REVIEW.md` incelendi. Raporun Y-01…D-04 bulguları `SPEC_ANALIZ_RAPORU.md` ile aynıdır ve v1.2'de işlenmişti. Yeni olan D-05 işlendi: §25.2'ye Türkçe arama fixture'ı eklendi ve §16.1 buna bağlandı. |
| 1.5 | 7 Kasım 2026 | §11.7 normalizasyon çakışması: örnekte B'nin eski değeri yeni değerden farklı yapıldı; değeri zaten taşıyan öğenin saklı değeri yeni değere eşitse onun koruduğu istisnası eklendi (F1.3 uygulamasında bulundu, ADR-0011). |
