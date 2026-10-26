<div align="center">

<img src="src-tauri/icons/icon.png" alt="Sağlık Denetimi" width="112" />

# Sağlık Denetimi

**Windows bilgisayarınızın sağlığını tek tıkla kontrol eder.**
Sorunları sade bir dille anlatır; siz onaylamadan hiçbir şeyi değiştirmez.

</div>

## Ne yapar?

Windows'un eski "Sorun Giderici"sinin yerini almayı hedefleyen, teknik bilgisi olmayan kullanıcılar için yazılmış bir araçtır.

- **16 kontrol, beş alan:** güvenlik, güncelleme, depolama, performans, sistem. İstediğiniz alanı seçip yalnızca onu tarayabilirsiniz.
- **Tek bir sağlık puanı** ve her sorun için "neden önemli" açıklaması.
- **Düzeltmeler önce önizlenir.** Ne değişeceği yazılır, onayınızı bekler. Birden çok düzeltmeyi tek onayla uygulayabilirsiniz.
- **Düzeltme sonrası yeniden tarama:** sorun gerçekten gittiyse "çözüldü" der, gitmediyse söyler.
- **Rapor:** sonuçları metin veya PDF olarak kaydedin.
- Ana uygulama yönetici olarak **çalışmaz**. Yönetici izni gereken işler ayrı, dar kapsamlı bir yardımcı programla (`fix-helper`) ve Windows'un kendi izin penceresiyle yapılır.

## Kurulum

[Son sürümü indirin](../../releases/latest) (`Saglik-Denetimi-Setup.exe`) ve çalıştırın. Yönetici izni gerekmez.

Uygulama açılışta GitHub'daki son sürüme bakar ve yenisi varsa güncelleme önerir. Bu, yalnızca sürüm bilgisini sorgulayan tek bir istektir; bilgisayarınızdan hiçbir veri gönderilmez. İndirilen kurucu, uygulamanın içine gömülü açık anahtarla doğrulanır; imzası tutmayan dosya çalıştırılmaz. Her sürümde ayrıca `.sha256` dosyası bulunur.

> Kurucu henüz bir kod imzalama sertifikasıyla imzalı değildir; Windows SmartScreen bir uyarı gösterebilir. Dosyanın özetini `.sha256` ile karşılaştırarak doğrulayabilirsiniz.

## Geliştirme

Gereken: Rust (stable), Node.js 20+, Windows 10/11.

```powershell
npm --prefix ui install
cargo test --workspace            # Rust testleri
npm --prefix ui test -- --run     # arayüz testleri
cargo clippy --workspace --all-targets -- -D warnings

npm --prefix ui run tauri dev     # geliştirme sürümü
```

Yapı:

| Klasör | İçerik |
|---|---|
| `crates/health-core` | Kontroller, puanlama, düzeltme kaydı, rapor, doğrulama. Sistem erişimi `SystemProbe` arkasında, testler sahte probe ile çalışır. |
| `crates/fix-helper` | Yönetici izniyle çalışan, izin listeli düzeltme yürütücüsü. |
| `src-tauri` | Tauri uygulaması: komutlar, yükseltme akışı, güncelleyici. |
| `ui` | React + TypeScript arayüzü (Türkçe metinler `ui/src/i18n/tr.json`). |
| `scripts` | Sürüm artırma, taşınabilir paket, kurucu ve yayın betikleri. |

Sürüm çıkarmak için (bakımcılar): `node scripts/bump-version.mjs [patch|minor|major]`, ardından `node scripts/release.mjs` kurucuyu üretir ve güncelleyici anahtarıyla imzalar; `--publish` GitHub sürümünü oluşturur. İmza anahtarı depoda tutulmaz.

## Güvenlik

Yükseltilmiş yardımcı programın tasarımı ve sınırları için `docs/superpowers/specs` altındaki belgeye bakın. Bir açık bulduysanız lütfen herkese açık issue yerine depo sahibine özel olarak bildirin.

## Lisans

[MIT](LICENSE)
