#!/usr/bin/env python3
"""
MarkItDown Worker — ClawWiki sidecar process.

Reads a JSON request from stdin, converts the file using MarkItDown,
writes a JSON response to stdout.

Request:  {"path": "/path/to/file.pdf"}
Response: {"ok": true, "title": "...", "markdown": "...", "source": "pdf"}
Error:    {"ok": false, "error": "..."}

Install: pip install 'markitdown[all]'
"""

import json
import sys
import os


def main():
    try:
        raw = sys.stdin.read()
        req = json.loads(raw)
        file_path = req.get("path", "")

        if not file_path or not os.path.isfile(file_path):
            json.dump({"ok": False, "error": f"File not found: {file_path}"}, sys.stdout)
            return

        # Import markitdown (fail fast with clear error if not installed)
        try:
            from markitdown import MarkItDown
        except ImportError:
            json.dump({
                "ok": False,
                "error": "markitdown not installed. Run: pip install 'markitdown[all]'"
            }, sys.stdout)
            return

        md = MarkItDown()
        result = md.convert(file_path)

        # Extract title from first heading or filename
        title = os.path.splitext(os.path.basename(file_path))[0]
        lines = result.text_content.split("\n")
        for line in lines:
            stripped = line.strip()
            if stripped.startswith("# "):
                title = stripped[2:].strip()
                break

        # Detect source type from extension
        ext = os.path.splitext(file_path)[1].lower().lstrip(".")
        source_map = {
            "pdf": "pdf", "docx": "docx", "doc": "docx",
            "pptx": "pptx", "ppt": "pptx",
            "xlsx": "xlsx", "xls": "xlsx",
            "jpg": "image", "jpeg": "image", "png": "image",
            "gif": "image", "webp": "image", "svg": "image",
            "mp3": "audio", "wav": "audio", "m4a": "audio",
            "mp4": "video", "mkv": "video", "avi": "video",
            "html": "html", "htm": "html",
            "csv": "csv", "json": "json", "xml": "xml",
            "epub": "epub", "ipynb": "notebook",
            "zip": "archive",
        }
        source = source_map.get(ext, ext or "unknown")

        json.dump({
            "ok": True,
            "title": title,
            "markdown": result.text_content,
            "source": source,
        }, sys.stdout, ensure_ascii=False)

    except Exception as e:
        json.dump({"ok": False, "error": str(e)}, sys.stdout, ensure_ascii=False)


if __name__ == "__main__":
    main()
