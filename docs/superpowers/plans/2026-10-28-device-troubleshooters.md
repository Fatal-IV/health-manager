# Ses, yazıcı, Bluetooth ve kamera sorun gidericileri Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Mevcut sorun giderici iskeletine dört servis tabanlı sorun giderici eklemek: ses, yazıcı, Bluetooth, kamera.

**Architecture:** Her sorun giderici `troubleshoot.rs` içinde bir tanı testi listesi ve `FixId` adımlarından oluşur. Beş yeni onarım `FixId` olarak kayıt defterine, yetkili yardımcıdaki izin listesine ve `tr.json`'a eklenir. Arayüz kimlikten veri çektiği için sayfalar değişmez; yalnızca kimlik listesi tek kaynağa indirilir.

**Tech Stack:** Rust (health-core, fix-helper, Tauri), React + TypeScript + Vitest, `tr.json` metinleri.

**Spec:** `docs/superpowers/specs/2026-10-28-device-troubleshooters-design.md`

## Global Constraints

- Servis adları yardımcıda koda sabit yazılır: `Audiosrv`, `AudioEndpointBuilder`, `Spooler`, `bthserv`, `FrameServer`. Dışarıdan gelen hiçbir değerle komut kurulmaz.
- Beş yeni `FixId` yönetici ister (`needs_admin` true), geri alınamaz (`reversible` false), argüman almaz (`takes_arg` false; argüman verilirse `Err`).
- Windows dışı derlemede her yeni yardımcı işlev `keys::NOT_AVAILABLE` döner.
- Çalıştırılamayan test "bilinmiyor" sayılır, asla "iyi" sayılmaz.
- Servis yeniden başlatma: `net.exe stop <servis> /y` listedeki sırayla, sonra `net.exe start <servis>` ters sırayla.
- Arayüz metinleri Türkçe ve `ui/src/i18n/tr.json` içinde; kodda düz metin yok.
- Her görev kendi commit'iyle biter; commit mesajı oturumun atıf satırlarıyla bitirilir. `git add` ile yalnızca görevin dosyaları alınır. `ui/src/theme/theme.css`'teki bekleyen üst boşluk değişikliği bu plana ait değildir, commit'lere karışmaz.
- Sürüm 0.3.15 yalnızca son görevde yükseltilir.

## Review Focus

- Yeni onarımlara argüman verilirse (kabuk karakterleri dahil) hiçbir şey çalışmadan reddedilir. Test: `actions.rs` ve `allowlist.rs`.
- Servis durumu okunamazsa sonuç "iyi" olmaz; `Suggest { unmeasured: true }` döner. Test: `troubleshoot.rs`.
- Hiç var olmayan (`Missing`) ses ya da yazdırma servisi arıza sayılır, sessizce geçmez. Test: `troubleshoot.rs`.
- Kuyruk temizleme yalnızca `.SPL` ve `.SHD` dosyalarını siler; `.txt`, uzantısız ya da `x.spl.exe` gibi adlara dokunmaz. Test: `repair.rs` içinde `is_spool_file`.
- Durdurma başarılı, başlatma başarısız olursa hata dönmeli; durdurma hatası başlatmayı engellememeli. Test: `repair.rs` içinde `restart_with`.

---

### Task 1: Tek kimlik kaynağı

**Files:**
- Modify: `crates/health-core/src/troubleshoot.rs` (`TroubleshooterId`, `from_str`, testler)
- Modify: `ui/src/api.ts:98` (tip), `ui/src/pages/TroubleshootPicker.tsx:5`, `ui/src/pages/Overview.tsx:4`
- Modify: `src-tauri/src/commands.rs:384` (test listesi)
- Test: `ui/src/pages/TroubleshootPicker.test.tsx`, `ui/src/pages/Overview.test.tsx`

**Interfaces:**
- Produces: `TroubleshooterId::ALL: [TroubleshooterId; N]` (şimdi `N = 4`, her görev bir artırır); `from_str` bu sabitten okur. TS: `export const TROUBLESHOOTERS = [...] as const;` ve `export type TroubleshooterId = (typeof TROUBLESHOOTERS)[number];`, ikisi de `ui/src/api.ts` içinde, `TROUBLESHOOTERS` tek satırda yazılır.

- [ ] **Step 1: Başarısız testler (Rust).** `troubleshoot.rs` testlerine: `ids_match_the_interface_list` (`include_str!("../../../ui/src/api.ts")` içinde `export const TROUBLESHOOTERS` ile başlayan satırdaki tırnaklı kimlikler, `TroubleshooterId::ALL.map(as_str)` ile aynı sırada eşit) ve `every_troubleshooter_has_its_texts_in_tr_json` (her `ALL` kimliği için `ui.troubleshoot.titles.<id>`, `descriptions.<id>`, `texts.<id>.healthy|resolved|exhausted` ve her adımın `ui.troubleshoot.why.<fix.as_str()>` anahtarı `tr.json`'da boş olmayan metin; arama kalıbı `fixes.rs`'teki `every_plan_key_exists_in_tr_json` ile aynı).
- [ ] **Step 2: Çalıştır.** `cargo test -p health-core troubleshoot` → derleme hatası (`ALL` yok), sonra tip hatası.
- [ ] **Step 3: Uygula.** `TroubleshooterId::ALL` sabitini ekle; `from_str` `ALL.into_iter()` kullansın. `api.ts`'e `TROUBLESHOOTERS` sabitini taşı, tipi ondan türet. `TroubleshootPicker.tsx` ve `Overview.tsx` diziyi `../api`'den alsın. `commands.rs` testi bir eşleme listesi yerine `TroubleshooterId::ALL` üzerinde dönsün ve beklenen `tested` değeri `!["system_files", "bluetooth", "camera"].contains(&id.as_str())` olsun (testsiz kimlikler spec'te sabit). Sonraki görevlerde bu test değişmez.
- [ ] **Step 4: Arayüz testlerini genelleştir.** `TroubleshootPicker.test.tsx`: dört kimliği elle sayan test, `TROUBLESHOOTERS` üzerinde dönüp her başlık için bir düğme arayan teste dönüşsün ve `onPick` çağrısını son kimlikle doğrulasın. `Overview.test.tsx`: `has a row for every troubleshooter` testi ekle (her kimlik için `st()` döndüren `answers`, `TROUBLESHOOTERS` üzerinde `row(id)` mevcut). Böylece sonraki görevlerde arayüz testi değişmez.
- [ ] **Step 5: Doğrula.** `cargo test --workspace` ve `npm --prefix ui test -- --run` → geçer.
- [ ] **Step 6: Commit.** `refactor(troubleshoot): one source for the troubleshooter ids`

### Task 2: Ortak yeniden başlatma ve ses sorun gidericisi

**Files:**
- Modify: `crates/health-core/src/fixes.rs`, `crates/health-core/src/troubleshoot.rs`
- Modify: `crates/fix-helper/src/repair.rs`, `crates/fix-helper/src/actions.rs`
- Modify: `ui/src/api.ts` (`TROUBLESHOOTERS`'a `"audio"`), `ui/src/i18n/tr.json`
- Test: `crates/fix-helper/tests/allowlist.rs`

**Interfaces:**
- Produces (fixes.rs): `FixId::RestartAudioServices` (`as_str` `restart_audio_services`, `index` 13, `change_count` 2, `needs_admin` true). `ALL` 14 elemanlı olur.
- Produces (repair.rs, platformdan bağımsız): `fn restart_with(run: &mut dyn FnMut(&[&str]) -> Result<i32, &'static str>, services: &[&str]) -> Result<(), &'static str>`. Her servis için `run(&["stop", s, "/y"])`, sonra ters sırayla `run(&["start", s])` çağırır. Durdurma sonucu (hata ya da çıkış kodu) yok sayılır ve başlatma yine yapılır. Başlatma çıkış kodu 0 ya da 2 (zaten çalışıyor) değilse ya da `run` hata dönerse `Err(keys::FAILED)`. Windows dışı derlemede kullanılmayacağından `#[cfg_attr(not(windows), allow(dead_code))]`.
- Produces (repair.rs, `imp` ve `stub`): `pub fn restart_audio_services(log: &mut Vec<String>) -> Result<Params, &'static str>`; `imp` içinde `fn restart_services(services: &[&str], log) -> Result<Params, &'static str>` `run_exe("net.exe", …)` ile `restart_with` çağırır. Ses için sabit liste `["Audiosrv", "AudioEndpointBuilder"]` (durdurma sırası).
- Produces (troubleshoot.rs): `Test::AudioServices`; `TroubleshooterId::Audio` (`as_str` `audio`); `AUDIO_STEPS = [Step { id: "audio.restart_services", fix: FixId::RestartAudioServices, recheck: Some(Test::AudioServices) }]`; `for_id(Audio)`: `diagnose: &[Test::AudioServices]`, `hints: &[]`. Ortak yardımcı `fn services_running(p: &dyn SystemProbe, names: &[&str]) -> Outcome` (hepsi `Running` ise Pass; biri `Stopped`/`Disabled`/`Missing` ise Fail ve Fail, Unknown'a baskın; okunamayan Unknown) sonraki görevlerde kullanılır.

- [ ] **Step 1: Başarısız testler (health-core).** `fixes.rs`: `witness` ve `expected_variants` listelerine yeni varyant; `device_fixes_have_expected_plans` testi (`needs_admin` true, `reversible` false, `changes.len() == 2`, `validate_arg(RestartAudioServices, Some("x"))` hata). `troubleshoot.rs`: yardımcı `audio_ok()` (`Audiosrv` ve `AudioEndpointBuilder` `Running`); `audio_is_healthy_when_both_services_run` (`Healthy`); `audio_stopped_disabled_or_missing_suggests_the_restart` (`Audiosrv` için üç durum ve hiç servis tanımsız `FakeProbe::default()` → `suggest(0)`, `steps[0].fix == RestartAudioServices`); `audio_unreadable_services_are_unmeasured_not_healthy` (`services_error: Some(ProbeError::Unavailable("x".into()))` → `State::Suggest { step: 0, unmeasured: true }`); `audio_restart_resolves_when_services_run_again` (`after_fix(0, &audio_ok())` → `Resolved { reboot: false }`, servis durmuşken → `Exhausted`). `ids_parse_and_every_id_has_a_troubleshooter` listesine `("audio", 1)`.
- [ ] **Step 2: Başarısız testler (fix-helper).** `repair.rs` testleri, bir kaydedici `run` ile: `restart_with_stops_in_order_then_starts_in_reverse` (çağrılar `["stop","A","/y"]`, `["stop","B","/y"]`, `["start","B"]`, `["start","A"]`); `restart_with_ignores_stop_errors_and_still_starts`; `restart_with_accepts_start_exit_2_and_fails_on_other_codes` (2 → `Ok`, 1060 → `Err(keys::FAILED)`). `actions.rs`: `device_fixes_reject_an_argument_before_touching_anything` (`run(fix, Some("x"))` → `keys::INVALID`, her yeni `FixId` için). `allowlist.rs`: `device_fixes_with_an_argument_exit_2_and_run_nothing` (`--fix <id> --arg x` → çıkış kodu 2, `fix.result.invalid`; kabuk karakterli argüman dahil).
- [ ] **Step 3: Çalıştır.** `cargo test --workspace` → derleme hatası, sonra başarısız testler.
- [ ] **Step 4: Uygula.** `fixes.rs` içinde varyantı, `ALL` boyutunu, `index`, `takes_arg`, `as_str`, `change_count`, `needs_admin` eşlemelerini ekle. `repair.rs`: yukarıdaki imzalar; `actions.rs` `run` eşlemesine yeni varyant. `troubleshoot.rs`: test, kimlik, adım, `for_id` kolu, `ALL` boyutu 5, `Test::run` kolu `services_running(p, &["Audiosrv", "AudioEndpointBuilder"])`.
- [ ] **Step 5: Metinler ve kimlik.** `tr.json`: `fix.restart_audio_services.summary` ve `change.1|2` (ikinci satır yönetici izni, geri alınamaz, kayda geçer); `ui.troubleshoot.titles|descriptions.audio`; `texts.audio.healthy` (servislerin çalıştığını, sorunun aygıtta ya da ses ayarında olabileceğini söylemeli), `resolved`, `exhausted`; `why.restart_audio_services`. `api.ts` `TROUBLESHOOTERS` satırına `"audio"`.
- [ ] **Step 6: Doğrula.** `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `npm --prefix ui test -- --run` → geçer.
- [ ] **Step 7: Commit.** `feat(troubleshoot): audio troubleshooter`

### Task 3: Yazıcı sorun gidericisi

**Files:** Task 2 ile aynı dosyalar.

**Interfaces:**
- Consumes: `services_running`, `restart_services`, `restart_with` (Task 2).
- Produces: `FixId::RestartSpooler` (`restart_spooler`, index 14, `change_count` 2) ve `FixId::ClearPrintQueue` (`clear_print_queue`, index 15, `change_count` 3); `ALL` 16 elemanlı. repair.rs: `pub fn restart_spooler(log)`, `pub fn clear_print_queue(log)` (`imp` ve `stub`), platformdan bağımsız `fn is_spool_file(name: &std::ffi::OsStr) -> bool` (uzantısı büyük/küçük harf fark etmeden `spl` ya da `shd`; `.SPL` gibi yalnızca uzantı adı ve uzantısız adlar `false`). Troubleshoot: `Test::SpoolerRunning` (`services_running(p, &["Spooler"])`), `TroubleshooterId::Printer` (`printer`, `ALL` 6), `PRINTER_STEPS` = `print.restart_spooler` → `RestartSpooler` (recheck `SpoolerRunning`), `print.clear_queue` → `ClearPrintQueue` (recheck `SpoolerRunning`).

- [ ] **Step 1: Başarısız testler.** `repair.rs`: `is_spool_file_matches_only_spl_and_shd` (`00001.SPL`, `00001.shd` → true; `x.txt`, `x.spl.exe`, `spl`, `.SPL`, `00001.SPLX`, boş → false). `troubleshoot.rs`: `printer_*` testleri ses ile aynı kalıpta (sağlıklı, durmuş/devre dışı/yok, okunamadı → `unmeasured: true`), ek olarak `printer_restart_that_does_not_help_moves_to_the_queue_step` (`after_fix(0, &durmuş)` → `suggest(1)`, `steps[1].fix == ClearPrintQueue`, `after_fix(1, &durmuş)` → `Exhausted`). `fixes.rs`: `device_fixes_have_expected_plans` iki yeni onarımı kapsasın (`ClearPrintQueue` için `changes.len() == 3`). `actions.rs` ve `allowlist.rs` argüman testleri iki yeni kimliği kapsasın. Adım sayıları listesine `("printer", 2)`.
- [ ] **Step 2: Çalıştır.** `cargo test --workspace` → başarısız.
- [ ] **Step 3: Uygula.** `clear_print_queue` (`imp`): `Spooler`'ı `net stop /y` ile durdur; kökü `final_path(system32_dir()/spool/PRINTERS)` ile al, bulunamazsa `ITEM_NOT_FOUND` ama yine de servisi başlat; klasördeki normal dosyalardan `is_spool_file` olanları `winfs::delete_verified(path, &root)` ile sil (alt klasör ve bağlantılara dokunma); silinen sayıyı `Params`'a `deleted` olarak ver; her durumda `Spooler`'ı başlat (`clean_update_leftovers` ve `reset_windows_update` kalıbı). `restart_spooler`, `restart_services(&["Spooler"], log)`.
- [ ] **Step 4: Metinler ve kimlik.** `tr.json`: iki onarımın `summary`/`change.N` (`clear_print_queue.change.1` bekleyen belgelerin silineceğini açıkça yazmalı), `why.restart_spooler` (en hafif adım), `why.clear_print_queue` (kuyrukta takılı belge olabilir), `titles|descriptions|texts.printer`. `api.ts`'e `"printer"`.
- [ ] **Step 5: Doğrula ve commit.** Task 2 Step 6 komutları → geçer. `feat(troubleshoot): printer troubleshooter`

### Task 4: Bluetooth sorun gidericisi

**Files:** Task 2 ile aynı dosyalar.

**Interfaces:**
- Consumes: `restart_services` (Task 2).
- Produces: `FixId::RestartBluetooth` (`restart_bluetooth`, index 16, `change_count` 2); `ALL` 17. repair.rs `pub fn restart_bluetooth(log)` (`bthserv`). `TroubleshooterId::Bluetooth` (`bluetooth`, `ALL` 7), `diagnose: &[]`, `BLUETOOTH_STEPS` = `bt.restart_service` → `RestartBluetooth`, `recheck: None`.

- [ ] **Step 1: Başarısız testler.** `troubleshoot.rs`: `bluetooth_has_no_test_so_it_always_offers_its_one_step` (`steps[0].fix == RestartBluetooth`, `recheck.is_none()`, `start(&FakeProbe::default()) == suggest(0)`, `after_fix(0, …) == Resolved { reboot: false }`, `skip(0) == Exhausted`). Adım sayıları listesine `("bluetooth", 1)`. `fixes.rs`, `actions.rs`, `allowlist.rs` testleri yeni kimliği kapsasın.
- [ ] **Step 2: Çalıştır, uygula, doğrula.** Task 2 kalıbıyla; metinler `tr.json`'da (`fix.restart_bluetooth.*`, `why.restart_bluetooth`, `titles|descriptions|texts.bluetooth`; `texts.bluetooth.healthy` testi olmadığını söylemeli, `system_files` gibi). `api.ts`'e `"bluetooth"`.
- [ ] **Step 3: Commit.** `feat(troubleshoot): bluetooth troubleshooter`

### Task 5: Kamera sorun gidericisi

**Files:** Task 2 ile aynı dosyalar.

**Interfaces:**
- Consumes: `restart_services` (Task 2).
- Produces: `FixId::RestartCameraService` (`restart_camera_service`, index 17, `change_count` 2); `ALL` 18. repair.rs `pub fn restart_camera_service(log)` (`FrameServer`). `TroubleshooterId::Camera` (`camera`, `ALL` 8), `diagnose: &[]`, `CAMERA_STEPS` = `cam.restart_service` → `RestartCameraService`, `recheck: None`.

- [ ] **Step 1: Başarısız testler.** Task 4 ile aynı kalıp (`camera_has_no_test_…`, `("camera", 1)`). `fixes.rs` `all_is_complete_and_distinct` 18 elemanla geçmeli.
- [ ] **Step 2: Çalıştır, uygula, doğrula.** Metinler: `fix.restart_camera_service.*`, `why.restart_camera_service`, `titles|descriptions|texts.camera`. `api.ts`'e `"camera"`.
- [ ] **Step 3: Commit.** `feat(troubleshoot): camera troubleshooter`

### Task 6: Sürüm 0.3.15 ve son doğrulama

**Files:**
- Modify: sürüm dosyaları (`node scripts/bump-version.mjs` yazar)
- Create: `release-notes/v0.3.15.md`

- [ ] **Step 1: Sürümü yükselt.** `node scripts/bump-version.mjs` → çıktı `version -> 0.3.15`.
- [ ] **Step 2: Sürüm notu.** `release-notes/v0.3.15.md`: `v0.3.14.md` gibi kısa Türkçe madde işaretleri; ses, yazıcı, Bluetooth ve kamera sorun gidericilerinin eklendiğini ve sorun giderici listesinin sekiz kart olduğunu söyler.
- [ ] **Step 3: Tam doğrulama.** `npm --prefix ui run build`, `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `npm --prefix ui test -- --run` → hepsi geçer.
- [ ] **Step 4: Commit.** `feat: audio, printer, bluetooth and camera troubleshooters (0.3.15)` (`git add` yalnızca sürüm dosyaları, `Cargo.lock` ve sürüm notu).
