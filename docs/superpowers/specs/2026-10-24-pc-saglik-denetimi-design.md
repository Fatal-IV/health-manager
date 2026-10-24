# Windows Sağlık Denetimi ve Sorun Çözücü: Tasarım (Aşama 1)

Tarih: 2026-10-24

## 1. Amaç

Windows için, eski "Sorun Giderici"nin yerini alacak modern arayüzlü bir bilgisayar sağlık denetimi uygulaması. Uygulama sorunları tarar, sade Türkçe ile açıklar ve kullanıcı onayıyla düzeltir.

**Hedef kitle:** Teknik bilgisi olmayan genel son kullanıcılar.

**Başarı ölçütleri**
- Kullanıcı tek ekrandan bilgisayarının durumunu anlar (sağlık puanı + kategori kartları).
- Her sorun "ne oldu / neden önemli / ne yapılacak" şeklinde teknik olmayan dille anlatılır.
- Her düzeltme uygulanmadan önce ne değişeceği gösterilir ve kullanıcı tek tek onaylar.
- Yönetici yetkisi yalnızca onaylanmış bir düzeltme için ve yalnızca o işlem süresince istenir.

## 2. Kapsam ve aşamalar

| Aşama | İçerik |
|---|---|
| **1 (bu spec)** | Çekirdek motor, sağlık taraması, düzeltmeler, sağlık puanı, rapor ve dışa aktarma |
| 2 | Şikayet bazlı sihirbaz ("internet çalışmıyor", "bilgisayar yavaş", "ses gelmiyor"). Aşama 1'deki Check modüllerini yeniden kullanır |
| 3 | Zamanlanmış tarama, geçmiş, cilalama |

Aşama 1'de **yok:** sihirbaz, zamanlanmış otomatik tarama, geçmiş grafikleri, uzaktan yönetim, "tümünü düzelt" toplu düğmesi.

## 3. Kararlar

- **Teknoloji:** Tauri (Rust arka uç, React + TypeScript arayüz).
- **Yetki modeli:** Uygulama yetkisiz başlar. Düzeltme gerektiğinde yalnızca o işlem için UAC ile yükseltilmiş yardımcı süreç başlatılır.
- **Dil:** Türkçe. Metinler baştan i18n dosyalarında tutulur, yeni dil eklemek yeni bir dil dosyası eklemekten ibarettir.
- **Düzeltme onayı:** Her düzeltme tek tek onaylanır. Toplu düzeltme yoktur.
- **Rapor:** Aşama 1 kapsamındadır (özet ekranı + dışa aktarma).

## 4. Mimari

### 4.1 Katmanlar

**Arayüz (React + TypeScript, Tauri webview).** Yalnızca sunumdan sorumludur: sağlık paneli, kartlar, rapor. Sisteme doğrudan erişmez, Tauri komutlarıyla çekirdeğe sorar. Tüm metinler i18n'den gelir.

**Çekirdek (Rust, yetkisiz süreç).** Her sağlık kontrolü ortak bir arayüzü uygulayan bir `Check` modülüdür:
- `scan()`: sistemi okur, durum (iyi / uyarı / kritik), kullanıcıya yönelik açıklama ve kanıt döner.
- `fix_plan()`: düzeltmenin neyi değiştireceğini, nedenini ve yönetici yetkisi gerekip gerekmediğini tarif eder. Önizleme ekranı bunu gösterir.

Tarama WMI, Windows API'leri ve registry okuma ile yapılır; yönetici yetkisi gerektirmez. Sistem erişimi bir trait arkasındadır, böylece Check mantığı sahte verilerle test edilebilir.

**Yükseltilmiş yardımcı (Rust, ayrı küçük exe).** Yalnızca düzeltme uygular.
- Serbest komut kabul etmez. Yalnızca önceden tanımlı, adlandırılmış düzeltme kimliklerini (örn. `reset_windows_update`) çalıştırır; arayüzden rastgele komut çalıştırılamaz.
- Her işlemi günlüğe yazar. Mümkün olduğunda geri alma için yedek veya geri yükleme noktası oluşturur.
- İş bitince kapanır.

### 4.2 Veri akışı

Tara → Check'ler çalışır → sonuçlar kartlara döner → kullanıcı bir sorunda "Düzelt" der → `fix_plan()` önizlemesi gösterilir → kullanıcı onaylar → (gerekirse) UAC → yardımcı düzeltmeyi uygular → çekirdek ilgili kontrolü yeniden tarar ve sonucu doğrular.

### 4.3 Hata durumları

- **UAC reddedildi:** "Yapılmadı, hiçbir değişiklik uygulanmadı" mesajı; sorun açık kalır.
- **Düzeltme başarısız:** Sade dilde açıklama, teknik detay "Ayrıntılar" bölümünde.
- **Tek bir Check başarısız oldu:** Diğer kontroller çalışmaya devam eder; o kart "kontrol edilemedi" gösterir. Tüm tarama çökmez.
- **Düzeltme sonrası doğrulama başarısız:** Sorun çözülmüş sayılmaz; kullanıcıya olduğu gibi bildirilir.

## 5. Kontroller

Hedef yaklaşık 12-15 kontrol. Tümü yetkisiz taranır.

| Kategori | Kontrol | Düzeltme (yönetici gerekir mi) |
|---|---|---|
| Depolama | Disk doluluğu, geçici dosyalar, geri dönüşüm kutusu, Windows Update artıkları | Temizle (geçici dosyalar: hayır, sistem artıkları: evet) |
| Performans | CPU/RAM yükü, en çok kaynak tüketen programlar, başlangıç programları | Başlangıçtan çıkar (hayır), süreç kapat (kullanıcı onayıyla) |
| Güncelleme | Bekleyen güncellemeler, Windows Update servisi sağlığı, son başarılı güncelleme tarihi | Update bileşenlerini sıfırla (evet) |
| Güvenlik | Defender durumu ve imza güncelliği, güvenlik duvarı, UAC düzeyi | Defender / duvarı etkinleştir (evet) |
| Sistem | Bozuk sistem dosyaları (SFC/DISM), sürücü hataları (uyarılı cihazlar), açılış süresi | SFC/DISM onarımı (evet), sürücü için yönlendirme |

Hızlı tarama uzun süren kontrolleri (SFC/DISM) içermez; bunlar "Derin tarama" olarak ayrıca sunulur.

## 6. Sağlık puanı

0-100 arası tek puan. Her sonucun ağırlığı vardır (kritik güvenlik sorunu, dolu bir geçici klasörden çok daha fazla puan düşürür). "Neden bu puan?" bağlantısı hesaplamayı şeffaf gösterir. Puan, kullanıcıyı doğru önceliğe yönlendirmek içindir.

## 7. Ekran akışı

1. **Ana ekran:** Büyük sağlık puanı, "Taramayı Başlat", son tarama tarihi. İlk açılışta kısa karşılama.
2. **Tarama:** Kategori kategori ilerleyen animasyonlu ilerleme. İptal edilebilir.
3. **Sonuç paneli:** Yeşil / sarı / kırmızı kategori kartları. Karta tıklayınca sorunlar "ne oldu / neden önemli" biçiminde açılır; teknik ayrıntı genişletilebilir.
4. **Düzeltme:** Her sorunda "Düzelt". Önizleme (neyin değişeceği) ve onay çıkar. Yönetici gerekiyorsa UAC bu noktada gelir.
5. **Rapor:** Tarama özeti, yapılan düzeltmeler, kalan sorunlar. Dışa aktarma (metin ve PDF); destek almak isteyenler paylaşabilir.

## 8. Görsel yön

Windows 11 diline yakın: yumuşak köşeler, Mica benzeri yarı saydam arka plan, açık/koyu tema (sistemi takip eder), sade tipografi, gösterişten çok anlamlı geçişler.

## 9. Test stratejisi

- **Birim testler:** Check mantığı ve puan hesabı, sistem erişimi trait'i arkasında sahte verilerle.
- **Yardımcı süreç:** Yalnızca tanımlı düzeltme kimliklerini kabul ettiği, bilinmeyen kimliği reddettiği test edilir.
- **Entegrasyon:** Gerçek Windows üzerinde tarama ve (güvenli kategorilerde) düzeltme akışı elle ve otomatik doğrulanır.
- **Arayüz:** Bileşen testleri ve temel uçtan uca akış (tara → düzelt → doğrula).

## 10. Açık noktalar

Yok. Sihirbaz ve zamanlama bilinçli olarak sonraki aşamalara bırakılmıştır.
