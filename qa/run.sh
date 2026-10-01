#!/usr/bin/env bash
# Headless-browser checks for output/report.html. Requires Google Chrome and python3.
# Usage: qa/run.sh            -> prints a pass/fail summary, exit 1 on any failure
set -euo pipefail
cd "$(dirname "$0")/.."
CHROME="${CHROME:-/Applications/Google Chrome.app/Contents/MacOS/Google Chrome}"
TMP="$(mktemp -d)"
python3 qa/make_copy.py "$TMP/page.html" qa/harness.js
"$CHROME" --headless=new --disable-gpu --dump-dom --virtual-time-budget=4000 "file://$TMP/page.html" > "$TMP/dom.html" 2>/dev/null
python3 qa/extract.py "$TMP/dom.html" > /dev/null
python3 - "$TMP/dom.json" <<'PY'
import json, sys
R = json.load(open(sys.argv[1])); fails = []
def check(name, ok, detail=''):
    print(('PASS' if ok else 'FAIL'), name, detail)
    if not ok: fails.append(name)
check('no script errors', not R['__errors'] and not R['__windowErrors'], str(R['__errors']) + str(R['__windowErrors']))
check('initial render', R['initial']['bars'] == 25 and R['initial']['checksRows'] > 0 and R['initial']['sources'] > 0)
for L in ['a25', 'f', 'pc', 'cpi', 'rel', 'n']:
    r = R['lens_' + L]
    check(f'lens {L}', r['nBars'] == 25 and r['desc'] and r['matchTable'] and len(r['steps']) >= 2, r['barTitle'])
check('whiskers only on a25/f', R['lens_a25']['whiskers'] == 25 and R['lens_f']['whiskers'] == 25 and R['lens_pc']['whiskers'] == 0)
check('confidence floor A', R['conf_A']['badTiers'] == 0 and R['conf_A']['badFills'] == 0)
check('confidence floor B', R['conf_B']['badTiers'] == 0 and R['conf_B']['badFills'] == 0)
check('era filter', R['era']['out'] == 0)
check('finder Sholay', not R['find_sholay']['hidden'] and R['find_sholay']['nCells'] == 6)
check('finder dubbed unranked', 'not ranked' in json.dumps(R['find_pushpa2']))
check('finder nonsense hides card', R['find_nonsense']['hidden'])
check('found film visible in table', R['find_hit_not_in_first100']['hitRowsOnNominal'] == 1)
check('show more reaches end', R['more']['moreDisplay'] == 'none')
check('no horizontal page overflow', R['overflow']['scrollWidth'] <= R['overflow']['innerWidth'])
check('a11y basics', not R['a11y']['unlabeled'] and R['a11y']['ariaSort'] == R['a11y']['thCount'] and R['a11y']['lang'] == 'en')
print(f"\n{len(fails)} failure(s)")
sys.exit(1 if fails else 0)
PY
