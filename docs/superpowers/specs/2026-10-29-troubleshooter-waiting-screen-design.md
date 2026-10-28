# Sorun gidericiler için bekleme ekranı

## Amaç

Bir sorun giderici çözümü uygularken kullanıcı bugün yalnızca "çalışıyor" görüyor; ne yapıldığını, hangi adımda olunduğunu ve ne kadar kaldığını bilmiyor. Çözüm uygulanırken ve ardından yapılan yeniden denetimde bir **bekleme ekranı** gösterilir: sorun gidericinin adı, "Adım n / toplam", yapılan işlemlerin listesi ve bir ilerleme çubuğu. Sonuç ekranı zaten vardır (`healthy`/`resolved`/`exhausted` kartları) ve değişmez.

İlerleme iki türde gösterilebilir ve kullanıcı Ana sayfadan seçer:

- **Canlı:** Yetkili yardımcı her işlemi bitirdikçe bildirir; liste ve çubuk gerçek durumu gösterir. Varsayılan.
- **Aşamalı:** Arayüzün bildiği aşamalarla (izin bekleniyor → işlemler → yeniden denetim) ilerler; işlemler tek tek işaretlenmez, çubuk aşama içinde yumuşakça kayar.

## Kapsam dışı

- İlk tanı sırasındaki "Denetleniyor…" metni değişmez.
- Tarama akışındaki `FixPreview` ve `BulkFix` değişmez; bekleme ekranı yalnızca sorun giderici akışında görünür.
- Başka ayarlar ya da Ayarlar sayfası eklenmez; tek ayar Ana sayfada durur.

## Tasarım

### 1. İşlem listesi (health-core)

- `fixes.rs`: `fn op_count(fix: FixId) -> usize` (kapsamlı `match`, joker yok; yeni kimlik derlenmez). `FixPlan` yeni bir `ops: Vec<String>` alanı alır; anahtarlar `fix.<id>.op.1` … `.n`.
- İşlem sayıları: `RestartAudioServices` 2 (durdur, başlat), `RestartSpooler` 2, `RestartBluetooth` 2, `RestartCameraService` 2, `ClearPrintQueue` 3 (durdur, kuyruğu sil, başlat), `ResetWindowsUpdate` 3 (durdur, klasörleri yedekle, başlat), `RunSfcDism` 2 (DISM, SFC), `RenewIp` 2 (bırak, yenile); diğer tüm onarımlar 1.
- `tr.json`: her `fix.<id>.op.N` için Türkçe metin (eylem cümlesi, örn. "Ses servisleri durduruluyor"). `fixes.rs` içindeki mevcut "her plan anahtarı tr.json'da var" testi `ops` anahtarlarını da kapsar.

### 2. İlerleme kanalı (yardımcı → uygulama)

- Uygulama, sonuç dosyasının yolundan `<sonuç yolu>.progress` adını türetir (yeni komut satırı bayrağı yok; izin listesi sözleşmesi değişmez) ve **yardımcıyı başlatmadan önce** boş bir dosya olarak kendisi oluşturur.
- Yardımcı bu dosyayı `winfs::open_no_follow` ile açar (bağlantı izlemez), sona ekler. Açılamazsa ilerleme sessizce kapanır; **ilerleme hiçbir zaman onarımı başarısız kılmaz**.
- Yardımcıda `progress` modülü: `progress::init(path)` bir kez çağrılır; `progress::done(n)` `{"op":n}\n` satırı ekler (işlem n bitti). Çağrı yerleri yalnızca çok işlemli onarımların işlem sınırlarıdır (yukarıdaki tablo); tek işlemli onarım bir şey çağırmaz, uygulama çıkışta son işlemi tamamlanmış sayar. Modül durumu `OnceLock` ile tutulur; `repair.rs` imzaları değişmez.
- Uygulama tarafı (`elevate.rs`): yardımcıyı bekleyen iş parçacığı dışında bir okuyucu, dosyayı ~250 ms'de bir okur ve `on_progress(done)` çağırır. Dosyaya **güvenilmeyen veri** gözüyle bakılır: en çok 4 KiB okunur, satır başına `{"op":<tamsayı>}` biçimi, `1..=op_count` dışı ve geri giden değerler yok sayılır, `done` yalnızca artar. Çıkışta dosya silinir.
- `apply_with` ek bir `on_progress: &dyn Fn(u32)` alır (mevcut testler `&|_| {}` geçer). `apply_fix_cmd` Tauri `fix-progress` olayını `{ done, total }` ile yayar.

### 3. Bekleme ekranı (arayüz)

- Yeni `Working` bileşeni (`ui/src/components/Working.tsx`), `Troubleshoot.tsx` içinde iki yerde kullanılır: onay verildikten sonra (`fix` aşamasında `busy`) ve `troubleshootAfterFix` sürerken. Sorun gidericide `FixPreview`'un `busy` hali bu bileşenle değiştirilir; `FixPreview` tarama akışı için olduğu gibi kalır.
- İçerik: başlık (`ui.troubleshoot.titles.<id>`), `Adım n / toplam`, işlem listesi (`plan.ops`; yapıldı ✓ / şu an ▸ / bekliyor ○), `role="progressbar"` çubuk (`aria-valuenow`), altında aşama metni ("İzin bekleniyor", "Onarım uygulanıyor", "Sonuç denetleniyor").
- Çubuk birimi `n_işlem + 1` (son birim yeniden denetim). **Canlı:** `done / (n+1)`; `fix-progress` olayıyla artar. **Aşamalı:** izin bekleniyor 0; onarım sırasında `(n / (n+1))` değerine zamanla yaklaşır (kalan mesafenin yarısı her ~1 s'de, asla ulaşmaz); yeniden denetimde `n/(n+1)` → 1. Sonuç gelince 1.
- Canlı türde yardımcıdan hiç olay gelmiyorsa (ör. ilerleme kapalı) liste "şu an" satırında kalır ve çubuk yalnızca aşamayla ilerler: bu, aşamalı türle aynı görünüm olduğundan ek durum gerekmez.
- Kullanıcı hareketi azalt tercihi açıksa (`prefers-reduced-motion`) çubuk geçişi animasyonsuz olur.

### 4. Ayar (Ana sayfa)

- `Settings { progress_mode: "live" | "staged" }`, dosya `%LOCALAPPDATA%\SaglikDenetimi\settings.json`. Dosya yoksa, okunamıyorsa ya da değer geçersizse `live` (hata gösterilmez).
- Tauri komutları `get_settings` ve `set_settings` (yalnızca iki geçerli değeri kabul eder; yazma hatası arayüze hata döner ama seçim o oturumda geçerli kalır).
- Ana sayfada küçük bir bölüm: "İlerleme ekranı" ve iki seçenek (Canlı / Aşamalı). Metinler `tr.json`'da.
- `Troubleshoot` sayfası modu sayfa açılırken `get_settings` ile alır.

## Hata ve kenar durumları

- Kullanıcı yönetici iznini reddederse: mevcut `Declined` akışı; bekleme ekranı kapanır, "Bu adım uygulanmadı" kartı görünür.
- Onarım başarısız olursa: bekleme ekranı kapanır, mevcut sonuç gösterimi.
- Zaman aşımı (60 dk, SFC/DISM): mevcut `K_TIMEOUT`; okuyucu iş parçacığı durdurulur, dosya silinir.
- Sorun giderici akışı dışında (tarama onarımı) ilerleme dosyası hiç oluşturulmaz; yalnızca `on_progress` verildiğinde oluşturulur ve okunur.

## Testler

- **health-core:** `op_count` her `FixId` için ≥ 1; `plan.ops.len() == op_count`; `ops` anahtarları `tr.json`'da; tabloyla uyum.
- **fix-helper:** `progress::done` satır biçimi; açılamayan yol sessizce yok sayılır; çok işlemli onarımların sırayla `1..n` bildirdiği (kayıtçı geri çağrı ile, `restart_with` kalıbı); bağlantıya yazmama (Windows testi).
- **elevate (uygulama):** `apply_with` sahte `Launcher` ile ilerleme dosyasından `on_progress` çağırır; hostile içerik (büyük dosya, bozuk satır, geri giden/aşan değer, UTF-8 olmayan bayt) olayı bozmaz ve onarım sonucunu etkilemez.
- **commands:** `get_settings` varsayılan/bozuk dosya/geçersiz değer → `live`; `set_settings` geçersiz değeri reddeder.
- **Arayüz:** `Working` canlıda olaylarla ilerler, aşamalıda olaysız ilerler, `role=progressbar` değerleri, liste durumları; `Troubleshoot` uygulama sırasında bekleme ekranını gösterir ve sonuçta kartı gösterir; Ana sayfa ayarı okur/yazar.

## Gözden geçirme odağı

- Yetkili yardımcı kullanıcı-yazılabilir bir yola yazıyor: bağlantı izlememe ve "ilerleme asla onarımı bozmaz" kuralı.
- Okunan ilerleme dosyası güvenilmeyen veridir; arayüze yalnızca `1..=n` aralığında artan sayı ulaşır.
- Yeni `FixId` eklendiğinde `op_count` ve `ops` metinleri olmadan derlenmez/testten geçmez.
