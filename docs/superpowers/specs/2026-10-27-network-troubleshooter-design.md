# Sorun giderici: çekirdek model ve ağ / internet (ilk dilim)

Tarih: 2026-10-27 · Hedef sürüm: 0.3.9

## Amaç

Uygulama bugün tarıyor ve tek tek düzeltiyor. Sorun giderici, kullanıcının bir belirtiyle başlayıp
uygulamanın sebebi bulmasını ve en hafiften en sert onarıma doğru, her adımda onay alarak ilerlemesini
sağlar. Windows'un sorun gidericisine rakip olmak hedefidir; farkı: her değişiklik önizlenir, tek tek
onaylanır, sonra doğrulanır.

Bu spec yalnızca **çekirdek modeli** ve **ağ / internet** sorun gidericisini kapsar.

## Kapsam dışı (sonraki dilimler)

- Windows Update, sistem dosyaları (sfc/DISM) ve yavaşlık sorun gidericileri. Aynı iskeleti kullanır;
  `ResetWindowsUpdate` ve `RunSfcDism` düzeltmeleri zaten vardır.
- Mevcut tarama sorunlarına "neden oldu / nasıl düzelir" rehberi.
- Belirti bazlı birden çok kartlı "Sorun Gider" ana ekranı (bu dilimde tek kart).
- Oturum durumunu diske yazmak.

## Değişmeyen ilkeler

- Her düzeltme ayrı önizleme ve onayla çalışır; otomatik ilerleme yoktur.
- Yetkili yardımcı yalnızca kapalı `FixId` listesini kabul eder. Sorun giderici yalnızca `FixId` taşır,
  komut metni taşımaz.
- Çalıştırılamayan test "bilinmiyor" sayılır, asla "iyi" sayılmaz.

## 1. Çekirdek (`crates/health-core/src/troubleshoot.rs`)

```rust
pub enum TroubleshooterId { Network }

pub enum NetTest { Gateway, Dns, Internet }   // sonra başka alanlar kendi test enum'ını ekler

pub struct Step {
    pub id: &'static str,       // "net.flush_dns"
    pub fix: FixId,
    pub recheck: NetTest,       // bu adımdan sonra çalışan, "düzeldi mi?" testi
}

pub struct Troubleshooter {
    pub id: TroubleshooterId,
    pub diagnose: &'static [NetTest],
    pub steps: &'static [Step], // hafiften sert olana
}
```

Durum makinesi saf Rust'tır, arayüzden ve Windows'tan bağımsızdır:

| Durum | Anlamı |
|---|---|
| `Healthy` | Tanı testlerinin hepsi geçti: sorun görünmüyor |
| `Suggest(step)` | Sıradaki basamak önerildi, kullanıcı onayı bekleniyor |
| `Resolved` | Bir basamaktan sonra yeniden test geçti |
| `Exhausted` | Basamaklar bitti, sorun sürüyor |

Geçişler: `start` → tanı çalışır; hepsi geçerse `Healthy`, aksi halde `Suggest(ilk basamak)`.
Düzeltme uygulanınca `recheck` çalışır; geçerse `Resolved`, geçmezse `Suggest(sonraki)` ya da
`Exhausted`. Kullanıcı bir basamağı atlayabilir (`Suggest(sonraki)`). Test sonucu üç değerlidir:
`Pass`, `Fail`, `Unknown`. `Unknown`, `Pass` sayılmaz; tanıda ve yeniden testte "geçmedi" gibi
davranır, ancak arayüzde "ölçülemedi" olarak ayrı gösterilir.

## 2. Probe

`SystemProbe` arayüzüne üç salt-okunur çağrı eklenir; `FakeProbe` ve gerçek probe uygular:

- `gateway_reachable() -> Result<bool, ProbeError>`: varsayılan ağ geçidine erişim.
- `dns_resolves() -> Result<bool, ProbeError>`: bilinen bir ada çözümleme.
- `internet_reachable() -> Result<bool, ProbeError>`: IP düzeyinde internet erişimi
  (DNS'ten bağımsız, böylece "ağ var ama DNS bozuk" ayırt edilir).

## 3. Yeni düzeltmeler (`fixes.rs`)

Üçü de argümansızdır; `ALL`, `index`, `takes_arg`, `as_str`, `change_count`, `needs_admin`, `reversible`
ve i18n anahtarları (`fix.<id>.summary`, `fix.<id>.change.n`) eklenir. Mevcut testler (ALL sırası,
tüm varyantların kapsanması) güncellenir.

| Düzeltme | Ne yapar | Yönetici | Geri alınabilir |
|---|---|---|---|
| `FlushDns` | DNS çözücü önbelleğini temizler | hayır | hayır (önbellek kendiliğinden dolar) |
| `RenewIp` | IP kiralamasını bırakıp yeniler | evet | hayır |
| `ResetWinsock` | Winsock kataloğunu sıfırlar; yeniden başlatma gerekir | evet | hayır |

Basamak sırası: `FlushDns` → `RenewIp` → `ResetWinsock`. `ResetWinsock` sonrası yeniden başlatma
gerektiği önizlemede açıkça yazar ve `Resolved` yerine "yeniden başlatınca denenmeli" notu gösterilir.

Yardımcı (`crates/fix-helper/src/actions.rs`) bu üç eylemi sabit, argümansız çağrılarla yapar;
kullanıcı girdisinden komut satırı kurulmaz.

## 4. Tauri komutları (`src-tauri/src/commands.rs`)

- `troubleshoot_start(id)` → durum (`Healthy` / `Suggest` / `Exhausted`)
- `troubleshoot_recheck(id, step)` → durum (`Resolved` / `Suggest` / `Exhausted`)
- Düzeltmenin kendisi mevcut `get_fix_plan` + `apply_fix` hattından geçer; sorun giderici bu hatta
  ikinci bir yol açmaz.

## 5. Arayüz

- Ana ekrana "Sorun Gider" girişi; ilk dilimde tek kart: **Ağ / internet**.
- Yeni `ui/src/pages/Troubleshoot.tsx` ve `ui/src/components/StepCard.tsx`. Akış: tanı sonucu → önerilen
  adım (neden, ne değişecek, yönetici gerekir mi) → "Uygula" ile mevcut önizleme/onay ekranı → otomatik
  yeniden test. Üstte "2 / 3" ilerleme.
- Bitiş ekranları: Çözüldü, Çözülemedi (raporu aç bağlantısıyla), Sorun görünmüyor.
- Metinler `ui/src/i18n/tr.json`.

## 6. Hata durumları

- Probe hatası → test `Unknown`.
- Yönetici izni reddedilirse basamak atlanmış sayılır; kullanıcıya "yeniden dene" / "sonrakine geç" sorulur.
- Düzeltme başarısız olursa hata gösterilir, otomatik devam edilmez.
- Uygulama kapanırsa oturum kaybolur; yeniden başlatınca tanı baştan çalışır.

## 7. Test

- Rust: durum makinesi `FakeProbe` ile şu yollardan geçirilir: sağlıklı, ilk basamakta çözüldü, üçüncüde
  çözüldü, çözülemedi, test `Unknown`, basamak atlandı. `fixes.rs` testleri üç yeni düzeltmeyi kapsar.
- Arayüz: panel akışı ve üç bitiş ekranı; `App.test.tsx` içinde başlatma → öneri → onay → çözüldü.
- Sürüm: 0.3.9, `release-notes/v0.3.9.md`.
