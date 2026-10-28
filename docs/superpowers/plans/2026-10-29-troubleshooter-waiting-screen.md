# Sorun gidericiler için bekleme ekranı Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Sorun giderici çözümü uygularken ve yeniden denetlerken, işlem listesi ve ilerleme çubuğu olan bir bekleme ekranı göstermek; ilerleme "canlı" (yardımcıdan) ya da "aşamalı" olabilir ve Ana sayfadan seçilir.

**Architecture:** `health-core` her onarım için sabit işlem sayısını ve işlem metin anahtarlarını bildirir (`FixPlan.ops`). Yetkili yardımcı, işlem bitince `<sonuç yolu>.progress` dosyasına `{"op":n}` satırı ekler; uygulama bu dosyayı güvenilmeyen veri olarak okuyup `fix-progress` olayı yayar. Arayüzde `Working` bileşeni iki modda çubuğu sürer; mod `settings.json`'dan gelir.

**Tech Stack:** Rust (health-core, fix-helper, Tauri), React + TypeScript + Vitest, `tr.json`.

**Spec:** `docs/superpowers/specs/2026-10-29-troubleshooter-waiting-screen-design.md`

## Global Constraints

- Yeni komut satırı bayrağı yok; izin listesi sözleşmesi değişmez. İlerleme dosyası `<sonuç yolu>.progress`.
- İlerleme dosyasını uygulama, yardımcıyı başlatmadan önce boş oluşturur; yardımcı `winfs::open_no_follow` ile açar (bağlantı izlemez). İlerleme **hiçbir zaman** onarımı başarısız kılmaz.
- Uygulama ilerleme dosyasından en çok 4 KiB okur; satır biçimi `{"op":<tamsayı>}`; `1..=op_count` dışı ve geri giden değerler yok sayılır; `done` yalnızca artar. Çıkışta dosya silinir.
- Okuma aralığı ~250 ms. Varsayılan mod `live`; `settings.json` yok/bozuk/geçersizse `live`.
- Metinler Türkçe, `ui/src/i18n/tr.json` içinde; kodda düz metin yok. Dosyalar CRLF'tir (Python ile düzenlerken varsayılan metin kipini kullan).
- Her görev kendi commit'iyle biter; commit mesajı oturumun atıf satırlarıyla bitirilir. `git add` ile yalnızca görevin dosyaları alınır; `setup/src-tauri/gen/schemas/*.json` bu plana ait değildir.
- Sürüm yalnızca son görevde yükseltilir (0.3.15 → 0.3.16).

**Spec açıklamaları (kararlar):**
1. `apply_fix_cmd` yeni bir `with_progress: bool` parametresi alır (spec'teki "yalnızca `on_progress` verildiğinde" kuralının komut karşılığı); yalnızca sorun giderici akışı `true` gönderir.
2. Arayüzde iki aşama etiketi vardır: "Onarım uygulanıyor" ve "Sonuç denetleniyor". Yönetici izni isteyen onarımlarda (`plan.needs_admin`) ayrıca "Windows yönetici izni isterse onaylayın." notu görünür. Ayrı bir "izin bekleniyor" aşaması yoktur, çünkü arayüz izin penceresinin ne zaman kapandığını bilemez.

## Review Focus

- Çok büyük, bozuk satırlı, UTF-8 olmayan, geri giden ya da `op_count`'u aşan değerli ilerleme dosyası olayı bozmaz ve onarım sonucunu değiştirmez. Test: `elevate/tests.rs`.
- İlerleme dosyası açılamıyor ya da bir bağlantıysa onarım yine tamamlanır. Test: `progress.rs`.
- Yardımcı çıkıp dosya silinirken okuyucu iş parçacığı takılmaz ve onarım zaman aşımında da durur. Test: `elevate/tests.rs`.
- Olay hiç gelmezse (canlı mod) çubuk aşamayla ilerler ve ekran donmuş görünmez. Test: `Working.test.tsx`.
- `settings.json` bozuk, boş ya da bilinmeyen değerliyse Ana sayfa ve bekleme ekranı `live` ile çalışır. Test: `settings.rs`, `Home.test.tsx`.

---

### Task 1: İşlem listesi (health-core)

**Files:**
- Modify: `crates/health-core/src/fixes.rs`, `ui/src/i18n/tr.json`

**Interfaces:**
- Produces: `pub fn op_count(fix: FixId) -> usize` (kapsamlı `match`, joker yok) ve `FixPlan.ops: Vec<String>` (`#[serde(default)]`; anahtarlar `fix.<id>.op.1..=n`). `fix_plan` bunu doldurur.
- İşlem sayıları: `reset_windows_update` 3; `clear_print_queue` 3; `run_sfc_dism`, `renew_ip`, `restart_audio_services`, `restart_spooler`, `restart_bluetooth`, `restart_camera_service` 2; diğer tüm kimlikler 1.

- [ ] **Step 1: Başarısız testler (`fixes.rs`).** `every_fix_has_ops_matching_op_count`: her `FixId::ALL` için `op_count(f) >= 1`, `fix_plan(f, None).ops.len() == op_count(f)` ve anahtarlar `fix.<id>.op.<n>`. `op_counts_match_the_spec_table`: yukarıdaki tablodaki değerler. Mevcut `every_plan_key_exists_in_tr_json` testi `plan.ops` anahtarlarını da denetlesin.
- [ ] **Step 2: Çalıştır.** `cargo test -p health-core fixes` → derleme hatası/başarısız.
- [ ] **Step 3: Uygula.** `op_count` ve `FixPlan.ops` (alan sonuna, `#[serde(default)]`). `fix_plan` `ops`'u doldurur.
- [ ] **Step 4: Metinler.** `tr.json` `fix.<id>.op.N` (eylem cümleleri, "…uluyor" biçimi): `clean_temp.op.1` "Geçici dosyalar siliniyor"; `empty_recycle_bin` "Geri dönüşüm kutusu boşaltılıyor"; `clean_update_leftovers` "Yarım kalan güncelleme indirmeleri siliniyor"; `disable_startup_item` "Başlangıç öğesi devre dışı bırakılıyor"; `end_process` "Program kapatılıyor"; `enable_defender_realtime` "Gerçek zamanlı koruma açılıyor"; `enable_firewall` "Güvenlik duvarı açılıyor"; `read_boot_time` "Açılış süresi okunuyor"; `flush_dns` "DNS önbelleği temizleniyor"; `reset_winsock` "Ağ bileşenleri sıfırlanıyor"; `reset_windows_update` 1 "Güncelleme servisleri durduruluyor", 2 "Güncelleme klasörleri yedekleniyor", 3 "Güncelleme servisleri başlatılıyor"; `run_sfc_dism` 1 "Windows görüntüsü onarılıyor (DISM)", 2 "Sistem dosyaları taranıp onarılıyor (SFC)"; `renew_ip` 1 "IP adresi bırakılıyor", 2 "Yeni IP adresi isteniyor"; `restart_audio_services` 1 "Ses servisleri durduruluyor", 2 "Ses servisleri başlatılıyor"; `restart_spooler` 1 "Yazdırma servisi durduruluyor", 2 "Yazdırma servisi başlatılıyor"; `clear_print_queue` 1 "Yazdırma servisi durduruluyor", 2 "Bekleyen belgeler siliniyor", 3 "Yazdırma servisi başlatılıyor"; `restart_bluetooth` 1 "Bluetooth servisi durduruluyor", 2 "Bluetooth servisi başlatılıyor"; `restart_camera_service` 1 "Kamera servisi durduruluyor", 2 "Kamera servisi başlatılıyor".
- [ ] **Step 5: Doğrula ve commit.** `cargo test --workspace` ve `cargo clippy --workspace --all-targets -- -D warnings` → geçer. `feat(fixes): per-fix operation list`

### Task 2: Yardımcıda ilerleme bildirimi

**Files:**
- Create: `crates/fix-helper/src/progress.rs`
- Modify: `crates/fix-helper/src/main.rs` (`mod progress;`, `req.result` ile `init`), `crates/fix-helper/src/repair.rs`

**Interfaces:**
- Produces (`progress.rs`): `pub fn init(result: &Path)` (yolu `<result>.progress` yapar; dosya **zaten varsa** `winfs::open_no_follow(path, false)` ile açar, sona konumlanır, `OnceLock<Mutex<File>>`'e koyar; her hata sessizce yok sayılır), `pub fn done(n: u32)` (`{"op":n}\n` ekler; başlatılmadıysa ya da yazma hatasında hiçbir şey yapmaz), `pub fn line(n: u32) -> String` (saf biçim işlevi).
- Consumes/değişir (`repair.rs`): `fn restart_with(run, services, done: &mut dyn FnMut(u32))` — duruş döngüsünden sonra `done(1)`, başlatma döngüsünden sonra `done(2)`. `imp::restart_services` bunu `progress::done` ile çağırır.
- Çağrı yerleri: `clear_print_queue` (`done(1)` durdurdan, `done(2)` silmeden, `done(3)` başlatmadan sonra), `reset_windows_update` (1 servis durdurma, 2 klasör yedekleme, 3 başlatma), `run_sfc_dism` (1 DISM, 2 SFC), `renew_ip` (1 release, 2 renew). Tek işlemli onarımlar bir şey çağırmaz.

- [ ] **Step 1: Başarısız testler.** `progress.rs`: `line_is_one_json_object_per_line` (`line(2) == "{\"op\":2}\n"`); `done_without_init_does_nothing`; `init_ignores_a_missing_file` (dosya yoksa `init` dosyayı **oluşturmaz**); `done_appends_lines_in_order` (geçici dosya oluştur, `init`, `done(1)`, `done(2)` → dosya içeriği iki satır). `repair.rs`: mevcut `restart_with_*` testleri `&mut |_| {}` geçer; yeni `restart_with_reports_stop_then_start` (kaydedici geri çağrı → `[1, 2]`, durdurma hatasında da `[1, 2]`, başlatma başarısızsa yalnızca `[1]`).
- [ ] **Step 2: Çalıştır.** `cargo test -p fix-helper` → derleme hatası/başarısız.
- [ ] **Step 3: Uygula.** `progress.rs` (yukarıdaki imzalar; modül `OnceLock` ile durum tutar, `repair.rs` imzaları dışarıya değişmez). `main.rs`: `real_main` içinde `fix` doğrulandıktan sonra, `actions::run`'dan önce `progress::init(&req.result)`. `repair.rs`: yukarıdaki çağrı yerleri.
- [ ] **Step 4: Doğrula ve commit.** `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings` → geçer. `feat(helper): report finished operations to a progress file`

### Task 3: Uygulama arka ucu (ilerleme okuma, olay, ayar)

**Files:**
- Create: `src-tauri/src/settings.rs`
- Modify: `src-tauri/src/elevate.rs`, `src-tauri/src/elevate/tests.rs`, `src-tauri/src/commands.rs`, `src-tauri/src/main.rs`

**Interfaces:**
- Consumes: `op_count` (Task 1).
- Produces (`elevate.rs`): `pub fn parse_progress(text: &str, total: u32, last: u32) -> u32` (saf; verilen metnin geçerli satırlarından `last`'tan büyük, `<= total` en büyük değeri döndürür, yoksa `last`; yalnızca ilk 4096 bayta bakılır, bozuk satırlar atlanır); `apply_with(l, fix, arg, on_progress: Option<&(dyn Fn(u32) + Sync)>)`; `apply_fix(fix, arg, on_progress)`. `on_progress` verilirse `<result>.progress` boş oluşturulur, `std::thread::scope` içinde ~250 ms'de bir okunur, değer arttıkça `on_progress(done)` çağrılır; yardımcı dönünce okuyucu durur ve dosya silinir. `None` ise dosya oluşturulmaz.
- Produces (`commands.rs`): `apply_fix_cmd(app, state, fix, arg, with_progress: bool)`; `with_progress` iken `app.emit("fix-progress", FixProgress { done, total })` (`total = op_count(fix)`). `get_settings() -> Settings`, `set_settings(progress_mode: String) -> Result<Settings, String>`.
- Produces (`settings.rs`): `#[derive(Serialize, Clone, PartialEq, Debug)] pub struct Settings { pub progress_mode: String }`; `pub fn parse(text: &str) -> Settings` (geçerli `"live"|"staged"`, aksi `live`); `pub fn load() -> Settings`; `pub fn save(mode: &str) -> Result<Settings, String>` (yalnızca iki geçerli değeri kabul eder; dosya `%LOCALAPPDATA%\SaglikDenetimi\settings.json`, klasör gerekirse oluşturulur).

- [ ] **Step 1: Başarısız testler.** `settings.rs`: `parse_defaults_to_live` (boş, `{}`, bozuk JSON, `{"progress_mode":"x"}`, sayı değeri → `live`); `parse_accepts_staged`; `save_rejects_unknown_mode` (`save("x").is_err()`, dosya yazılmadan). `elevate/tests.rs`: `parse_progress_takes_the_highest_valid_op` (`"{\"op\":1}\n{\"op\":3}\n"`, total 3, last 0 → 3); `parse_progress_ignores_bad_input` (bozuk satır, `{"op":99}` total 3, `{"op":-1}`, `{"op":1.5}`, boş, 8 KiB çöp, UTF-8 olmayan bayt dizisi → `last` döner; geri giden değer `last`'ı düşürmez); `apply_with_reports_progress_and_still_returns_the_result` (Fake launcher `args.result.with_extension`-yerine `<result>.progress` dosyasına `{"op":1}\n{"op":2}\n` yazar; `on_progress` toplayıcısı `2` görür; sonuç `Applied`; çıkışta `.progress` dosyası yok); `apply_with_ignores_hostile_progress_file` (8 KiB çöp içerikle sonuç yine `Applied`, `on_progress` çağrılmaz); `no_progress_file_without_a_callback` (`None` iken Fake `.progress` dosyasının var olmadığını doğrular). Mevcut `apply_with(&f, …)` çağrıları `None` geçer.
- [ ] **Step 2: Çalıştır.** `cargo test -p app` → derleme hatası/başarısız.
- [ ] **Step 3: Uygula.** `parse_progress`; okuyucu `std::thread::scope` içinde bir `AtomicBool` ile durdurulur (zaman aşımında da launcher dönünce durur), dosyayı en çok 4 KiB `take` ile okur; `settings.rs`; `commands.rs` olay ve iki komut; `main.rs` kayıt (`commands::get_settings`, `commands::set_settings`). `apply_fix_cmd` mevcut `apply_and_verify` kapanışında `elevate::apply_fix(f, a, cb)` çağırır (`with_progress` false iken `None`).
- [ ] **Step 4: Doğrula ve commit.** `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings` → geçer. `feat(app): read helper progress, emit it, keep the progress setting`

### Task 4: Bekleme ekranı (arayüz)

**Files:**
- Create: `ui/src/components/Working.tsx`, `ui/src/components/Working.test.tsx`
- Modify: `ui/src/api.ts`, `ui/src/testing/fixtures.ts`, `ui/src/pages/Troubleshoot.tsx`, `ui/src/pages/Troubleshoot.test.tsx`, `ui/src/i18n/tr.json`, `ui/src/theme/theme.css`

**Interfaces:**
- Consumes: Task 3 komutları/olayı.
- Produces (`api.ts`): `FixPlan.ops: string[]`; `type ProgressMode = "live" | "staged"`; `interface Settings { progress_mode: ProgressMode }`; `api.applyFix(fix, arg?, withProgress = false)` (komuta `with_progress` gönderir); `api.getSettings()`, `api.setSettings(mode)`; `api.onFixProgress(cb: (p: { done: number; total: number }) => void)` (`fix-progress` olayı). `fixtures.plan()` varsayılanı `ops: ["fix.clean_temp.op.1"]`.
- Produces (`Working.tsx`): `Working({ title, step, total, plan, stage, mode })`; `stage: "applying" | "checking"`; `mode: ProgressMode`. `plan.ops` işlem listesi; `<ol>` içinde her satırın `data-state="done|current|pending"`; `role="progressbar"` `aria-valuenow` 0–100. Canlı modda `api.onFixProgress` ile `done` günceller ve `done/(n+1)` gösterir; `checking` aşamasında `n/(n+1)`. Aşamalı modda `applying` sırasında değer her 1 s'de kalan mesafenin yarısı kadar `n/(n+1)`'e yaklaşır (asla ulaşmaz), `checking`'de `1`'e yaklaşır; tüm satırlar `current` ya da `pending` kalır (`applying`'de ilk satır `current`, `checking`'de hepsi `done`). `plan.needs_admin` iken izin notu görünür. `prefers-reduced-motion` iken çubuk geçişi CSS'te kapalı.
- Produces (`Troubleshoot.tsx`): onaydan sonra (`busy`) ve `troubleshootAfterFix` sürerken `Working` gösterilir; mod sayfa açılırken `api.getSettings()` ile alınır (hata → `live`); `api.applyFix(fix, undefined, true)`.

- [ ] **Step 1: Başarısız testler (`Working.test.tsx`).** `live_mode_marks_operations_as_events_arrive` (`onFixProgress` sahte olarak olay geri çağrısını yakalar; başta ilk satır `current`; `{done:1,total:2}` sonrası ilk `done`, ikinci `current`, `aria-valuenow` = `round(100*1/3)`); `live_mode_without_events_keeps_the_first_operation_current_and_moves_with_the_stage` (`checking` → bütün satırlar `done`, değer `round(100*2/3)`); `staged_mode_eases_toward_but_never_reaches_the_applying_target` (sahte zamanlayıcı: 1 s sonra değer > 0 ve < `round(100*2/3)`, 10 s sonra yine `< round(100*2/3)`; satırlarda `done` yok); `staged_checking_stage_moves_toward_complete`; `shows_the_admin_note_only_for_admin_fixes`; `shows_title_and_step` (`Adım 2 / 4`). `Troubleshoot.test.tsx`: `shows_the_waiting_screen_while_the_fix_runs` (`applyFix` bekleyen söz → `progressbar` görünür) ve `shows_the_waiting_screen_while_rechecking` (`troubleshootAfterFix` bekleyen söz → `progressbar`, sonra sonuç kartı); mevcut testler `getSettings`/`onFixProgress` sahtelerini (`vi.spyOn`) ekler.
- [ ] **Step 2: Çalıştır.** `npm --prefix ui test -- --run` → başarısız.
- [ ] **Step 3: Uygula.** `api.ts` değişiklikleri, `fixtures.ts`, `Working.tsx`, `Troubleshoot.tsx` (`Phase`'e `checking` eklenir: `{ name: "checking"; plan: FixPlan; s: TroubleshootState }`; `done()` içinde `loading` yerine bu faz). Aşama etiketleri `ui.working.applying` ("Onarım uygulanıyor"), `ui.working.checking` ("Sonuç denetleniyor"), `ui.working.admin_note` ("Windows yönetici izni isterse onaylayın."), `ui.working.progress` (çubuğun erişilebilir adı "İlerleme") `tr.json`'da. Stiller `theme.css`'te (`.working`, `.working-bar`, satır durumları, `@media (prefers-reduced-motion: reduce)`).
- [ ] **Step 4: Doğrula ve commit.** `npm --prefix ui test -- --run`, `npm --prefix ui run build` → geçer. `feat(ui): waiting screen with operation list and progress bar`

### Task 5: Ana sayfa ayarı ve sürüm 0.3.16

**Files:**
- Modify: `ui/src/pages/Home.tsx`, `ui/src/pages/Home.test.tsx`, `ui/src/i18n/tr.json`, `ui/src/theme/theme.css`
- Create: `release-notes/v0.3.16.md`
- Modify: sürüm dosyaları (`node scripts/bump-version.mjs` yazar)

**Interfaces:**
- Consumes: `api.getSettings`, `api.setSettings` (Task 4).
- Produces: Ana sayfada "İlerleme ekranı" bölümü: iki seçenek (`radio`, adlar "Canlı", "Aşamalı"); açılışta `getSettings` ile seçili gelir (hata → Canlı); seçim `setSettings` çağırır, hata olursa seçim yine de arayüzde kalır ve kısa bir hata metni görünür.

- [ ] **Step 1: Başarısız testler (`Home.test.tsx`).** `setting_defaults_to_live_when_reading_fails` (`getSettings` hata → "Canlı" seçili); `setting_shows_the_saved_mode` (`staged` → "Aşamalı" seçili); `choosing_a_mode_saves_it` (`setSettings` `"staged"` ile çağrılır); `a_failed_save_keeps_the_choice_and_says_so`. Mevcut Home testleri `getSettings` sahtesi ekler.
- [ ] **Step 2: Çalıştır.** `npm --prefix ui test -- --run Home` → başarısız.
- [ ] **Step 3: Uygula.** `Home.tsx` bölümü; metinler `tr.json` (`ui.home.progress_title`, `ui.home.progress_live`, `ui.home.progress_staged`, `ui.home.progress_hint` bir cümle: Canlı gerçek durumu, Aşamalı aşamaları gösterir, `ui.home.progress_save_error`); stil `theme.css`.
- [ ] **Step 4: Sürüm.** `node scripts/bump-version.mjs` → çıktı `version -> 0.3.16`. `release-notes/v0.3.16.md`: `v0.3.15.md` gibi kısa Türkçe maddeler; sorun gidericilerde bekleme ekranı (işlem listesi, adım bilgisi, ilerleme çubuğu) ve Ana sayfadan Canlı/Aşamalı seçimi.
- [ ] **Step 5: Tam doğrulama.** `npm --prefix ui run build`, `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `npm --prefix ui test -- --run` → hepsi geçer.
- [ ] **Step 6: Commit.** `feat: waiting screen for troubleshooters with live or staged progress (0.3.16)` (`git add` yalnızca bu görevin dosyaları, sürüm dosyaları, `Cargo.lock` ve sürüm notu).
