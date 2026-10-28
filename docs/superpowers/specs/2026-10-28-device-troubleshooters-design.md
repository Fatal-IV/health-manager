# Sorun gidericiler: ses, yazıcı, Bluetooth, kamera

Tarih: 2026-10-28 · Hedef sürüm: 0.3.15

## Amaç

Windows'un sorun giderme listesindeki hizmetleri, mevcut sorun giderici iskeletiyle (ağ, Windows Update,
sistem dosyaları, yavaşlık) tek tek eklemek. Bu spec ilk grubu kapsar: servis tabanlı dört sorun giderici.
Windows'un kendi sorun gidericileri (`msdt`) kullanılmaz; Microsoft bu aracı kullanımdan kaldırıyor. Her
sorun giderici yerel tanı testleri ve izin listesindeki onarımlarla yazılır.

## Kapsam dışı (sonraki dilimler)

- **Video oynatma ve Windows Media Player:** net bir tanı testi yok. Uygulanabilir bir onarım bulunursa
  ayrı spec ile eklenir, bulunmazsa kapsamdan çıkarılır.
- **Program uyumluluğu:** kullanıcıdan program yolu alır ve kayıt defterine yazar. Yardımcıdaki "argüman
  yalnızca veridir" kuralı için ayrı bir güvenlik tasarımı gerekir.
- **Arka plan akıllı aktarım hizmeti (BITS):** ayrı kart olmaz. `bits` servisi Windows Update
  sorun gidericisinin tanı ve onarımında zaten vardır.
- Sessize alınmış ses cihazı, sıfır ses seviyesi, yanlış varsayılan çıkış aygıtı ve kameranın gizlilik
  ayarı. Bunlar probe'a yeni okuma (Core Audio, kayıt defteri) ekler.
- Servis devre dışıysa onu etkinleştirmek. Bu grup yalnızca yeniden başlatır.

## Değişmeyen ilkeler

- Her düzeltme ayrı önizleme ve onayla çalışır; otomatik ilerleme yoktur.
- Yetkili yardımcı yalnızca kapalı `FixId` listesini kabul eder. Sorun giderici yalnızca `FixId` taşır,
  komut metni taşımaz.
- Çalıştırılamayan test "bilinmiyor" sayılır, asla "iyi" sayılmaz.
- Servis adları yardımcıda koda sabit yazılır. Dışarıdan gelen hiçbir değerle komut kurulmaz.

## 1. Altyapı: tek kimlik kaynağı

Sorun giderici kimlikleri bugün dört yerde elle tekrar ediliyor: `ui/src/api.ts` tipi,
`ui/src/pages/TroubleshootPicker.tsx` dizisi, `crates/health-core/src/troubleshoot.rs` (`as_str` ve
`from_str`) ve `src-tauri/src/commands.rs` testi.

- Arayüzde `TROUBLESHOOTERS` dizisi `as const` olarak tek yerde tutulur, `TroubleshooterId` tipi ondan
  türetilir. `Overview` ve `TroubleshootPicker` aynı diziyi kullanır.
- Rust'ta `TroubleshooterId::ALL` sabiti eklenir. `from_str` ve `commands.rs` testi bu sabitten okur.
- Sekiz kart düz liste olarak sığar; ızgara ya da gruplama eklenmez.

## 2. Tanı ve adımlar

| Sorun giderici | Kimlik | Tanı | Adımlar (hafiften ağıra) |
|---|---|---|---|
| Ses | `audio` | `Test::AudioServices` | `audio.restart_services` → `RestartAudioServices`, yeniden test: `AudioServices` |
| Yazıcı | `printer` | `Test::SpoolerRunning` | `print.restart_spooler` → `RestartSpooler`, yeniden test: `SpoolerRunning`; `print.clear_queue` → `ClearPrintQueue`, yeniden test: `SpoolerRunning` |
| Bluetooth | `bluetooth` | yok (her zaman önerir) | `bt.restart_service` → `RestartBluetooth`, yeniden test yok |
| Kamera | `camera` | yok (her zaman önerir) | `cam.restart_service` → `RestartCameraService`, yeniden test yok |

**Testler.** `AudioServices`: `Audiosrv` ve `AudioEndpointBuilder` ikisi de `Running` ise geçer; biri
`Stopped`, `Disabled` ya da `Missing` ise başarısız; okunamazsa `Unknown`. `SpoolerRunning`: `Spooler`
için aynı kural. Her ikisi de otomatik başlayan servisler olduğundan "durmuş" gerçek bir arızadır.

**Neden Bluetooth ve kamera testsiz.** `bthserv` ve `FrameServer` ihtiyaç olunca başlar; durmuş olmaları
normaldir ve bir arıza kanıtı değildir. Bu yüzden sistem dosyaları sorun gidericisi gibi `diagnose`
boştur, adım her zaman önerilir ve özet ekranında "test yok" satırı çıkar. Yeniden test olmadığından
onarımın kendi başarısı sayılır.

**Dürüst sınırlama.** Ses ve yazıcıda test geçerse metin, servislerin sağlam olduğunu ve sorunun aygıtta
ya da ayarda olabileceğini söyler (`texts.<id>.healthy`). Uygulama aygıt ve ayar düzeyindeki sorunları
bu sürümde algılamaz.

## 3. Yeni onarımlar (`FixId`)

`ALL` 13'ten 18 elemana çıkar.

| `FixId` | Yaptığı | Yönetici | Geri alınabilir |
|---|---|---|---|
| `RestartAudioServices` | `Audiosrv` ve `AudioEndpointBuilder` servislerini durdurup başlatır | evet | hayır |
| `RestartSpooler` | `Spooler` servisini durdurup başlatır | evet | hayır |
| `ClearPrintQueue` | `Spooler`'ı durdurur, spool klasöründeki `.SPL` ve `.SHD` dosyalarını siler, `Spooler`'ı başlatır. Bekleyen belgeler silinir | evet | hayır |
| `RestartBluetooth` | `bthserv` servisini durdurup başlatır | evet | hayır |
| `RestartCameraService` | `FrameServer` servisini durdurup başlatır | evet | hayır |

Hiçbiri argüman almaz (`takes_arg` false, argüman verilirse `Err`). Her biri için `index`, `as_str`,
`change_count`, `needs_admin`, plan metinleri (`fix.<ad>.summary`, `fix.<ad>.change.N`) ve yardımcıdaki
`run` eşlemesi eklenir. `ClearPrintQueue` onay metni, bekleyen belgelerin silineceğini açıkça yazar.

## 4. Yardımcı (`crates/fix-helper`)

- `repair.rs`: tek bir ortak işlev, sabit servis listesini alır, her birini `net.exe stop` ile durdurur,
  sonra `net.exe start` ile başlatır. Kalıp `reset_windows_update` ile aynıdır. Bir servisin durdurulması
  bağımlı servisleri de durduracağından (`AudioEndpointBuilder` → `Audiosrv`) durdurma `/y` ile yapılır ve
  başlatma ters sırayla yapılır.
- Windows dışı derlemede her yeni işlev mevcut `na` kalıbıyla `keys::NOT_AVAILABLE` döner.
- `ClearPrintQueue` yalnızca `%SystemRoot%\System32\spool\PRINTERS` içindeki `.SPL` ve `.SHD` uzantılı
  dosyaları siler; klasör yolu koda sabittir.
- İzin listesi testi (`crates/fix-helper/tests/allowlist.rs`) beş yeni `FixId`'yi kapsar ve argüman
  reddini doğrular.

## 5. Arayüz

- `tr.json`: `ui.troubleshoot.titles.*`, `descriptions.*`, `texts.<id>.{healthy,resolved,exhausted}`,
  beş onarımın `fix.*` anahtarları ve `ui.overview` altında gereken satırlar.
- `api.ts`: yalnızca türetilen tipin güncellenmesi; yeni komut yoktur.
- `Troubleshoot.tsx`, `StepCard.tsx`, `Overview.tsx` değişmez. Hepsi kimlikten veri çeker.

## 6. Test

- `troubleshoot.rs`: her yeni sorun giderici için `FakeProbe` ile sağlıklı, servis durmuş, servis devre
  dışı, servis okunamadı durumları. Testsiz iki sorun giderici için ilk önerinin doğru adım olduğu.
  Adım sayıları testi (`("network", 3), …`) sekiz kimliğe genişler.
- `fixes.rs`: yeni onarımların planları (`needs_admin`, `reversible`, `changes` uzunlukları) ve `ALL`
  kapsamı testi.
- `fix-helper`: izin listesi ve Windows dışı derlemede `NOT_AVAILABLE`.
- `commands.rs`: `ALL` üzerinden her kimliğin eşlendiği ve testli/testsiz bayrağı.
- Arayüz: seçici testi sekiz kimliği listeler, özet testi sekiz satır gösterir.

## 7. Sürümleme ve sıra

Her sorun giderici kendi commit'iyle gelir: altyapı, ses, yazıcı, Bluetooth, kamera. Dördü bitince tek
sürüm olarak 0.3.15 çıkar (`scripts/bump-version.mjs`) ve `release-notes/v0.3.15.md` yazılır.
