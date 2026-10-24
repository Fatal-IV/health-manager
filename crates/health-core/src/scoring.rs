use crate::model::{Category, CheckResult, Status};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ScoreItem {
    pub issue_id: String,
    pub penalty: u8,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Score {
    pub value: u8,
    pub breakdown: Vec<ScoreItem>,
}

fn weight(category: Category, status: Status) -> u32 {
    let security = category == Category::Security;
    match (status, security) {
        (Status::Critical, true) => 30,
        (Status::Critical, false) => 15,
        (Status::Warning, true) => 10,
        (Status::Warning, false) => 5,
        (Status::Good | Status::Unknown, _) => 0,
    }
}

/// Computes the health score. Penalties are capped so the total never
/// exceeds 100; the breakdown is clamped too, so it always sums to `100 - value`.
pub fn score(results: &[CheckResult]) -> Score {
    let mut remaining: u32 = 100;
    let mut breakdown = Vec::new();
    for issue in results.iter().flat_map(|r| r.issues.iter()) {
        let penalty = weight(issue.category, issue.status).min(remaining);
        remaining -= penalty;
        breakdown.push(ScoreItem { issue_id: issue.id.clone(), penalty: penalty as u8 });
    }
    Score { value: remaining as u8, breakdown }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Issue;

    fn issue(id: &str, category: Category, status: Status) -> Issue {
        Issue {
            id: id.into(),
            category,
            status,
            title_key: format!("{id}.title"),
            why_key: format!("{id}.why"),
            evidence: String::new(),
            params: Default::default(),
            fix: None,
        }
    }

    fn result(category: Category, issues: Vec<Issue>) -> CheckResult {
        CheckResult { check_id: "c".into(), category, issues, error: None }
    }

    fn penalty_of(i: Issue) -> u32 {
        let c = i.category;
        let s = score(&[result(c, vec![i])]);
        100 - s.value as u32
    }

    #[test]
    fn no_issues_scores_100() {
        let s = score(&[]);
        assert_eq!(s.value, 100);
        assert!(s.breakdown.is_empty());
        assert_eq!(score(&[result(Category::System, vec![])]).value, 100);
    }

    #[test]
    fn critical_security_costs_more_than_warning_temp_files() {
        let sec = penalty_of(issue("fw", Category::Security, Status::Critical));
        let tmp = penalty_of(issue("tmp", Category::Storage, Status::Warning));
        assert!(sec > tmp);
        assert_eq!(sec, 30);
        assert_eq!(tmp, 5);
    }

    #[test]
    fn weights_match_spec() {
        assert_eq!(penalty_of(issue("a", Category::Update, Status::Critical)), 15);
        assert_eq!(penalty_of(issue("b", Category::Security, Status::Warning)), 10);
        assert_eq!(penalty_of(issue("c", Category::Security, Status::Good)), 0);
    }

    #[test]
    fn score_never_below_zero() {
        let issues: Vec<Issue> = (0..10)
            .map(|n| issue(&format!("s{n}"), Category::Security, Status::Critical))
            .collect();
        let s = score(&[result(Category::Security, issues)]);
        assert_eq!(s.value, 0);
    }

    #[test]
    fn unknown_results_do_not_inflate_score() {
        let mut r = result(
            Category::Security,
            vec![issue("u", Category::Security, Status::Unknown)],
        );
        r.error = Some("boom".into());
        let failed = CheckResult { check_id: "f".into(), category: Category::System, issues: vec![], error: Some("x".into()) };
        let s = score(&[r, failed]);
        assert!(s.value <= 100);
        assert!(s.breakdown.iter().all(|i| i.penalty == 0));
    }

    #[test]
    fn breakdown_sums_to_penalty() {
        let s = score(&[result(
            Category::Security,
            vec![
                issue("a", Category::Security, Status::Critical),
                issue("b", Category::Storage, Status::Warning),
                issue("c", Category::Update, Status::Critical),
            ],
        )]);
        let sum: u32 = s.breakdown.iter().map(|i| i.penalty as u32).sum();
        assert_eq!(100 - s.value as u32, sum);
        assert_eq!(sum, 50);
    }

    #[test]
    fn breakdown_stays_honest_when_clamped() {
        let issues: Vec<Issue> = (0..5)
            .map(|n| issue(&format!("s{n}"), Category::Security, Status::Critical))
            .collect();
        let s = score(&[result(Category::Security, issues)]);
        let sum: u32 = s.breakdown.iter().map(|i| i.penalty as u32).sum();
        assert_eq!(s.value, 0);
        assert_eq!(sum, 100);
    }
}
