#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Web Article Fetcher — Playwright + defuddle two-stage pipeline.

Stage 1: Playwright renders the page in a real browser (bypasses JS anti-scraping)
Stage 2: defuddle (Node.js) extracts article content as clean Markdown

Falls back to CSS selector extraction if defuddle is unavailable.

stdin:  {"url": "https://mp.weixin.qq.com/s/xxx"}
stdout: {"ok": true, "title": "...", "author": "...", "markdown": "..."}
error:  {"ok": false, "error": "..."}
"""

import json
import sys
import os
import re
import subprocess
import random


def defuddle_extract(html: str, url: str) -> dict | None:
    """Try defuddle Node.js worker for content extraction."""
    worker = os.path.join(os.path.dirname(os.path.abspath(__file__)), "defuddle_worker.js")
    if not os.path.exists(worker):
        return None

    # Check node_modules exists
    node_modules = os.path.join(os.path.dirname(worker), "node_modules")
    if not os.path.exists(node_modules):
        return None

    try:
        req = json.dumps({"html": html, "url": url}, ensure_ascii=False)
        result = subprocess.run(
            ["node", worker],
            input=req,
            capture_output=True,
            text=True,
            timeout=20,
            encoding="utf-8",
        )
        if result.returncode == 0 and result.stdout.strip():
            data = json.loads(result.stdout)
            if data.get("ok") and data.get("markdown") and len(data["markdown"]) > 50:
                return data
    except Exception as e:
        print(f"[wechat_fetcher] defuddle failed: {e}", file=sys.stderr)

    return None


def css_selector_extract(page) -> dict:
    """Fallback: extract using WeChat-specific CSS selectors."""
    title = ""
    for sel in [".rich_media_title", "#activity-name", "h1"]:
        el = page.query_selector(sel)
        if el:
            title = el.inner_text().strip()
            if title:
                break
    if not title:
        title = page.title() or "Untitled"

    author = ""
    for sel in ["#js_name", ".profile_nickname", ".rich_media_meta_nickname"]:
        el = page.query_selector(sel)
        if el:
            author = el.inner_text().strip()
            if author:
                break

    publish_time = ""
    for sel in ["#publish_time", ".rich_media_meta_text"]:
        el = page.query_selector(sel)
        if el:
            publish_time = el.inner_text().strip()
            if publish_time:
                break

    content_html = ""
    for sel in [".rich_media_content", "#js_content", "article", ".content"]:
        el = page.query_selector(sel)
        if el:
            content_html = el.inner_html()
            if len(content_html) > 100:
                break

    if not content_html or len(content_html) < 50:
        body = page.query_selector("body")
        markdown = body.inner_text() if body else ""
    else:
        markdown = basic_html_to_markdown(content_html)

    return {
        "title": title,
        "author": author,
        "published": publish_time,
        "markdown": markdown,
    }


def basic_html_to_markdown(html: str) -> str:
    """Basic HTML → Markdown fallback (regex-based)."""
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


def find_local_chrome() -> str | None:
    """Find Chrome or Edge executable on the local machine."""
    import platform
    candidates = []
    if platform.system() == "Windows":
        for env in ["PROGRAMFILES", "PROGRAMFILES(X86)", "LOCALAPPDATA"]:
            base = os.environ.get(env, "")
            if base:
                candidates.extend([
                    os.path.join(base, "Google", "Chrome", "Application", "chrome.exe"),
                    os.path.join(base, "Microsoft", "Edge", "Application", "msedge.exe"),
                ])
    elif platform.system() == "Darwin":
        candidates.extend([
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
        ])
    else:
        # Linux
        candidates.extend([
            "/usr/bin/google-chrome", "/usr/bin/google-chrome-stable",
            "/usr/bin/chromium-browser", "/usr/bin/chromium",
            "/usr/bin/microsoft-edge",
        ])
    for path in candidates:
        if os.path.isfile(path):
            return path
    return None


def main():
    if sys.platform == "win32":
        sys.stdout.reconfigure(encoding="utf-8")
        sys.stdin.reconfigure(encoding="utf-8")

    try:
        raw = sys.stdin.read()
        req = json.loads(raw)
        url = req.get("url", "").strip()

        if not url:
            json.dump({"ok": False, "error": "URL is empty"}, sys.stdout, ensure_ascii=False)
            return

        try:
            from playwright.sync_api import sync_playwright
        except ImportError:
            json.dump({"ok": False, "error": "Playwright 未安装"}, sys.stdout, ensure_ascii=False)
            return

        with sync_playwright() as p:
            # Use local Chrome/Edge instead of downloading Chromium
            browser = None
            local_chrome = find_local_chrome()
            if local_chrome:
                try:
                    browser = p.chromium.launch(
                        executable_path=local_chrome,
                        headless=True,
                        args=["--disable-blink-features=AutomationControlled", "--no-sandbox"]
                    )
                except Exception:
                    pass
            # Fallback to Playwright's bundled Chromium
            if not browser:
                try:
                    browser = p.chromium.launch(
                        headless=True,
                        args=["--disable-blink-features=AutomationControlled", "--no-sandbox"]
                    )
                except Exception as e:
                    json.dump({"ok": False, "error": f"无法启动浏览器: {e}"}, sys.stdout, ensure_ascii=False)
                    return
            context = browser.new_context(
                user_agent="Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
                viewport={"width": 1280, "height": 900},
                locale="zh-CN",
            )
            context.add_init_script("Object.defineProperty(navigator, 'webdriver', { get: () => undefined });")
            page = context.new_page()
            page.set_extra_http_headers({
                "Referer": "https://mp.weixin.qq.com/",
                "Accept-Language": "zh-CN,zh;q=0.9,en;q=0.8",
            })

            # Navigate with retry. WeChat article pages often keep background
            # requests open, so `networkidle` can hang until the Rust-side
            # outer timeout kills this worker. Prefer DOM readiness, then wait
            # briefly for the article container.
            loaded = False
            for attempt in range(2):
                try:
                    page.goto(url, wait_until="domcontentloaded", timeout=20000)
                    loaded = True
                    break
                except Exception:
                    try:
                        page.goto(url, wait_until="load", timeout=12000)
                        loaded = True
                        break
                    except Exception:
                        if attempt == 0:
                            page.wait_for_timeout(random.randint(1000, 3000))

            if not loaded:
                json.dump({"ok": False, "error": "页面加载失败"}, sys.stdout, ensure_ascii=False)
                browser.close()
                return

            try:
                page.wait_for_selector(
                    "#js_content, .rich_media_content, article, body",
                    timeout=8000,
                )
            except Exception:
                pass

            page.wait_for_timeout(random.randint(800, 1600))

            # Check for CAPTCHA
            page_text = page.inner_text("body")
            if "环境异常" in page_text or "完成验证" in page_text:
                json.dump({"ok": False, "error": "微信反爬验证触发"}, sys.stdout, ensure_ascii=False)
                browser.close()
                return

            # Stage 2: Extract content
            # Try defuddle first (best quality)
            full_html = page.content()
            article = defuddle_extract(full_html, url)

            if not article:
                # Fallback: CSS selector extraction
                article = css_selector_extract(page)

            browser.close()

            if not article.get("markdown") or len(article.get("markdown", "")) < 50:
                json.dump({"ok": False, "error": "文章内容提取失败或内容过短"}, sys.stdout, ensure_ascii=False)
                return

            json.dump({
                "ok": True,
                "title": article.get("title", ""),
                "author": article.get("author", ""),
                "publish_time": article.get("published", ""),
                "markdown": article["markdown"],
            }, sys.stdout, ensure_ascii=False)

    except Exception as e:
        json.dump({"ok": False, "error": str(e)}, sys.stdout, ensure_ascii=False)


if __name__ == "__main__":
    main()
