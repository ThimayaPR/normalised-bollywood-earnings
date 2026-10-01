import sys, re, json, html
d = open(sys.argv[1], encoding='utf-8').read()
m = re.search(r'<pre id="qa">(.*?)</pre>', d, re.S)
if not m: print('NO QA PRE FOUND'); sys.exit(1)
R = json.loads(html.unescape(m.group(1)))
json.dump(R, open(sys.argv[1].replace('.html','.json'),'w'), indent=1)
print(json.dumps(R, indent=1, ensure_ascii=False))
