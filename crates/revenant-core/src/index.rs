//! Capture index: parses the TSV form of a Wayback CDX query into per-URL captures.
//!
//! Only HTTP 200 captures count as archived content. Other statuses are dropped and counted.
//! A file whose only captures are 404s has no usable capture, so it classifies as Missing.

use std::collections::BTreeMap;

use crate::fidelity::Capture;
use crate::time::parse_cdx_timestamp;

/// Parsed index: successful captures keyed by the file's label, plus a count of dropped rows.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct CaptureIndex {
    by_label: BTreeMap<String, Vec<Capture>>,
    dropped_non_200: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub enum IndexError {
    /// A data row does not have six tab-separated columns.
    BadColumnCount { line: usize },
    /// A row's timestamp is not a valid CDX timestamp.
    BadTimestamp { line: usize },
}

impl CaptureIndex {
    /// Successful captures for one file label, in no particular order. Empty when none exist.
    pub fn captures(&self, label: &str) -> &[Capture] {
        self.by_label.get(label).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Number of rows dropped because their HTTP status was not 200.
    pub fn dropped_non_200(&self) -> usize {
        self.dropped_non_200
    }

    /// Every label with at least one successful capture.
    pub fn labels(&self) -> impl Iterator<Item = &str> {
        self.by_label.keys().map(String::as_str)
    }
}

/// Parse the TSV index. The first line is a header. Columns are:
/// `label  timestamp  digest  mimetype  status  original`.
pub fn parse_index(tsv: &str) -> Result<CaptureIndex, IndexError> {
    let mut index = CaptureIndex::default();
    for (i, line) in tsv.lines().enumerate().skip(1) {
        if line.trim().is_empty() {
            continue;
        }
        let line_no = i + 1;
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() != 6 {
            return Err(IndexError::BadColumnCount { line: line_no });
        }
        let (label, cdx_ts, digest, status) = (cols[0], cols[1], cols[2], cols[4]);
        let ts = parse_cdx_timestamp(cdx_ts).ok_or(IndexError::BadTimestamp { line: line_no })?;
        if status != "200" {
            index.dropped_non_200 += 1;
            continue;
        }
        index
            .by_label
            .entry(label.to_string())
            .or_default()
            .push(Capture {
                ts,
                digest: digest.to_string(),
            });
    }
    Ok(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str = "label\tts\tdigest\tmime\tstatus\toriginal";

    #[test]
    fn keeps_only_200_and_counts_the_rest() {
        let tsv = format!(
            "{HEADER}\nfile.gif\t20001013054719\tAAA\ttext/html\t404\thttp://x/file.gif\n\
             file.gif\t20000101000000\tBBB\timage/gif\t200\thttp://x/file.gif\n"
        );
        let idx = parse_index(&tsv).expect("parses");
        assert_eq!(idx.dropped_non_200(), 1);
        assert_eq!(idx.captures("file.gif").len(), 1);
        assert_eq!(idx.captures("file.gif")[0].digest, "BBB");
    }

    #[test]
    fn unknown_label_has_no_captures() {
        let idx = parse_index(HEADER).expect("parses");
        assert!(idx.captures("nope").is_empty());
    }

    #[test]
    fn rejects_short_rows() {
        let tsv = format!("{HEADER}\nfile.gif\t20000101000000\tBBB\n");
        assert_eq!(
            parse_index(&tsv),
            Err(IndexError::BadColumnCount { line: 2 })
        );
    }

    #[test]
    fn rejects_bad_timestamps() {
        let tsv = format!("{HEADER}\nfile.gif\tnot-a-date\tBBB\timage/gif\t200\thttp://x\n");
        assert_eq!(parse_index(&tsv), Err(IndexError::BadTimestamp { line: 2 }));
    }
}
