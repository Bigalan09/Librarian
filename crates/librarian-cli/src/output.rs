//! Output formatting: tracing initialisation, progress bars, and summary table.

use indicatif::{ProgressBar, ProgressStyle};
use librarian_core::plan::PlanStats;
use tracing_subscriber::{EnvFilter, fmt};

pub fn init_tracing(verbose: bool, json: bool, quiet: bool) -> anyhow::Result<()> {
    let filter = if verbose {
        EnvFilter::new("debug")
    } else if quiet {
        EnvFilter::new("error")
    } else {
        EnvFilter::new("info")
    };

    if json {
        fmt()
            .json()
            .with_env_filter(filter)
            .with_target(false)
            .init();
    } else {
        fmt().with_env_filter(filter).with_target(false).init();
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Progress bars
// ---------------------------------------------------------------------------

fn bar_style() -> ProgressStyle {
    ProgressStyle::with_template("{msg:>25} [{bar:40.cyan/blue}] {pos}/{len}")
        .expect("valid progress bar template")
        .progress_chars("=>-")
}

/// Create a progress bar for the scanning phase.
///
/// The bar is initialised with `total` steps and pre-labelled "Scanning".
/// Callers should update the message with the source name once known, e.g.:
///
/// ```ignore
/// pb.set_message(format!("Scanning {source}"));
/// ```
pub fn create_scan_progress(total: u64) -> ProgressBar {
    let pb = ProgressBar::new(total);
    pb.set_style(bar_style());
    pb.set_message("Scanning");
    pb
}

/// Create a progress bar for the classification phase.
pub fn create_classify_progress(total: u64) -> ProgressBar {
    let pb = ProgressBar::new(total);
    pb.set_style(bar_style());
    pb.set_message("Classifying");
    pb
}

// ---------------------------------------------------------------------------
// Summary table
// ---------------------------------------------------------------------------

/// Print the post-process summary table to stdout.
///
/// ```text
/// Summary
/// -------
/// Matched rules           412
/// AI classified           387
/// Low confidence          224  -> NeedsReview
/// Collisions skipped        8
/// Ignored                  24
/// ```
pub fn print_summary(stats: &PlanStats) {
    println!("Summary");
    println!("-------");
    println!("{:<24} {:>5}", "Matched rules", stats.rule_matched);
    println!("{:<24} {:>5}", "AI classified", stats.ai_classified);
    println!(
        "{:<24} {:>5}  -> NeedsReview",
        "Low confidence", stats.needs_review
    );
    println!("{:<24} {:>5}", "Collisions skipped", stats.collisions);
    println!("{:<24} {:>5}", "Ignored", stats.ignored);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_progress_bar_has_correct_length() {
        let pb = create_scan_progress(42);
        assert_eq!(pb.length(), Some(42));
    }

    #[test]
    fn classify_progress_bar_has_correct_length() {
        let pb = create_classify_progress(10);
        assert_eq!(pb.length(), Some(10));
    }

    #[test]
    fn print_summary_does_not_panic() {
        let stats = PlanStats {
            total_files: 100,
            rule_matched: 40,
            ai_classified: 30,
            needs_review: 10,
            collisions: 5,
            ignored: 15,
            skipped: 0,
            limit_reached: false,
        };
        // Should not panic
        print_summary(&stats);
    }

    #[test]
    fn print_summary_with_zeros() {
        let stats = PlanStats::default();
        print_summary(&stats);
    }
}
