"""Build a copy of output/report.html with the QA harness injected (and optional data-theme)."""
import sys, pathlib
root = pathlib.Path(__file__).resolve().parent.parent
src = (root / 'output' / 'report.html').read_text(encoding='utf-8')
out, script_path = sys.argv[1], sys.argv[2]
theme = sys.argv[3] if len(sys.argv) > 3 else None
js = pathlib.Path(script_path).read_text(encoding='utf-8')
if theme:
    src = src.replace('<html lang="en">', f'<html lang="en" data-theme="{theme}">', 1)
inject = '<script>\nwindow.addEventListener("error", e => { (window.__qaErrors = window.__qaErrors||[]).push(String(e.message)+" @"+e.lineno); });\n</script>\n'
src = src.replace('<body>', '<body>\n' + inject, 1)
src = src.replace('</body>', '<script>\n' + js + '\n</script>\n</body>', 1)
pathlib.Path(out).write_text(src, encoding='utf-8')
