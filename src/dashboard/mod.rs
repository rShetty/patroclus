pub fn dashboard_html() -> String {
    r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Patroclus — Authorization Infrastructure</title>
<style>
*{margin:0;padding:0;box-sizing:border-box}
body{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif;background:#0f1117;color:#e0e0e0;font-size:14px}
a{color:#6c8aff;text-decoration:none}
.header{background:#1a1d29;padding:14px 24px;border-bottom:1px solid #2a2d3a;display:flex;align-items:center;gap:12px;position:sticky;top:0;z-index:100}
.header h1{font-size:20px;color:#6c8aff}
.header .badge{background:#6c8aff;color:#fff;padding:2px 10px;border-radius:4px;font-size:11px;font-weight:600}
.header .health{margin-left:auto;font-size:12px;color:#4caf50;display:flex;align-items:center;gap:6px}
.header .health::before{content:'';width:8px;height:8px;background:#4caf50;border-radius:50%;display:inline-block}
.container{padding:20px;max-width:1400px;margin:0 auto}
.nav{display:flex;gap:4px;margin-bottom:20px;flex-wrap:wrap}
.nav button{background:#1a1d29;color:#888;border:1px solid #2a2d3a;padding:8px 16px;border-radius:6px;cursor:pointer;font-size:13px;transition:all .15s}
.nav button:hover{background:#1e2130;color:#e0e0e0}
.nav button.active{background:#6c8aff;color:#fff;border-color:#6c8aff}
.tab-content{display:none}
.tab-content.active{display:block}
.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(220px,1fr));gap:14px;margin-bottom:20px}
.card{background:#1a1d29;border-radius:8px;padding:18px;border:1px solid #2a2d3a}
.card h3{font-size:11px;color:#666;text-transform:uppercase;letter-spacing:1px;margin-bottom:8px}
.stat{font-size:28px;font-weight:700;color:#6c8aff}
.stat.green{color:#4caf50}
.stat.red{color:#f44336}
.stat.orange{color:#ff9800}
.stat-label{font-size:11px;color:#555;margin-top:4px}
.panel{background:#1a1d29;border-radius:8px;border:1px solid #2a2d3a;overflow:hidden;margin-bottom:20px}
.panel-header{padding:12px 18px;border-bottom:1px solid #2a2d3a;display:flex;align-items:center;justify-content:space-between}
.panel-header h2{font-size:14px;color:#e0e0e0}
.panel-body{padding:0}
table{width:100%;border-collapse:collapse}
th{background:#141620;padding:9px 14px;text-align:left;font-size:11px;color:#666;text-transform:uppercase;letter-spacing:.5px;font-weight:600}
td{padding:9px 14px;border-top:1px solid #2a2d3a;font-size:13px}
tr:hover{background:#1e2130}
.badge-tag{display:inline-block;padding:2px 8px;border-radius:4px;font-size:11px;font-weight:600}
.badge-active{background:#1b3a1b;color:#4caf50}
.badge-suspended{background:#3a2a1b;color:#ff9800}
.badge-decommissioned{background:#3a1b1b;color:#f44336}
.badge-pending{background:#3a2a1b;color:#ff9800}
.badge-approved{background:#1b3a1b;color:#4caf50}
.badge-denied{background:#3a1b1b;color:#f44336}
.badge-allow{background:#1b3a1b;color:#4caf50}
.badge-deny{background:#3a1b1b;color:#f44336}
.badge-require_approval{background:#3a2a1b;color:#ff9800}
.badge-killed{background:#3a1b1b;color:#f44336}
.badge-service{background:#1b2a3a;color:#6c8aff}
.badge-autonomous{background:#3a1b3a;color:#c66aff}
.badge-delegated{background:#3a3a1b;color:#ffc107}
.badge-low{background:#1b3a1b;color:#4caf50}
.badge-medium{background:#3a3a1b;color:#ffc107}
.badge-high{background:#3a2a1b;color:#ff9800}
.badge-critical{background:#3a1b1b;color:#f44336}
.btn{padding:6px 14px;border-radius:5px;border:none;cursor:pointer;font-size:12px;font-weight:600;transition:all .15s}
.btn-green{background:#2e7d32;color:#fff}
.btn-green:hover{background:#388e3c}
.btn-red{background:#c62828;color:#fff}
.btn-red:hover{background:#d32f2f}
.btn-blue{background:#3949ab;color:#fff}
.btn-blue:hover{background:#3f51b5}
.btn-sm{padding:4px 10px;font-size:11px}
.form-row{display:flex;gap:12px;margin-bottom:12px;flex-wrap:wrap}
.form-group{flex:1;min-width:200px}
.form-group label{display:block;font-size:11px;color:#888;margin-bottom:4px;text-transform:uppercase;letter-spacing:.5px}
.form-group input,.form-group select,.form-group textarea{width:100%;padding:8px 10px;background:#0f1117;border:1px solid #2a2d3a;border-radius:5px;color:#e0e0e0;font-size:13px}
.form-group textarea{min-height:120px;font-family:'SF Mono',Monaco,monospace;font-size:12px}
.form-actions{display:flex;gap:8px;margin-top:8px}
.mono{font-family:'SF Mono',Monaco,monospace;font-size:12px}
.truncate{max-width:200px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.toast{position:fixed;bottom:20px;right:20px;padding:14px 20px;border-radius:8px;font-size:13px;z-index:200;animation:slideIn .3s ease}
.toast-success{background:#2e7d32;color:#fff}
.toast-error{background:#c62828;color:#fff}
@keyframes slideIn{from{transform:translateX(100%)}to{transform:translateX(0)}}
.empty{text-align:center;padding:40px;color:#555;font-style:italic}
.pulse{animation:pulse 2s infinite}
@keyframes pulse{0%,100%{opacity:1}50%{opacity:.5}}
.code-block{background:#0f1117;border:1px solid #2a2d3a;border-radius:5px;padding:12px;font-family:'SF Mono',Monaco,monospace;font-size:12px;overflow-x:auto;white-space:pre-wrap;word-break:break-all}
</style>
</head>
<body>
<div class="header">
<h1>PATROCLUS</h1>
<span class="badge">Authorization Infrastructure</span>
<div class="health" id="health-status">Healthy</div>
</div>
<div class="container">
<div class="nav">
<button class="active" onclick="showTab('overview')">Overview</button>
<button onclick="showTab('agents')">Agents</button>
<button onclick="showTab('principals')">Principals</button>
<button onclick="showTab('policies')">Policies</button>
<button onclick="showTab('approvals')">Approvals</button>
<button onclick="showTab('sessions')">Sessions</button>
<button onclick="showTab('audit')">Audit Trail</button>
<button onclick="showTab('vault')">Vault</button>
<button onclick="showTab('idp')">IdP</button>
</div>

<div id="overview" class="tab-content active">
<div class="grid">
<div class="card"><h3>Agents</h3><div class="stat" id="ov-agents">—</div><div class="stat-label">Registered agents</div></div>
<div class="card"><h3>Active Sessions</h3><div class="stat green" id="ov-sessions">—</div><div class="stat-label">In-memory sessions</div></div>
<div class="card"><h3>Pending Approvals</h3><div class="stat orange" id="ov-pending">—</div><div class="stat-label">Awaiting decision</div></div>
<div class="card"><h3>Killed Sessions</h3><div class="stat red" id="ov-killed">—</div><div class="stat-label">Emergency stops</div></div>
<div class="card"><h3>Policies</h3><div class="stat" id="ov-policies">—</div><div class="stat-label">Active policies</div></div>
<div class="card"><h3>Audit Entries</h3><div class="stat" id="ov-audit">—</div><div class="stat-label">Hash-chained log</div></div>
</div>
<div class="panel">
<div class="panel-header"><h2>Recent Authorization Decisions</h2></div>
<div class="panel-body">
<table><thead><tr><th>Time</th><th>Agent</th><th>Action</th><th>Resource</th><th>Decision</th><th>Reason</th></tr></thead>
<tbody id="ov-recent"></tbody></table>
</div>
</div>
</div>

<div id="agents" class="tab-content">
<div class="panel">
<div class="panel-header"><h2>Register New Agent</h2></div>
<div class="panel-body" style="padding:16px">
<div class="form-row">
<div class="form-group"><label>Name</label><input id="ag-name" placeholder="my-agent"></div>
<div class="form-group"><label>Type</label><select id="ag-type"><option value="autonomous">autonomous</option><option value="service">service</option><option value="delegated">delegated</option></select></div>
<div class="form-group"><label>Owner ID (Principal UUID)</label><input id="ag-owner" placeholder="optional"></div>
</div>
<div class="form-actions"><button class="btn btn-blue" onclick="createAgent()">Create Agent</button></div>
</div>
</div>
<div class="panel">
<div class="panel-header"><h2>Agents</h2></div>
<div class="panel-body">
<table><thead><tr><th>ID</th><th>Name</th><th>Type</th><th>Status</th><th>Owner</th><th>Created</th><th>Actions</th></tr></thead>
<tbody id="ag-list"></tbody></table>
</div>
</div>
</div>

<div id="principals" class="tab-content">
<div class="panel">
<div class="panel-header"><h2>Register New Principal</h2></div>
<div class="panel-body" style="padding:16px">
<div class="form-row">
<div class="form-group"><label>External ID</label><input id="pr-ext" placeholder="user-123"></div>
<div class="form-group"><label>IdP Provider</label><input id="pr-idp" value="local"></div>
<div class="form-group"><label>Email</label><input id="pr-email" placeholder="user@example.com"></div>
<div class="form-group"><label>Display Name</label><input id="pr-name" placeholder="User Name"></div>
</div>
<div class="form-actions"><button class="btn btn-blue" onclick="createPrincipal()">Create Principal</button></div>
</div>
</div>
<div class="panel">
<div class="panel-header"><h2>Active Grants</h2></div>
<div class="panel-body">
<table><thead><tr><th>Grant ID</th><th>Agent</th><th>Scopes</th><th>Expires</th><th>Actions</th></tr></thead>
<tbody id="pr-grants"></tbody></table>
</div>
</div>
</div>

<div id="policies" class="tab-content">
<div class="panel">
<div class="panel-header"><h2>Create Policy</h2></div>
<div class="panel-body" style="padding:16px">
<div class="form-row">
<div class="form-group"><label>Policy Name</label><input id="po-name" placeholder="my-policy"></div>
<div class="form-group"><label>Engine</label><select id="po-engine"><option value="yaml">yaml</option></select></div>
</div>
<div class="form-group"><label>Definition (YAML rules array)</label><textarea id="po-def" placeholder="- name: allow-read&#10;  agent_types: [&quot;autonomous&quot;]&#10;  actions: [&quot;read&quot;]&#10;  resources: [&quot;documents/*&quot;]&#10;  decision: allow"></textarea></div>
<div class="form-actions"><button class="btn btn-blue" onclick="createPolicy()">Create Policy</button></div>
</div>
</div>
<div class="panel">
<div class="panel-header"><h2>Policies</h2></div>
<div class="panel-body">
<table><thead><tr><th>ID</th><th>Name</th><th>Engine</th><th>Status</th><th>Definition</th></tr></thead>
<tbody id="po-list"></tbody></table>
</div>
</div>
</div>

<div id="approvals" class="tab-content">
<div class="panel">
<div class="panel-header"><h2>Pending Approvals</h2></div>
<div class="panel-body">
<table><thead><tr><th>ID</th><th>Agent</th><th>Action</th><th>Resource</th><th>Scopes</th><th>Expires</th><th>Actions</th></tr></thead>
<tbody id="ap-list"></tbody></table>
</div>
</div>
</div>

<div id="sessions" class="tab-content">
<div class="grid">
<div class="card"><h3>Total Sessions</h3><div class="stat" id="ss-total">—</div></div>
<div class="card"><h3>Killed</h3><div class="stat red" id="ss-killed">—</div></div>
<div class="card"><h3>Active</h3><div class="stat green" id="ss-active">—</div></div>
</div>
<div class="panel">
<div class="panel-header"><h2>Active Sessions</h2></div>
<div class="panel-body">
<table><thead><tr><th>Session ID</th><th>Agent</th><th>Actions</th><th>Spend</th><th>Tokens</th><th>Trust</th><th>Status</th><th>Action</th></tr></thead>
<tbody id="ss-list"></tbody></table>
</div>
</div>
</div>

<div id="audit" class="tab-content">
<div class="panel">
<div class="panel-header"><h2>Audit Trail (Hash-Chained)</h2></div>
<div class="panel-body">
<table><thead><tr><th>#</th><th>Time</th><th>Agent</th><th>Action</th><th>Resource</th><th>Decision</th><th>Row Hash</th></tr></thead>
<tbody id="au-list"></tbody></table>
</div>
</div>
</div>

<div id="vault" class="tab-content">
<div class="panel">
<div class="panel-header"><h2>Store Credential</h2></div>
<div class="panel-body" style="padding:16px">
<div class="form-row">
<div class="form-group"><label>Principal ID</label><input id="va-pid" placeholder="UUID"></div>
<div class="form-group"><label>Provider</label><input id="va-prov" placeholder="github"></div>
<div class="form-group"><label>Refresh Token</label><input id="va-token" placeholder="token"></div>
</div>
<div class="form-row">
<div class="form-group"><label>Scopes (comma-separated)</label><input id="va-scopes" placeholder="repo,read:user"></div>
</div>
<div class="form-actions"><button class="btn btn-blue" onclick="storeCredential()">Store</button></div>
</div>
</div>
<div class="panel">
<div class="panel-header"><h2>IdP Providers</h2></div>
<div class="panel-body" style="padding:16px" id="va-idp"></div>
</div>
</div>

<div id="idp" class="tab-content">
<div class="panel">
<div class="panel-header"><h2>Identity Provider Federation</h2></div>
<div class="panel-body" style="padding:16px" id="idp-info"><p style="color:#666">Loading...</p></div>
</div>
</div>
</div>

<div id="toast"></div>

<script>
const API='';
function showTab(t){document.querySelectorAll('.tab-content').forEach(e=>e.classList.remove('active'));document.getElementById(t).classList.add('active');loadTab(t)}
async function f(url,opts){const r=await fetch(API+url,opts);if(!r.ok){const t=await r.text();throw new Error(t)}return r.json()}
async function fPost(url,body){return f(url,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)})}
function toast(msg,type='success'){const d=document.getElementById('toast');d.className='toast toast-'+type;d.textContent=msg;setTimeout(()=>d.className='',3000)}
function fmt(ts){if(!ts)return '—';return new Date(ts).toLocaleString()}
function short(s,n=12){if(!s)return '—';return s.length>n?s.substring(0,n)+'…':s}
function badge(val){return `<span class="badge-tag badge-${val}">${val}</span>`}

async function loadOverview(){
try{
const[agents,sessions,approvals,audit,policies]=await Promise.all([
f('/v1/admin/agents'),
f('/v1/sessions'),
f('/v1/principal/approvals'),
f('/v1/admin/audit'),
f('/v1/admin/policies')
]);
document.getElementById('ov-agents').textContent=agents.length;
const killed=sessions.sessions.filter(s=>s.killed).length;
document.getElementById('ov-sessions').textContent=sessions.sessions.length-killed;
document.getElementById('ov-killed').textContent=killed;
document.getElementById('ov-pending').textContent=approvals.length;
document.getElementById('ov-policies').textContent=policies.policies?.length||0;
document.getElementById('ov-audit').textContent=audit.length;
document.getElementById('health-status').textContent='Healthy';
const recent=audit.slice(0,10);
document.getElementById('ov-recent').innerHTML=recent.map(a=>`<tr><td>${fmt(a.timestamp)}</td><td class="mono">${short(a.agent_id)}</td><td>${a.action}</td><td class="truncate">${a.resource}</td><td>${badge(a.decision)}</td><td class="truncate">${a.reason||'—'}</td></tr>`).join('')||'<tr><td colspan="6" class="empty">No audit entries yet</td></tr>';
}catch(e){document.getElementById('health-status').textContent='Error';console.error(e)}
}

async function loadAgents(){
try{
const agents=await f('/v1/admin/agents');
document.getElementById('ag-list').innerHTML=agents.map(a=>`<tr><td class="mono">${short(a.id)}</td><td>${a.name}</td><td>${badge(a.principal_type)}</td><td>${badge(a.status)}</td><td class="mono">${short(a.owner_id)}</td><td>${fmt(a.created_at)}</td><td><button class="btn btn-red btn-sm" onclick="killAgent('${a.id}')">Kill</button></td></tr>`).join('')||'<tr><td colspan="7" class="empty">No agents registered</td></tr>';
}catch(e){console.error(e)}
}
async function createAgent(){
try{
const body={name:ag_name.value,principal_type:ag_type.value};
if(ag_owner.value)body.owner_id=ag_owner.value;
await fPost('/v1/admin/agents',body);
toast('Agent created');ag_name.value='';ag_owner.value='';loadAgents();loadOverview();
}catch(e){toast(e.message,'error')}
}
async function killAgent(id){
if(!confirm('Kill all sessions and revoke tokens for this agent?'))return;
try{const r=await fPost('/v1/admin/agents/'+id+'/kill',{});toast('Agent killed: '+r.sessions_killed+' sessions terminated');loadAgents();loadOverview()}catch(e){toast(e.message,'error')}
}

async function loadPrincipals(){
try{
const grants=await f('/v1/principal/grants');
document.getElementById('pr-grants').innerHTML=grants.grants?.map(g=>`<tr><td class="mono">${short(g.id)}</td><td class="mono">${short(g.agent_id)}</td><td>${(g.scopes||[]).join(', ')}</td><td>${fmt(g.expires_at)}</td><td><button class="btn btn-red btn-sm" onclick="revokeGrant('${g.id}')">Revoke</button></td></tr>`).join('')||'<tr><td colspan="5" class="empty">No grants</td></tr>';
}catch(e){console.error(e)}
}
async function createPrincipal(){
try{
await fPost('/v1/admin/principals',{external_id:pr_ext.value,idp_provider:pr_idp.value,email:pr_email.value,display_name:pr_name.value});
toast('Principal created');pr_ext.value='';pr_email.value='';pr_name.value='';loadPrincipals();
}catch(e){toast(e.message,'error')}
}
async function revokeGrant(id){
try{await fPost('/v1/principal/grants/'+id+'/revoke',{});toast('Grant revoked');loadPrincipals()}catch(e){toast(e.message,'error')}
}

async function loadPolicies(){
try{
const p=await f('/v1/admin/policies');
document.getElementById('po-list').innerHTML=p.policies?.map(po=>`<tr><td class="mono">${po.id}</td><td>${po.name}</td><td>${po.engine}</td><td>${badge(po.status)}</td><td class="truncate mono">${po.definition?.substring(0,100)}</td></tr>`).join('')||'<tr><td colspan="5" class="empty">No policies</td></tr>';
}catch(e){console.error(e)}
}
async function createPolicy(){
try{
await fPost('/v1/admin/policies',{name:po_name.value,engine:po_engine.value,definition:po_def.value});
toast('Policy created');po_name.value='';po_def.value='';loadPolicies();loadOverview();
}catch(e){toast(e.message,'error')}
}

async function loadApprovals(){
try{
const ap=await f('/v1/principal/approvals');
document.getElementById('ap-list').innerHTML=ap.map(a=>`<tr><td class="mono">${short(a.id)}</td><td class="mono">${short(a.agent_id)}</td><td>${a.action}</td><td class="truncate">${a.resource}</td><td>${(a.requested_scopes||[]).join(', ')}</td><td>${fmt(a.expires_at)}</td><td><button class="btn btn-green btn-sm" onclick="approveReq('${a.id}')">Approve</button> <button class="btn btn-red btn-sm" onclick="denyReq('${a.id}')">Deny</button></td></tr>`).join('')||'<tr><td colspan="7" class="empty">No pending approvals</td></tr>';
}catch(e){console.error(e)}
}
async function approveReq(id){
const approver=prompt('Approver ID (Principal UUID):');if(!approver)return;
try{await fPost('/v1/principal/approvals/'+id+'/approve',{approver_id:approver});toast('Approved');loadApprovals();loadOverview()}catch(e){toast(e.message,'error')}
}
async function denyReq(id){
const approver=prompt('Approver ID (Principal UUID):');if(!approver)return;
try{await fPost('/v1/principal/approvals/'+id+'/deny',{approver_id:approver});toast('Denied');loadApprovals();loadOverview()}catch(e){toast(e.message,'error')}
}

async function loadSessions(){
try{
const ss=await f('/v1/sessions');
const killed=ss.sessions.filter(s=>s.killed).length;
document.getElementById('ss-total').textContent=ss.sessions.length;
document.getElementById('ss-killed').textContent=killed;
document.getElementById('ss-active').textContent=ss.sessions.length-killed;
document.getElementById('ss-list').innerHTML=ss.sessions.map(s=>`<tr><td class="mono truncate">${s.session_id}</td><td class="mono">${short(s.agent_id)}</td><td>${s.actions_count}</td><td>$${s.spend_total.toFixed(4)}</td><td>${s.tokens_used}</td><td>${(s.trust_level*100).toFixed(0)}%</td><td>${s.killed?badge('killed'):'<span class="badge-tag badge-active">active</span>'}</td><td>${s.killed?'—':'<button class="btn btn-red btn-sm" onclick="killSession(\\''+s.session_id+'\\')">Kill</button>'}</td></tr>`).join('')||'<tr><td colspan="8" class="empty">No sessions</td></tr>';
}catch(e){console.error(e)}
}
async function killSession(id){
try{await fPost('/v1/sessions/'+encodeURIComponent(id)+'/kill',{});toast('Session killed');loadSessions();loadOverview()}catch(e){toast(e.message,'error')}
}

async function loadAudit(){
try{
const au=await f('/v1/admin/audit');
document.getElementById('au-list').innerHTML=au.map(a=>`<tr><td class="mono">${a.id}</td><td>${fmt(a.timestamp)}</td><td class="mono">${short(a.agent_id)}</td><td>${a.action}</td><td class="truncate">${a.resource}</td><td>${badge(a.decision)}</td><td class="mono truncate">${a.row_hash?.substring(0,16)}</td></tr>`).join('')||'<tr><td colspan="7" class="empty">No audit entries</td></tr>';
}catch(e){console.error(e)}
}

async function loadVault(){
try{
const idp=await f('/v1/idp/providers');
let html='<p style="color:#666;margin-bottom:8px">IdP Federation: '+(idp.enabled?'<span style="color:#4caf50">Enabled</span>':'<span style="color:#f44336">Disabled</span>')+'</p>';
if(idp.providers?.length){html+='<table><thead><tr><th>Provider</th><th>Issuer</th><th>Client ID</th><th>Scopes</th></tr></thead><tbody>';idp.providers.forEach(p=>{html+=`<tr><td>${p.name}</td><td class="truncate">${p.issuer}</td><td class="mono">${short(p.client_id,16)}</td><td>${(p.scopes||[]).join(', ')}</td></tr>`});html+='</tbody></table>'}else{html+='<p class="empty">No IdP providers configured</p>'}
document.getElementById('va-idp').innerHTML=html;
document.getElementById('idp-info').innerHTML=html;
}catch(e){console.error(e)}
}
async function storeCredential(){
try{
await fPost('/v1/vault/credentials',{principal_id:va_pid.value,provider:va_prov.value,refresh_token:va_token.value,scopes:va_scopes.value.split(',').map(s=>s.trim()).filter(Boolean)});
toast('Credential stored');va_pid.value='';va_prov.value='';va_token.value='';va_scopes.value='';
}catch(e){toast(e.message,'error')}
}

function loadTab(t){
if(t==='overview')loadOverview();
else if(t==='agents')loadAgents();
else if(t==='principals')loadPrincipals();
else if(t==='policies')loadPolicies();
else if(t==='approvals')loadApprovals();
else if(t==='sessions')loadSessions();
else if(t==='audit')loadAudit();
else if(t==='vault')loadVault();
else if(t==='idp')loadVault();
}

loadOverview();
setInterval(()=>{const active=document.querySelector('.tab-content.active');if(active)loadTab(active.id)},5000);
</script>
</body>
</html>"#.to_string()
}
