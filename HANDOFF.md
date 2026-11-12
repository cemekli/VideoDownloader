# Devir notu — 12 Kasım 2026

Projenin ayrıntılı durumu [STATE.md](./STATE.md) dosyasındadır. Bu dosya kaldığımız yeri ve ilk adımları özetler; bir sonraki oturum başlarken okunur, iş ilerledikçe güncellenir.

## Nerede kaldık

- **12 Kas: F6.4 yazıldı (🟨, ADR-0030): dökümde arama.** Arama kutusuna videoda söylenen bir kelime yazınca öğe bulunur. Kartta ilk an zaman koduyla görünür (`0:03 … Kadıköy …`). Details'te "Said in the video" bölümü anları listeler; dosyası olan öğede "Play from 0:12" oynatıcıyı o segmentin başından açar. Döküm aramadan sonra değiştiyse uyarı verir. Aynı PR'da F6.5 (🟨, ADR-0031): dökümü olan öğede oynatıcının yanında döküm paneli açılır; o an söylenen satır işaretlenir, zaman koduna tıklayınca video oradan oynar; satırlar seçilip "Export note…" ile Markdown not (önizleme, sonra "Save as…") kaydedilir. Sıradaki: F6.6 (kaynak sınırları ve AI Processing ekranı) ve F6.7 kalite raporu.
- **11 Kas akşam: F6.2 bitti (✅).** Cuma yerel transkripsiyonu Windows'ta 21:44'lük bir videoda dört modelle çalıştırdı. Süreler: Tiny 0:49, Base 1:20, Small 3:29, Medium 10:29 (CPU). PR #49 ile iş sürerken model adı, geçen/kalan süre ve bitince "Last run: …, took …" görünüyor. Kalite karşılaştırması (F6.7) henüz yapılmadı. F6.3 yazıldı (ADR-0029), Cuma'nın testini bekliyor; sonra F6.4–F6.5 (arama, panel, zaman koduna gitme).
- **11 Kas: F6.2 ikinci yarı yazıldı (🟨, ADR-0028): yerel transkripsiyon.** Details → Transcript → "Transcribe on this PC": dil (otomatik/Türkçe/İngilizce), model (indirilmiş olanlar, varsayılan en büyük), ilerleme yüzdesi, iptal. İş: FFmpeg öğenin dosyasını 16 kHz mono WAV yapar → `whisper-cli` (CPU) JSON yazar → `generated_local` döküm. Çevrimdışı çalışır. Dosyası olmayan öğede düğme yok.
  - Paket bulguları: whisper.cpp v1.9.2 `whisper-bin-x64.zip` (8 194 445 bayt, SHA-256 `49dcc16de826f20bd53d44f947a1ae49dfa81f86cad67a64d80820cb192d674a`). `whisper-cli.exe` yanında `whisper.dll`, `ggml.dll`, `ggml-base.dll` ve 9 `ggml-cpu-*.dll` gerekir (CPU'ya göre biri yüklenir); hepsi `tools/manifest.json` → `companions` altında hash'li. SDL2/parakeet alınmaz.
  - İkili Visual C++ çalışma zamanını ister (MSVCP140, VCRUNTIME140, VCOMP140); zip'te yok. Cuma'nın makinesinde (Visual Studio Build Tools kurulu) ve GitHub Windows imajında var. Yoksa Diagnostics'te whisper-cli "Missing" görünür. F4 kurulumu bunu çözmeli.
  - whisper.cpp Linux ikilisi yayımlamıyor: araç isteğe bağlı, Linux'ta "Not available on this system"; Linux CI sahte `whisper-cli` (`crates/vv-engine/src/bin/vv-fake-whisper.rs`) kullanır. Windows CI gerçek ikiliyi Tiny modelle bir kez çalıştırır (Hugging Face erişilebilirse).
- **10 Kas: F6 başladı (transkript ve zaman kodlu arama).** Plan beş PR: (1) veri modeli + izinli model indirme (F6.1), (2) site altyazısı + yerel whisper.cpp (F6.2), (3) uzun ses parçalama, checkpoint, iptal (F6.3), (4) transkript araması, panel, zaman koduna gitme, Markdown not (F6.4–F6.5), (5) kaynak sınırları ve AI Processing ekranı (F6.6). F6.7 ölçüm raporu Cuma'nın Windows'ta TR/EN denemesiyle.
- **F6.1 bitti (✅, ADR-0026, PR #43).** Settings → Transcription: modeller (Tiny/Base/Small/Medium, çok dilli), izin penceresi, Hugging Face'ten sabit commit, SHA-256 + SHA-1 kontrolü. Cuma Windows'ta test etti, çalışıyor (10 Kas).
- **F6.2 ilk yarı yazıldı (🟨, ADR-0027): sitenin altyazısı.** Details panelinde "Transcript" bölümü → "Get site captions". Uygulama sitenin altyazı izlerini okur, videonun dilindeki yükleyici altyazısını (yoksa sitenin otomatik altyazısını) alır ve zaman kodlu döküm olarak saklar; başka dile otomatik çeviriler alınmaz. Sonraki PR: whisper.cpp ile yerel transkripsiyon (altyazısı olmayan videolar ve yerel dosyalar).
- **10 Kas akşamüstü: F5 bitti, sıradaki F6.** Instagram'da herkese açık olmayan reel'ler oturumla iniyor (PR #39). Uygulama içi giriş çalışıyor (PR #40–#42): robot kontrolü ve Google girişi artık pencerede açılıyor; `freezePrototype` uygulamanın kendi sayfasına taşındı (`public/freeze-prototype.js`). VK takipçilere özel/özel video ertelendi (`docs/platform-support.md`).

- **10 Kas: VK takipçilere özel video ertelendi (Cuma'nın kararı).** yt-dlp tek başına da çerezle `Video only available to followers` diyor; VK giriş çerezleri yalnızca `vkvideo.ru`'da. Bilinen kısıt olarak `docs/platform-support.md`'ye yazıldı; VK-01…10 not run. F5.3 ✅ için gerçek çerez dosyası testi Reddit cookies.txt ile yapılabilir. Sıradaki aday F5.7 (uygulama içi giriş PoC).
- **10 Kas: F5.7 uygulama içi giriş yazıldı (🟨, ADR-0025).** Sessions → Connect → "Sign in here, in a VideoVault window": özel bir pencerede sitenin giriş sayfası açılır, Cuma giriş yapar, "Done, I'm signed in" ile yalnızca o sitenin çerezleri saklanır. Bekleyen: Cuma'nın Windows'ta gerçek testi (Reddit, sonra VK takipçilere özel video, VK-04).

- **9 Kas akşamı, Cuma'nın VK çerez dosyası testinde durduk (PR #37, açık):** PR #37'de üç düzeltme var:
  - Check now düğmesi izin yokken de görünür (devre dışı ve açıklamalı).
  - Sessions ve Downloads ekranları pencereye sığmayınca kayar; daha önce Facebook satırı yarıda kesiliyordu.
  - VK'nın `Video only available to followers` / "sadece topluluk üyelerine" hataları artık `LOGIN_REQUIRED` sayılıyor. Önceden `EXTRACTOR_FAILED` ("The video couldn't be read from the page") oluyordu, bu yüzden çerezle yeniden deneme hiç çalışmıyordu.

  Bulgular:
  - VK test videosu `vk.com/video-159700045_456248459` sadece takipçilere açık. yt-dlp tek başına çerezsiz aynı hatayı veriyor.
  - Çerez dosyasıyla bağlanan ilk VK videosu herkese açıkmış. Metadata girişsiz geldiği için Check now "Nothing to check with yet" dedi; bu doğru davranış (AC-54).
  - O videonun küçük resmi gelmedi; bu henüz incelenmedi.

- **F5.3–F5.5 (9 Kas, PR #36 merge edildi; F5.4 ve F5.5 ✅, Cuma'nın Reddit'te Check now testi):** çerez dosyasıyla oturum (yalnızca platformun çerezleri DPAPI'de; her iş için kullanıcıya özel klasörde kopya, iş bitince silinir), `verify_session` ("Check now", §12.1 (a) kanıtı), `ACCESS_DENIED` de kanıt sayılır ve profile dokunmaz (AC-19), kenar çubuğunda Sessions ekranı. ADR-0024. Bekleyen yalnızca F5.3'ün gerçek çerez dosyası testi (VK-10).
- **F5.2 (9 Kas, PR #35, merge edildi, ✅):** Firefox çereziyle oturum. Settings → Sessions'ta site + Firefox profili + izin; giriş bekleyen indirmede "Connect a session". Motor önce profilsiz dener, `LOGIN_REQUIRED` olursa izinli profille yeniden dener (ADR-0023). Cuma Reddit gönderisini Firefox oturumuyla indirip oynattı.
- **İlk açılış gecikmesi çözüldü (9 Kas):** taze derlemeden sonra thumbnail'lerin geç gelmesi ve pencerenin bir kez küçülmesi Avast Antivirus'ün taramasıymış; Avast kapalıyken hemen geliyor. Kodda değişiklik gerekmiyor; F4.7 bilinen kısıtlarına not düşülecek.
- **F5.1 (9 Kas, PR #32, merge edildi):** `crates/vv-secrets` (DPAPI secret store), migration 0012 `auth_profile`, `vv_persistence::auth` (izin kapsamı, kanıt kuralı, clear/delete), canary testi `crates/vv-persistence/tests/auth.rs`, ADR-0022. Arayüz ve masaüstü komutu yok; bunlar F5.2 ile gelir.
- **Son durum (9 Kas, akşam):** F3 tamamlandı (PR #25–#30). Cuma Windows 11'de YouTube, Facebook, Instagram, TikTok, X, VK ve Twitch VOD ve Dailymotion indirdi; sonuçlar `docs/platform-support.md`'de. Reddit girişsiz engelli (yt-dlp tek başına da `Account authentication is required` diyor).
- **Tamamlananlar:** F0, F1, F2, F3, F5.1, F5.2, F5.4, F5.5, F6.1, F6.2 (47/74).
- **Sıra değişikliği:** F4 (installer, imzalı güncelleme) ertelendi; sıradaki faz **F5 yetkili oturum** (Cuma'nın kararı, 9 Kas).
- **Uygulamada şu an:** bookmark kaydedilince başlık, kanal, süre ve küçük resim gelir; Download ile dört önayar; Downloads ekranı; canlı yayınlar reddedilir; yerel import thumbnail'leri arka planda, en yeniden başlayarak, paralel yapılır. Bilinen platformlar: YouTube, Facebook, Instagram, TikTok, X, VK, Dailymotion, Twitch, Reddit.

## Sonraki ilk adımlar

0. **F6.4 elle test (Cuma, release exe).** Dökümü olan bir videoda (site altyazısı ya da yerel) dökümde geçen bir kelimeyi arama kutusuna yaz. Kart bulunmalı ve satırda zaman kodu görünmeli. Kartı aç → Details → "Said in the video" → bir ana tıkla: video o saniyeden başlamalı (±1 sn). Türkçe bir kelimeyi büyük harfle (ör. `ISIK`) ara: yine bulmalı.
0. **Zaman kayması düzeltmesi (ADR-0032) elle test (Cuma, release exe).** Merge'den sonra 12 Kas'ta denenen İngilizce videoyu aynı modelle yeniden dök (Transcribe on this PC). İlk satır konuşmanın başladığı saniyede (yaklaşık 0:05) olmalı; panelde satırlara tıklayınca ses ile metin arasındaki fark yarım saniyeyi geçmemeli. Dökümün eskisinden belirgin yavaş olup olmadığını not et.
0. **F6.5 elle test (Cuma, release exe).** Dökümü olan videoyu oynat: sağda döküm paneli açılmalı, o an söylenen satır işaretli olmalı. Bir satırın zaman koduna tıkla: video o satırın başından oynamalı; sesle metnin farkı yarım saniyeyi geçmemeli (NFR-10). İki satır seç → "Export note…" → önizleme → "Save as…": `.md` dosyası başlık, kaynak bağlantısı, notların ve seçilen iki an (zaman sırasıyla) ile kaydedilmeli.
0. ~~F6.2 yerel transkripsiyon elle test~~: yapıldı (11 Kas, Cuma), süreler STATE F6.7'de. Kalan F6.7 işi: aynı TR ve bir EN videoda modellerin metin kalitesini karşılaştırmak (Türkçe karakter, özel isim, noktalama).
0. **F6.3 elle test (Cuma, release exe).** 20 dakikadan uzun bir video seç → "Transcribe on this PC" (Small ya da Medium). İlk parça (~10 dk ses) bitince döküm satırında "partial, up to …" görünmeli ve metin açılabilmeli. Sonra uygulamayı kapat ve yeniden aç: iş kendiliğinden devam etmeli, yüzde ilk parçanın sonundan başlamalı. Bitince satır "partial" olmadan görünmeli ve parça sınırında tekrar eden ya da eksik cümle olmamalı. Bir kez de ikinci parçada Cancel'a bas: ilk parçanın metni "partial" olarak kalmalı.

1. PR #44 (F6.2 ilk yarı) merge edildi (11 Kas). Site altyazısı testi (aşağıda 5. adım) hâlâ bekliyor.

1. PR #37 CI'ı Windows ve Linux'ta yeşil mi bak, sonra Cuma merge eder.
2. F5.3 elle test (Cuma, release exe): VK zaten çerez dosyasıyla bağlı. Takipçilere özel videoda "Try again"e basılır. Beklenen sonuç: video çerezle iner ve VK satırı "Signed in" olur. İşten sonra `%APPDATA%\dev.videovault.desktop\staging\cookies` boş olmalı. Sonucu `docs/platform-support.md`'ye yaz (VK-10), STATE F5.3 ✅.
3. Herkese açık VK videosunda küçük resmin neden gelmediğine bak.
4. F5.7 elle test (Cuma, release exe): Sessions → Reddit → Connect → "Sign in here" → pencerede giriş → Done → Reddit gönderisi iner. Sonra aynısı VK için (`vk.com` girişi) ve takipçilere özel videoda Try again (VK-04). Sonuç `docs/platform-support.md`'ye.
5. ~~F6.2 site altyazısı elle test~~ — yapıldı (11 Kas, Cuma): TR otomatik altyazı, EN video, sil/yeniden al çalışıyor.

## Bilinmesi gerekenler

- **Çalışma düzeni:** Claude bir dalda PR açar, Cuma merge eder ve `C:\Users\cumae\VideoDownloader` içinde `git pull` yapar. STATE.md'de bir alt görev yalnızca kanıtla ✅ olur: CI linki, test sonucu ya da elle kontrol. CI yalnızca hazır (taslak olmayan) PR'larda çalışır.
- **Windows yerel komutları:**
  - `npm run verify`: tüm kapılar.
  - `npm run desktop`: uygulamayı geliştirme modunda açar. İndirme ve import için önce `node scripts/fetch-tools.mjs` (yt-dlp, Deno, FFmpeg, ffprobe `tools/bin`'e iner); yoksa Diagnostics "Missing" der ve Download düğmesi çalışmaz.
  - `npm run dev:fixture`, sonra tarayıcıda `?fixture=empty|many|error|slow|first-run|download-error`. `download-error`: her indirmenin ilk denemesi ağ hatasıyla düşer, Try again başarır (Yolculuk 6).
  - `npm run desktop:e2e-exe` ve ardından `npm run e2e:desktop`: gerçek uygulama E2E.
- **Testler:** motor `crates/vv-engine/tests` (sahte yt-dlp `src/bin/vv-fake-ytdlp.rs`; `real_ytdlp.rs` gerçek araçlarla yerel sunucuya karşı, araçlar yoksa yerelde atlanır, CI'da zorunlu).
- **Açık noktalar:** oturum gerektiren içerik `blocked`/`auth` olur (giriş M3'te). Pause motor tarafında var, arayüzde yok. `MEDIA_URL_EXPIRED` henüz üretilmiyor (ADR-0003). FFmpeg kurulum boyutu (ADR-0006) F4'te.
- **Engeller:** STATE.md "Engeller ve açık noktalar" bölümünde. Uygulama simgesi hâlâ geçici.
