//! Runtime prerequisite detection.
//!
//! Classifies stderr / IngestError strings into `MissingPrerequisite`
//! variants so that upstream layers (URL ingest orchestrator, Ask
//! enrichment side-channel, WeChat Bridge error card) can surface
//! consistent, actionable Chinese guidance instead of raw subprocess
//! stderr.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MissingPrerequisite {
    Node,
    Npx,
    Python,
    Playwright,    // Python pip package
    Chromium,      // System Chrome / Edge / Chromium (Playwright uses system browser)
    OpenCli,       // opencli / @jackwener/opencli
    BrowserBridge, // OpenCLI Chrome extension not connected
    Markitdown,    // pip markitdown[all]
    Other(String),
}

impl MissingPrerequisite {
    /// Match common error strings to a prerequisite. Order matters:
    /// Playwright / Chromium / Markitdown are checked before generic
    /// "Python not found" so a more specific message wins.
    pub fn detect(err: &str) -> Option<Self> {
        let low = err.to_lowercase();

        // Node / npx — from wxkefu-rs pipeline + deployer
        if low.contains("node.js not found") || low.contains("'node' is not recognized") {
            return Some(Self::Node);
        }
        if low.contains("npx not found") || low.contains("'npx' is not recognized") {
            return Some(Self::Npx);
        }

        // Playwright pip package
        if low.contains("no module named 'playwright'") || low.contains("playwright not installed")
        {
            return Some(Self::Playwright);
        }

        // Chromium / system browser
        if low.contains("executable doesn't exist") || low.contains("browsertype.launch") {
            return Some(Self::Chromium);
        }

        // OpenCLI
        if low.contains("opencli unavailable") {
            return Some(Self::OpenCli);
        }

        // Browser bridge
        if low.contains("browser bridge") && low.contains("not connected") {
            return Some(Self::BrowserBridge);
        }

        // markitdown
        if low.contains("no module named 'markitdown'") || low.contains("markitdown not installed")
        {
            return Some(Self::Markitdown);
        }

        // Python (fallback — must come AFTER Playwright / markitdown checks
        // so a "Python + Playwright missing" scenario routes to Playwright,
        // which provides more actionable guidance.)
        if (low.contains("failed to spawn python") || low.contains("python not found"))
            && !low.contains("playwright")
            && !low.contains("markitdown")
        {
            return Some(Self::Python);
        }

        None
    }

    /// Machine-readable identifier sent to the frontend alongside the hint.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Node => "node",
            Self::Npx => "npx",
            Self::Python => "python",
            Self::Playwright => "playwright",
            Self::Chromium => "chromium",
            Self::OpenCli => "opencli",
            Self::BrowserBridge => "browser-bridge",
            Self::Markitdown => "markitdown",
            Self::Other(_) => "other",
        }
    }

    /// Chinese human-readable hint — the exact phrasing is what the
    /// user sees in the error banner / toast.
    pub fn human_hint(&self) -> &'static str {
        match self {
            Self::Node          => "未找到 Node.js。请安装 Node 18+，并确认终端里 node -v 可以运行。",
            Self::Npx           => "未找到 npx。通常是 Node.js 未安装或 PATH 未生效，确认 npx -v 能运行。",
            Self::Python        => "未找到 Python。请安装 Python 3.11+ 并加入 PATH（python --version 能运行）。",
            Self::Playwright    => "Python Playwright 未安装。运行 pip install playwright 并执行 python -m playwright install chromium。",
            Self::Chromium      => "未找到可用的 Chrome/Edge/Chromium。请安装任意一款 Chromium 内核浏览器。",
            Self::OpenCli       => "未找到 OpenCLI。请安装 @jackwener/opencli，或允许桌面端通过 npx 拉起。",
            Self::BrowserBridge => "OpenCLI Browser Bridge 扩展未连接。请在 Chrome chrome://extensions 启用扩展后重试。",
            Self::Markitdown    => "markitdown 未安装。运行 pip install 'markitdown[all]' 或点击自动安装。",
            Self::Other(_)      => "环境检查失败，详见原始错误。",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_playwright_module_missing() {
        let err = "Failed to spawn Python: ... No module named 'playwright'";
        assert_eq!(
            MissingPrerequisite::detect(err),
            Some(MissingPrerequisite::Playwright)
        );
    }

    #[test]
    fn detect_python_not_found_when_no_playwright() {
        let err = "Failed to spawn Python: OS error — not found";
        assert_eq!(
            MissingPrerequisite::detect(err),
            Some(MissingPrerequisite::Python)
        );
    }

    #[test]
    fn detect_python_skips_playwright_scenario() {
        // "Failed to spawn Python ... Is Playwright installed?" should route
        // to Playwright variant, NOT Python (the hint is more actionable).
        let err = "Failed to spawn Python: No module named 'playwright'. Is Playwright installed?";
        assert_eq!(
            MissingPrerequisite::detect(err),
            Some(MissingPrerequisite::Playwright)
        );
    }

    #[test]
    fn detect_chromium_executable() {
        let err = "BrowserType.launch: Executable doesn't exist at /foo/chrome";
        assert_eq!(
            MissingPrerequisite::detect(err),
            Some(MissingPrerequisite::Chromium)
        );
    }

    #[test]
    fn detect_node_windows_recognized() {
        let err = "'node' is not recognized as an internal or external command";
        assert_eq!(
            MissingPrerequisite::detect(err),
            Some(MissingPrerequisite::Node)
        );
    }

    #[test]
    fn detect_opencli() {
        let err = "OpenCLI unavailable. Install it globally or allow npx --yes";
        assert_eq!(
            MissingPrerequisite::detect(err),
            Some(MissingPrerequisite::OpenCli)
        );
    }

    #[test]
    fn detect_markitdown() {
        let err = "No module named 'markitdown'";
        assert_eq!(
            MissingPrerequisite::detect(err),
            Some(MissingPrerequisite::Markitdown)
        );
    }

    #[test]
    fn detect_returns_none_for_unrelated_errors() {
        let err = "network timeout: connection refused";
        assert_eq!(MissingPrerequisite::detect(err), None);
    }

    #[test]
    fn hint_is_non_empty_for_all_variants() {
        let all = [
            MissingPrerequisite::Node,
            MissingPrerequisite::Npx,
            MissingPrerequisite::Python,
            MissingPrerequisite::Playwright,
            MissingPrerequisite::Chromium,
            MissingPrerequisite::OpenCli,
            MissingPrerequisite::BrowserBridge,
            MissingPrerequisite::Markitdown,
        ];
        for p in all {
            assert!(!p.human_hint().is_empty(), "hint missing for {:?}", p);
            assert!(!p.as_str().is_empty(), "as_str missing for {:?}", p);
        }
    }
}
