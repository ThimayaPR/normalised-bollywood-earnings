(function(){
const R = {}; const errs = [];
const q = s => document.querySelector(s), qa = s => [...document.querySelectorAll(s)];
function fire(sel, val, ev){ const el = q(sel); if (typeof val === 'boolean') el.checked = val; else el.value = val; el.dispatchEvent(new Event(ev, {bubbles:true})); }
function tableRows(){ return qa('#tbl tbody tr').map(tr => { const td = [...tr.children]; return { rank: td[0].textContent.trim(), title: td[1].textContent.trim(), titleHtml: td[1].innerHTML, year: +td[2].textContent, tier: td[3].textContent.trim(), a25: td[4].textContent.trim(), f: td[5].textContent.trim(), pc: td[7].textContent.trim(), n: td[8].textContent.trim(), cpi: td[10].textContent.trim(), rel: td[11].textContent.trim(), cls: tr.className }; }); }
function bars(){ return qa('#chart-bars g.bar').map(g => { const t = g.querySelectorAll('text'); return { label: t[0].textContent, value: t[1].textContent, whiskers: g.querySelectorAll('line').length, fill: g.querySelector('path').getAttribute('fill') }; }); }
function num(s){ return parseFloat(String(s).replace(/[^0-9.\-]/g,'')); }
function step(name, fn){ try { R[name] = fn(); } catch(e){ errs.push(name + ': ' + (e.stack||e)); R[name] = {error: String(e)}; } }
function reset(){ fire('#lens','a25','change'); fire('#conf','C','change'); fire('#y0','1940','input'); fire('#y1','2026','input'); fire('#q','','input'); fire('#dub',false,'change'); fire('#find','','input'); }

step('initial', () => ({ nFilms: q('#n-films').textContent, count: q('#count').textContent, rows: tableRows().length, bars: bars().length, moreVisible: getComputedStyle(q('#more')).display, barTitle: q('#bar-title').textContent, checksRows: qa('#checks tbody tr').length, sources: qa('#sources li').length, disagree: qa('#disagree tbody tr').length, consensus: qa('#consensus tbody tr').length, datalist: qa('#titles option').length, cardHidden: q('#film-card').hidden }));

// 2. lenses
const lenses = ['a25','f','pc','cpi','rel','n'];
R.lens = {};
for (const L of lenses) step('lens_'+L, () => {
  fire('#lens', L, 'change');
  const b = bars(), rows = tableRows();
  const vals = b.map(x => num(x.value));
  const desc = vals.every((v,i) => i===0 || v <= vals[i-1]);
  const colKey = {a25:'a25', f:'f', pc:'pc', cpi:'cpi', rel:'rel', n:'n'}[L];
  const matchTable = b.every((x,i) => rows[i] && x.label.startsWith(rows[i].title.replace(/dubbed|≈/g,'').trim().slice(0,20)) && num(x.value) === num(rows[i][colKey]));
  const mism = b.map((x,i) => [x.label, x.value, rows[i] && rows[i].title, rows[i] && rows[i][colKey]]).filter(a => num(a[1]) !== num(a[3]));
  return { barTitle: q('#bar-title').textContent, nBars: b.length, desc, matchTable, mism: mism.slice(0,5), whiskers: b.filter(x=>x.whiskers>0).length, bandKeyDisplay: q('#band-key').style.display, exTitle: q('#example-title').textContent, exSub: q('#example-sub').textContent, steps: qa('#example .step h4').map(h=>h.textContent), note: (q('#example .note')||{}).textContent, checks: qa('#checks tbody tr').length, sources: qa('#sources li').length, count: q('#count').textContent, firstRows: rows.slice(0,3).map(r=>[r.rank,r.title,r.year,r[colKey]]), tbl0: rows[0] };
});

// 3. confidence
step('conf_A', () => { reset(); fire('#conf','A','change'); const rows = tableRows(); const fills = bars().map(b=>b.fill); return { count: q('#count').textContent, badTiers: rows.filter(r=>r.tier!=='A').length, badFills: fills.filter(f=>f!=='var(--tierA)').length, n: rows.length }; });
step('conf_B', () => { fire('#conf','B','change'); const rows = tableRows(); const fills = bars().map(b=>b.fill); return { count: q('#count').textContent, badTiers: rows.filter(r=>r.tier==='C').length, badFills: fills.filter(f=>f==='var(--tierC)').length, minYear: Math.min(...rows.map(r=>r.year)) }; });

// 4. era
step('era', () => { reset(); fire('#y0','1994','input'); fire('#y1','2010','input'); const rows = tableRows(); const yrs = rows.map(r=>r.year); return { count: q('#count').textContent, min: Math.min(...yrs), max: Math.max(...yrs), out: yrs.filter(y=>y<1994||y>2010).length }; });
step('era_inverted', () => { fire('#y0','2010','input'); fire('#y1','1994','input'); return { count: q('#count').textContent, rows: tableRows().length, chartHtml: q('#chart-bars').innerHTML.slice(0,200), barTitle: q('#bar-title').textContent, moreDisplay: q('#more').style.display, exTitle: q('#example-title').textContent }; });
step('era_blank', () => { fire('#y0','','input'); fire('#y1','','input'); return { count: q('#count').textContent }; });

// 5. dubbed
step('dub_off', () => { reset(); fire('#q','', 'input'); const want = ['Pushpa 2','KGF Chapter 2','Bahubali 2 - The Conclusion']; const all = []; // show all rows
  while (q('#more').style.display !== 'none') q('#more').click(); const rows = tableRows(); return { total: rows.length, present: want.filter(w => rows.some(r=>r.title.startsWith(w))), dubbedRows: rows.filter(r=>r.cls.includes('dubbed')).length }; });
step('dub_on', () => { fire('#dub', true, 'change'); while (q('#more').style.display !== 'none') q('#more').click(); const rows = tableRows(); const want = ['Pushpa 2','KGF Chapter 2','Bahubali 2 - The Conclusion']; const hits = want.map(w => { const r = rows.find(r=>r.title.startsWith(w)); return r && { title:r.title, rank:r.rank, pill: /class="pill">dubbed/.test(r.titleHtml), cls:r.cls, idx: rows.indexOf(r) }; }); return { total: rows.length, hits, dubbedRows: rows.filter(r=>r.cls.includes('dubbed')).length, barsWithDub: bars().filter(b=>/Pushpa 2|KGF|Bahubali 2/.test(b.label)).length, count: q('#count').textContent, barTitle: q('#bar-title').textContent }; });
step('dub_on_sort_rank', () => { const th = qa('#tbl thead th')[0]; th.click(); const rows = tableRows(); return { header: th.textContent, first: rows.slice(0,3).map(r=>[r.rank,r.title]), last: rows.slice(-3).map(r=>[r.rank,r.title]) }; });

// 6. filter
step('q_sholay', () => { reset(); fire('#q','sholay','input'); const rows = tableRows(); return { count: q('#count').textContent, rows: rows.map(r=>[r.title,r.year]), bars: bars().length }; });
step('q_nonsense', () => { fire('#q','zzqqxx','input'); return { count: q('#count').textContent, rows: tableRows().length, chart: q('#chart-bars').textContent.trim(), barTitle: q('#bar-title').textContent, more: q('#more').style.display }; });

// 7. find
function findTest(v){ fire('#find', v, 'input'); const card = q('#film-card'); const cells = qa('#film-card .lensgrid > div').map(d => ({k: d.querySelector('.k').textContent, v: d.querySelector('.v').textContent, r: d.querySelector('.r').textContent})); const hit = qa('#tbl tbody tr.hit').map(tr=>tr.children[1].textContent); return { hidden: card.hidden, h3: card.querySelector('h3') && card.querySelector('h3').textContent, nCells: cells.length, cells, hitRows: hit, errs: (window.__qaErrors||[]).length }; }
step('find_sholay', () => { reset(); return findTest('Sholay (1975)'); });
step('find_3idiots', () => findTest('3 Idiots'));
step('find_jawan', () => findTest('Jawan'));
step('find_pushpa2', () => { fire('#dub', true, 'change'); const r = findTest('Pushpa 2 (2024)'); fire('#dub', false, 'change'); return r; });
step('find_pushpa2_dubHidden', () => findTest('Pushpa 2 (2024)'));
step('find_nonsense', () => findTest('qqzz not a film'));
step('find_prefix', () => findTest('Hum Aapke'));
step('find_prefix2', () => findTest('Hum Aapke H'));
step('find_empty', () => findTest(''));
step('find_hit_not_in_first100', () => { const r = findTest('Sholay (1975)'); fire('#lens','n','change'); return { hitRowsOnNominal: qa('#tbl tbody tr.hit').length, count: q('#count').textContent }; });

// 8. sorting
step('sort_year', () => { reset(); const th = qa('#tbl thead th').find(t=>t.textContent.trim().startsWith('Year')); th.click(); let rows = tableRows(); const y1 = rows.map(r=>r.year); const desc = y1.every((v,i)=>i===0||v<=y1[i-1]); const hdr1 = qa('#tbl thead th').find(t=>t.dataset.k==='y').textContent; qa('#tbl thead th').find(t=>t.dataset.k==='y').click(); rows = tableRows(); const y2 = rows.map(r=>r.year); const asc = y2.every((v,i)=>i===0||v>=y2[i-1]); const hdr2 = qa('#tbl thead th').find(t=>t.dataset.k==='y').textContent; return { desc, asc, hdr1, hdr2, first1: y1.slice(0,3), first2: y2.slice(0,3), onClass: qa('#tbl thead th.on').length }; });
step('sort_title', () => { const th = qa('#tbl thead th').find(t=>t.dataset.k==='t'); th.click(); const t = tableRows().map(r=>r.title); return { first: t.slice(0,4), hdr: th.textContent }; });
step('sort_then_lens', () => { fire('#lens','f','change'); return { on: qa('#tbl thead th.on').length, first: tableRows().slice(0,2).map(r=>[r.rank,r.title]) }; });

// 9. show more
step('more', () => { reset(); const c0 = q('#count').textContent, n0 = tableRows().length; q('#more').click(); const c1 = q('#count').textContent, n1 = tableRows().length; let clicks = 2; while (q('#more').style.display !== 'none' && clicks < 50) { q('#more').click(); clicks++; } return { c0, n0, c1, n1, clicks, final: q('#count').textContent, finalRows: tableRows().length, moreDisplay: q('#more').style.display }; });
step('more_reset_on_lens', () => { fire('#lens','pc','change'); return { count: q('#count').textContent, rows: tableRows().length }; });
step('more_after_conf', () => { reset(); q('#more').click(); fire('#conf','A','change'); return { count: q('#count').textContent, rows: tableRows().length }; });

// 12. index chart geometry
step('index_chart', () => { reset(); const svg = q('#chart-index svg'); const W = 1000; const texts = qa('#chart-index svg text'); const rupee = texts.filter(t => /^₹/.test(t.textContent) && t.closest('.axis')); const axisLine = qa('#chart-index svg line').find(l => l.getAttribute('x1') === l.getAttribute('x2') && l.getAttribute('x1') === '922'); const ax = 922; const endLabels = qa('#chart-index svg text.lbl').map(t => { const b = t.getBBox(); return { text: t.textContent, x: b.x, right: b.x + b.width, y: b.y, overlapsAxis: b.x + b.width > ax }; }); const callouts = texts.filter(t => t.classList.contains('val') && t.getAttribute('text-anchor') === 'middle').map(t => ({ text: t.textContent, x: +t.getAttribute('x'), y: +t.getAttribute('y') })); const dashed = qa('#chart-index svg path[stroke-dasharray]').length; const over = texts.map(t=>{const b=t.getBBox(); return {text:t.textContent, right:b.x+b.width, x: b.x, w: b.width};}).filter(o=>o.right > W); const rupeeTicks = rupee.map(t=>({text:t.textContent, x:+t.getAttribute('x'), y:+t.getAttribute('y')})); const r = svg.getBoundingClientRect(); return { role: svg.getAttribute('role'), aria: svg.getAttribute('aria-label'), rupeeTicks, axisLineFound: !!axisLine, endLabels, callouts, nCallouts: callouts.length, dashedPaths: dashed, textsBeyondViewBox: over, svgPx: [r.width, r.height], labelCollide: endLabels.map((l,i)=>i>0 && l.y - endLabels[i-1].y < 14) }; });
step('bars_geometry', () => { reset(); const svg = q('#chart-bars svg'); const r = svg.getBoundingClientRect(); const labels = qa('#chart-bars svg g.bar text.lbl').map(t => { const b = t.getBBox(); return { text: t.textContent, x: b.x, right: b.x + b.width, w: b.width }; }); const vals = qa('#chart-bars svg g.bar text.val').map(t => { const b = t.getBBox(); return { text: t.textContent, right: b.x + b.width }; }); return { role: svg.getAttribute('role'), aria: svg.getAttribute('aria-label'), svgPx:[r.width,r.height], maxLabelRight: Math.max(...labels.map(l=>l.right)), minLabelX: Math.min(...labels.map(l=>l.x)), longest: labels.sort((a,b)=>b.w-a.w).slice(0,3), valsBeyond: vals.filter(v=>v.right>1000), maxValRight: Math.max(...vals.map(v=>v.right)), viewBox: svg.getAttribute('viewBox'), scaledFontPx: 12.5 * r.width/1000 }; });

// 11. overflow
step('overflow', () => ({ scrollWidth: document.documentElement.scrollWidth, innerWidth: window.innerWidth, bodyScroll: document.body.scrollWidth, wide: qa('body *').filter(e => { const b = e.getBoundingClientRect(); return b.right > window.innerWidth + 1 && getComputedStyle(e).position !== 'fixed' && !e.closest('.tablewrap') ; }).slice(0,15).map(e => e.tagName + (e.id?'#'+e.id:'') + (e.className && typeof e.className==='string' ? '.'+e.className.split(' ').join('.') : '') + ' right=' + Math.round(e.getBoundingClientRect().right)) }));

// 14. a11y
step('a11y', () => { const inputs = qa('input, select, button'); const unl = inputs.filter(i => !(i.labels && i.labels.length) && !i.getAttribute('aria-label') && i.tagName !== 'BUTTON').map(i=>i.id||i.outerHTML.slice(0,60)); const svgs = qa('svg').map(s=>({role:s.getAttribute('role'), aria:s.getAttribute('aria-label')})); const focusRule = [...document.styleSheets].flatMap(ss=>{try{return [...ss.cssRules]}catch(e){return []}}).some(r=>r.selectorText && r.selectorText.includes(':focus-visible')); const thCount = qa('#tbl thead th').length, thKeyboard = qa('#tbl thead th').filter(t=>t.tabIndex>=0 || t.getAttribute('role')==='button' || t.querySelector('button')).length; const tdInThead = qa('#tbl thead td').length; return { unlabeled: unl, svgs, focusRule, thCount, thKeyboard, tdInThead, lang: document.documentElement.lang, ariaSort: qa('#tbl thead th[aria-sort]').length, tipRole: q('#tip').getAttribute('role') }; });

// tier C contrast — computed colors
step('colors', () => { reset(); const c = qa('#tbl tbody .tier.C')[0] || (()=>{ const s=document.createElement('span'); s.className='tier C'; s.textContent='C'; document.body.appendChild(s); return s; })(); const cs = getComputedStyle(c); const a = (()=>{ const s=document.createElement('span'); s.className='tier A'; document.body.appendChild(s); return getComputedStyle(s); })(); const b = (()=>{ const s=document.createElement('span'); s.className='tier B'; document.body.appendChild(s); return getComputedStyle(s); })(); const tip = getComputedStyle(q('#tip')); const svgT = getComputedStyle(q('#chart-index svg .axis text')); const lbl = getComputedStyle(q('#chart-index svg text.lbl')); const pill = (()=>{ const s=document.createElement('span'); s.className='pill'; document.body.appendChild(s); return getComputedStyle(s); })(); const dim = getComputedStyle(q('#tbl td.dim')||document.body); const root = getComputedStyle(document.documentElement); return { theme: document.documentElement.dataset.theme||'(none)', prefersDark: matchMedia('(prefers-color-scheme: dark)').matches, tierC: {color: cs.color, bg: cs.backgroundColor}, tierA: {color:a.color,bg:a.backgroundColor}, tierB:{color:b.color,bg:b.backgroundColor}, tip:{color:tip.color,bg:tip.backgroundColor}, axisText: svgT.fill, lblFill: lbl.fill, pill:{color:pill.color,bg:pill.backgroundColor}, ground: getComputedStyle(document.body).backgroundColor, surface: root.getPropertyValue('--surface'), ink3: root.getPropertyValue('--ink-3'), surface2: root.getPropertyValue('--surface-2'), ink2: root.getPropertyValue('--ink-2') }; });

reset();
R.__errors = errs; R.__windowErrors = window.__qaErrors || [];
const pre = document.createElement('pre'); pre.id = 'qa'; pre.textContent = JSON.stringify(R); document.body.appendChild(pre);
})();
