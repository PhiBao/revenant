//! Autopsy: page-level vital signs over the files a page references.
//!
//! Definitions follow `PLAN.md` §R.3:
//! - Completeness: the share of referenced files with any usable capture.
//! - Fidelity: the share of recovered files whose verdict is Proven or Likely.

use crate::fidelity::{classify, Verdict};
use crate::index::CaptureIndex;

/// One referenced file and its verdict at the target time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerRow {
    pub label: String,
    pub verdict: Verdict,
}

/// Page-level summary for one target time.
#[derive(Debug, Clone, PartialEq)]
pub struct Autopsy {
    /// Percent, 0.0 to 100.0. `None` when the page references no files.
    pub completeness: Option<f64>,
    /// Percent, 0.0 to 100.0. `None` when no file was recovered.
    pub fidelity: Option<f64>,
    /// Files the archive could not be asked about. Not counted as missing.
    pub unknown: usize,
    pub ledger: Vec<LedgerRow>,
}

/// Build the autopsy for a page. `referenced` lists the file labels the page points to.
pub fn autopsy(target: i64, referenced: &[&str], index: &CaptureIndex) -> Autopsy {
    let ledger: Vec<LedgerRow> = referenced
        .iter()
        .map(|label| LedgerRow {
            label: (*label).to_string(),
            verdict: classify(target, index.captures(label)).verdict,
        })
        .collect();
    summarize(ledger)
}

/// Build the autopsy from verdicts that are already known.
pub fn summarize(ledger: Vec<LedgerRow>) -> Autopsy {
    let answered: Vec<&LedgerRow> = ledger
        .iter()
        .filter(|r| r.verdict != Verdict::Unknown)
        .collect();
    let recovered: Vec<&&LedgerRow> = answered
        .iter()
        .filter(|r| r.verdict != Verdict::Missing)
        .collect();
    let trusted = recovered
        .iter()
        .filter(|r| matches!(r.verdict, Verdict::Proven | Verdict::Likely))
        .count();

    let completeness = (!answered.is_empty()).then(|| percent(recovered.len(), answered.len()));
    let fidelity = (!recovered.is_empty()).then(|| percent(trusted, recovered.len()));
    let unknown = ledger.len() - answered.len();

    Autopsy {
        completeness,
        fidelity,
        unknown,
        ledger,
    }
}

fn percent(part: usize, whole: usize) -> f64 {
    part as f64 * 100.0 / whole as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::parse_index;
    use crate::time::parse_cdx_timestamp;

    const TSV: &str = "label\tts\tdigest\tmime\tstatus\toriginal\n\
        a.gif\t19991013000000\tA1\timage/gif\t200\thttp://x/a.gif\n\
        a.gif\t20000301000000\tA2\timage/gif\t200\thttp://x/a.gif\n\
        b.gif\t19991101000000\tB1\timage/gif\t200\thttp://x/b.gif\n\
        b.gif\t19991201000000\tB1\timage/gif\t200\thttp://x/b.gif\n\
        b.gif\t20000201000000\tB1\timage/gif\t200\thttp://x/b.gif\n\
        c.gif\t20001013000000\tC1\timage/gif\t404\thttp://x/c.gif\n";

    fn target() -> i64 {
        parse_cdx_timestamp("19991231000000").expect("valid")
    }

    #[test]
    fn scores_a_mixed_page() {
        let idx = parse_index(TSV).expect("parses");
        // a: UNCERTAIN, b: PROVEN (same digest across T), c: MISSING (only 404s).
        let a = autopsy(target(), &["a.gif", "b.gif", "c.gif"], &idx);
        assert_eq!(a.ledger[0].verdict, Verdict::Uncertain);
        assert_eq!(a.ledger[1].verdict, Verdict::Proven);
        assert_eq!(a.ledger[2].verdict, Verdict::Missing);
        let c = a.completeness.expect("has files");
        let f = a.fidelity.expect("has recovered files");
        assert!((c - 66.666).abs() < 0.01, "completeness {c}");
        // Two recovered files, one trusted (Proven), so 50%.
        assert!((f - 50.0).abs() < 1e-9, "fidelity {f}");
    }

    #[test]
    fn empty_page_has_no_scores() {
        let idx = parse_index(TSV).expect("parses");
        let a = autopsy(target(), &[], &idx);
        assert_eq!(a.completeness, None);
        assert_eq!(a.fidelity, None);
    }

    #[test]
    fn nothing_recovered_has_no_fidelity() {
        let idx = parse_index(TSV).expect("parses");
        let a = autopsy(target(), &["c.gif"], &idx);
        assert_eq!(a.completeness, Some(0.0));
        assert_eq!(a.fidelity, None);
    }
}
