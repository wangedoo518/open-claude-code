#!/usr/bin/env python3
"""
WeChat Article Fetcher — uses Playwright to render mp.weixin.qq.com pages
and extract the article content as Markdown.

Bypasses WeChat's anti-scraping by using a real browser with the user's
existing Chrome profile (already logged into WeChat).

Request (stdin JSON):
  {"url": "https://mp.weixin.qq.com/s/xxx"}

Response (stdout JSON):
  {"ok": true, "title": "...", "author": "...", "publish_time": "...", "markdown": "..."}
  {"ok": false, "error": "..."}

Install: pip install playwright && python -m playwright install chromium
"""

import json
import sys
import os
import re


def extract_article(page) -> dict:
    """Extract article content from a rendered WeChat page."""

    # Wait for the main content to load
    try:
        page.wait_for_selector(".rich_media_content", timeout=15000)
    except Exception:
        # Fallback: wait for any article-like content
        try:
            page.wait_for_selector("article, .content, #js_content", timeout=10000)
        except Exception:
            pass

    # Extract title
    title = ""
    for sel in [".rich_media_title", "#activity-name", "h1"]:
        el = page.query_selector(sel)
        if el:
            title = el.inner_text().strip()
            if title:
                break
    if not title:
        title = page.title() or "Untitled"

    # Extract author
    author = ""
    for sel in ["#js_name", ".profile_nickname", ".rich_media_meta_nickname"]:
        el = page.query_selector(sel)
        if el:
            author = el.inner_text().strip()
            if author:
                break

    # Extract publish time
    publish_time = ""
    for sel in ["#publish_time", ".rich_media_meta_text"]:
        el = page.query_selector(sel)
        if el:
            publish_time = el.inner_text().strip()
            if publish_time:
                break

    # Extract main content as HTML then convert to markdown
    content_html = ""
    for sel in [".rich_media_content", "#js_content", "article", ".content"]:
        el = page.query_selector(sel)
        if el:
            content_html = el.inner_html()
            if len(content_html) > 100:
                break

    if not content_html or len(content_html) < 50:
        # Last resort: get full page text
        content_html = page.query_selector("body").inner_text() if page.query_selector("body") else ""
        markdown = content_html
    else:
        markdown = html_to_markdown(content_html)

    return {
        "title": title,
        "author": author,
        "publish_time": publish_time,
        "markdown": markdown,
    }


def html_to_markdown(html: str) -> str:
    """Convert HTML to Markdown. Uses markitdown if available, else basic regex."""
    try:
        from markitdown import MarkItDown
        import tempfile
        # Write HTML to temp file and convert
        with tempfile.NamedTemporaryFile(suffix=".html", mode="w", encoding="utf-8", delete=False) as f:
            f.write(html)
            tmp_path = f.name
        try:
            md = MarkItDown()
            result = md.convert(tmp_path)
            return result.text_content
        finally:
            os.unlink(tmp_path)
    except ImportError:
        pass

    # Fallback: basic HTML → text conversion
    text = re.sub(r"<br\s*/?>", "\n", html)
    text = re.sub(r"<p[^>]*>", "\n\n", text)
    text = re.sub(r"</p>", "", text)
    text = re.sub(r"<h(\d)[^>]*>(.*?)</h\1>", lambda m: "\n\n" + "#" * int(m.group(1)) + " " + m.group(2) + "\n\n", text)
    text = re.sub(r"<strong>(.*?)</strong>", r"**\1**", text)
    text = re.sub(r"<em>(.*?)</em>", r"*\1*", text)
    text = re.sub(r"<a[^>]+href=['\"]([^'\"]+)['\"][^>]*>(.*?)</a>", r"[\2](\1)", text)
    text = re.sub(r"<img[^>]+src=['\"]([^'\"]+)['\"][^>]*>", r"![](\1)", text)
    text = re.sub(r"<[^>]+>", "", text)
    text = re.sub(r"\n{3,}", "\n\n", text)
    return text.strip()


def get_chrome_user_data_dir() -> str:
    """Find the user's Chrome profile directory for cookie reuse."""
    if sys.platform == "win32":
        return os.path.join(os.environ.get("LOCALAPPDATA", ""), "Google", "Chrome", "User Data")
    elif sys.platform == "darwin":
        return os.path.expanduser("~/Library/Application Support/Google/Chrome")
    else:
        return os.path.expanduser("~/.config/google-chrome")


def main():
    try:
        raw = sys.stdin.read()
        req = json.loads(raw)
        url = req.get("url", "").strip()

        if not url:
            json.dump({"ok": False, "error": "URL is empty"}, sys.stdout)
            return

        if "mp.weixin.qq.com" not in url and "weixin.qq.com" not in url:
            json.dump({"ok": False, "error": "Not a WeChat URL"}, sys.stdout)
            return

        try:
            from playwright.sync_api import sync_playwright
        except ImportError:
            json.dump({
                "ok": False,
                "error": "Playwright not installed. Run: pip install playwright && python -m playwright install chromium"
            }, sys.stdout)
            return

        with sync_playwright() as p:
            # Use Chromium with stealth settings
            browser = p.chromium.launch(
                headless=True,
                args=[
                    "--disable-blink-features=AutomationControlled",
                    "--no-sandbox",
                ]
            )

            context = browser.new_context(
                user_agent="Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
                viewport={"width": 1280, "height": 900},
                locale="zh-CN",
            )

            # Bypass webdriver detection
            context.add_init_script("""
                Object.defineProperty(navigator, 'webdriver', { get: () => undefined });
            """)

            page = context.new_page()

            # Set referer
            page.set_extra_http_headers({
                "Referer": "https://mp.weixin.qq.com/",
            })

            try:
                page.goto(url, wait_until="networkidle", timeout=30000)
            except Exception:
                # networkidle might timeout, try domcontentloaded
                try:
                    page.goto(url, wait_until="domcontentloaded", timeout=15000)
                except Exception as e:
                    json.dump({"ok": False, "error": f"Page load failed: {e}"}, sys.stdout)
                    browser.close()
                    return

            # Extra wait for dynamic content
            page.wait_for_timeout(2000)

            # Check for CAPTCHA / verification page
            page_text = page.inner_text("body")
            if "环境异常" in page_text or "完成验证" in page_text:
                json.dump({
                    "ok": False,
                    "error": "WeChat anti-scraping triggered. Please open the URL in your browser first to pass verification."
                }, sys.stdout)
                browser.close()
                return

            # Extract article
            article = extract_article(page)
            browser.close()

            if not article["markdown"] or len(article["markdown"]) < 50:
                json.dump({
                    "ok": False,
                    "error": "Article content too short or extraction failed"
                }, sys.stdout)
                return

            json.dump({
                "ok": True,
                "title": article["title"],
                "author": article["author"],
                "publish_time": article["publish_time"],
                "markdown": article["markdown"],
            }, sys.stdout, ensure_ascii=False)

    except Exception as e:
        json.dump({"ok": False, "error": str(e)}, sys.stdout, ensure_ascii=False)


if __name__ == "__main__":
    main()
