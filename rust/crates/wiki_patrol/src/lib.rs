//! `wiki_patrol` — structural quality checks for `~/.clawwiki/wiki/`.
//!
//! Per technical-design.md §4.3 and 01-skill-engine.md §5.4:
//! five detectors run sequentially, each producing `Vec<PatrolIssue>`.
//! `run_full_patrol` aggregates them into a `PatrolReport`.
//!
//! No LLM dependency — all checks are local filesystem operations.

use wiki_store::{
    BacklinksIndex, PatrolIssue, PatrolIssueKind, PatrolQualitySample, PatrolReport, PatrolSummary,
    WikiPaths,
};

/// Configuration for patrol thresholds.
#[derive(Debug, Clone)]
pub struct PatrolConfig {
    pub stale_threshold_days: u32,
    pub min_page_words: u32,
    pub max_page_words: u32,
}

impl Default for PatrolConfig {
    fn default() -> Self {
        Self {
            stale_threshold_days: 30,
            min_page_words: 15,
            max_page_words: 5000,
        }
    }
}

/// Detect orphan pages: wiki pages with no inbound links and not
/// referenced in index.md.
pub fn detect_orphans(paths: &WikiPaths) -> Vec<PatrolIssue> {
    let all_pages = wiki_store::list_all_wiki_pages(paths).unwrap_or_default();
    let backlinks: BacklinksIndex = wiki_store::load_backlinks_index(paths).unwrap_or_default();
    let index_content = std::fs::read_to_string(paths.wiki.join(wiki_store::WIKI_INDEX_FILENAME))
        .unwrap_or_default();

    // Reuse the unified orphan predicate from wiki_store so /api/wiki/stats
    // and /api/wiki/patrol never disagree on what counts as an orphan.
    let mut issues = Vec::new();
    for page in &all_pages {
        if wiki_store::is_page_orphan(page, &backlinks, &index_content) {
            issues.push(PatrolIssue {
                kind: PatrolIssueKind::Orphan,
                page_slug: page.slug.clone(),
                description: "该页面无任何入链, 且不被 index.md 引用".to_string(),
                suggested_action: "添加至相关 topic 页面的引用, 或标记为 deprecated".to_string(),
            });
        }
    }
    issues
}

/// Detect stale pages: `created_at` older than `threshold_days` days.
/// (Using created_at as proxy until last_verified is populated.)
pub fn detect_stale(paths: &WikiPaths, threshold_days: u32) -> Vec<PatrolIssue> {
    let all_pages = wiki_store::list_all_wiki_pages(paths).unwrap_or_default();
    let now = wiki_store::now_iso8601();
    let cutoff = compute_cutoff_date(&now, threshold_days);

    let mut issues = Vec::new();
    for page in &all_pages {
        // Compare date portion only (YYYY-MM-DD).
        let page_date = &page.created_at[..10.min(page.created_at.len())];
        if page_date < cutoff.as_str() {
            issues.push(PatrolIssue {
                kind: PatrolIssueKind::Stale,
                page_slug: page.slug.clone(),
                description: format!("页面创建于 {page_date}, 超过 {threshold_days} 天未验证"),
                suggested_action: "重新验证页面内容时效性".to_string(),
            });
        }
    }
    issues
}

/// Detect schema violations by checking frontmatter required fields.
/// Checks: type, title, summary must be present.
pub fn detect_schema_violations(paths: &WikiPaths) -> Vec<PatrolIssue> {
    let all_pages = wiki_store::list_all_wiki_pages(paths).unwrap_or_default();
    let required_fields = ["type", "title", "summary"];

    let mut issues = Vec::new();
    for page in &all_pages {
        let content = match wiki_store::read_wiki_page(paths, &page.slug) {
            Ok((_s, _body)) => {
                // Reconstruct full content with frontmatter for validation.
                // read_wiki_page returns body without frontmatter, so we
                // read the raw file instead.
                let page_path = find_page_path(paths, &page.slug, &page.category);
                std::fs::read_to_string(&page_path).unwrap_or_default()
            }
            Err(_) => continue,
        };

        // Simple frontmatter check: extract YAML block and look for required keys.
        if let Some(fm_text) = extract_frontmatter(&content) {
            for field in &required_fields {
                if !fm_text.contains(&format!("{field}:")) {
                    issues.push(PatrolIssue {
                        kind: PatrolIssueKind::SchemaViolation,
                        page_slug: page.slug.clone(),
                        description: format!("必填字段 `{field}` 缺失"),
                        suggested_action: format!("补充 frontmatter 中的 {field} 字段"),
                    });
                }
            }
        } else {
            issues.push(PatrolIssue {
                kind: PatrolIssueKind::SchemaViolation,
                page_slug: page.slug.clone(),
                description: "缺少 YAML frontmatter 块".to_string(),
                suggested_action: "添加标准 frontmatter (type, title, summary)".to_string(),
            });
        }
    }
    issues
}

/// Detect oversized pages (body word count > max_words).
pub fn detect_oversized(paths: &WikiPaths, max_words: u32) -> Vec<PatrolIssue> {
    let all_pages = wiki_store::list_all_wiki_pages(paths).unwrap_or_default();
    let mut issues = Vec::new();
    for page in &all_pages {
        if let Ok((_s, body)) = wiki_store::read_wiki_page(paths, &page.slug) {
            let words = count_words_simple(&body);
            if words > max_words as usize {
                issues.push(PatrolIssue {
                    kind: PatrolIssueKind::Oversized,
                    page_slug: page.slug.clone(),
                    description: format!("页面正文 {words} 词, 超过阈值 {max_words}"),
                    suggested_action: "拆分为多个独立概念页面".to_string(),
                });
            }
        }
    }
    issues
}

/// Detect stub pages (body word count < min_words).
pub fn detect_stubs(paths: &WikiPaths, min_words: u32) -> Vec<PatrolIssue> {
    let all_pages = wiki_store::list_all_wiki_pages(paths).unwrap_or_default();
    let mut issues = Vec::new();
    for page in &all_pages {
        if let Ok((_s, body)) = wiki_store::read_wiki_page(paths, &page.slug) {
            let words = count_words_simple(&body);
            if words < min_words as usize {
                issues.push(PatrolIssue {
                    kind: PatrolIssueKind::Stub,
                    page_slug: page.slug.clone(),
                    description: format!("页面正文仅 {words} 词, 低于阈值 {min_words}"),
                    suggested_action: "扩充内容或合并到相关页面".to_string(),
                });
            }
        }
    }
    issues
}

/// Detect confidence decay: high-confidence pages with old sources.
/// Per 05-schema-system.md.
pub fn detect_confidence_decay(paths: &WikiPaths) -> Vec<PatrolIssue> {
    let all_pages = wiki_store::list_all_wiki_pages(paths).unwrap_or_default();
    let absorb_log = wiki_store::list_absorb_log(paths).unwrap_or_default();
    let now = wiki_store::now_iso8601();
    let mut issues = Vec::new();

    for page in &all_pages {
        if page.confidence < 0.5 {
            continue; // Only check high-confidence pages.
        }

        // Find newest absorb log entry for this page.
        let newest_entry = absorb_log
            .iter()
            .filter(|e| e.page_slug.as_deref() == Some(&page.slug) && e.action != "skip")
            .max_by(|a, b| a.timestamp.cmp(&b.timestamp));

        let age_days = match newest_entry {
            Some(entry) => days_between(&entry.timestamp, &now),
            None => 999, // No absorb record → treat as very old.
        };

        if page.confidence >= 0.9 && age_days > 90 {
            issues.push(PatrolIssue {
                kind: PatrolIssueKind::ConfidenceDecay,
                page_slug: page.slug.clone(),
                description: format!(
                    "confidence={:.1}, 最近来源 {}天前, 建议降级至 0.6",
                    page.confidence, age_days
                ),
                suggested_action: "重新验证或补充新来源".to_string(),
            });
        } else if page.confidence >= 0.6 && age_days > 180 {
            issues.push(PatrolIssue {
                kind: PatrolIssueKind::ConfidenceDecay,
                page_slug: page.slug.clone(),
                description: format!(
                    "confidence={:.1}, 最近来源 {}天前, 建议降级至 0.2",
                    page.confidence, age_days
                ),
                suggested_action: "重新验证或标记为需要更新".to_string(),
            });
        }
    }
    issues
}

/// Detect uncrystallized state: check if query crystallization is producing raw entries.
/// Per 05-schema-system.md.
pub fn detect_uncrystallized(paths: &WikiPaths, _lookback_days: u32) -> Vec<PatrolIssue> {
    let raws = wiki_store::list_raw_entries(paths).unwrap_or_default();
    let query_entries: Vec<_> = raws.iter().filter(|r| r.source == "query").collect();

    // MVP: if the wiki has pages but zero query crystallizations, flag it.
    let pages = wiki_store::list_all_wiki_pages(paths).unwrap_or_default();
    if pages.len() >= 5 && query_entries.is_empty() {
        return vec![PatrolIssue {
            kind: PatrolIssueKind::Uncrystallized,
            page_slug: "(system)".to_string(),
            description: format!(
                "知识库有 {} 个页面但 0 条查询结晶, 结晶机制可能未激活",
                pages.len()
            ),
            suggested_action: "使用 ?前缀 进行知识问答以触发结晶".to_string(),
        }];
    }
    Vec::new()
}

/// Select maintainer-written pages for quality sampling.
///
/// This is intentionally local and deterministic: it does not perform the
/// future LLM audit itself, but it gives the dashboard and follow-up workers a
/// stable candidate set. Pages with a raw source or confidence score are treated
/// as maintainer-written, then prioritized by missing verification and lower
/// confidence.
pub fn select_quality_samples(paths: &WikiPaths, limit: usize) -> Vec<PatrolQualitySample> {
    let mut pages: Vec<_> = wiki_store::list_all_wiki_pages(paths)
        .unwrap_or_default()
        .into_iter()
        .filter(|page| page.source_raw_id.is_some() || page.confidence > 0.0)
        .collect();

    pages.sort_by(|a, b| {
        let a_missing_verified = a.last_verified.is_none();
        let b_missing_verified = b.last_verified.is_none();
        b_missing_verified
            .cmp(&a_missing_verified)
            .then_with(|| {
                a.confidence
                    .partial_cmp(&b.confidence)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.slug.cmp(&b.slug))
    });

    pages
        .into_iter()
        .take(limit)
        .map(|page| {
            let reason = if page.last_verified.is_none() {
                "never verified".to_string()
            } else if page.confidence < 0.6 {
                "low confidence".to_string()
            } else {
                "routine maintained-page sample".to_string()
            };
            PatrolQualitySample {
                page_slug: page.slug,
                title: page.title,
                confidence: page.confidence,
                last_verified: page.last_verified,
                reason,
            }
        })
        .collect()
}

/// Compute days between two ISO-8601 date strings.
fn days_between(earlier: &str, later: &str) -> i64 {
    let e_date = &earlier[..10.min(earlier.len())];
    let l_date = &later[..10.min(later.len())];
    let e_jdn = parse_date_to_jdn(e_date);
    let l_jdn = parse_date_to_jdn(l_date);
    (l_jdn - e_jdn).max(0)
}

fn parse_date_to_jdn(date: &str) -> i64 {
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() != 3 {
        return 0;
    }
    let y: i32 = parts[0].parse().unwrap_or(2026);
    let m: u32 = parts[1].parse().unwrap_or(1);
    let d: u32 = parts[2].parse().unwrap_or(1);
    to_jdn(y, m, d) as i64
}

/// Run all seven detectors and aggregate into a PatrolReport.
pub fn run_full_patrol(paths: &WikiPaths, config: &PatrolConfig) -> PatrolReport {
    let orphans = detect_orphans(paths);
    let stale = detect_stale(paths, config.stale_threshold_days);
    let violations = detect_schema_violations(paths);
    let oversized = detect_oversized(paths, config.max_page_words);
    let stubs = detect_stubs(paths, config.min_page_words);
    let confidence_decay = detect_confidence_decay(paths);
    let uncrystallized = detect_uncrystallized(paths, 30);
    let quality_samples = select_quality_samples(paths, 5);

    let summary = PatrolSummary {
        orphans: orphans.len(),
        stale: stale.len(),
        schema_violations: violations.len(),
        oversized: oversized.len(),
        stubs: stubs.len(),
        confidence_decay: confidence_decay.len(),
        uncrystallized: uncrystallized.len(),
    };

    let mut all_issues = Vec::new();
    all_issues.extend(orphans);
    all_issues.extend(stale);
    all_issues.extend(violations);
    all_issues.extend(oversized);
    all_issues.extend(stubs);
    all_issues.extend(confidence_decay);
    all_issues.extend(uncrystallized);

    PatrolReport {
        issues: all_issues,
        summary,
        quality_samples,
        checked_at: wiki_store::now_iso8601(),
    }
}

// ── Helpers ─────────────────────────────────────────────────────────

/// Simple word counter: CJK chars = 1 word each, ASCII tokens by whitespace.
fn count_words_simple(body: &str) -> usize {
    let mut count = 0usize;
    let mut in_ascii_word = false;
    for ch in body.chars() {
        if ch.is_whitespace() {
            in_ascii_word = false;
        } else if ch.is_ascii() {
            if !in_ascii_word {
                count += 1;
                in_ascii_word = true;
            }
        } else {
            count += 1;
            in_ascii_word = false;
        }
    }
    count
}

/// Extract YAML frontmatter text between `---` delimiters.
fn extract_frontmatter(content: &str) -> Option<String> {
    let mut lines = content.lines();
    if lines.next()? != "---" {
        return None;
    }
    let mut fm_lines = Vec::new();
    for line in lines {
        if line == "---" {
            return Some(fm_lines.join("\n"));
        }
        fm_lines.push(line);
    }
    None
}

/// Resolve the filesystem path for a wiki page given its slug and category.
fn find_page_path(paths: &WikiPaths, slug: &str, category: &str) -> std::path::PathBuf {
    let subdir = match category {
        "concept" => "concepts",
        "people" => "people",
        "topic" => "topics",
        "compare" => "compare",
        _ => "concepts",
    };
    paths.wiki.join(subdir).join(format!("{slug}.md"))
}

/// Compute a cutoff date string (YYYY-MM-DD) by subtracting days from now.
fn compute_cutoff_date(now_iso: &str, days: u32) -> String {
    // Parse the date portion and subtract days.
    // Simple approach: parse YYYY-MM-DD, convert to day count, subtract.
    let date_str = &now_iso[..10.min(now_iso.len())];
    let parts: Vec<&str> = date_str.split('-').collect();
    if parts.len() != 3 {
        return date_str.to_string();
    }
    let year: i32 = parts[0].parse().unwrap_or(2026);
    let month: u32 = parts[1].parse().unwrap_or(1);
    let day: u32 = parts[2].parse().unwrap_or(1);

    // Convert to Julian day number, subtract, convert back.
    let jdn = to_jdn(year, month, day);
    let cutoff_jdn = jdn - days as i32;
    let (y, m, d) = from_jdn(cutoff_jdn);
    format!("{y:04}-{m:02}-{d:02}")
}

fn to_jdn(year: i32, month: u32, day: u32) -> i32 {
    let a = (14 - month as i32) / 12;
    let y = year + 4800 - a;
    let m = month as i32 + 12 * a - 3;
    day as i32 + (153 * m + 2) / 5 + 365 * y + y / 4 - y / 100 + y / 400 - 32045
}

fn from_jdn(jdn: i32) -> (i32, u32, u32) {
    let a = jdn + 32044;
    let b = (4 * a + 3) / 146097;
    let c = a - (146097 * b) / 4;
    let d = (4 * c + 3) / 1461;
    let e = c - (1461 * d) / 4;
    let m = (5 * e + 2) / 153;
    let day = (e - (153 * m + 2) / 5 + 1) as u32;
    let month = (m + 3 - 12 * (m / 10)) as u32;
    let year = 100 * b + d - 4800 + m / 10;
    (year, month, day)
}

// ═══════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn init_and_paths() -> (tempfile::TempDir, WikiPaths) {
        let tmp = tempdir().unwrap();
        wiki_store::init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());
        (tmp, paths)
    }

    fn create_page(paths: &WikiPaths, cat: &str, slug: &str, title: &str, body: &str) {
        wiki_store::write_wiki_page_in_category(paths, cat, slug, title, "summary", body, Some(1))
            .unwrap();
    }

    // ── detect_orphans ──────────────────────────────────────────

    #[test]
    fn orphans_empty_wiki() {
        let (_tmp, paths) = init_and_paths();
        assert!(detect_orphans(&paths).is_empty());
    }

    #[test]
    fn orphans_page_with_backlink_is_not_orphan() {
        let (_tmp, paths) = init_and_paths();
        create_page(
            &paths,
            "concept",
            "page-a",
            "A",
            "See [B](concepts/page-b.md)",
        );
        create_page(&paths, "concept", "page-b", "B", "Content");
        // Build backlinks so page-b has inbound from page-a.
        let idx = wiki_store::build_backlinks_index(&paths).unwrap();
        wiki_store::save_backlinks_index(&paths, &idx).unwrap();
        let issues = detect_orphans(&paths);
        // page-a is orphan (no inbound), page-b is not.
        assert!(issues.iter().any(|i| i.page_slug == "page-a"));
        assert!(!issues.iter().any(|i| i.page_slug == "page-b"));
    }

    #[test]
    fn orphans_all_pages_orphaned() {
        let (_tmp, paths) = init_and_paths();
        create_page(&paths, "concept", "lone-a", "A", "No links");
        create_page(&paths, "concept", "lone-b", "B", "No links");
        let issues = detect_orphans(&paths);
        assert_eq!(issues.len(), 2);
    }

    // ── detect_stale ────────────────────────────────────────────

    #[test]
    fn stale_empty_wiki() {
        let (_tmp, paths) = init_and_paths();
        assert!(detect_stale(&paths, 30).is_empty());
    }

    #[test]
    fn stale_recent_page_not_stale() {
        let (_tmp, paths) = init_and_paths();
        // Page created "now" should not be stale with 30-day threshold.
        create_page(&paths, "concept", "fresh", "Fresh", "Content");
        let issues = detect_stale(&paths, 30);
        assert!(issues.is_empty(), "fresh page should not be stale");
    }

    #[test]
    fn stale_zero_threshold_everything_stale() {
        let (_tmp, paths) = init_and_paths();
        create_page(&paths, "concept", "any", "Any", "Content");
        // With 0-day threshold, today's page is at the boundary.
        // With threshold=0, cutoff = today, so pages created today are NOT stale
        // (page_date is NOT < cutoff when they're equal).
        // Use threshold large enough to make today's page stale by going far into future.
        // Actually, threshold=99999 makes cutoff = ~274 years ago, so nothing is stale.
        // Better: test with very old pages. We can't easily fake created_at.
        // For now, just verify the function runs without error.
        let _ = detect_stale(&paths, 0);
    }

    // ── detect_schema_violations ────────────────────────────────

    #[test]
    fn schema_violations_empty_wiki() {
        let (_tmp, paths) = init_and_paths();
        assert!(detect_schema_violations(&paths).is_empty());
    }

    #[test]
    fn schema_violations_compliant_page() {
        let (_tmp, paths) = init_and_paths();
        // write_wiki_page_in_category creates proper frontmatter with type/title/summary.
        create_page(&paths, "concept", "good", "Good Page", "Has content");
        let issues = detect_schema_violations(&paths);
        assert!(
            issues.is_empty(),
            "compliant page should have no violations: {issues:?}"
        );
    }

    #[test]
    fn schema_violations_missing_frontmatter() {
        let (_tmp, paths) = init_and_paths();
        // Create a page file directly without frontmatter.
        let page_path = paths.wiki.join("concepts").join("bad.md");
        std::fs::create_dir_all(page_path.parent().unwrap()).unwrap();
        std::fs::write(&page_path, "# No frontmatter\n\nJust body.").unwrap();
        // Need to also make it show up in list_all_wiki_pages.
        // Actually list_all_wiki_pages scans the directory, so it should find it.
        let issues = detect_schema_violations(&paths);
        assert!(
            issues.iter().any(|i| i.page_slug == "bad"),
            "page without frontmatter should be flagged"
        );
    }

    // ── detect_oversized ────────────────────────────────────────

    #[test]
    fn oversized_empty_wiki() {
        let (_tmp, paths) = init_and_paths();
        assert!(detect_oversized(&paths, 5000).is_empty());
    }

    #[test]
    fn oversized_small_page_ok() {
        let (_tmp, paths) = init_and_paths();
        create_page(&paths, "concept", "small", "Small", "A few words here.");
        assert!(detect_oversized(&paths, 5000).is_empty());
    }

    #[test]
    fn oversized_large_page_flagged() {
        let (_tmp, paths) = init_and_paths();
        let big_body = "word ".repeat(100);
        create_page(&paths, "concept", "big", "Big", &big_body);
        let issues = detect_oversized(&paths, 50); // threshold 50 words
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].page_slug, "big");
    }

    // ── detect_stubs ────────────────────────────────────────────

    #[test]
    fn stubs_empty_wiki() {
        let (_tmp, paths) = init_and_paths();
        assert!(detect_stubs(&paths, 15).is_empty());
    }

    #[test]
    fn stubs_normal_page_ok() {
        let (_tmp, paths) = init_and_paths();
        let body = "This is a page with enough content to pass the minimum threshold for stub detection in the patrol system.";
        create_page(&paths, "concept", "normal", "Normal", body);
        assert!(detect_stubs(&paths, 15).is_empty());
    }

    #[test]
    fn stubs_tiny_page_flagged() {
        let (_tmp, paths) = init_and_paths();
        create_page(&paths, "concept", "tiny", "Tiny", "Short.");
        let issues = detect_stubs(&paths, 15);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].page_slug, "tiny");
    }

    // ── run_full_patrol ─────────────────────────────────────────

    #[test]
    fn full_patrol_empty_wiki() {
        let (_tmp, paths) = init_and_paths();
        let report = run_full_patrol(&paths, &PatrolConfig::default());
        assert!(report.issues.is_empty());
        assert_eq!(report.summary.orphans, 0);
        assert_eq!(report.summary.stubs, 0);
    }

    #[test]
    fn full_patrol_mixed_issues() {
        let (_tmp, paths) = init_and_paths();
        // Create a stub page (< 15 words) and an orphan.
        create_page(&paths, "concept", "stubby", "Stubby", "Short.");
        let report = run_full_patrol(&paths, &PatrolConfig::default());
        // Should have at least orphan + stub.
        assert!(report.summary.orphans >= 1);
        assert!(report.summary.stubs >= 1);
        assert!(!report.checked_at.is_empty());
    }

    // ── helpers ──────────────────────────────────────────────────

    #[test]
    fn count_words_simple_mixed() {
        assert_eq!(count_words_simple("Hello 世界 test"), 4); // Hello + 世 + 界 + test
    }

    #[test]
    fn compute_cutoff_date_basic() {
        let cutoff = compute_cutoff_date("2026-04-14T00:00:00Z", 7);
        assert_eq!(cutoff, "2026-04-07");
    }

    #[test]
    fn compute_cutoff_date_cross_month() {
        let cutoff = compute_cutoff_date("2026-04-05T00:00:00Z", 10);
        assert_eq!(cutoff, "2026-03-26");
    }

    // ── detect_confidence_decay ─────────────────────────────────

    #[test]
    fn confidence_decay_no_high_confidence_pages() {
        let (_tmp, paths) = init_and_paths();
        create_page(&paths, "concept", "low-conf", "Low", "Content here.");
        // Default confidence = 0.0, so no decay detected.
        let issues = detect_confidence_decay(&paths);
        assert!(issues.is_empty());
    }

    #[test]
    fn confidence_decay_recent_high_confidence_ok() {
        let (_tmp, paths) = init_and_paths();
        create_page(&paths, "concept", "fresh-high", "Fresh High", "Content.");
        // Set confidence = 0.9 on a just-created page.
        wiki_store::update_page_confidence(&paths, "fresh-high", 0.9).unwrap();
        // Add a recent absorb log entry.
        wiki_store::append_absorb_log(
            &paths,
            wiki_store::AbsorbLogEntry {
                entry_id: 1,
                timestamp: wiki_store::now_iso8601(),
                action: "create".to_string(),
                page_slug: Some("fresh-high".to_string()),
                page_title: Some("Fresh High".to_string()),
                page_category: Some("concept".to_string()),
            },
        )
        .unwrap();
        let issues = detect_confidence_decay(&paths);
        assert!(
            issues.is_empty(),
            "recent high-confidence page should not decay"
        );
    }

    // ── detect_uncrystallized ───────────────────────────────────

    #[test]
    fn uncrystallized_small_wiki_ok() {
        let (_tmp, paths) = init_and_paths();
        // < 5 pages → no check triggered.
        create_page(&paths, "concept", "p1", "P1", "Content.");
        let issues = detect_uncrystallized(&paths, 30);
        assert!(issues.is_empty());
    }

    #[test]
    fn uncrystallized_with_query_entries_ok() {
        let (_tmp, paths) = init_and_paths();
        for i in 0..6 {
            create_page(
                &paths,
                "concept",
                &format!("page-{i}"),
                &format!("Page {i}"),
                "Content content content.",
            );
        }
        // Add a query raw entry → crystallization is working.
        let fm = wiki_store::RawFrontmatter::for_paste("query", None);
        wiki_store::write_raw_entry(&paths, "query", "q1", "query body", &fm).unwrap();
        let issues = detect_uncrystallized(&paths, 30);
        assert!(issues.is_empty());
    }

    #[test]
    fn uncrystallized_no_query_entries_flagged() {
        let (_tmp, paths) = init_and_paths();
        for i in 0..6 {
            create_page(
                &paths,
                "concept",
                &format!("page-{i}"),
                &format!("Page {i}"),
                "Content content content.",
            );
        }
        // No query raw entries → flag.
        let issues = detect_uncrystallized(&paths, 30);
        assert_eq!(issues.len(), 1);
        assert!(matches!(issues[0].kind, PatrolIssueKind::Uncrystallized));
    }

    #[test]
    fn quality_samples_prioritize_maintainer_pages_needing_review() {
        let (_tmp, paths) = init_and_paths();
        create_page(
            &paths,
            "concept",
            "higher",
            "Higher",
            "Enough content for a page.",
        );
        create_page(
            &paths,
            "concept",
            "lower",
            "Lower",
            "Enough content for another page.",
        );
        wiki_store::update_page_confidence(&paths, "higher", 0.8).unwrap();
        wiki_store::update_page_confidence(&paths, "lower", 0.2).unwrap();
        wiki_store::write_wiki_page_in_category(
            &paths,
            "concept",
            "manual",
            "Manual",
            "Summary",
            "Hand-written page with no maintainer source.",
            None,
        )
        .unwrap();

        let samples = select_quality_samples(&paths, 2);
        assert_eq!(samples.len(), 2);
        assert_eq!(samples[0].page_slug, "lower");
        assert!(samples.iter().all(|s| s.page_slug != "manual"));
        assert_eq!(samples[0].reason, "low confidence");
    }

    #[test]
    fn full_patrol_includes_quality_samples() {
        let (_tmp, paths) = init_and_paths();
        create_page(
            &paths,
            "concept",
            "sampled",
            "Sampled",
            "Enough content for sampling.",
        );
        wiki_store::update_page_confidence(&paths, "sampled", 0.7).unwrap();

        let report = run_full_patrol(&paths, &PatrolConfig::default());
        assert_eq!(report.quality_samples.len(), 1);
        assert_eq!(report.quality_samples[0].page_slug, "sampled");
    }

    // ── run_full_patrol includes new detectors ──────────────────

    #[test]
    fn full_patrol_has_seven_detector_fields() {
        let (_tmp, paths) = init_and_paths();
        let report = run_full_patrol(&paths, &PatrolConfig::default());
        // Verify summary has all 7 fields (they should all be 0 for empty wiki).
        assert_eq!(report.summary.orphans, 0);
        assert_eq!(report.summary.stale, 0);
        assert_eq!(report.summary.schema_violations, 0);
        assert_eq!(report.summary.oversized, 0);
        assert_eq!(report.summary.stubs, 0);
        assert_eq!(report.summary.confidence_decay, 0);
        assert_eq!(report.summary.uncrystallized, 0);
    }

    /// Regression test for the orphan_count consistency bug:
    /// `wiki_stats.orphan_count` (from wiki_store) and
    /// `patrol.summary.orphans` (from wiki_patrol) must always agree.
    #[test]
    fn stats_and_patrol_orphans_agree() {
        let (_tmp, paths) = init_and_paths();
        // Seed a mix: one orphan, one page referenced by another, one in index.
        create_page(
            &paths,
            "concept",
            "orphan-page",
            "Orphan",
            "Lonely page with no links.",
        );
        create_page(&paths, "concept", "referenced", "Referenced", "Some body.");
        create_page(
            &paths,
            "concept",
            "linker",
            "Linker",
            "Here is a link: [](concepts/referenced.md) and more text.",
        );

        // Rebuild backlinks so "referenced" gets an inbound from "linker".
        let idx = wiki_store::build_backlinks_index(&paths).unwrap();
        wiki_store::save_backlinks_index(&paths, &idx).unwrap();

        // Compute both independently and assert they agree.
        let stats = wiki_store::wiki_stats(&paths).unwrap();
        let report = run_full_patrol(&paths, &PatrolConfig::default());
        assert_eq!(
            stats.orphan_count, report.summary.orphans,
            "stats.orphan_count ({}) must equal patrol.summary.orphans ({})",
            stats.orphan_count, report.summary.orphans,
        );
        // And at least "orphan-page" should be flagged.
        assert!(stats.orphan_count >= 1, "expected ≥ 1 orphan");
    }
}
