//! The static shell (P7: `default-src 'self'` — every asset serves from
//! the bound origin; no CDN, no inline scripts). The shell is the only
//! unauthenticated surface (it carries no data); the human pastes the
//! printed token once — the JS sends it as `Authorization: Bearer` on
//! every fetch (P2) and never puts it in a URL (P9).

pub const INDEX: &str = r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<title>hh-web — run observer</title>
<link rel="stylesheet" href="/app.css"></head><body>
<header>
  <h1>hh-web <span class="tag">read-only instrument</span></h1>
  <nav id="nav"></nav>
  <div id="tokbox"><input id="tok" type="password" placeholder="surface token" autocomplete="off"><button id="tokbtn">set</button></div>
</header>
<main id="main"><p class="dim">Paste the surface token (printed at <code>hh-web</code> startup), then pick a view.</p></main>
<script src="/app.js"></script>
</body></html>
"#;

pub const CSS: &str = r#"body{font-family:ui-monospace,SFMono-Regular,Menlo,monospace;margin:0;color:#1b1b1b;background:#fafafa}
header{display:flex;gap:1em;align-items:baseline;padding:.6em 1em;border-bottom:1px solid #ddd;background:#fff;position:sticky;top:0}
h1{font-size:1em;margin:0}.tag{font-size:.7em;color:#666}
nav{display:flex;gap:.5em;flex-wrap:wrap}nav a{cursor:pointer;color:#0550ae;text-decoration:none}nav a.on{text-decoration:underline;font-weight:600}
main{padding:1em;max-width:1100px}table{border-collapse:collapse;width:100%;font-size:.85em}th,td{border:1px solid #ddd;padding:.25em .5em;text-align:left;vertical-align:top}
.dim{color:#777}.warn{color:#9a6700}.err{color:#cf222e}.ok{color:#1a7f37}
pre{white-space:pre-wrap;word-break:break-word;background:#f0f0f0;padding:.5em;font-size:.8em;max-height:30em;overflow:auto}
button{font:inherit}#tokbox{margin-left:auto}
.badge{border:1px solid #ccc;border-radius:.4em;padding:0 .4em;font-size:.8em}
"#;

pub const JS: &str = r#"let TOK=sessionStorage.getItem('hh-tok')||'';
const views=[['v1_runs','runs'],['v11_run','run'],['v5_monitor','monitor'],['v3_context','context'],['v2_scorecard','scorecard'],['v6_comparison','compare'],['v7_traversal','traverse'],['v8_inbox','inbox'],['v9_delivery','delivery'],['v10_supervision','fleet']];
const nav=document.getElementById('nav'),main=document.getElementById('main');
document.getElementById('tok').value=TOK;
document.getElementById('tokbtn').onclick=()=>{TOK=document.getElementById('tok').value;sessionStorage.setItem('hh-tok',TOK);};
function api(op,params){return fetch('/api',{method:'POST',headers:{'authorization':'Bearer '+TOK,'content-type':'application/json','x-hh-script':'1'},body:JSON.stringify({op,params})}).then(r=>r.json().then(j=>({status:r.status,body:j})));}
function esc(s){return String(s).replace(/[&<>"]/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;'}[c]));}
function table(rows,cols){let h='<table><tr>'+cols.map(c=>'<th>'+esc(c)+'</th>').join('')+'</tr>';
 for(const r of rows)h+='<tr>'+cols.map(c=>'<td>'+esc(r[c]??'')+'</td>').join('')+'</tr>';return h+'</table>';}
function show(v,params){api('view.'+v,params).then(({status,body})=>{
 if(status===401){main.innerHTML='<p class="err">unauthorized — set the token.</p>';return;}
 if(body.error){main.innerHTML='<p class="err">'+esc(body.error)+(body.reason?' ('+esc(body.reason)+')':'')+'</p>';return;}
 render(v,body);});}
function render(v,b){
 if(v==='v1_runs'){const rs=b.runs||b.entries||[];main.innerHTML=table(rs,['run_id','run_kind','status','outcome_class','head_seq','opened_ms']);
  main.querySelectorAll('tr').forEach((tr,i)=>{if(i)tr.onclick=()=>show('v11_run',{run_id:rs[i-1].run_id});});return;}
 if(v==='v11_run'){main.innerHTML='<h2>'+esc(b.run_id)+'</h2>'+kv(b.run_summary)+kv(b.account)+kv(b.head)+kv(b.describe)+kv(b.leases);return;}
 if(v==='v5_monitor'){main.innerHTML='<h2>monitor '+esc(b.run_id)+'</h2>'+kv(b.run_summary)+kv(b.checkpoint)+kv(b.account)+kv(b.head)+kv(b.tail);return;}
 if(v==='v3_context'){main.innerHTML='<h2>context '+esc(b.run_id)+'</h2>'+kv(b.context_view)+kv(b.context_rows);return;}
 if(v==='v7_traversal'){main.innerHTML='<h2>traversal '+esc(b.run_id)+'</h2>'+kv(b.trace_view)+kv(b.cost_view)+kv(b.effect_ledger)+kv(b.permissions);return;}
 if(v==='v8_inbox'){const it=b.items||[];main.innerHTML='<h2>inbox '+esc(b.run_id)+'</h2>'+it.map(x=>inbox(x)).join('');wire();return;}
 if(v==='v9_delivery'){main.innerHTML='<h2>delivery '+esc(b.run_id)+'</h2>'+kv(b.lifecycle)+kv(b.kernel_status);return;}
 if(v==='v10_supervision'){main.innerHTML='<h2>fleet supervision</h2>'+kv(b.fleets)+fleetview(b.fleet_view)+kv(b.control||{})+kv(b.catalogue||{});return;}
 main.innerHTML=kv(b);}
function kv(j){return '<pre>'+esc(JSON.stringify(j,null,1))+'</pre>';}
// V10's fleet lane — the canonical `hh.fleet.view/2` verbatim: items
// table with the spec member set (§7.1), the capacity/escalation folds,
// the metrics member, and `approvals` — the inbox join keyed on
// `work_item_id` (a pending row naming the item renders inline).
function fleetview(fv){if(!fv)return'';
 const items=fv.items||[];let h='<h3>fleet '+esc(fv.run||'')+'</h3>'+table(items.map(it=>Object.assign({approvals_n:(it.approvals||[]).length},it)),
  ['work_item_id','state','owner','source_state','activation_no','escalations_open','approvals_n','children_open']);
 h+=kv(fv.capacity)+kv(fv.escalations)+kv(fv.totals)+kv(fv.approval_inbox)+kv(fv.metrics);return h;}
function inbox(x){const id=x.permission_id||'',opts=x.options||x.offered_options||[];
 const wi=x.work_item_id?(' <span class="badge">work_item '+esc(x.work_item_id)+'</span>'):'';
 return '<div class="badge">'+esc(x.event_class||'pending')+'</div> <code>'+esc(id)+'</code>'+wi+' '+esc(x.request?JSON.stringify(x.request):'')+
 ' '+opts.map(o=>'<button data-pid="'+esc(id)+'" data-out="'+esc(typeof o==='string'?o:o.kind||o)+'">'+esc(typeof o==='string'?o:o.kind||o)+'</button>').join(' ');}
function wire(){main.querySelectorAll('button[data-pid]').forEach(b=>b.onclick=()=>{
 const run_id=window.__run;api('respond_permission',{run_id,permission_id:b.dataset.pid,outcome:b.dataset.out}).then(({body})=>show('v8_inbox',{run_id}));});}
window.__run=null;
for(const[id,label]of views){const a=document.createElement('a');a.textContent=label;a.onclick=()=>{
 if(id==='v1_runs'||id==='v10_supervision')show(id,{});
 else{const r=window.__run||prompt('run_id');if(!r)return;window.__run=r;show(id,{run_id:r});}};nav.appendChild(a);}
show('v1_runs',{});
"#;
