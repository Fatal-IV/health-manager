//! Scan report: data type, plain-text and PDF rendering, export to a file.
//!
//! All user-facing words come from the i18n file through a translator closure
//! (`t`), so the report is in the same language as the UI. Issues carry
//! language-neutral `params`; `{{name}}` placeholders in the translated text are
//! filled from them.

use crate::fixes::FixId;
use crate::model::{Category, CheckResult, Issue, Status};
use crate::scoring::{score, Score};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ScanReport {
    pub results: Vec<CheckResult>,
    pub score: Score,
    /// A cancelled scan is incomplete: its score must not be shown.
    pub cancelled: bool,
    /// ISO-8601 UTC, e.g. `2026-10-24T09:30:00Z`.
    pub scanned_at: String,
}

impl ScanReport {
    pub fn new(results: Vec<CheckResult>, cancelled: bool, now: SystemTime) -> Self {
        let score = score(&results);
        ScanReport {
            results,
            score,
            cancelled,
            scanned_at: iso_utc(now),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct AppliedFix {
    pub fix: FixId,
    /// `applied` (verified by a re-scan) or `unverified`.
    pub outcome: String,
    pub at: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReportFormat {
    Text,
    Pdf,
}

#[derive(Debug, Error)]
pub enum ReportError {
    #[error("pdf: {0}")]
    Pdf(String),
    #[error("io: {0}")]
    Io(String),
    #[error("invalid path: {0}")]
    Path(String),
}

/// `YYYY-MM-DDTHH:MM:SSZ` in UTC (civil-from-days, Hinnant).
pub fn iso_utc(t: SystemTime) -> String {
    let secs = t
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Fills `{{name}}` placeholders; unknown placeholders are left as they are.
pub fn interpolate(template: &str, params: &BTreeMap<String, String>) -> String {
    let mut out = template.to_string();
    for (k, v) in params {
        out = out.replace(&format!("{{{{{k}}}}}"), v);
    }
    out
}

static TR: OnceLock<serde_json::Value> = OnceLock::new();

/// Looks a dotted key (`issue.storage.disk_full.title`) up in the UI's Turkish
/// strings; a missing key is returned as the key itself (visible, not blank).
pub fn translate(key: &str) -> String {
    let root = TR.get_or_init(|| {
        serde_json::from_str(include_str!("../../../ui/src/i18n/tr.json"))
            .unwrap_or(serde_json::Value::Null)
    });
    let mut cur = root;
    for part in key.split('.') {
        match cur.get(part) {
            Some(v) => cur = v,
            None => return key.to_string(),
        }
    }
    cur.as_str()
        .map(str::to_string)
        .unwrap_or_else(|| key.to_string())
}

fn category_key(c: Category) -> &'static str {
    match c {
        Category::Storage => "category.storage",
        Category::Performance => "category.performance",
        Category::Update => "category.update",
        Category::Security => "category.security",
        Category::System => "category.system",
    }
}

fn status_key(s: Status) -> &'static str {
    match s {
        Status::Good => "status.good",
        Status::Warning => "status.warning",
        Status::Critical => "status.critical",
        Status::Unknown => "status.unknown",
    }
}

fn issue_lines(i: &Issue, t: &dyn Fn(&str) -> String, out: &mut Vec<String>) {
    out.push(format!(
        "  - [{}] {}",
        t(status_key(i.status)),
        interpolate(&t(&i.title_key), &i.params)
    ));
    out.push(format!("    {}", interpolate(&t(&i.why_key), &i.params)));
    if !i.evidence.is_empty() {
        out.push(format!("    {}: {}", t("report.detail"), i.evidence));
    }
}

/// Plain-text report as lines (the PDF renderer reuses them).
pub fn render_lines(
    r: &ScanReport,
    fixes: &[AppliedFix],
    t: &dyn Fn(&str) -> String,
) -> Vec<String> {
    let mut out = vec![
        t("report.title"),
        format!("{}: {}", t("report.scanned_at"), r.scanned_at),
    ];
    if r.cancelled {
        out.push(t("report.incomplete"));
    } else {
        out.push(format!("{}: {}/100", t("report.score"), r.score.value));
    }

    out.push(String::new());
    out.push(t("report.applied"));
    if fixes.is_empty() {
        out.push(format!("  {}", t("report.none")));
    }
    for f in fixes {
        out.push(format!(
            "  - {} ({}, {})",
            t(&format!("fix.{}.summary", f.fix.as_str())),
            t(&format!("report.outcome.{}", f.outcome)),
            f.at
        ));
    }

    out.push(String::new());
    out.push(t("report.remaining"));
    let mut any = false;
    for c in [
        Category::Storage,
        Category::Performance,
        Category::Update,
        Category::Security,
        Category::System,
    ] {
        let issues: Vec<&Issue> = r
            .results
            .iter()
            .filter(|x| x.category == c)
            .flat_map(|x| x.issues.iter())
            .collect();
        if issues.is_empty() {
            continue;
        }
        any = true;
        out.push(format!("{}:", t(category_key(c))));
        for i in issues {
            issue_lines(i, t, &mut out);
        }
    }
    if !any {
        out.push(format!("  {}", t("report.none")));
    }

    let errors: Vec<&CheckResult> = r.results.iter().filter(|x| x.error.is_some()).collect();
    if !errors.is_empty() {
        out.push(String::new());
        out.push(t("report.not_checked"));
        for e in errors {
            out.push(format!(
                "  - {}: {}",
                e.check_id,
                e.error.as_deref().unwrap_or("")
            ));
        }
    }
    out
}

pub fn render_text(r: &ScanReport, fixes: &[AppliedFix], t: &dyn Fn(&str) -> String) -> String {
    let mut s = render_lines(r, fixes, t).join("\r\n");
    s.push_str("\r\n");
    s
}

/// Built-in PDF fonts cannot show ş ğ ı İ; used only when no system font is found.
fn fold_turkish(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'ş' => 's',
            'Ş' => 'S',
            'ğ' => 'g',
            'Ğ' => 'G',
            'ı' => 'i',
            'İ' => 'I',
            _ => c,
        })
        .collect()
}

fn wrap(line: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for word in line.split(' ') {
        if !cur.is_empty() && cur.chars().count() + 1 + word.chars().count() > width {
            out.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(word);
    }
    out.push(cur);
    out
}

#[cfg(windows)]
fn system_font() -> Option<std::fs::File> {
    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
    for name in ["segoeui.ttf", "arial.ttf"] {
        let p = Path::new(&root).join("Fonts").join(name);
        if let Ok(f) = std::fs::File::open(p) {
            return Some(f);
        }
    }
    None
}

#[cfg(not(windows))]
fn system_font() -> Option<std::fs::File> {
    None
}

pub fn render_pdf(
    r: &ScanReport,
    fixes: &[AppliedFix],
    t: &dyn Fn(&str) -> String,
) -> Result<Vec<u8>, ReportError> {
    use printpdf::{BuiltinFont, Mm, PdfDocument};
    let title = t("report.title");
    let (doc, p1, l1) = PdfDocument::new(&title, Mm(210.0), Mm(297.0), "1");
    let ext = system_font().and_then(|f| doc.add_external_font(f).ok());
    let builtin = if ext.is_none() {
        Some(
            doc.add_builtin_font(BuiltinFont::Helvetica)
                .map_err(|e| ReportError::Pdf(e.to_string()))?,
        )
    } else {
        None
    };
    let font = ext.as_ref().or(builtin.as_ref()).expect("one font is set");

    let mut layer = doc.get_page(p1).get_layer(l1);
    let (top, bottom, line_h) = (282.0_f32, 15.0_f32, 5.0_f32);
    let mut y = top;
    for raw in render_lines(r, fixes, t) {
        let text = if ext.is_some() {
            raw
        } else {
            fold_turkish(&raw)
        };
        for part in wrap(&text, 95) {
            if y < bottom {
                let (page, lay) = doc.add_page(Mm(210.0), Mm(297.0), "n");
                layer = doc.get_page(page).get_layer(lay);
                y = top;
            }
            layer.use_text(part, 10.0, Mm(15.0), Mm(y), font);
            y -= line_h;
        }
    }
    doc.save_to_bytes()
        .map_err(|e| ReportError::Pdf(e.to_string()))
}

/// Writes the report; the path must be absolute and not an existing directory.
pub fn write_report(
    format: ReportFormat,
    path: &Path,
    r: &ScanReport,
    fixes: &[AppliedFix],
    t: &dyn Fn(&str) -> String,
) -> Result<(), ReportError> {
    if !path.is_absolute() {
        return Err(ReportError::Path("path must be absolute".into()));
    }
    if path.is_dir() {
        return Err(ReportError::Path("path is a directory".into()));
    }
    let want = match format {
        ReportFormat::Text => "txt",
        ReportFormat::Pdf => "pdf",
    };
    let ok_ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case(want))
        .unwrap_or(false);
    if !ok_ext {
        return Err(ReportError::Path(format!("expected a .{want} file")));
    }
    let bytes = match format {
        // UTF-8 with BOM so Notepad shows Turkish letters correctly.
        ReportFormat::Text => {
            let mut b = vec![0xEF, 0xBB, 0xBF];
            b.extend_from_slice(render_text(r, fixes, t).as_bytes());
            b
        }
        ReportFormat::Pdf => render_pdf(r, fixes, t)?,
    };
    std::fs::write(path, bytes).map_err(|e| ReportError::Io(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn tr(k: &str) -> String {
        format!("<{k}>")
    }

    fn issue(id: &str, fix: Option<FixId>) -> Issue {
        Issue {
            id: id.into(),
            category: Category::Storage,
            status: Status::Warning,
            title_key: format!("t.{id}"),
            why_key: format!("w.{id}"),
            evidence: "C: 95% used".into(),
            params: BTreeMap::new(),
            fix,
        }
    }

    fn report(cancelled: bool) -> ScanReport {
        let results = vec![
            CheckResult {
                check_id: "storage.temp".into(),
                category: Category::Storage,
                issues: vec![issue("storage.temp_files", Some(FixId::CleanTemp))],
                error: None,
            },
            CheckResult {
                check_id: "system.boot".into(),
                category: Category::System,
                issues: vec![],
                error: Some("access denied".into()),
            },
        ];
        ScanReport::new(
            results,
            cancelled,
            UNIX_EPOCH + Duration::from_secs(1_729_762_200),
        )
    }

    fn applied() -> Vec<AppliedFix> {
        vec![AppliedFix {
            fix: FixId::EmptyRecycleBin,
            outcome: "applied".into(),
            at: "2026-10-24T09:31:00Z".into(),
        }]
    }

    #[test]
    fn text_report_lists_applied_and_remaining_issues() {
        let text = render_text(&report(false), &applied(), &tr);
        assert!(text.contains(
            "<fix.empty_recycle_bin.summary> (<report.outcome.applied>, 2026-10-24T09:31:00Z)"
        ));
        assert!(text.contains("[<status.warning>] <t.storage.temp_files>"));
        assert!(text.contains("<w.storage.temp_files>"));
        assert!(text.contains("<report.detail>: C: 95% used"));
        assert!(text.contains("<report.score>: 95/100"));
        // a check that could not run is listed, not silently dropped
        assert!(text.contains("<report.not_checked>"));
        assert!(text.contains("system.boot: access denied"));
        // applied section precedes the remaining section
        assert!(text.find("<report.applied>") < text.find("<report.remaining>"));
    }

    #[test]
    fn cancelled_scan_report_says_incomplete_and_hides_score() {
        let text = render_text(&report(true), &[], &tr);
        assert!(text.contains("<report.incomplete>"));
        assert!(!text.contains("<report.score>"));
        assert!(!text.contains("/100"));
    }

    #[test]
    fn empty_report_says_none_for_both_sections() {
        let r = ScanReport::new(vec![], false, UNIX_EPOCH);
        let text = render_text(&r, &[], &tr);
        assert_eq!(text.matches("<report.none>").count(), 2);
        assert!(text.contains("<report.score>: 100/100"));
    }

    #[test]
    fn placeholders_are_filled_from_params() {
        let mut params = BTreeMap::new();
        params.insert("drive".to_string(), "C:".to_string());
        assert_eq!(
            interpolate("Disk {{drive}} dolu {{x}}", &params),
            "Disk C: dolu {{x}}"
        );
    }

    #[test]
    fn iso_utc_formats_known_instants() {
        assert_eq!(iso_utc(UNIX_EPOCH), "1970-01-01T00:00:00Z");
        assert_eq!(
            iso_utc(UNIX_EPOCH + Duration::from_secs(1_709_164_800)),
            "2024-02-29T00:00:00Z"
        );
        assert_eq!(
            iso_utc(UNIX_EPOCH + Duration::from_secs(1_729_762_200)),
            "2024-10-24T09:30:00Z"
        );
    }

    #[test]
    fn translate_reads_ui_strings_and_falls_back_to_the_key() {
        assert_eq!(translate("app.title"), "Sağlık Denetimi");
        assert_eq!(translate("no.such.key"), "no.such.key");
    }

    #[test]
    fn every_report_key_exists_in_tr_json() {
        let seen = std::cell::RefCell::new(Vec::<String>::new());
        let record = |k: &str| {
            seen.borrow_mut().push(k.to_string());
            k.to_string()
        };
        let _ = render_lines(&report(false), &applied(), &record);
        let keys = seen.into_inner();
        // issue keys (t.*, w.*) belong to the fixture; every other key is static
        let static_keys: Vec<&String> = keys
            .iter()
            .filter(|k| !k.starts_with("t.") && !k.starts_with("w."))
            .collect();
        assert!(static_keys.len() >= 8, "renderer asked for {static_keys:?}");
        for k in static_keys {
            assert_ne!(translate(k), *k, "missing tr.json key {k}");
        }
        for c in ["storage", "performance", "update", "security", "system"] {
            assert_ne!(translate(&format!("category.{c}")), format!("category.{c}"));
        }
        for s in ["good", "warning", "critical", "unknown"] {
            assert_ne!(translate(&format!("status.{s}")), format!("status.{s}"));
        }
        for o in ["applied", "unverified"] {
            let k = format!("report.outcome.{o}");
            assert_ne!(translate(&k), k);
        }
    }

    #[test]
    fn report_writes_to_path_with_turkish_chars_and_spaces() {
        let dir = std::env::temp_dir().join("Mini Yönetim Uygulaması rapor");
        std::fs::create_dir_all(&dir).unwrap();
        let txt = dir.join("sağlık raporu.txt");
        write_report(ReportFormat::Text, &txt, &report(false), &applied(), &tr).unwrap();
        let bytes = std::fs::read(&txt).unwrap();
        assert_eq!(&bytes[..3], &[0xEF, 0xBB, 0xBF]);
        assert!(String::from_utf8(bytes[3..].to_vec())
            .unwrap()
            .contains("<report.title>"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_report_rejects_bad_paths() {
        let r = report(false);
        let rel = write_report(ReportFormat::Text, Path::new("rapor.txt"), &r, &[], &tr);
        assert!(matches!(rel, Err(ReportError::Path(_))));
        let dir = std::env::temp_dir();
        let isdir = write_report(ReportFormat::Text, &dir, &r, &[], &tr);
        assert!(matches!(isdir, Err(ReportError::Path(_))));
        let wrong_ext = std::env::temp_dir().join("rapor.exe");
        let e = write_report(ReportFormat::Text, &wrong_ext, &r, &[], &tr);
        assert!(matches!(e, Err(ReportError::Path(_))));
        assert!(!wrong_ext.exists());
        let pdf_as_txt = std::env::temp_dir().join("rapor.txt");
        let e = write_report(ReportFormat::Pdf, &pdf_as_txt, &r, &[], &tr);
        assert!(matches!(e, Err(ReportError::Path(_))));
    }

    #[test]
    fn pdf_report_is_valid_pdf_header() {
        let bytes = render_pdf(&report(false), &applied(), &translate).unwrap();
        assert!(bytes.starts_with(b"%PDF-"));
        assert!(bytes.len() > 500);
    }

    #[test]
    fn pdf_spills_onto_more_pages_for_long_reports() {
        let many: Vec<CheckResult> = (0..120)
            .map(|n| CheckResult {
                check_id: format!("c{n}"),
                category: Category::Storage,
                issues: vec![issue(&format!("i{n}"), None)],
                error: None,
            })
            .collect();
        let r = ScanReport::new(many, false, UNIX_EPOCH);
        let short = render_pdf(&report(false), &[], &tr).unwrap().len();
        let long = render_pdf(&r, &[], &tr).unwrap();
        assert!(long.starts_with(b"%PDF-"));
        assert!(long.len() > short);
    }

    #[test]
    fn wrap_splits_long_lines_without_losing_words() {
        let line = "bir iki üç dört beş altı yedi sekiz dokuz on";
        let parts = wrap(line, 12);
        assert!(parts.iter().all(|p| p.chars().count() <= 12));
        assert_eq!(parts.join(" "), line);
    }

    #[test]
    fn turkish_letters_are_folded_for_the_builtin_font() {
        assert_eq!(fold_turkish("Sağlık Şİ"), "Saglik SI");
    }
}
