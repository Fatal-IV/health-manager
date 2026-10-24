# Windows Sağlık Denetimi Aşama 1 Uygulama Planı

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Windows için sağlık taraması, tek tek onaylı düzeltme, sağlık puanı ve dışa aktarılabilir rapor sunan Tauri uygulamasının Aşama 1'ini kurmak.

**Architecture:** Cargo workspace içinde üç Rust crate'i: `health-core` (Check modülleri, puan, düzeltme planı; sistem erişimi `SystemProbe` trait'i arkasında), `fix-helper` (UAC ile başlatılan, yalnızca izin listesindeki düzeltme kimliklerini çalıştıran küçük exe) ve `app` (Tauri komutları). Arayüz React + TypeScript, metinler i18n dosyasından.

**Tech Stack:** Tauri 2, Rust (crate'ler: `windows`, `wmi`, `serde`, `thiserror`, `sysinfo`), React 18 + TypeScript + Vite, Vitest + Testing Library, i18next, `printpdf` (PDF dışa aktarma).

**Spec:** `docs/superpowers/specs/2026-10-24-pc-saglik-denetimi-design.md`

## Global Constraints

- Arayüz dili Türkçe; tüm kullanıcı metinleri `ui/src/i18n/tr.json` içinde, koda gömülü metin yok.
- Tarama yetkisiz çalışır; yönetici yetkisi yalnızca onaylı bir düzeltme için, `fix-helper` süreciyle istenir.
- `fix-helper` serbest komut kabul etmez; yalnızca `FixId` enum'unda tanımlı kimlikleri çalıştırır, bilinmeyen kimlik reddedilir.
- Her düzeltme tek tek onaylanır; toplu "tümünü düzelt" yoktur.
- Hızlı tarama SFC/DISM içermez; bunlar "Derin tarama"dır.
- Sağlık puanı 0-100 arası tamsayıdır.
- Hedef yaklaşık 12-15 kontrol; durumlar `Good | Warning | Critical | Unknown`.
- Sistem durumu okuma metin ayrıştırmaya değil API/WMI'ya dayanır (Windows dili Türkçe olmayabilir).

## Review Focus

- Bir kontrol hata verirse (WMI erişilemez): yalnızca o kart "kontrol edilemedi" olur, tarama çökmez.
- UAC reddedildi (Win32 hata 1223): "Yapılmadı" sonucu döner, durum değişmez.
- Sistem İngilizce/başka dilde: servis ve Defender durumu yerelleşmiş metinden değil kodlardan okunur.
- Birden fazla / çıkarılabilir / boyutu 0 görünen sürücü: bölme hatası veya yanlış uyarı yok.
- Yolda Türkçe karakter ve boşluk (ör. `Mini Yönetim Uygulaması`): rapor dışa aktarma ve günlük yazma çalışır.
- Tarama ortasında iptal: kısmi sonuç puanı yanlış yüksek göstermez, arayüz tutarlı kalır.

---

## Dosya yapısı

```
Cargo.toml                         # workspace
crates/health-core/src/{lib,model,scoring,engine,fixes,report}.rs
crates/health-core/src/probe/{mod,windows,fake}.rs
crates/health-core/src/checks/{mod,storage,performance,update,security,system}.rs
crates/fix-helper/src/{main,actions,log}.rs
src-tauri/src/{main,commands,elevate}.rs   # app crate
ui/src/{main.tsx,App.tsx}
ui/src/i18n/{index.ts,tr.json}
ui/src/theme/theme.css
ui/src/api.ts                      # Tauri invoke sarmalayıcıları + tipler
ui/src/pages/{Home,Scanning,Results,FixPreview,Report}.tsx
ui/src/components/{ScoreRing,CategoryCard,IssueItem}.tsx
```

---

### Task 1: İskele ve çalışan boş uygulama

**Files:**
- Create: `Cargo.toml`, `crates/health-core/Cargo.toml`, `crates/fix-helper/Cargo.toml`, `src-tauri/*`, `ui/*`
- Test: `crates/health-core/src/lib.rs` (smoke), `ui/src/App.test.tsx`

**Interfaces:**
- Produces: `health_core::version() -> &'static str`; Tauri komutu `app_version` ; `App` bileşeni "Sağlık Denetimi" başlığını render eder.

- [ ] **Step 1:** Workspace ve üç crate'i, `ui/` altında Vite React-TS projesini ve Tauri 2 yapılandırmasını oluştur (`cargo tauri init`; frontendDist `ui/dist`).
- [ ] **Step 2: Başarısız testleri yaz:** Rust `version_is_not_empty`; Vitest `App renders app title` (`getByText("Sağlık Denetimi")`, metin i18n'den).
- [ ] **Step 3:** `version()` ve `App`'i minimal uygula; `tr.json`'a `app.title` ekle.
- [ ] **Step 4:** Çalıştır: `cargo test -p health-core` ve `npm --prefix ui test -- --run`; Expected: PASS. `cargo tauri dev` pencereyi açar.
- [ ] **Step 5:** Commit `chore: scaffold tauri workspace`.

---

### Task 2: Veri modeli ve sağlık puanı

**Files:**
- Create: `crates/health-core/src/model.rs`, `crates/health-core/src/scoring.rs`
- Test: `crates/health-core/src/scoring.rs` (`#[cfg(test)]`)

**Interfaces:**
- Produces:
  - `enum Category { Storage, Performance, Update, Security, System }`
  - `enum Status { Good, Warning, Critical, Unknown }`
  - `struct Issue { id: String, category: Category, status: Status, title_key: String, why_key: String, evidence: String, fix: Option<FixId> }` (`FixId` Task 9'da tanımlanır; bu task'ta `pub struct FixId(pub &'static str)` yer tutucu değil, Task 9'da enum'a çevrilir. Burada `fix: Option<String>` kullan, Task 9 tipini değiştirir.)
  - `struct CheckResult { check_id: String, category: Category, issues: Vec<Issue>, error: Option<String> }`
  - `fn score(results: &[CheckResult]) -> Score` ; `struct Score { value: u8, breakdown: Vec<ScoreItem> }` ; `struct ScoreItem { issue_id: String, penalty: u8 }`

- [ ] **Step 1: Başarısız testler:** `no_issues_scores_100`, `critical_security_costs_more_than_warning_temp_files` (aynı sonuç setinde güvenlik kritik > depolama uyarısı ceza), `score_never_below_zero`, `unknown_results_do_not_inflate_score` (hata veren kontroller puandan 100 üstüne çıkarmaz; yalnızca Unknown içeren set için `value` ≤ 100 ve breakdown'da ceza 0), `breakdown_sums_to_penalty` (100 − value == toplam ceza, 0 alt sınırı hariç).
- [ ] **Step 2:** Çalıştır, başarısızlığı doğrula.
- [ ] **Step 3: Uygula** `score`. Ağırlık tablosu: Critical güvenlik 30, Critical diğer 15, Warning güvenlik 10, Warning diğer 5; Good/Unknown 0. Toplam 100'e kırpılır.
- [ ] **Step 4:** `cargo test -p health-core scoring`; Expected: PASS.
- [ ] **Step 5:** Commit `feat(core): data model and health score`.

---

### Task 3: SystemProbe ve tarama motoru

**Files:**
- Create: `crates/health-core/src/probe/{mod,fake}.rs`, `crates/health-core/src/checks/mod.rs`, `crates/health-core/src/engine.rs`
- Test: `crates/health-core/src/engine.rs`

**Interfaces:**
- Produces:
  - `trait SystemProbe: Send + Sync` (metotlar sonraki task'larda genişler; bu task'ta `fn disks(&self) -> Result<Vec<DiskInfo>, ProbeError>`). `struct DiskInfo { mount: String, total_bytes: u64, free_bytes: u64, removable: bool }`
  - `trait Check: Send + Sync { fn id(&self) -> &'static str; fn category(&self) -> Category; fn scan(&self, p: &dyn SystemProbe) -> Result<Vec<Issue>, ProbeError>; fn deep(&self) -> bool { false } }`
  - `fn run_scan(checks: &[Box<dyn Check>], p: &dyn SystemProbe, mode: ScanMode, progress: &dyn Fn(Progress), cancel: &AtomicBool) -> ScanOutcome` ; `enum ScanMode { Quick, Deep }`; `struct Progress { done: usize, total: usize, category: Category }`; `struct ScanOutcome { results: Vec<CheckResult>, cancelled: bool }`
  - `FakeProbe` (test için ayarlanabilir değerler ve hata enjeksiyonu)

- [ ] **Step 1: Başarısız testler:** `failing_check_does_not_abort_scan` (bir Check `Err` döner → o `CheckResult.error` dolu, diğerleri çalışır), `quick_scan_skips_deep_checks`, `deep_scan_includes_deep_checks`, `cancel_stops_and_marks_cancelled`, `progress_reports_each_check`.
- [ ] **Step 2:** Çalıştır, başarısızlığı doğrula.
- [ ] **Step 3: Uygula** `run_scan`; her Check'i `catch_unwind` ile de sar (panik tek kartı etkiler). İptalde `cancelled = true` döner; kısmi sonuçlar puana verilmeden önce arayüz "tamamlanmadı" gösterecek (bayrak Task 12'de taşınır).
- [ ] **Step 4:** `cargo test -p health-core engine`; Expected: PASS.
- [ ] **Step 5:** Commit `feat(core): probe trait and scan engine`.

---

### Task 4: Depolama kontrolleri

**Files:**
- Create: `crates/health-core/src/checks/storage.rs`, `crates/health-core/src/probe/windows.rs` (diskler, geçici dosya boyutu, geri dönüşüm kutusu boyutu, WU artıkları boyutu)
- Test: `crates/health-core/src/checks/storage.rs`

**Interfaces:**
- Consumes: `Check`, `SystemProbe::disks`
- Produces: `SystemProbe::temp_bytes(&self) -> Result<u64, ProbeError>`, `recycle_bin_bytes`, `update_leftovers_bytes`; `struct DiskSpaceCheck`, `TempFilesCheck`, `RecycleBinCheck`, `UpdateLeftoversCheck`.

- [ ] **Step 1: Başarısız testler:** `disk_over_90_percent_is_critical`, `disk_over_80_percent_is_warning`, `disk_with_zero_total_is_ignored` (bölme hatası yok), `removable_disks_are_skipped`, `multiple_disks_report_each_separately`, `temp_over_1gb_is_warning_with_fix`.
- [ ] **Step 2:** Çalıştır, başarısızlığı doğrula.
- [ ] **Step 3: Uygula** eşikleri: disk ≥%90 Critical, ≥%80 Warning; geçici dosya ≥1 GiB Warning; `fix` alanı sırasıyla `clean_temp`, `empty_recycle_bin`, `clean_update_leftovers`. Gerçek `WindowsProbe` boyutları Win32 (`GetDiskFreeSpaceExW`, `SHQueryRecycleBinW`) ile okur.
- [ ] **Step 4:** `cargo test -p health-core storage`; Expected: PASS.
- [ ] **Step 5:** Commit `feat(core): storage checks`.

---

### Task 5: Performans kontrolleri

**Files:**
- Create: `crates/health-core/src/checks/performance.rs`
- Test: aynı dosya

**Interfaces:**
- Produces: `SystemProbe::cpu_percent(&self)`, `ram_usage(&self) -> Result<(u64 used, u64 total), ProbeError>`, `top_processes(&self, n: usize) -> Result<Vec<ProcInfo>, ProbeError>`, `startup_items(&self) -> Result<Vec<StartupItem>, ProbeError>`; `struct ProcInfo { pid: u32, name: String, cpu: f32, ram_bytes: u64 }`, `struct StartupItem { id: String, name: String, impact: Impact }`; `CpuRamCheck`, `TopProcessesCheck`, `StartupItemsCheck`. `FixId` değerleri: `disable_startup_item`, `end_process`.

- [ ] **Step 1: Başarısız testler:** `ram_over_90_percent_is_critical`, `cpu_spike_single_sample_is_not_critical` (tek örnek yalnızca Warning), `more_than_10_high_impact_startup_items_is_warning`, `top_process_issue_names_process_in_evidence`, `zero_total_ram_is_unknown_not_panic`.
- [ ] **Step 2–4:** Başarısızlığı doğrula, uygula (`sysinfo` ile ölçüm; CPU için 500 ms arayla iki örnek), testleri geçir: `cargo test -p health-core performance`.
- [ ] **Step 5:** Commit `feat(core): performance checks`.

---

### Task 6: Güncelleme kontrolleri

**Files:**
- Create: `crates/health-core/src/checks/update.rs`
- Test: aynı dosya

**Interfaces:**
- Produces: `SystemProbe::pending_updates(&self) -> Result<u32, ProbeError>`, `service_state(&self, name: &str) -> Result<ServiceState, ProbeError>` (`enum ServiceState { Running, Stopped, Disabled, Missing }`, Service Control Manager'dan okunur, metinden değil), `last_update_success(&self) -> Result<Option<SystemTime>, ProbeError>`; `PendingUpdatesCheck`, `UpdateServiceCheck`, `LastUpdateCheck`. `FixId`: `reset_windows_update`.

- [ ] **Step 1: Başarısız testler:** `disabled_update_service_is_critical_with_reset_fix`, `no_update_in_60_days_is_warning`, `no_update_in_180_days_is_critical`, `never_updated_is_warning_not_crash` (`None`), `pending_updates_over_0_is_warning`.
- [ ] **Step 2–4:** Başarısızlığı doğrula, uygula (WUA COM API `IUpdateSearcher` çevrimdışı önbellekle; ağ yoksa `Unknown`), testleri geçir.
- [ ] **Step 5:** Commit `feat(core): update checks`.

---

### Task 7: Güvenlik kontrolleri

**Files:**
- Create: `crates/health-core/src/checks/security.rs`
- Test: aynı dosya

**Interfaces:**
- Produces: `SystemProbe::defender(&self) -> Result<DefenderState, ProbeError>` (`struct DefenderState { realtime: bool, signature_age_days: u32 }`, WMI `root\Microsoft\Windows\Defender` `MSFT_MpComputerStatus` alanlarından), `firewall_enabled(&self) -> Result<bool, ProbeError>`, `uac_level(&self) -> Result<UacLevel, ProbeError>` (registry `ConsentPromptBehaviorAdmin`/`EnableLUA`); `DefenderCheck`, `FirewallCheck`, `UacCheck`. `FixId`: `enable_defender_realtime`, `enable_firewall`.

- [ ] **Step 1: Başarısız testler:** `realtime_off_is_critical`, `signatures_older_than_7_days_is_warning`, `firewall_disabled_is_critical`, `uac_disabled_is_warning_without_auto_fix` (`fix == None`, yalnızca yönlendirme metni), `third_party_av_makes_defender_check_unknown_not_critical` (Defender başka AV nedeniyle kapalıysa yanlış alarm yok).
- [ ] **Step 2–4:** Başarısızlığı doğrula, uygula, testleri geçir.
- [ ] **Step 5:** Commit `feat(core): security checks`.

---

### Task 8: Sistem kontrolleri ve derin tarama

**Files:**
- Create: `crates/health-core/src/checks/system.rs`
- Test: aynı dosya

**Interfaces:**
- Produces: `SystemProbe::problem_devices(&self) -> Result<Vec<DeviceProblem>, ProbeError>` (CM_Get_DevNode_Status ile `ConfigManagerErrorCode != 0`), `boot_seconds(&self) -> Result<Option<u32>, ProbeError>` (Event Log 100 numaralı olay), `integrity_scan(&self) -> Result<IntegrityResult, ProbeError>` (SFC/DISM çıkış kodlarından; yalnızca derin); `ProblemDevicesCheck`, `BootTimeCheck`, `IntegrityCheck` (`deep() == true`). `FixId`: `run_sfc_dism`; sürücü sorununda `fix == None` ve yönlendirme metni.

- [ ] **Step 1: Başarısız testler:** `problem_device_is_warning_with_no_auto_fix`, `boot_over_90s_is_warning`, `missing_boot_event_is_unknown`, `integrity_check_is_deep_only`, `corrupt_files_found_is_critical_with_fix`.
- [ ] **Step 2–4:** Başarısızlığı doğrula, uygula (SFC çıktısı metin değil çıkış koduyla yorumlanır), testleri geçir.
- [ ] **Step 5:** Commit `feat(core): system checks and deep scan`.

---

### Task 9: Düzeltme kayıt defteri ve önizleme planı

**Files:**
- Create: `crates/health-core/src/fixes.rs`
- Modify: `crates/health-core/src/model.rs` (`Issue.fix: Option<FixId>`)
- Test: `crates/health-core/src/fixes.rs`

**Interfaces:**
- Produces:
  - `enum FixId { CleanTemp, EmptyRecycleBin, CleanUpdateLeftovers, DisableStartupItem, EndProcess, ResetWindowsUpdate, EnableDefenderRealtime, EnableFirewall, RunSfcDism }` ile `fn as_str(&self) -> &'static str` ve `FromStr` (`snake_case` kimlikleri; bilinmeyen kimlik `Err`)
  - `struct FixPlan { fix: FixId, summary_key: String, changes: Vec<String>, needs_admin: bool, reversible: bool }`
  - `fn fix_plan(fix: FixId, arg: Option<&str>) -> FixPlan`

- [ ] **Step 1: Başarısız testler:** `every_fix_id_roundtrips_through_str`, `unknown_fix_id_is_rejected`, `admin_requirements_match_spec` (CleanTemp/EmptyRecycleBin/DisableStartupItem: false; CleanUpdateLeftovers/ResetWindowsUpdate/EnableDefenderRealtime/EnableFirewall/RunSfcDism: true; EndProcess: false), `plan_lists_at_least_one_change`.
- [ ] **Step 2–4:** Başarısızlığı doğrula, uygula, testleri geçir; Task 4–8'deki `fix` alanlarını enum'a taşı ve tüm test paketini çalıştır: `cargo test -p health-core`.
- [ ] **Step 5:** Commit `feat(core): fix registry and previews`.

---

### Task 10: fix-helper (yükseltilmiş yardımcı)

**Files:**
- Create: `crates/fix-helper/src/{main,actions,log}.rs`
- Test: `crates/fix-helper/tests/allowlist.rs`

**Interfaces:**
- Consumes: `health_core::FixId` (FromStr)
- Produces: CLI `fix-helper --fix <id> [--arg <value>] --result <path>`. Çıkış kodu 0 başarı, 2 bilinmeyen kimlik, 3 uygulama hatası. `--result` yoluna JSON `{ "ok": bool, "message": string, "log": string }` yazar. `fn run(fix: FixId, arg: Option<&str>) -> Result<ActionReport, ActionError>` (`actions.rs`); `fn append_log(entry: &LogEntry)` (`%ProgramData%\SaglikDenetimi\logs\fix.log`).

- [ ] **Step 1: Başarısız testler:** `unknown_id_exits_with_code_2_and_runs_nothing`, `shell_metacharacters_in_arg_are_not_executed` (`--arg "x & calc"` argüman olarak işlenir; komut satırına birleştirilmez), `result_file_is_written_even_on_failure`, `log_written_to_path_with_unicode_and_spaces` (geçici dizin `Mini Yönetim Uygulaması`).
- [ ] **Step 2:** Çalıştır, başarısızlığı doğrula.
- [ ] **Step 3: Uygula** her `FixId` için bir eylem: dosya silmeler Rust'tan, servis işlemleri SCM API'si, WU sıfırlama servis durdur → `SoftwareDistribution`/`catroot2` yeniden adlandır → başlat, Defender/duvar `Set-MpPreference`/`netsh advfirewall` ile argüman dizisi (shell yok, `Command::args`). Geri alma: WU klasörleri `.bak` olarak saklanır; sistem değiştiren işlerden önce geri yükleme noktası denenir (başarısızlığı raporlanır, işi engellemez).
- [ ] **Step 4:** `cargo test -p fix-helper`; Expected: PASS.
- [ ] **Step 5:** Commit `feat(helper): allowlisted elevated fix runner`.

---

### Task 11: Yükseltme başlatıcısı ve düzeltme akışı

**Files:**
- Create: `src-tauri/src/elevate.rs`
- Test: `src-tauri/src/elevate.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: `FixPlan.needs_admin`, `fix-helper` CLI
- Produces: `fn apply_fix(fix: FixId, arg: Option<&str>) -> FixOutcome` ; `enum FixOutcome { Applied { message: String }, Declined, Failed { message: String, detail: String } }`. Yönetici gerekmeyen düzeltmeler UAC'siz `fix-helper`'ı normal başlatır; gerekenler `ShellExecuteExW` `runas` ile başlatır. Sonuç `--result` dosyasından okunur. UAC iptali (hata 1223) `Declined`.

- [ ] **Step 1: Başarısız testler** (başlatıcı `trait Launcher` arkasında): `uac_cancelled_maps_to_declined`, `non_admin_fix_does_not_request_elevation`, `admin_fix_requests_elevation`, `missing_result_file_maps_to_failed`, `helper_exit_2_maps_to_failed`.
- [ ] **Step 2–4:** Başarısızlığı doğrula, uygula, testleri geçir: `cargo test -p app elevate`.
- [ ] **Step 5:** Commit `feat(app): elevation launcher`.

---

### Task 12: Tauri komutları ve rapor modülü

**Files:**
- Create: `src-tauri/src/commands.rs`, `crates/health-core/src/report.rs`
- Modify: `src-tauri/src/main.rs`
- Test: `crates/health-core/src/report.rs`

**Interfaces:**
- Consumes: `run_scan`, `score`, `fix_plan`, `apply_fix`
- Produces:
  - Komutlar: `start_scan(mode: ScanMode) -> ScanReport` (ilerleme `scan-progress` olayıyla), `cancel_scan()`, `get_fix_plan(fix: String, arg: Option<String>) -> FixPlan`, `apply_fix_cmd(fix: String, arg: Option<String>) -> FixOutcome` (sonra ilgili kontrolü yeniden tarar ve `verified: bool` ekler), `export_report(format: ReportFormat, path: String) -> Result<(), String>`
  - `struct ScanReport { results: Vec<CheckResult>, score: Score, cancelled: bool, scanned_at: String }`
  - `fn render_text(r: &ScanReport, fixes: &[AppliedFix], t: &dyn Fn(&str) -> String) -> String`, `fn render_pdf(...) -> Result<Vec<u8>, ReportError>`; `struct AppliedFix { fix: FixId, outcome: String, at: String }`

- [ ] **Step 1: Başarısız testler:** `text_report_lists_applied_and_remaining_issues`, `cancelled_scan_report_says_incomplete_and_hides_score`, `report_writes_to_path_with_turkish_chars_and_spaces`, `pdf_report_is_valid_pdf_header` (`%PDF-`), `apply_fix_cmd_reports_unverified_when_rescan_still_shows_issue`.
- [ ] **Step 2–4:** Başarısızlığı doğrula, uygula (komutlar yalnızca `FixId::from_str` ile geçerli kimlikleri geçirir), testleri geçir: `cargo test --workspace`.
- [ ] **Step 5:** Commit `feat(app): tauri commands and report export`.

---

### Task 13: Arayüz temeli: i18n, tema, API katmanı

**Files:**
- Create: `ui/src/i18n/{index.ts,tr.json}`, `ui/src/theme/theme.css`, `ui/src/api.ts`, `ui/src/App.tsx` (router: home/scanning/results/fix/report)
- Test: `ui/src/i18n/i18n.test.ts`, `ui/src/api.test.ts`

**Interfaces:**
- Produces: `t(key: string, vars?: Record<string, string|number>): string` ; `api.startScan(mode)`, `api.cancelScan()`, `api.getFixPlan(fix, arg?)`, `api.applyFix(fix, arg?)`, `api.exportReport(format, path)`, `api.onProgress(cb)`; tipler `ScanReport`, `CheckResult`, `Issue`, `FixPlan`, `FixOutcome` Rust ile birebir.
- Tema: `prefers-color-scheme` takip eder; Mica benzeri yarı saydam yüzey, 12 px köşe yarıçapı, durum renkleri yeşil/sarı/kırmızı.

- [ ] **Step 1: Başarısız testler:** `every_key_used_in_src_exists_in_tr_json` (kaynaktaki `t("...")` anahtarlarını tarar), `missing_key_returns_key_and_logs_warning`, `api_maps_invoke_errors_to_typed_result`.
- [ ] **Step 2–4:** Başarısızlığı doğrula, uygula, testleri geçir: `npm --prefix ui test -- --run`.
- [ ] **Step 5:** Commit `feat(ui): i18n, theme and api layer`.

---

### Task 14: Ana ekran ve tarama ekranı

**Files:**
- Create: `ui/src/pages/{Home,Scanning}.tsx`, `ui/src/components/ScoreRing.tsx`
- Test: `ui/src/pages/Home.test.tsx`, `Scanning.test.tsx`

**Interfaces:**
- Consumes: `api.startScan`, `api.cancelScan`, `api.onProgress`
- Produces: `<Home onStart={(mode) => void} lastScan?: string, score?: number />`, `<Scanning mode onDone={(r: ScanReport) => void} onCancel={() => void} />`, `<ScoreRing value: number | null />` (null = "—").

- [ ] **Step 1: Başarısız testler:** `home_shows_first_run_welcome_only_once`, `start_button_calls_onStart_with_quick`, `deep_scan_option_calls_onStart_with_deep`, `scanning_shows_category_progress_from_events`, `cancel_calls_cancelScan_and_returns_home`, `score_ring_shows_dash_when_scan_incomplete`.
- [ ] **Step 2–4:** Başarısızlığı doğrula, uygula, testleri geçir.
- [ ] **Step 5:** Commit `feat(ui): home and scanning screens`.

---

### Task 15: Sonuç paneli ve düzeltme onayı

**Files:**
- Create: `ui/src/pages/{Results,FixPreview}.tsx`, `ui/src/components/{CategoryCard,IssueItem}.tsx`
- Test: `ui/src/pages/Results.test.tsx`, `FixPreview.test.tsx`

**Interfaces:**
- Consumes: `api.getFixPlan`, `api.applyFix`
- Produces: `<Results report onFix={(issue: Issue) => void} />`, `<FixPreview issue onConfirm onCancel />`. Her sorunda yalnızca tek "Düzelt" düğmesi; toplu düğme yoktur. Önizleme `FixPlan.changes` listesini ve `needs_admin` ise "Windows izin isteyecek" uyarısını gösterir.

- [ ] **Step 1: Başarısız testler:** `card_color_follows_worst_issue_status`, `unknown_check_shows_could_not_check_message`, `fix_button_absent_when_issue_has_no_fix`, `no_bulk_fix_button_exists`, `preview_lists_changes_before_confirm`, `declined_outcome_shows_nothing_changed_message`, `failed_outcome_hides_technical_detail_until_expanded`, `unverified_outcome_keeps_issue_open`.
- [ ] **Step 2–4:** Başarısızlığı doğrula, uygula, testleri geçir.
- [ ] **Step 5:** Commit `feat(ui): results and per-fix confirmation`.

---

### Task 16: Rapor ekranı ve dışa aktarma

**Files:**
- Create: `ui/src/pages/Report.tsx`
- Test: `ui/src/pages/Report.test.tsx`

**Interfaces:**
- Consumes: `api.exportReport`; Tauri dosya kaydet iletişim kutusu (`@tauri-apps/plugin-dialog` `save`)
- Produces: `<Report report appliedFixes: AppliedFix[] />` — özet, yapılan düzeltmeler, kalan sorunlar, "Neden bu puan?" açılır bölümü (`Score.breakdown`), "Metin olarak kaydet" / "PDF olarak kaydet".

- [ ] **Step 1: Başarısız testler:** `report_lists_applied_and_remaining`, `why_this_score_shows_each_penalty`, `export_buttons_call_api_with_chosen_path`, `export_cancelled_dialog_does_nothing`, `export_error_shows_friendly_message`.
- [ ] **Step 2–4:** Başarısızlığı doğrula, uygula, testleri geçir.
- [ ] **Step 5:** Commit `feat(ui): report screen and export`.

---

### Task 17: Uçtan uca doğrulama ve paketleme

**Files:**
- Create: `tests/e2e/flow.rs` (veya `ui/e2e/flow.spec.ts` WebDriver), `docs/RUNNING.md`
- Modify: `src-tauri/tauri.conf.json` (`fix-helper` exe'sini `externalBin` olarak paketle)

**Interfaces:**
- Consumes: tüm önceki task'lar

- [ ] **Step 1: Başarısız e2e testi:** `scan_fix_verify_flow` — sahte probe ile tara → depolama sorunu görünür → önizleme → onay → `Applied` → yeniden tarama sorunu kaldırır → rapor "yapılan" listesinde görünür.
- [ ] **Step 2:** Sahte probe'u uygulamaya `HEALTH_FAKE_PROBE=1` ortam değişkeniyle bağla (yalnızca debug derlemede).
- [ ] **Step 3:** Testi geçir. Gerçek Windows üzerinde elle kontrol: tara, geçici dosya temizle (UAC'siz), WU sıfırla (UAC çıkar; reddedince "değişiklik yapılmadı"), rapor PDF kaydet (klasör adı Türkçe karakterli).
- [ ] **Step 4:** `cargo tauri build`; kurulumun açıldığını ve `fix-helper`'ın pakette bulunduğunu doğrula.
- [ ] **Step 5:** Commit `test: e2e flow and packaging`.

---

## Kendi gözden geçirmem

- **Spec kapsamı:** Mimari (T1, 3, 10, 11, 12), kontrol tablosu (T4-8), puan (T2, 16), düzeltme önizleme ve tek tek onay (T9, 15), UAC modeli (T10, 11), i18n (T13), rapor/dışa aktarma (T12, 16), derin tarama (T3, 8), hata durumları (T3, 11, 15), görsel yön (T13), test stratejisi (T3 sahte probe, T10, T17).
- **Tür tutarlılığı:** `FixId` T2'de `Option<String>` olarak geçici başlar, T9'da enum'a çevrilir ve T4-8 buna taşınır (T9 Step 4'te açık).
- **Orantı:** Plan gövdeleri imza ve test adlarıyla sınırlı; uygulama gövdeleri verilmedi.
