# Ağ sorun gidericisi Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Kullanıcıya, ağ/internet sorununu sıralı ve onaylı adımlarla gideren bir "Sorun Gider" akışı vermek (tanı → en hafif düzeltme → yeniden test → sonraki).

**Architecture:** `health-core` içine saf Rust bir durum makinesi (`troubleshoot.rs`) ve üç salt-okunur probe testi eklenir. Düzeltmeler mevcut kapalı `FixId` listesine üç yeni üye olarak girer ve mevcut önizleme/onay/yardımcı hattından geçer. Arayüz yeni bir `Troubleshoot.tsx` sayfasıyla durum makinesini sürer.

**Tech Stack:** Rust (health-core, fix-helper, src-tauri), React + TypeScript + vitest, Tauri komutları.

**Spec:** `docs/superpowers/specs/2026-10-27-network-troubleshooter-design.md`

## Global Constraints

- Her düzeltme ayrı önizleme ve onayla çalışır; sorun giderici otomatik ilerlemez.
- Sorun giderici yalnızca `FixId` taşır; kullanıcı girdisinden komut satırı kurulmaz. Yeni düzeltmeler argümansızdır.
- Çalıştırılamayan test `Unknown` olur; `Unknown` asla `Pass` sayılmaz.
- Sürüm: 0.3.9 (`node scripts/bump-version.mjs patch`), `release-notes/v0.3.9.md`.
- Commit mesajı sonuna `Co-authored-by: vertablx <218491099+vertablx@users.noreply.github.com>` eklenir; Claude satırı eklenmez.
- Rust ve CRLF: dosyalar CRLF; düzenlemede satır sonlarını korumak için Edit aracı kullanılır.
- Her adımdan önce Graphify'a sorulur, sonra graph güncellenir (`graphify update .`).

## Review Focus

- Ağ hiç yokken (ağ geçidi yok): `Gateway` başarısız, `Internet` başarısız; ilk öneri yine `FlushDns` olur, ama özet "ağ bağlantısı yok" der.
- Tüm testler `Unknown` (probe hatası): `Healthy` denmez; arayüz "ölçülemedi" gösterir.
- `ResetWinsock` sonrası yeniden başlatma gerekir: durum `Resolved` olarak işaretlenmez, "yeniden başlatınca dene" notu çıkar.
- Kullanıcı yönetici izni penceresini reddeder: basamak atlanmış sayılır, sorun giderici durmaz ve çökmez.
- Aynı basamak iki kez önerilmez (yeniden test sürekli başarısız olsa bile durum makinesi ilerler ve `Exhausted` olur).

---

### Task 1: Probe — ağ testleri

**Files:**
- Modify: `crates/health-core/src/probe/mod.rs` (trait `SystemProbe`, `PartialEq` hata tipleri aynen kalır)
- Modify: `crates/health-core/src/probe/fake.rs`
- Modify: `crates/health-core/src/probe/windows.rs`

**Interfaces:**
- Produces: `SystemProbe::gateway_reachable(&self) -> Result<bool, ProbeError>`, `dns_resolves(&self) -> Result<bool, ProbeError>`, `internet_reachable(&self) -> Result<bool, ProbeError>`. `FakeProbe` alanları: `gateway: Option<bool>`, `gateway_error: Option<ProbeError>`, `dns: Option<bool>`, `dns_error`, `internet: Option<bool>`, `internet_error` (`None` = erişilebilir).

- [ ] **Step 1: Failing test** — `fake.rs` testlerine `network_probes_default_to_reachable` (üç çağrı `Ok(true)`) ve `network_probe_error_is_injected` (`dns_error: Some(ProbeError::Unavailable("x".into()))` → `dns_resolves()` `Err`).
- [ ] **Step 2: Run** `cargo test -p health-core network_probe` → derleme hatası (yöntem yok).
- [ ] **Step 3: Implement** trait yöntemleri (varsayılan gövde yok), `FakeProbe` alanları ve `pick(...)` ile uygulama.
- [ ] **Step 4: Windows probe.** `windows.rs`: `internet_reachable` = `TcpStream::connect_timeout` ile `1.1.1.1:53`, 2 sn; `dns_resolves` = `ToSocketAddrs` ile `www.msftconnecttest.com:80`; `gateway_reachable` = sabit bir PowerShell komutu (`(Get-NetRoute -DestinationPrefix 0.0.0.0/0 | Sort-Object RouteMetric | Select-Object -First 1).NextHop` → `Test-Connection -Count 1 -Quiet`, çıktı `True`/`False`); komut metni sabit, girdi yok. Çıktı ayrıştırılamazsa `ProbeError::Unavailable`.
- [ ] **Step 5: Run** `cargo test -p health-core` ve `cargo check --workspace` → geçer.
- [ ] **Step 6: Commit** `feat(probe): gateway, DNS and internet reachability probes`.

### Task 2: Yeni düzeltmeler — `FixId`

**Files:**
- Modify: `crates/health-core/src/fixes.rs`
- Test: aynı dosyadaki mevcut test modülü

**Interfaces:**
- Produces: `FixId::FlushDns`, `FixId::RenewIp`, `FixId::ResetWinsock` (`as_str`: `flush_dns`, `renew_ip`, `reset_winsock`); `ALL` 13 elemanlı olur, sıra `index` ile aynı.

- [ ] **Step 1: Failing test** — `new_network_fixes_have_expected_plans`: `fix_plan(FixId::FlushDns, None)` → `needs_admin == false`, `reversible == false`; `RenewIp` ve `ResetWinsock` → `needs_admin == true`, `reversible == false`; `changes` uzunlukları sırasıyla 1, 2, 2; `validate_arg(FixId::RenewIp, Some("x"))` → `Err`.
- [ ] **Step 2: Run** `cargo test -p health-core fixes` → FAIL.
- [ ] **Step 3: Implement** — üç varyantı enum'a, `ALL`'a (sabit boyut 13), `index`, `takes_arg` (false), `as_str`, `change_count`, `needs_admin` ekle (`FlushDns` hariç ikisi). Mevcut "ALL tüm varyantları kapsar" testindeki bağımsız liste güncellenir.
- [ ] **Step 4: Run** `cargo test --workspace` → derleme `fix-helper` ve `src-tauri` içindeki kapsamlı `match`'lerde kırılır; bu beklenen, Task 3'te giderilir. Bu adımda yalnızca `cargo test -p health-core` geçmeli.
- [ ] **Step 5: Commit** `feat(fixes): FlushDns, RenewIp, ResetWinsock`.

### Task 3: Yardımcı eylemleri

**Files:**
- Modify: `crates/fix-helper/src/actions.rs` (`run` içindeki `match`, `FlushDns`)
- Modify: `crates/fix-helper/src/repair.rs` (`renew_ip`, `reset_winsock`; `stub` modülüne iki `na` karşılığı)
- Modify: `src-tauri/src/elevate.rs` / `tests.rs` yalnızca kapsamlı `match` kırılıyorsa

**Interfaces:**
- Consumes: Task 2 `FixId` üyeleri; `run_exe(exe, args, log)` (repair.rs içindeki mevcut yardımcı).
- Produces: `repair::renew_ip(&mut Vec<String>) -> Result<Params, &'static str>`, `repair::reset_winsock(...)` aynı imza; `FlushDns` için `actions.rs` içinde `flush_dns` (yönetici gerekmez, `ipconfig.exe /flushdns`).

- [ ] **Step 1: Failing test** — `actions.rs` testlerine, Windows dışı derlemede `run(FixId::ResetWinsock, None)` → `Err` anahtarı `keys::NOT_AVAILABLE`; `run(FixId::RenewIp, Some("x"))` → `keys::INVALID` (argüman reddi).
- [ ] **Step 2: Run** `cargo test -p fix-helper` → FAIL (derleme).
- [ ] **Step 3: Implement** — `flush_dns`: `ipconfig.exe /flushdns`, çıkış 0 → `Ok`. `renew_ip`: `ipconfig.exe /release` sonra `/renew`; ikisi 0 değilse `Err(keys::FAILED)`. `reset_winsock`: `netsh.exe winsock reset`; başarıda `Params` içinde `reboot_required = "true"`. Çıkış kodu kalıbı `enable_firewall` ile aynı. Tüm komut argümanları sabittir.
- [ ] **Step 4: Run** `cargo test --workspace` → geçer; `tr.json` anahtar denetimi için Task 6 anahtarları gelene kadar yalnızca yeni `fix.*` anahtarları eksik uyarısı verebilir, bu durumda Task 6'daki anahtarları şimdi `tr.json`'a ekle (`fix.flush_dns.*`, `fix.renew_ip.*`, `fix.reset_winsock.*`: `summary`, `change.1..n`).
- [ ] **Step 5: Commit** `feat(helper): DNS flush, IP renew and Winsock reset`.

### Task 4: Sorun giderici çekirdeği

**Files:**
- Create: `crates/health-core/src/troubleshoot.rs`
- Modify: `crates/health-core/src/lib.rs` (`pub mod troubleshoot;`)

**Interfaces:**
- Consumes: Task 1 probe yöntemleri, Task 2 `FixId`.
- Produces:
  - `enum NetTest { Gateway, Dns, Internet }` + `fn run(self, &dyn SystemProbe) -> Outcome`
  - `enum Outcome { Pass, Fail, Unknown }`
  - `struct Step { id: &'static str, fix: FixId, recheck: NetTest }`
  - `fn network() -> Troubleshooter` (`diagnose = [Gateway, Dns, Internet]`, `steps = [flush_dns→Dns, renew_ip→Internet, reset_winsock→Internet]`)
  - `enum State { Healthy, Suggest { step: usize }, Resolved { reboot: bool }, Exhausted }` (`Serialize`)
  - `Troubleshooter::start(&self, &dyn SystemProbe) -> State`
  - `Troubleshooter::after_fix(&self, step: usize, &dyn SystemProbe) -> State`
  - `Troubleshooter::skip(&self, step: usize) -> State`

- [ ] **Step 1: Failing tests** (`FakeProbe`): `healthy_when_all_pass`; `first_step_resolves` (DNS bozuk: `dns: Some(false)`; `start` → `Suggest{0}`; probe düzelince `after_fix(0)` → `Resolved{reboot:false}`); `third_step_resolves_needs_reboot` (`after_fix(2)` düzelince `Resolved{reboot:true}`); `exhausted_after_last_step`; `unknown_is_not_healthy` (üç `*_error: Some(..)` → `start` `Healthy` DEĞİL, `Suggest{0}`); `skip_moves_on_and_exhausts` (`skip(2)` → `Exhausted`); `no_step_is_suggested_twice` (sürekli başarısız probe ile `after_fix(0)`→`Suggest{1}`→`Suggest{2}`→`Exhausted`).
- [ ] **Step 2: Run** `cargo test -p health-core troubleshoot` → FAIL.
- [ ] **Step 3: Implement** yukarıdaki imzalarla. `Unknown` ve `Fail` tanıda ve yeniden testte "geçmedi" sayılır; `Healthy` yalnızca üç testin hepsi `Pass` iken döner.
- [ ] **Step 4: Run** `cargo test -p health-core` → geçer.
- [ ] **Step 5: Commit** `feat(troubleshoot): network troubleshooter state machine`.

### Task 5: Tauri komutları ve API

**Files:**
- Modify: `src-tauri/src/commands.rs`, `src-tauri/src/main.rs` (`generate_handler!`)
- Modify: `ui/src/api.ts`

**Interfaces:**
- Consumes: Task 4 `network()`, `State`.
- Produces: komutlar `troubleshoot_start() -> Result<TroubleshootState, String>` ve `troubleshoot_after_fix(step: usize) -> Result<TroubleshootState, String>` ve `troubleshoot_skip(step: usize)`; yanıt `{ state: "healthy" | "suggest" | "resolved" | "exhausted", step?: number, fix?: string, reboot?: boolean, unmeasured: boolean }` (`unmeasured` = tanıdaki testlerden en az biri `Unknown`; `fix` = önerilen basamağın `FixId` metni). TS: `api.troubleshootStart()`, `api.troubleshootAfterFix(step)`, `api.troubleshootSkip(step)`.

- [ ] **Step 1: Failing test** — `commands.rs` testlerine `troubleshoot_start_maps_state` (FakeProbe ile `healthy` ve `suggest` eşlemesi; `suggest` yanıtında `fix == "flush_dns"`); `ui/src/api.test.ts` içinde `troubleshootStart` çağrısının `invoke("troubleshoot_start")`'ı çağırdığını doğrulayan test.
- [ ] **Step 2: Run** `cargo test -p saglik-denetimi troubleshoot` ve `npx vitest run src/api.test.ts` → FAIL.
- [ ] **Step 3: Implement** — durum eşleme saf bir fonksiyon (`fn to_response(&Troubleshooter, State) -> TroubleshootState`), komutlar gerçek probe'u kullanır; TS tipleri ve üç `api` fonksiyonu.
- [ ] **Step 4: Run** aynı komutlar → PASS.
- [ ] **Step 5: Commit** `feat(app): troubleshoot commands and api`.

### Task 6: Arayüz

**Files:**
- Create: `ui/src/pages/Troubleshoot.tsx`, `ui/src/pages/Troubleshoot.test.tsx`, `ui/src/components/StepCard.tsx`
- Modify: `ui/src/App.tsx` (yeni `View` üyesi `troubleshoot`, mevcut `fix` görünümüne geçiş ve dönüş), `ui/src/pages/Home.tsx` (giriş), `ui/src/i18n/tr.json`

**Interfaces:**
- Consumes: Task 5 `api.troubleshoot*`; mevcut `api.getFixPlan` / `FixPreview` akışı.
- Produces: `Troubleshoot({ onFix(fix: string): Promise<boolean>, onBack(): void })` — `onFix` mevcut önizleme/onay akışını açar, sonuç uygulandıysa `true` döner; reddedilirse/başarısızsa `false`.

- [ ] **Step 1: Failing tests** (`Troubleshoot.test.tsx`, `api` mock'lanır): `shows_healthy_screen`; `suggests_first_step_then_resolves` (başlat → `suggest` kartı → "Uygula" → `onFix` çağrılır → `troubleshootAfterFix` `resolved` → "Çözüldü"); `denied_fix_offers_retry_or_skip` (`onFix` `false` → "Yeniden dene" ve "Sonrakine geç" görünür, `skip` çağrılırsa `troubleshootSkip`); `exhausted_links_to_report`; `reboot_note_shown_when_resolved_with_reboot`; `unknown_shows_could_not_measure` (durum `suggest`, ama arayüz "ölçülemedi" metnini çıkaran alanı gösterir — yanıttaki `unmeasured` alanı true).
- [ ] **Step 2: Run** `npx vitest run src/pages/Troubleshoot.test.tsx` → FAIL.
- [ ] **Step 3: Implement** — `StepCard` (neden, ne değişecek, yönetici gerekir mi, basamak "2 / 3"), `Troubleshoot` durum sürücüsü; `App.tsx`: `onFix` içinde `api.getFixPlan` → mevcut `fix` görünümü; uygulama sonrası `troubleshoot` görünümüne dön (tarama ekranına DEĞİL); `Home.tsx`'e "Sorun Gider" düğmesi. Türkçe metinler `tr.json` içine `ui.troubleshoot.*`.
- [ ] **Step 4: Run** `npx vitest run` ve `npx tsc --noEmit` → hepsi geçer.
- [ ] **Step 5: Commit** `feat(ui): guided network troubleshooter`.

### Task 7: Sürüm

**Files:**
- Create: `release-notes/v0.3.9.md`
- Modify: sürüm dosyaları (`node scripts/bump-version.mjs patch`)

- [ ] **Step 1:** Sürüm notu (Türkçe, madde madde): "Sorun Gider: ağ/internet için sıralı, onaylı adımlar; DNS temizleme, IP yenileme, Winsock sıfırlama."
- [ ] **Step 2:** `node scripts/bump-version.mjs patch`, `cargo update --workspace --offline`.
- [ ] **Step 3: Run** `cargo test --workspace` ve `cd ui && npx vitest run && npx tsc --noEmit` → hepsi geçer.
- [ ] **Step 4:** `graphify update .`, commit `feat: network troubleshooter (0.3.9)`.
- [ ] **Step 5:** Yayın yalnızca kullanıcı onaylarsa: `node scripts/release.mjs --publish`.
