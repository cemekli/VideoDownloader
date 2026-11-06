# VideoVault SPEC.md — Kapsamlı İnceleme Raporu

**Belge:** `SPEC.md`, sürüm 1.1 (6 Kasım 2026)  
**Görev:** Eksiklik, yanlışlık, tutarsızlık ve belirsizlik tespiti. `SPEC.md` değiştirilmedi.
**Önemli not:** `DEC-01…DEC-17` ile bilinçli olarak ertelendiği yazılmış doğrulama kararları (sürüm pin, platform davranışı, ölçüm) tek başına hata sayılmadı. Dış platformlar/araçlar bu incelemede canlı test edilmedi.

---

## 0. Özet tablo

| # | Başlık | Tür | Seviye | İlgili bölüm |
|---|---|---|---|---|
| Y-01 | Medya kökü çoklu kökü/bağlantı durumunu temsil edemiyor | Veri modeli eksikliği | Yüksek | §11.2, §11.6, §14.2 |
| Y-02 | Medya taşımanın crash/atomiklik sözleşmesi belirsiz | Belirsizlik | Yüksek | §7.3, §13, §14.3 |
| Y-03 | Canonical URL çakışmasının kalıcı temsili yok | Eksiklik/yanlışlık riski | Yüksek | §7.1, §11.7 |
| Y-04 | Soft-delete sonrası aynı URL'yi yeniden kaydetme tanımsız | Eksiklik | Yüksek | §7.1, §11.7 |
| Y-05 | Türetilmiş varlıkların kaynak varlık bağlantısı eksik | Veri modeli eksikliği | Yüksek | §11.3, §16.2 |
| O-01 | `published_precision` zorunlu kuraldır ama `LibraryItem`'da yok | Eksiklik | Orta | §11.1, §11.2 |
| O-02 | CAP-04 `offline` durumu `metadata_status` enum'unda yok | Yanlışlık/tutarsızlık | Orta | §7.1, §11.2 |
| O-03 | Etiket merge Undo semantiği ile çelişiyor | Tutarsızlık | Orta | §7.3, §7.3 Undo |
| O-04 | “Sahipsiz medya” listesinin üretim yolu tanımsız | Eksiklik | Orta | §7.3, §14.4 |
| O-05 | Çok segmentli chunk için tek `segment_id` yetmiyor | Veri modeli tutarsızlığı | Orta | §11.5, §16.2 |
| O-06 | AI etiket önerisi kabul/ret/provenance modeli yok | Eksiklik | Orta | §11.4, §17.3 |
| O-07 | Alt koleksiyon silme P0, ağaç P1; hiyerarşi kapsamı net değil | Öncelik tutarsızlığı | Orta | §7.3 |
| O-08 | Bazı P0 gereksinimlerin doğrudan kabul senaryosu yok | Test boşluğu | Orta | §7 – §25.3, §26 |
| O-09 | AC-30 beklenen sonucu iki davranışı karıştırıyor | Test oracle belirsizliği | Orta | §25.3 |
| O-10 | AC-36 bazı hassas URL'lerin reddedilmesi ile çelişiyor | Test önkoşulu | Orta | §7.1, §25.3 |
| O-11 | M3 çıkış ölçütü “PoC” mu “destek” mi net değil | Milestone belirsizliği | Orta | §3, §26.3 |
| D-01 | Sıralama sözleşmesi tanımsız | Belirsizlik | Düşük | §9.2, §16.1 |
| D-02 | Dış referansların export davranışı kesin değil | Belirsizlik | Düşük | §7.7, §15.1 |
| D-03 | %200 metin ölçeği zorunluluk seviyesi tutarsız | Tutarsızlık | Düşük | §9.4, §25.3 |
| D-04 | Zorunlu domain komutlarının bir kısmı komut tablosunda yok | Eksiklik | Düşük | §18.1 |
| D-05 | “Ğ”/Türkçe normalizasyon testinin fixture kapsamı belirsiz | Belirsizlik | Düşük | §16.1, §25.2 |

---

## 1. Yüksek seviye bulgular (uygulamadan önce çözülmeli)

### Y-01 — Medya kökü tek kök + relative yol modeline sığmıyor

**İlgili yerler:** `MediaAsset.storage_ref` (§11.2/11.3-523), `AppSettings` (§11.6-575), DATA-08/DATA-09 (§7.7-255/256), depolama düzeni (§14.2-758).

- Managed varlıkların yolu “kullanıcı köküne göre relative” tanımlı. Ancak varlıkta hangi köke ait olduğunu gösteren yabancı anahtar yok.
- DATA-08'in “yalnızca yeni indirmelerde kullan” seçeneği, tek aktif kök ayarıyla temsil edilemez: eski varlıklar eski kökte, yeniler yeni kökte kalır.
- DATA-09'un istediği kök düzeyinde “bağlı değil” / otomatik doğrulama durumu için bir `StorageRoot` varlığı tanımlı değil; bu, öğelerin gereksiz yere tek tek `missing` işaretlenmesini önlemenin veri tarafını eksik bırakıyor.

**Öneri:** `StorageRoot { id, path, kind, status, last_verified_at }` + `MediaAsset.storage_root_id` + ayrı `active_download_root_id` ayarı. Kök durumları (`unknown | available | disconnected | inaccessible`) ve export/import'ta kök eşleme kuralı tanımlanmalı.

---

### Y-02 — Medya kökü taşımanın kesinti davranışı tanımlı değil

**İlgili yerler:** DATA-08 (§7.7-255), iş türleri (§13.1-678), finalizasyon (§14.3-774), AC senaryoları.

- “Kopyala–doğrula–eskiyi sil” ile “kesintide yarım kalmamalı” birlikte kullanılıyor, ancak çok dosyalı taşımada dosya sistemi + SQLite tamamen atomik olamaz.
- Kesinti sonrası bazı varlıkların yeni, bazılarının eski kökte olup olmayacağı; DB referansının ne zaman değişeceği; eski dosyanın ne zaman silineceği ve işin nasıl sürdürüleceği yazılı değil.
- §13.1 iş türleri listesinde kalıcı `storage_move` işi yok.

**Öneri:** Ölçülebilir garanti yazın (ör. her varlıkta kopyala→doğrula→DB referansını değiştir→eski kopyayı temizle; idempotent checkpoint; tek doğrulanmış kopyanın kaybolmaması). `storage_move` iş türü ve kesintili taşıma için kabul senaryosu ekleyin. Taşıma sırasında download/play/delete/export davranışını netleştirin.

---

### Y-03 — Canonical URL migration çakışmasının temsili tanımsız

**İlgili yerler:** CAP-06 (§7.1-154), `canonical_url`/`normalization_version` (§11.2-493/494), unique/migration kuralları (§11.7-582/583).

- §11.7, çakışmada “eski URL korunur, iki öğe duplicate adayı olarak işaretlenir” diyor ama duplicate adayının nerede tutulacağı, hangi satırın yeni değeri alacağı, diğerinin `canonical_url`/`normalization_version` değerinin ne olacağı belirtilmemiş.
- CAP-06 yönlendirmesi (hangi öğeye git/git) tanımsız; unique constraint ihlali bu noktada migration'ı da etkileyeceğinden “migration tamamlandı” tanımı da belirsiz.

**Öneri:** `normalization_conflict` durumu + `DuplicateCandidate` kaydı; eski canonical değeri koruma; çakışma çözülmeden capture/metadata/export davranışı; kullanıcının iki kaydı bilinçli birleştirmesi için ayrı sözleşme.

---

### Y-04 — Soft-delete sonrası aynı URL'yi kaydetme tanımsız

**İlgili yerler:** CAP-06 (§7.1-154), soft-delete (§7.3-192), `removed_at` (§11.2-508), unique (§11.7-582).

- Soft-delete edilen öğe görünmez ama canonical URL unique kaydı kalıyor. Aynı URL tekrar girildiğinde CAP-06 tek öğeye yönlendirir; bu öğe removed olduğundan davranış belirsiz: otomatik geri yükleme mi, geri yükleme önerisi mi, yoksa yeni kayıt mı (yeni kayıt unique kuralıyla çelişir)?

**Öneri:** Capture akışına “Recently removed'da mevcut; geri yükle / kalıcı sil / kaldı olarak bırak” dalı ekleyin; üstüne değerlerin korunması, ilişkilerin geri dönüşü, aktif işlerin davranışı ve bir kabul senaryosu tanımlayın.

---

### Y-05 — Türetilmiş medya varlıklarının kaynak bağlantısı yok

**İlgili yerler:** `MediaAsset` (§11.3-519/533), `Transcript` (§11.5-553), bağımlılık zinciri (§13.1-686), stale kuralı (§11.3-534), §16.2-853.

- `MediaAsset` yalnızca `library_item_id` ve `source_fingerprint` taşıyor. Bir öğede birden fazla sürüm/özgün medya olduğunda `preview`, `thumbnail`, `extracted_audio`, `sampled_frame` varlığının hangi `MediaAsset.id` üzerinden üretildiği kesin değil.
- `source_fingerprint` yardımcı olsa da kaynak kayıt ilişkisini ve üretim ayarlarını tek başına karşılamıyor; özellikle “özgün medya değişirse eski transkriptin sessizce bağlanmaması” kuralının implementasyonu maliyetli/hataya açık.

**Öneri:** Türetilmiş varlıklara zorunlu `source_asset_id` + `derivation_config` + `generator/provider_version`. Özgün değişince hangi türevlerin `stale` olacağı ve cascade kuralı; `Transcript.asset_id` zorunluluk koşulları.

---

## 2. Orta seviye bulgular

### O-01 — `published_precision` modelde yok (§11.1-481 vs §11.2)
§11.1 zorunlu kural olarak tanımlıyor, §11.2 alan listesinde yok. Override ile birlikte davranışı da belirsiz. → Alanı ekleyin, override yapısıyla ilişkisini netleştirin.

### O-02 — `offline` ve `metadata_status` uyuşmuyor (§7.1-152 vs §11.2-504)
CAP-04 `pending/failed/offline` diyor, enum `pending|fetching|ready|partial|failed`. Offline kalıcı bir değer mi yoksa global Offline Mode'dan türetilen görüntü mü belli değil. → Kalıcı enum'u şişirmeden UI eşlemesi tanımlayın; Offline kapanınca işin hangi koşulda yeniden kuyruğa gireceğini yazın.

### O-03 — Etiket merge vs Undo (§7.3 ORG-09-187, Undo-194; AC-39-1210)
ORG-09 merge'i Undo kapsamına alıyor; genel Undo listesi yalnızca ekleme-çıkarma/favori/arşiv/kaldırmayı kapsıyor. AC-39 iki adım (rename + merge) yapıp “Undo çalışır” diyor; ikinci değişiklik ilk Undo'yu geçersiz kıldığından tek Undo'nun neyi geri alacağı belirsiz. → Undo kapsamına rename/merge'i ekleyin veya AC-39'u ikiye bölüp composite transaction olarak tanımlayın.

### O-04 — “Sahipsiz medya” üretim yolu tanımsız (§7.3-192, §14.4-786)
Kayıt silinip medya korunuyor; listenin bir tablodan mı, disk taramasından mı üretileceği; ne zaman tarandığı; aktif işlerin nasıl ele alınacağı belirtilmemiş. → `OrphanMedia` kaydı veya deterministik tarama sözleşmesi seçin; AC-37'yi genişletin.

### O-05 — Çok segmentli chunk tek `segment_id` ile çelişiyor (§11.5-555 vs §16.2-851)
Chunk komşu segmentleri birleştirebiliyor ve ilk/son segmente referans vermeli diyor; modelde tek `segment_id?` var. → `start_segment_id`/`end_segment_id` veya ilişki tablosu.

### O-06 — AI etiket önerisi lifecycle modeli yok (§11.4-536 vs §17.3 AI-B01/02)
Kabul/ret + provenance zorunlu kılınıyor, `Tag`'de karşılığı yok. → `AIProposal` tipinde model; ret sonrası yeniden öneri politikası; OCR/görsel sonuçlar için provider/model provenance alanları.

### O-07 — Alt koleksiyon silme P0 ama ağaç P1 (§7.3 ORG-08-186, ORG-10-188)
Alt koleksiyonla silme P0; hiyerarşi oluşturma/taşıma P1. → Öncelikleri hizalayın veya M1'de yalnızca düz koleksiyon olduğunu açıkça yazın.

### O-08 — P0 gereksinimlerde kabul senaryosu boşlukları (§7, §25.3, §26)
Doğrudan AC karşılığı olmayan P0'lar:
- **DATA-08** taşıma / kesintiden devam (AC yok).
- **DATA-09** sürücü çıkarıldığında öğelerin tek tek `missing` olmaması ve geri gelince doğrulama (AC yok).
- **ORG-10** alt koleksiyonları taşı/birlikte sil (AC-06 yalnızca genel silmeyi sınıyor).
- **PLAY-02** izleme konumu restart sonrası geri yükleme (AC yok).
- **§15.3** tutarlı tam DB backup/restore + migration öncesi yedekten dönüş (AC-15 yalnızca JSON round-trip).
- **§7.3** korunan managed dosyanın “Sahipsiz medya”dan yeniden import'u (AC yok).

→ Her P0 için en az bir AC veya manuel test kimliği bağlayın; gereksinim–test kapsama matrisi üretin.

### O-09 — AC-30 beklenen sonucu çelişkili (§25.3-1201)
“Referans reddedilir **veya** dış referans olarak işaretlenir” ile “mevcut kütüphane değişmez” aynı oracle'da. → İki ayrı test: (a) izinsiz yol tam red + değişiklik yok; (b) onaylı geçerli external reference kontrollü import; her ikisinde kök dışına yazılmaması ayrı doğrulanır.

### O-10 — AC-36 bazı hassas URL'lerin reddiyle çelişiyor (§7.1-159 vs §25.3-1207)
§7.1 güvenli canonical'a dönüştürülemeyen sırlı URL'yi reddederken AC-36 imzalı URL'yi “kaydet ve hata üret” olarak varsayıyor. → AC-36'yı güvenli canonical üretilebilen girdiyle sınırlayın; dönüştürülemeyen sır için ayrı `URL_HAS_CREDENTIALS` + kayıt oluşmaması testi ekleyin.

### O-11 — M3 çıkış ölçütü belirsiz (§3-69, §26.3-1251)
M3 “PoC”. Çıkış kapısı tüm canlı yolların `not run` olmasına izin veriyor; bütün yollar başarısız olsa bile M3'ün kapanıp kapanamayacağı net değil. → “M3 araştırması tamamlandı” ile “oturum özelliği destekleniyor”u ayırın; destekli saymak için en az bir yöntemde gerçek yetkili VK başarısı zorunlu olsun.

---

## 3. Düşük seviye / netleştirme bulguları

### D-01 — Sıralama sözleşmesi tanımsız (§9.2-338, §16.1-847)
Üst çubuk sıralama istiyor ama alanlar/yön/null sıralaması/kalıcılık/kararlı tie-break belirtilmemiş.

### D-02 — Dış referansların export davranışı (§7.7, §15.1-819)
Absolute path export'ta “mümkün olduğunca” dışarıda bırakılıyor; yol çıkarılınca hangi alanların kalacağı ve restore önizlemesi netleştirilmeli.

### D-03 — %200 ölçek zorunluluğu (§9.4-374, §25.3 AC-27)
§9.4 “mümkün olduğunca”, AC-27 zorunlu sonuç. Seviye aynı yapılmalı; teknik istisna varsa sınırlandırılmalı.

### D-04 — Zorunlu komutların eksikliği (§18.1-911)
Rename/delete collection, tag merge, Undo/restore, orphan management, storage move gibi zorunlu işlemlerin komut grubu karşılığı görünmüyor. (Tablo örnek niteliğinde ama hangi grup ele alacak belirtilmeli.)

### D-05 — Türkçe `ı/I/İ` normalizasyon fixture kapsamı (§16.1-845, §25.2)
Normalizasyon test edilecek deniyor; §25.2 fixture'ında bu varyantların bulunup bulunmadığı açık değil.

---

## 4. Doğrulandı / yanlışlık olmayan alanlar (false positive'leri önlemek için)

Bu maddeleri denetledim; raporun diğer bölümlerindeki “eksik/hatalı” nitelemeleriyle karıştırılmaması için buraya aldım:

- **kind → remote_video geçişi:** §11.2 “Bookmark-first → doğrulanmış medya → remote_video” kuralı §2.1 ve INV-02 ile tutarlı. `kind` ile indirilmiş durumu karıştırılmaması doğru vurgulanmış.
- **Ayrı durum makineleri (§12):** Öğe/profil/iş katmanı ayrımı §11 alanlarıyla tutarlı; `access_state` enum'ları §12.1 ve §19 eşlemesiyle örtüşüyor. `access_state`'te `not_connected` olmaması bilinçli (profil katmanına ayrıldığı §12.1'de açıklanmış) — bug değil.
- **Hata kodu → durum eşlemesi (§12.1):** `SESSION_EXPIRED` öğeyi `login_required` + profili `expired` yapması; `ACCESS_DENIED`'in profili değiştirmemesi; `MEDIA_URL_EXPIRED`'ın oturumla karıştırılmaması §19 ve risk tablosuyla tutarlı.
- **Offline Mode (DATA-05 AC-17):** İlinti olarak İnternet kopması ile bilinçli Offline Mode ayrımı §7.7 ve §8.6'da tutarlı.
- **Immutability (INV-01…INV-13):** §13.1/13.2 (lease, generation, idempotency, single-instance, process kill) bu değişmezlerle çelişmiyor.
- **PLAY-07 remux önizlemesi:** §14.2'deki cache kapsamı, §14.4 cache temizleme ve DEC-04 ile tutarlı; MKV import P0 fakat oynatma matrisi gerçek dosyayla ölçülene kadar iddia yok — doğru.
- **yt-dlp JS runtime (§10.2/14.1/23.1):** Manifest güven modeli, TOOL_MISSING davranışı ve bookmark koruma kuralları birbiriyle tutarlı.
- **Yerelleştirme/hata (§9.6, §19):** Backend kodu + UI mesajı çözümü ve “yonelleyici belirsiz hata dönerse daha kesin açıklama uydurulmaz” kuralı tutarlı.

---

## 5. Önerilen düzeltme sırası

1. **Veri bütünlüğü:** Y-01, Y-02, Y-03, Y-04, Y-05 → `StorageRoot`, kimlik çakışması, varlık soy ağacı kararlarını ADR adayı yapın.
2. **Sözleşme çelişkileri:** O-01, O-02, O-03, O-05 metin/alan düzeyinde.
3. **Yaşam döngüsü:** O-04, O-06, O-07.
4. **Kabul testleri:** O-08…O-11 için yeni scenario + net önkoşullar.
5. **Netleştirme:** D-01…D-05.

---

## 6. Sonuç

`SPEC.md` ürün/güvenlik çerçevesi olarak güçlü ve büyük ölçüde uygulanabilir durumda. Uygulama öncesi kritik boşluklar **depolama kökü modeli, medya taşıma toparlanması, canonical kimlik çakışması ve türetilmiş varlık ilişkileri** üzerinde yoğunlaşıyor. Bunlar 1.2 sözleşme netleştirme revizyonuyla kapatıldığında, farklı geliştiricilerin birbiriyle uyumsuz implementasyon üretme riski belirgin biçimde düşecektir.