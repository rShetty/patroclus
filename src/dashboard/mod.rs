pub fn dashboard_html() -> String {
    r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Patroclus — Authorization Infrastructure</title>
<style>
:root{--bg:#0a0b0f;--surface:#13141a;--surface2:#1a1b24;--border:#252634;--border2:#2e2f40;--text:#e8e8f0;--text2:#8b8ba3;--text3:#5a5a73;--primary:#7c6cff;--primary-dim:rgba(124,108,255,.15);--green:#4ade80;--green-dim:rgba(74,222,128,.12);--red:#f87171;--red-dim:rgba(248,113,113,.12);--orange:#fb923c;--orange-dim:rgba(251,146,60,.12);--yellow:#facc15;--radius:12px;--radius-sm:8px;--shadow:0 4px 24px rgba(0,0,0,.4);--transition:.2s cubic-bezier(.4,0,.2,1)}
*{margin:0;padding:0;box-sizing:border-box}
*::-webkit-scrollbar{width:6px;height:6px}
*::-webkit-scrollbar-track{background:transparent}
*::-webkit-scrollbar-thumb{background:var(--border2);border-radius:3px}
*::-webkit-scrollbar-thumb:hover{background:var(--text3)}
body{font-family:'Inter',-apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif;background:var(--bg);color:var(--text);font-size:14px;overflow-x:hidden}
a{color:var(--primary);text-decoration:none}

/* Layout */
.app{display:flex;min-height:100vh}
.sidebar{width:240px;background:var(--surface);border-right:1px solid var(--border);position:fixed;top:0;bottom:0;left:0;display:flex;flex-direction:column;z-index:100;transition:transform var(--transition)}
.sidebar-header{padding:20px;border-bottom:1px solid var(--border);display:flex;align-items:center;gap:10px}
.sidebar-header .logo{width:32px;height:32px;background:linear-gradient(135deg,var(--primary),#9d8cff);border-radius:var(--radius-sm);display:flex;align-items:center;justify-content:center;font-size:16px;font-weight:800;color:#fff}
.sidebar-header h1{font-size:16px;font-weight:700;letter-spacing:-.3px}
.sidebar-header h1 span{color:var(--primary)}
.sidebar-nav{flex:1;padding:12px 8px;overflow-y:auto}
.nav-section{margin-bottom:16px}
.nav-section-title{font-size:10px;font-weight:600;color:var(--text3);text-transform:uppercase;letter-spacing:1.5px;padding:8px 12px}
.nav-item{display:flex;align-items:center;gap:10px;padding:10px 12px;border-radius:var(--radius-sm);cursor:pointer;color:var(--text2);font-size:13px;font-weight:500;transition:all var(--transition);border:1px solid transparent;margin-bottom:2px}
.nav-item:hover{background:var(--surface2);color:var(--text)}
.nav-item.active{background:var(--primary-dim);color:var(--primary);border-color:rgba(124,108,255,.3)}
.nav-item svg{width:16px;height:16px;flex-shrink:0}
.sidebar-footer{padding:12px;border-top:1px solid var(--border)}
.sidebar-footer a{display:flex;align-items:center;gap:8px;color:var(--text3);font-size:12px;padding:8px 12px;border-radius:var(--radius-sm);transition:all var(--transition)}
.sidebar-footer a:hover{background:var(--red-dim);color:var(--red)}

.main{margin-left:240px;flex:1;min-height:100vh;display:flex;flex-direction:column}
.topbar{padding:16px 28px;display:flex;align-items:center;gap:16px;border-bottom:1px solid var(--border);position:sticky;top:0;background:var(--bg);z-index:50}
.topbar .page-title{font-size:20px;font-weight:700;letter-spacing:-.3px}
.topbar .health-pill{margin-left:auto;display:flex;align-items:center;gap:6px;padding:6px 14px;border-radius:20px;font-size:12px;font-weight:600;background:var(--green-dim);color:var(--green);border:1px solid rgba(74,222,128,.2)}
.health-dot{width:8px;height:8px;border-radius:50%;background:var(--green);animation:pulse 2s infinite}
.health-pill.err{background:var(--red-dim);color:var(--red);border-color:rgba(248,113,113,.2)}
.health-pill.err .health-dot{background:var(--red)}
.mobile-toggle{display:none;background:transparent;border:none;color:var(--text);cursor:pointer;padding:4px}
.mobile-toggle svg{width:24px;height:24px}

.content{padding:28px;flex:1;max-width:100%;overflow-x:hidden}

/* Components */
.tab-content{display:none;animation:fadeIn .3s ease}
.tab-content.active{display:block}
@keyframes fadeIn{from{opacity:0;transform:translateY(8px)}to{opacity:1;transform:translateY(0)}}

.stats-grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(180px,1fr));gap:16px;margin-bottom:24px}
.stat-card{background:var(--surface);border:1px solid var(--border);border-radius:var(--radius);padding:20px;transition:all var(--transition)}
.stat-card:hover{border-color:var(--border2);transform:translateY(-2px);box-shadow:var(--shadow)}
.stat-card .label{font-size:11px;font-weight:600;color:var(--text3);text-transform:uppercase;letter-spacing:1px;margin-bottom:8px}
.stat-card .value{font-size:28px;font-weight:800;letter-spacing:-1px}
.stat-card .value.purple{color:var(--primary)}.stat-card .value.green{color:var(--green)}.stat-card .value.red{color:var(--red)}.stat-card .value.orange{color:var(--orange)}
.stat-card .sub{font-size:11px;color:var(--text3);margin-top:4px}

.panel{background:var(--surface);border:1px solid var(--border);border-radius:var(--radius);overflow:hidden;margin-bottom:20px}
.panel-header{padding:16px 20px;border-bottom:1px solid var(--border);display:flex;align-items:center;justify-content:space-between}
.panel-header h2{font-size:15px;font-weight:600}
.panel-header .hint{font-size:12px;color:var(--text3)}
.panel-body{padding:0}
.panel-body.padded{padding:20px}

table{width:100%;border-collapse:collapse}
th{background:var(--surface2);padding:10px 16px;text-align:left;font-size:10px;font-weight:600;color:var(--text3);text-transform:uppercase;letter-spacing:.8px}
td{padding:12px 16px;border-top:1px solid var(--border);font-size:13px}
tr{transition:background var(--transition)}
tr:hover{background:var(--surface2)}

.chip{display:inline-flex;align-items:center;padding:3px 10px;border-radius:20px;font-size:11px;font-weight:600;letter-spacing:.2px}
.chip-green{background:var(--green-dim);color:var(--green);border:1px solid rgba(74,222,128,.15)}
.chip-red{background:var(--red-dim);color:var(--red);border:1px solid rgba(248,113,113,.15)}
.chip-orange{background:var(--orange-dim);color:var(--orange);border:1px solid rgba(251,146,60,.15)}
.chip-purple{background:var(--primary-dim);color:var(--primary);border:1px solid rgba(124,108,255,.15)}
.chip-yellow{background:rgba(250,204,21,.12);color:var(--yellow);border:1px solid rgba(250,204,21,.15)}
.chip-gray{background:var(--surface2);color:var(--text2);border:1px solid var(--border)}

.btn{display:inline-flex;align-items:center;gap:6px;padding:8px 16px;border-radius:var(--radius-sm);border:1px solid var(--border2);cursor:pointer;font-size:13px;font-weight:600;transition:all var(--transition);background:var(--surface2);color:var(--text)}
.btn:hover{border-color:var(--text3);background:var(--border)}
.btn-primary{background:var(--primary);color:#fff;border-color:var(--primary)}
.btn-primary:hover{background:#6b5ff0;border-color:#6b5ff0}
.btn-success{background:var(--green);color:#0a0b0f;border-color:var(--green)}
.btn-success:hover{background:#3dd070}
.btn-danger{background:var(--red);color:#fff;border-color:var(--red)}
.btn-danger:hover{background:#e55555}
.btn-sm{padding:5px 12px;font-size:11px}
.btn-ghost{background:transparent;border-color:transparent;color:var(--text2)}
.btn-ghost:hover{background:var(--surface2)}

.form-grid{display:flex;gap:16px;margin-bottom:14px;flex-wrap:wrap}
.form-field{flex:1;min-width:200px}
.form-field label{display:block;font-size:11px;font-weight:600;color:var(--text3);text-transform:uppercase;letter-spacing:.8px;margin-bottom:6px}
.form-field input,.form-field select,.form-field textarea{width:100%;padding:10px 14px;background:var(--bg);border:1px solid var(--border);border-radius:var(--radius-sm);color:var(--text);font-size:14px;font-family:inherit;transition:all var(--transition)}
.form-field input:focus,.form-field select:focus,.form-field textarea:focus{outline:none;border-color:var(--primary);box-shadow:0 0 0 3px var(--primary-dim)}
.form-field textarea{min-height:120px;font-family:'SF Mono',Monaco,monospace;font-size:13px;resize:vertical}
.form-field select{cursor:pointer}
.form-field select option{background:var(--surface)}
.form-actions{display:flex;gap:8px;margin-top:14px}

.mono{font-family:'SF Mono',Monaco,monospace;font-size:12px}
.truncate{max-width:180px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.result-box{background:var(--bg);border:1px solid var(--border);border-radius:var(--radius-sm);padding:14px;margin-top:14px;font-family:'SF Mono',Monaco,monospace;font-size:12px;overflow-x:auto;white-space:pre-wrap;word-break:break-all;display:none;max-height:300px;overflow-y:auto}
.result-box.show{display:block;animation:fadeIn .2s ease}
.result-box.ok{border-color:rgba(74,222,128,.3)}
.result-box.fail{border-color:rgba(248,113,113,.3)}

.toast-wrap{position:fixed;bottom:24px;right:24px;z-index:300;display:flex;flex-direction:column;gap:8px}
.toast{padding:14px 20px;border-radius:var(--radius);font-size:13px;font-weight:500;animation:slideIn .3s ease;box-shadow:var(--shadow);display:flex;align-items:center;gap:10px}
.toast-success{background:var(--green-dim);color:var(--green);border:1px solid rgba(74,222,128,.2)}
.toast-error{background:var(--red-dim);color:var(--red);border:1px solid rgba(248,113,113,.2)}
@keyframes slideIn{from{transform:translateX(120%)}to{transform:translateX(0)}}
@keyframes pulse{0%,100%{opacity:1}50%{opacity:.4}}

.empty{text-align:center;padding:48px;color:var(--text3);font-size:14px}
.code-block{background:var(--bg);border:1px solid var(--border);border-radius:var(--radius-sm);padding:14px;font-family:'SF Mono',Monaco,monospace;font-size:12px;overflow-x:auto;white-space:pre-wrap;word-break:break-all;margin-top:8px}

.modal-bg{position:fixed;inset:0;background:rgba(0,0,0,.6);backdrop-filter:blur(4px);z-index:300;display:none;align-items:center;justify-content:center}
.modal-bg.show{display:flex;animation:fadeIn .2s ease}
.modal{background:var(--surface);border:1px solid var(--border);border-radius:var(--radius);padding:28px;width:680px;max-width:90vw;max-height:80vh;overflow-y:auto;box-shadow:var(--shadow)}
.modal-head{display:flex;align-items:center;justify-content:space-between;margin-bottom:20px}
.modal-head h2{font-size:18px;font-weight:700}
.modal-close{background:transparent;border:none;color:var(--text3);cursor:pointer;font-size:24px;line-height:1}

/* Stepper */
.stepper{display:flex;flex-direction:column;gap:0;margin-bottom:24px}
.step{display:flex;align-items:flex-start;gap:14px;padding-bottom:20px;position:relative}
.step:not(:last-child)::after{content:'';position:absolute;left:15px;top:32px;bottom:0;width:2px;background:var(--border)}
.step-num{width:30px;height:30px;border-radius:50%;display:flex;align-items:center;justify-content:center;font-size:12px;font-weight:700;flex-shrink:0;transition:all var(--transition);background:var(--surface2);color:var(--text3);border:2px solid var(--border)}
.step.active .step-num{background:var(--primary);color:#fff;border-color:var(--primary)}
.step.done .step-num{background:var(--green);color:#0a0b0f;border-color:var(--green)}
.step-body{flex:1;padding-top:3px}
.step-body strong{font-size:14px;font-weight:600;display:block;margin-bottom:2px}
.step-body span{font-size:12px;color:var(--text3)}

.loading-bar{height:3px;background:var(--surface);overflow:hidden;border-radius:2px;margin-bottom:20px;display:none}
.loading-bar.show{display:block}
.loading-bar .bar{height:100%;width:30%;background:var(--primary);border-radius:2px;animation:loading 1.5s infinite}
@keyframes loading{0%{margin-left:-30%}100%{margin-left:100%}}

/* Mobile */
.mobile-overlay{display:none;position:fixed;inset:0;background:rgba(0,0,0,.5);z-index:99}
@media(max-width:768px){
.sidebar{transform:translateX(-100%)}
.sidebar.open{transform:translateX(0)}
.main{margin-left:0}
.mobile-toggle{display:flex}
.mobile-overlay.show{display:block}
.content{padding:16px}
.stats-grid{grid-template-columns:repeat(2,1fr)}
.form-grid{flex-direction:column}
.form-field{min-width:100%}
}
</style>
</head>
<body>
<div class="app">
<nav class="sidebar" id="sidebar">
<div class="sidebar-header">
<div class="logo">P</div>
<h1>Patro<span>clus</span></h1>
</div>
<div class="sidebar-nav">
<div class="nav-section">
<div class="nav-section-title">Dashboard</div>
<div class="nav-item active" onclick="showTab('overview')">
<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="3" width="7" height="7" rx="1"/><rect x="14" y="3" width="7" height="7" rx="1"/><rect x="3" y="14" width="7" height="7" rx="1"/><rect x="14" y="14" width="7" height="7" rx="1"/></svg>
Overview</div>
<div class="nav-item" onclick="showTab('guided')">
<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M9 11l3 3L22 4"/><path d="M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11"/></svg>
Guided Flow</div>
</div>
<div class="nav-section">
<div class="nav-section-title">Identity</div>
<div class="nav-item" onclick="showTab('principals')">
<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/></svg>
Principals</div>
<div class="nav-item" onclick="showTab('agents')">
<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="11" width="18" height="10" rx="2"/><circle cx="12" cy="5" r="2"/><path d="M12 7v4"/><line x1="8" y1="16" x2="8" y2="16"/><line x1="16" y1="16" x2="16" y2="16"/></svg>
Agents</div>
<div class="nav-item" onclick="showTab('resources')">
<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M22 12h-4l-3 9L9 3l-3 9H2"/></svg>
Resources</div>
<div class="nav-item" onclick="showTab('idp')">
<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 2l-2 2m-7.61 7.61a5.5 5.5 0 1 1-7.778 7.778 5.5 5.5 0 1 1 7.778-7.778zm0 0L15.5 7.5m0 0l3 3L22 7l-3-3m-3.5 3.5L15 7"/></svg>
IdP Federation</div>
</div>
<div class="nav-section">
<div class="nav-section-title">Authorization</div>
<div class="nav-item" onclick="showTab('policies')">
<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/></svg>
Policies</div>
<div class="nav-item" onclick="showTab('delegation')">
<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="16 18 22 12 16 6"/><polyline points="8 6 2 12 8 18"/></svg>
Delegation</div>
<div class="nav-item" onclick="showTab('access')">
<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/></svg>
Test Access</div>
<div class="nav-item" onclick="showTab('approvals')">
<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M9 12l2 2 4-4"/><path d="M21 12c0 4.97-4.03 9-9 9s-9-4.03-9-9 4.03-9 9-9c2.39 0 4.68.94 6.36 2.64"/></svg>
Approvals</div>
</div>
<div class="nav-section">
<div class="nav-section-title">Monitoring</div>
<div class="nav-item" onclick="showTab('sessions')">
<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/></svg>
Sessions</div>
<div class="nav-item" onclick="showTab('audit')">
<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M3 3v18h18"/><path d="M7 17l4-4 3 3 5-6"/></svg>
Audit Trail</div>
</div>
<div class="nav-section">
<div class="nav-section-title">Security</div>
<div class="nav-item" onclick="showTab('vault')">
<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="11" width="18" height="11" rx="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/></svg>
Vault</div>
</div>
</div>
<div class="sidebar-footer">
<a href="/logout">
<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="16" height="16"><path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4"/><polyline points="16 17 21 12 16 7"/><line x1="21" y1="12" x2="9" y2="12"/></svg>
Logout</a>
</div>
</nav>
<div class="mobile-overlay" id="mobile-overlay" onclick="closeSidebar()"></div>

<div class="main">
<div class="topbar">
<button class="mobile-toggle" onclick="toggleSidebar()"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="3" y1="6" x2="21" y2="6"/><line x1="3" y1="12" x2="21" y2="12"/><line x1="3" y1="18" x2="21" y2="18"/></svg></button>
<div class="page-title" id="page-title">Overview</div>
<div class="health-pill" id="health-status"><div class="health-dot"></div>Healthy</div>
</div>
<div class="loading-bar" id="loading-bar"><div class="bar"></div></div>

<div class="content">

<div id="overview" class="tab-content active">
<div class="stats-grid">
<div class="stat-card"><div class="label">Agents</div><div class="value purple" id="ov-agents">—</div><div class="sub">Registered</div></div>
<div class="stat-card"><div class="label">Resources</div><div class="value purple" id="ov-resources">—</div><div class="sub">Registered</div></div>
<div class="stat-card"><div class="label">Active Sessions</div><div class="value green" id="ov-sessions">—</div><div class="sub">In-memory</div></div>
<div class="stat-card"><div class="label">Pending Approvals</div><div class="value orange" id="ov-pending">—</div><div class="sub">Awaiting decision</div></div>
<div class="stat-card"><div class="label">Active Grants</div><div class="value purple" id="ov-grants">—</div><div class="sub">Issued tokens</div></div>
<div class="stat-card"><div class="label">Policies</div><div class="value purple" id="ov-policies">—</div><div class="sub">Active</div></div>
<div class="stat-card"><div class="label">Audit Entries</div><div class="value purple" id="ov-audit">—</div><div class="sub">Hash-chained</div></div>
<div class="stat-card"><div class="label">Killed Sessions</div><div class="value red" id="ov-killed">—</div><div class="sub">Emergency stops</div></div>
</div>
<div class="panel">
<div class="panel-header"><h2>Recent Authorization Decisions</h2></div>
<div class="panel-body">
<table><thead><tr><th>Time</th><th>Agent</th><th>Action</th><th>Resource</th><th>Decision</th><th>Reason</th><th>Token</th></tr></thead>
<tbody id="ov-recent"></tbody></table>
</div>
</div>
</div>

<div id="guided" class="tab-content">
<div class="panel"><div class="panel-body padded">
<div class="panel-header" style="border:none;padding:0;margin-bottom:20px"><h2>End-to-End Guided Flow</h2></div>
<p style="color:var(--text2);font-size:13px;margin-bottom:24px">Walk through the full authorization lifecycle: create principal, agent, policy, delegate scopes, request access, approve, and view audit trail.</p>
<div class="stepper" id="stepper"></div>
<div id="guided-action"></div>
<div class="result-box" id="guided-result"></div>
</div></div>
</div>

<div id="principals" class="tab-content">
<div class="panel"><div class="panel-header"><h2>Register Principal</h2></div><div class="panel-body padded">
<div class="form-grid">
<div class="form-field"><label>External ID</label><input id="pr-ext" placeholder="user-123"></div>
<div class="form-field"><label>IdP Provider</label><input id="pr-idp" value="local"></div>
<div class="form-field"><label>Email</label><input id="pr-email" placeholder="user@example.com"></div>
<div class="form-field"><label>Display Name</label><input id="pr-name" placeholder="User Name"></div>
</div>
<div class="form-actions"><button class="btn btn-primary" onclick="createPrincipal()">Create Principal</button></div>
<div class="result-box" id="pr-result"></div>
</div></div>
<div class="panel"><div class="panel-header"><h2>Active Grants</h2></div><div class="panel-body">
<table><thead><tr><th>Grant ID</th><th>Agent</th><th>Scopes</th><th>Token JTI</th><th>Expires</th><th>Actions</th></tr></thead>
<tbody id="pr-grants"></tbody></table>
</div></div>
</div>

<div id="agents" class="tab-content">
<div class="panel"><div class="panel-header"><h2>Register Agent</h2></div><div class="panel-body padded">
<div class="form-grid">
<div class="form-field"><label>Name</label><input id="ag-name" placeholder="my-agent"></div>
<div class="form-field"><label>Type</label><select id="ag-type"><option value="autonomous">autonomous</option><option value="service">service</option><option value="delegated">delegated</option></select></div>
<div class="form-field"><label>Owner ID (Principal)</label><input id="ag-owner" placeholder="optional UUID"></div>
</div>
<div class="form-actions"><button class="btn btn-primary" onclick="createAgent()">Create Agent</button></div>
<div class="result-box" id="ag-result"></div>
</div></div>
<div class="panel"><div class="panel-header"><h2>Agents</h2></div><div class="panel-body">
<table><thead><tr><th>ID</th><th>Name</th><th>Type</th><th>Status</th><th>Owner</th><th>Created</th><th>Actions</th></tr></thead>
<tbody id="ag-list"></tbody></table>
</div></div>
<div class="panel"><div class="panel-header"><h2>Record Spend</h2></div><div class="panel-body padded">
<div class="form-grid">
<div class="form-field"><label>Agent ID</label><input id="sp-agent" placeholder="UUID"></div>
<div class="form-field"><label>Amount ($)</label><input id="sp-amount" type="number" step="0.0001" placeholder="0.002"></div>
<div class="form-field"><label>Session ID (optional)</label><input id="sp-session"></div>
</div>
<div class="form-actions"><button class="btn btn-primary" onclick="recordSpend()">Record</button></div>
<div class="result-box" id="sp-result"></div>
</div></div>
</div>

<div id="resources" class="tab-content">
<div class="panel"><div class="panel-header"><h2>Register Resource</h2></div><div class="panel-body padded">
<div class="form-grid">
<div class="form-field"><label>Name</label><input id="rs-name" placeholder="my-api"></div>
<div class="form-field"><label>Type</label><select id="rs-type"><option value="api">api</option><option value="mcp_server">mcp_server</option><option value="database">database</option><option value="cloud_service">cloud_service</option></select></div>
<div class="form-field"><label>URI</label><input id="rs-uri" placeholder="https://api.example.com"></div>
<div class="form-field"><label>Sensitivity</label><select id="rs-sens"><option value="low">low</option><option value="medium">medium</option><option value="high">high</option><option value="critical">critical</option></select></div>
</div>
<div class="form-field"><label>Actions (JSON)</label><input id="rs-actions" value='["read","write"]'></div>
<div class="form-actions"><button class="btn btn-primary" onclick="createResource()">Create</button></div>
<div class="result-box" id="rs-result"></div>
</div></div>
<div class="panel"><div class="panel-header"><h2>Resources</h2></div><div class="panel-body">
<table><thead><tr><th>ID</th><th>Name</th><th>Type</th><th>URI</th><th>Sensitivity</th><th>Actions</th></tr></thead>
<tbody id="rs-list"></tbody></table>
</div></div>
</div>

<div id="policies" class="tab-content">
<div class="panel"><div class="panel-header"><h2>Create Policy</h2></div><div class="panel-body padded">
<div class="form-grid">
<div class="form-field"><label>Policy Name</label><input id="po-name" placeholder="my-policy"></div>
<div class="form-field"><label>Engine</label><select id="po-engine"><option value="yaml">yaml</option></select></div>
</div>
<div class="form-field"><label>Definition (YAML rules array)</label><textarea id="po-def">- name: allow-read
  agent_types: ["autonomous"]
  actions: ["read", "write"]
  resources: ["documents/*"]
  decision: allow
- name: deny-delete
  agent_types: ["autonomous"]
  actions: ["delete"]
  resources: ["*"]
  decision: deny
- name: approval-required
  agent_types: ["autonomous"]
  actions: ["execute"]
  resources: ["database/*"]
  decision: require_approval</textarea></div>
<div class="form-actions"><button class="btn btn-primary" onclick="createPolicy()">Create Policy</button></div>
<div class="result-box" id="po-result"></div>
</div></div>
<div class="panel"><div class="panel-header"><h2>Policies</h2></div><div class="panel-body">
<table><thead><tr><th>ID</th><th>Name</th><th>Engine</th><th>Status</th><th>Actions</th></tr></thead>
<tbody id="po-list"></tbody></table>
</div></div>
</div>

<div id="delegation" class="tab-content">
<div class="panel"><div class="panel-header"><h2>Principal to Agent</h2><span class="hint">Grant scopes to an agent</span></div><div class="panel-body padded">
<div class="form-grid">
<div class="form-field"><label>Agent ID</label><input id="dl-agent" placeholder="UUID"></div>
<div class="form-field"><label>Scopes</label><input id="dl-scopes" placeholder="documents:read,documents:write"></div>
<div class="form-field"><label>TTL (seconds)</label><input id="dl-ttl" type="number" value="900"></div>
</div>
<div class="form-field"><label>Constraints (JSON, optional)</label><input id="dl-constraints" placeholder='{"max_amount": 10.0}'></div>
<div class="form-actions"><button class="btn btn-primary" onclick="principalDelegate()">Delegate Scopes</button></div>
<div class="result-box" id="dl-result"></div>
</div></div>
<div class="panel"><div class="panel-header"><h2>Agent to Agent</h2><span class="hint">Sub-agent from parent grant</span></div><div class="panel-body padded">
<div class="form-grid">
<div class="form-field"><label>Parent Token (JWT)</label><input id="sd-token" placeholder="eyJ0eXAi..."></div>
<div class="form-field"><label>Sub-Agent ID</label><input id="sd-agent" placeholder="UUID"></div>
<div class="form-field"><label>Scopes</label><input id="sd-scopes" placeholder="documents:read"></div>
<div class="form-field"><label>TTL (seconds)</label><input id="sd-ttl" type="number" value="900"></div>
</div>
<div class="form-actions"><button class="btn btn-primary" onclick="agentDelegate()">Delegate Token</button></div>
<div class="result-box" id="sd-result"></div>
</div></div>
<div class="panel"><div class="panel-header"><h2>Revoke Token by JTI</h2></div><div class="panel-body padded">
<div class="form-grid"><div class="form-field"><label>Token JTI</label><input id="rv-jti" placeholder="01a00042-..."></div></div>
<div class="form-actions"><button class="btn btn-danger" onclick="revokeToken()">Revoke Token</button></div>
<div class="result-box" id="rv-result"></div>
</div></div>
</div>

<div id="access" class="tab-content">
<div class="panel"><div class="panel-header"><h2>Request Access</h2><span class="hint">Issues token or triggers approval</span></div><div class="panel-body padded">
<div class="form-grid">
<div class="form-field"><label>Agent ID</label><input id="ac-agent" placeholder="UUID"></div>
<div class="form-field"><label>Action</label><input id="ac-action" placeholder="read"></div>
<div class="form-field"><label>Resource</label><input id="ac-resource" placeholder="documents/report.pdf"></div>
<div class="form-field"><label>Scopes</label><input id="ac-scopes" placeholder="documents:read"></div>
</div>
<div class="form-grid">
<div class="form-field"><label>Delegation Token (optional)</label><input id="ac-dtoken" placeholder="JWT"></div>
<div class="form-field"><label>Session ID (optional)</label><input id="ac-session"></div>
</div>
<div class="form-actions"><button class="btn btn-success" onclick="requestAccess()">Request Access</button><button class="btn btn-primary" onclick="checkAccess()">Check Access (dry-run)</button></div>
<div class="result-box" id="ac-result"></div>
</div></div>
<div class="panel"><div class="panel-header"><h2>Check Approval Status</h2></div><div class="panel-body padded">
<div class="form-grid"><div class="form-field"><label>Approval ID</label><input id="ap-id-check" placeholder="UUID"></div></div>
<div class="form-actions"><button class="btn btn-primary" onclick="checkApprovalStatus()">Check Status</button></div>
<div class="result-box" id="ap-check-result"></div>
</div></div>
</div>

<div id="approvals" class="tab-content">
<div class="panel"><div class="panel-header"><h2>Pending Approvals</h2></div><div class="panel-body">
<table><thead><tr><th>ID</th><th>Agent</th><th>Action</th><th>Resource</th><th>Scopes</th><th>Status</th><th>Expires</th><th>Actions</th></tr></thead>
<tbody id="ap-list"></tbody></table>
</div></div>
</div>

<div id="sessions" class="tab-content">
<div class="stats-grid">
<div class="stat-card"><div class="label">Total</div><div class="value purple" id="ss-total">—</div></div>
<div class="stat-card"><div class="label">Active</div><div class="value green" id="ss-active">—</div></div>
<div class="stat-card"><div class="label">Killed</div><div class="value red" id="ss-killed">—</div></div>
</div>
<div class="panel"><div class="panel-header"><h2>Sessions</h2></div><div class="panel-body">
<table><thead><tr><th>Session ID</th><th>Agent</th><th>Actions</th><th>Spend</th><th>Tokens</th><th>Trust</th><th>Status</th><th>Action</th></tr></thead>
<tbody id="ss-list"></tbody></table>
</div></div>
</div>

<div id="audit" class="tab-content">
<div class="panel"><div class="panel-header"><h2>Audit Trail</h2><span class="hint">Hash-chained, tamper-evident</span></div><div class="panel-body">
<table><thead><tr><th>#</th><th>Time</th><th>Agent</th><th>Action</th><th>Resource</th><th>Decision</th><th>Reason</th><th>Row Hash</th><th>Prev Hash</th></tr></thead>
<tbody id="au-list"></tbody></table>
</div></div>
</div>

<div id="vault" class="tab-content">
<div class="panel"><div class="panel-header"><h2>Vault Key Management</h2></div><div class="panel-body padded">
<div class="form-actions"><button class="btn btn-primary" onclick="generateVaultKey()">Generate New Vault Key</button></div>
<div class="result-box" id="vk-result"></div>
</div></div>
<div class="panel"><div class="panel-header"><h2>Store Credential</h2></div><div class="panel-body padded">
<div class="form-grid">
<div class="form-field"><label>Principal ID</label><input id="va-pid" placeholder="UUID"></div>
<div class="form-field"><label>Provider</label><input id="va-prov" placeholder="github"></div>
<div class="form-field"><label>Refresh Token</label><input id="va-token" placeholder="token"></div>
</div>
<div class="form-grid"><div class="form-field"><label>Scopes</label><input id="va-scopes" placeholder="repo,read:user"></div></div>
<div class="form-actions"><button class="btn btn-primary" onclick="storeCredential()">Store</button></div>
<div class="result-box" id="va-result"></div>
</div></div>
</div>

<div id="idp" class="tab-content">
<div class="panel"><div class="panel-header"><h2>Identity Provider Federation</h2></div><div class="panel-body padded" id="idp-info"><p style="color:var(--text3)">Loading...</p></div></div>
</div>

</div>
</div>
</div>

<div class="toast-wrap" id="toast-wrap"></div>
<div class="modal-bg" id="modal-bg" onclick="if(event.target===this)closeModal()">
<div class="modal"><div class="modal-head"><h2 id="modal-title"></h2><button class="modal-close" onclick="closeModal()">&times;</button></div><div id="modal-content"></div></div>
</div>

<script>
const API='';
const state={guidedStep:1,guidedPrincipalId:null,guidedAgentId:null,guidedApprovalId:null};
const TITLES={overview:'Overview',guided:'Guided Flow',principals:'Principals',agents:'Agents',resources:'Resources',policies:'Policies',delegation:'Delegation',access:'Test Access',approvals:'Approvals',sessions:'Sessions',audit:'Audit Trail',vault:'Vault',idp:'IdP Federation'};

function toggleSidebar(){document.getElementById('sidebar').classList.toggle('open');document.getElementById('mobile-overlay').classList.toggle('show')}
function closeSidebar(){document.getElementById('sidebar').classList.remove('open');document.getElementById('mobile-overlay').classList.remove('show')}
function showTab(t){document.querySelectorAll('.tab-content').forEach(e=>e.classList.remove('active'));document.getElementById(t).classList.add('active');document.querySelectorAll('.nav-item').forEach(b=>b.classList.remove('active'));const btn=[...document.querySelectorAll('.nav-item')].find(b=>b.getAttribute('onclick')?.includes("'"+t+"'"));if(btn)btn.classList.add('active');document.getElementById('page-title').textContent=TITLES[t]||t;closeSidebar();loadTab(t)}
async function f(url,opts){const r=await fetch(API+url,opts);const t=await r.text();if(!r.ok)throw new Error(t);try{return JSON.parse(t)}catch{return t}}
async function fPost(url,body){return f(url,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)})}
function toast(msg,type='success'){const wrap=document.getElementById('toast-wrap');const d=document.createElement('div');d.className='toast toast-'+type;d.textContent=msg;wrap.appendChild(d);setTimeout(()=>{d.remove()},3000)}
function showResult(id,data,ok=true){const el=document.getElementById(id);el.className='result-box show '+(ok?'ok':'fail');el.textContent=typeof data==='string'?data:JSON.stringify(data,null,2)}
function fmt(ts){if(!ts)return '\u2014';return new Date(ts).toLocaleString()}
function short(s,n=12){if(!s)return '\u2014';return s.length>n?s.substring(0,n)+'\u2026':s}
function chip(val){const cls={'allow':'green','deny':'red','require_approval':'orange','pending':'orange','approved':'green','denied':'red','active':'green','suspended':'orange','decommissioned':'red','killed':'red','service':'purple','autonomous':'purple','delegated':'yellow','low':'green','medium':'yellow','high':'orange','critical':'red','mcp_server':'purple','api':'green','database':'yellow','cloud_service':'purple'};return '<span class="chip chip-'+(cls[val]||'gray')+'">'+val+'</span>'}
function openModal(title,content){document.getElementById('modal-title').textContent=title;document.getElementById('modal-content').innerHTML=content;document.getElementById('modal-bg').classList.add('show')}
function closeModal(){document.getElementById('modal-bg').classList.remove('show')}

async function loadOverview(){
try{
const[agents,sessions,approvals,audit,policies,resources,grants]=await Promise.all([f('/v1/admin/agents'),f('/v1/sessions'),f('/v1/principal/approvals'),f('/v1/admin/audit'),f('/v1/admin/policies'),f('/v1/admin/resources'),f('/v1/principal/grants')]);
document.getElementById('ov-agents').textContent=agents.length;
document.getElementById('ov-resources').textContent=resources.length;
const killed=sessions.sessions.filter(s=>s.killed).length;
document.getElementById('ov-sessions').textContent=sessions.sessions.length-killed;
document.getElementById('ov-killed').textContent=killed;
document.getElementById('ov-pending').textContent=approvals.length;
document.getElementById('ov-grants').textContent=grants.grants?.length||0;
document.getElementById('ov-policies').textContent=policies.policies?.length||0;
document.getElementById('ov-audit').textContent=audit.length;
const hs=document.getElementById('health-status');hs.classList.remove('err');hs.innerHTML='<div class="health-dot"></div>Healthy';
document.getElementById('ov-recent').innerHTML=audit.slice(0,10).map(a=>'<tr><td>'+fmt(a.timestamp)+'</td><td class="mono">'+short(a.agent_id)+'</td><td>'+a.action+'</td><td class="truncate">'+a.resource+'</td><td>'+chip(a.decision)+'</td><td class="truncate">'+(a.reason||'\u2014')+'</td><td class="mono truncate">'+short(a.token_jti)+'</td></tr>').join('')||'<tr><td colspan="7" class="empty">No audit entries</td></tr>';
}catch(e){document.getElementById('health-status').classList.add('err');document.getElementById('health-status').innerHTML='<div class="health-dot"></div>Error';console.error(e)}
}

// GUIDED FLOW
const STEPS=[
{t:'Create Principal',d:'Register a user who owns agents'},
{t:'Create Agent',d:'Register an AI agent owned by the principal'},
{t:'Create Policy',d:'Define authorization rules (allow/deny/require_approval)'},
{t:'Delegate Scopes',d:'Principal grants scopes to the agent'},
{t:'Request Access',d:'Agent requests access to a resource'},
{t:'Approve/Deny',d:'Principal resolves pending approval'},
{t:'View Audit Trail',d:'Verify hash-chained audit log'}
];
async function loadGuided(){state.guidedStep=1;renderGuided()}
function renderGuided(){
const sc=document.getElementById('stepper');sc.innerHTML=STEPS.map((s,i)=>{const cls=i<state.guidedStep-1?'done':i===state.guidedStep-1?'active':'';return '<div class="step '+cls+'"><div class="step-num">'+(i<state.guidedStep-1?'\u2713':i+1)+'</div><div class="step-body"><strong>'+s.t+'</strong><span>'+s.d+'</span></div></div>'}).join('');
const step=state.guidedStep;const a=document.getElementById('guided-action');const r=document.getElementById('guided-result');r.className='result-box';
if(step===1){a.innerHTML='<div class="form-grid"><div class="form-field"><label>External ID</label><input id="g1-ext" value="guided-user"></div><div class="form-field"><label>Email</label><input id="g1-email" value="guided@example.com"></div><div class="form-field"><label>Display Name</label><input id="g1-name" value="Guided User"></div></div><div class="form-actions"><button class="btn btn-primary" onclick="guidedStep1()">Create Principal</button></div>'}
else if(step===2){a.innerHTML='<div class="form-grid"><div class="form-field"><label>Name</label><input id="g2-name" value="guided-agent"></div><div class="form-field"><label>Type</label><select id="g2-type"><option value="autonomous">autonomous</option></select></div><div class="form-field"><label>Owner</label><input id="g2-owner" value="'+state.guidedPrincipalId+'" readonly></div></div><div class="form-actions"><button class="btn btn-primary" onclick="guidedStep2()">Create Agent</button></div>'}
else if(step===3){a.innerHTML='<div class="form-grid"><div class="form-field"><label>Policy Name</label><input id="g3-name" value="guided-policy"></div></div><div class="form-field"><label>Definition</label><textarea id="g3-def">- name: allow-read\n  agent_types: ["autonomous"]\n  actions: ["read"]\n  resources: ["documents/*"]\n  decision: allow\n- name: require-approval-write\n  agent_types: ["autonomous"]\n  actions: ["write"]\n  resources: ["documents/*"]\n  decision: require_approval\n- name: deny-delete\n  agent_types: ["autonomous"]\n  actions: ["delete"]\n  resources: ["*"]\n  decision: deny</textarea></div><div class="form-actions"><button class="btn btn-primary" onclick="guidedStep3()">Create Policy</button></div>'}
else if(step===4){a.innerHTML='<div class="form-grid"><div class="form-field"><label>Agent</label><input id="g4-agent" value="'+state.guidedAgentId+'" readonly></div><div class="form-field"><label>Scopes</label><input id="g4-scopes" value="documents:read,documents:write"></div><div class="form-field"><label>TTL</label><input id="g4-ttl" type="number" value="900"></div></div><div class="form-actions"><button class="btn btn-primary" onclick="guidedStep4()">Delegate Scopes</button></div>'}
else if(step===5){a.innerHTML='<div class="form-grid"><div class="form-field"><label>Agent</label><input id="g5-agent" value="'+state.guidedAgentId+'" readonly></div><div class="form-field"><label>Action</label><select id="g5-action"><option value="read">read (allow)</option><option value="write">write (require_approval)</option><option value="delete">delete (deny)</option></select></div><div class="form-field"><label>Resource</label><input id="g5-resource" value="documents/report.pdf"></div><div class="form-field"><label>Scopes</label><input id="g5-scopes" value="documents:read"></div></div><div class="form-actions"><button class="btn btn-success" onclick="guidedStep5()">Request Access</button></div>'}
else if(step===6){a.innerHTML='<p style="color:var(--text2);margin-bottom:12px">If write action triggered require_approval, approve or deny it:</p><div class="form-grid"><div class="form-field"><label>Approval ID</label><input id="g6-id" value="'+(state.guidedApprovalId||'')+'"></div><div class="form-field"><label>Approver</label><input id="g6-approver" value="'+state.guidedPrincipalId+'" readonly></div></div><div class="form-actions"><button class="btn btn-success" onclick="guidedStep6(true)">Approve</button><button class="btn btn-danger" onclick="guidedStep6(false)">Deny</button></div>'}
else if(step===7){a.innerHTML='<p style="color:var(--green)">Flow complete! Check Audit Trail for the full hash-chained log.</p><div class="form-actions"><button class="btn btn-primary" onclick="showTab(\'audit\')">View Audit Trail</button><button class="btn" onclick="loadGuided()">Restart</button></div>'}
}
async function guidedStep1(){try{const r=await fPost('/v1/admin/principals',{external_id:g1_ext.value,idp_provider:'local',email:g1_email.value,display_name:g1_name.value});state.guidedPrincipalId=r.id;showResult('guided-result',r);toast('Principal created');state.guidedStep++;renderGuided()}catch(e){showResult('guided-result',e.message,false);toast(e.message,'error')}}
async function guidedStep2(){try{const r=await fPost('/v1/admin/agents',{name:g2_name.value,principal_type:g2_type.value,owner_id:state.guidedPrincipalId});state.guidedAgentId=r.id;showResult('guided-result',r);toast('Agent created');state.guidedStep++;renderGuided()}catch(e){showResult('guided-result',e.message,false);toast(e.message,'error')}}
async function guidedStep3(){try{const r=await fPost('/v1/admin/policies',{name:g3_name.value,engine:'yaml',definition:g3_def.value});showResult('guided-result',r);toast('Policy created');state.guidedStep++;renderGuided()}catch(e){showResult('guided-result',e.message,false);toast(e.message,'error')}}
async function guidedStep4(){try{const r=await fPost('/v1/principal/delegate',{agent_id:state.guidedAgentId,scopes:g4_scopes.value.split(','),expires_in_seconds:parseInt(g4_ttl.value)});showResult('guided-result',r);toast('Scopes delegated');state.guidedStep++;renderGuided()}catch(e){showResult('guided-result',e.message,false);toast(e.message,'error')}}
async function guidedStep5(){try{const action=g5_action.value;const scope=action==='write'?'documents:write':action==='delete'?'documents:delete':'documents:read';const r=await fPost('/v1/agent/request-access',{agent_id:state.guidedAgentId,action,resource:g5_resource.value,requested_scopes:[scope]});showResult('guided-result',r);if(r.decision==='require_approval'&&r.approval){state.guidedApprovalId=r.approval.request_id;toast('Approval required')}else{toast('Decision: '+r.decision)}state.guidedStep++;renderGuided()}catch(e){showResult('guided-result',e.message,false);toast(e.message,'error')}}
async function guidedStep6(approve){try{const id=g6_id.value||state.guidedApprovalId;if(!id){toast('No approval ID','error');return}const r=await fPost('/v1/principal/approvals/'+id+'/'+(approve?'approve':'deny'),{approver_id:state.guidedPrincipalId});showResult('guided-result',r);toast(approve?'Approved':'Denied');state.guidedStep++;renderGuided()}catch(e){showResult('guided-result',e.message,false);toast(e.message,'error')}}

// PRINCIPALS
async function loadPrincipals(){try{const g=await f('/v1/principal/grants');document.getElementById('pr-grants').innerHTML=g.grants?.map(x=>'<tr><td class="mono">'+short(x.id)+'</td><td class="mono">'+short(x.agent_id)+'</td><td>'+(x.scopes||[]).join(', ')+'</td><td class="mono truncate">'+short(x.token_jti||x.jti)+'</td><td>'+fmt(x.expires_at)+'</td><td><button class="btn btn-danger btn-sm" onclick="revokeGrant(\''+x.id+'\')">Revoke</button></td></tr>').join('')||'<tr><td colspan="6" class="empty">No grants</td></tr>'}catch(e){console.error(e)}}
async function createPrincipal(){try{const r=await fPost('/v1/admin/principals',{external_id:pr_ext.value,idp_provider:pr_idp.value,email:pr_email.value,display_name:pr_name.value});showResult('pr-result',r);toast('Principal created');pr_ext.value='';pr_email.value='';pr_name.value='';loadPrincipals();loadOverview()}catch(e){showResult('pr-result',e.message,false);toast(e.message,'error')}}
async function revokeGrant(id){try{await fPost('/v1/principal/grants/'+id+'/revoke',{});toast('Grant revoked');loadPrincipals();loadOverview()}catch(e){toast(e.message,'error')}}

// AGENTS
async function loadAgents(){try{const a=await f('/v1/admin/agents');document.getElementById('ag-list').innerHTML=a.map(x=>'<tr><td class="mono">'+short(x.id)+'</td><td>'+x.name+'</td><td>'+chip(x.principal_type)+'</td><td>'+chip(x.status)+'</td><td class="mono">'+short(x.owner_id)+'</td><td>'+fmt(x.created_at)+'</td><td><button class="btn btn-sm" onclick="viewAgent(\''+x.id+'\')">View</button> <button class="btn btn-danger btn-sm" onclick="killAgent(\''+x.id+'\')">Kill</button></td></tr>').join('')||'<tr><td colspan="7" class="empty">No agents</td></tr>'}catch(e){console.error(e)}}
async function createAgent(){try{const b={name:ag_name.value,principal_type:ag_type.value};if(ag_owner.value)b.owner_id=ag_owner.value;const r=await fPost('/v1/admin/agents',b);showResult('ag-result',r);toast('Agent created');ag_name.value='';ag_owner.value='';loadAgents();loadOverview()}catch(e){showResult('ag-result',e.message,false);toast(e.message,'error')}}
async function viewAgent(id){try{const a=await f('/v1/admin/agents/'+id);openModal('Agent Details','<table style="width:100%"><tr><td style="color:var(--text3)">ID</td><td class="mono">'+a.id+'</td></tr><tr><td style="color:var(--text3)">Name</td><td>'+a.name+'</td></tr><tr><td style="color:var(--text3)">Type</td><td>'+chip(a.principal_type)+'</td></tr><tr><td style="color:var(--text3)">Status</td><td>'+chip(a.status)+'</td></tr><tr><td style="color:var(--text3)">Owner</td><td class="mono">'+(a.owner_id||'\u2014')+'</td></tr><tr><td style="color:var(--text3)">Created</td><td>'+fmt(a.created_at)+'</td></tr></table>')}catch(e){toast(e.message,'error')}}
async function killAgent(id){if(!confirm('Kill all sessions and revoke tokens?'))return;try{const r=await fPost('/v1/admin/agents/'+id+'/kill',{});showResult('ag-result',r);toast('Agent killed');loadAgents();loadOverview()}catch(e){showResult('ag-result',e.message,false);toast(e.message,'error')}}
async function recordSpend(){try{const b={amount:parseFloat(sp_amount.value)};if(sp_session.value)b.session_id=sp_session.value;const r=await fPost('/v1/admin/agents/'+sp_agent.value+'/spend',b);showResult('sp-result',r);toast('Spend recorded');loadSessions()}catch(e){showResult('sp-result',e.message,false);toast(e.message,'error')}}

// RESOURCES
async function loadResources(){try{const r=await f('/v1/admin/resources');document.getElementById('rs-list').innerHTML=r.map(x=>'<tr><td class="mono">'+short(x.id)+'</td><td>'+x.name+'</td><td>'+chip(x.resource_type)+'</td><td class="truncate">'+x.uri+'</td><td>'+chip(x.sensitivity)+'</td><td>'+(x.actions?JSON.stringify(x.actions):'\u2014')+'</td></tr>').join('')||'<tr><td colspan="6" class="empty">No resources</td></tr>'}catch(e){console.error(e)}}
async function createResource(){try{const actions=JSON.parse(rs_actions.value||'[]');const r=await fPost('/v1/admin/resources',{name:rs_name.value,resource_type:rs_type.value,uri:rs_uri.value,actions,sensitivity:rs_sens.value});showResult('rs-result',r);toast('Resource created');rs_name.value='';rs_uri.value='';loadResources();loadOverview()}catch(e){showResult('rs-result',e.message,false);toast(e.message,'error')}}

// POLICIES
let _policies=[];
async function loadPolicies(){try{const p=await f('/v1/admin/policies');_policies=p.policies||[];document.getElementById('po-list').innerHTML=_policies.map(x=>'<tr><td class="mono">'+x.id+'</td><td>'+x.name+'</td><td>'+x.engine+'</td><td>'+chip(x.status)+'</td><td><button class="btn btn-sm" onclick="viewPolicy('+x.id+')">View YAML</button></td></tr>').join('')||'<tr><td colspan="5" class="empty">No policies</td></tr>'}catch(e){console.error(e)}}
function viewPolicy(id){const p=_policies.find(x=>x.id===id);if(p)openModal('Policy: '+p.name,'<div class="code-block">'+p.definition+'</div>')}
async function createPolicy(){try{const r=await fPost('/v1/admin/policies',{name:po_name.value,engine:po_engine.value,definition:po_def.value});showResult('po-result',r);toast('Policy created');po_name.value='';loadPolicies();loadOverview()}catch(e){showResult('po-result',e.message,false);toast(e.message,'error')}}

// DELEGATION
async function principalDelegate(){try{const b={agent_id:dl_agent.value,scopes:dl_scopes.value.split(',').map(s=>s.trim()).filter(Boolean),expires_in_seconds:parseInt(dl_ttl.value)};if(dl_constraints.value)b.constraints=JSON.parse(dl_constraints.value);const r=await fPost('/v1/principal/delegate',b);showResult('dl-result',r);toast('Scopes delegated');loadPrincipals();loadOverview()}catch(e){showResult('dl-result',e.message,false);toast(e.message,'error')}}
async function agentDelegate(){try{const r=await fPost('/v1/agent/delegate',{parent_grant_token:sd_token.value,sub_agent_id:sd_agent.value,scopes:sd_scopes.value.split(',').map(s=>s.trim()).filter(Boolean),expires_in_seconds:parseInt(sd_ttl.value)});showResult('sd-result',r);toast('Token delegated')}catch(e){showResult('sd-result',e.message,false);toast(e.message,'error')}}
async function revokeToken(){try{const r=await fPost('/v1/admin/tokens/'+encodeURIComponent(rv_jti.value)+'/revoke',{});showResult('rv-result',r);toast('Token revoked')}catch(e){showResult('rv-result',e.message,false);toast(e.message,'error')}}

// ACCESS
async function requestAccess(){try{const b={agent_id:ac_agent.value,action:ac_action.value,resource:ac_resource.value,requested_scopes:ac_scopes.value.split(',').map(s=>s.trim()).filter(Boolean)};if(ac_dtoken.value)b.delegation_token=ac_dtoken.value;if(ac_session.value)b.context={session_id:ac_session.value};const r=await fPost('/v1/agent/request-access',b);showResult('ac-result',r);toast('Decision: '+r.decision);loadOverview();loadAudit()}catch(e){showResult('ac-result',e.message,false);toast(e.message,'error')}}
async function checkAccess(){try{const b={agent_id:ac_agent.value,action:ac_action.value,resource:ac_resource.value,requested_scopes:ac_scopes.value.split(',').map(s=>s.trim()).filter(Boolean)};if(ac_dtoken.value)b.delegation_token=ac_dtoken.value;if(ac_session.value)b.context={session_id:ac_session.value};const r=await fPost('/v1/agent/check',b);showResult('ac-result',r);toast('Decision: '+r.decision)}catch(e){showResult('ac-result',e.message,false);toast(e.message,'error')}}
async function checkApprovalStatus(){try{const r=await f('/v1/agent/approval-status/'+ap_id_check.value);showResult('ap-check-result',r);toast('Status: '+r.status)}catch(e){showResult('ap-check-result',e.message,false);toast(e.message,'error')}}

// APPROVALS
async function loadApprovals(){try{const a=await f('/v1/principal/approvals');document.getElementById('ap-list').innerHTML=a.map(x=>'<tr><td class="mono">'+short(x.id)+'</td><td class="mono">'+short(x.agent_id)+'</td><td>'+x.action+'</td><td class="truncate">'+x.resource+'</td><td>'+(x.requested_scopes||[]).join(', ')+'</td><td>'+chip(x.status)+'</td><td>'+fmt(x.expires_at)+'</td><td><button class="btn btn-success btn-sm" onclick="approveReq(\''+x.id+'\')">Approve</button> <button class="btn btn-danger btn-sm" onclick="denyReq(\''+x.id+'\')">Deny</button></td></tr>').join('')||'<tr><td colspan="8" class="empty">No pending approvals</td></tr>'}catch(e){console.error(e)}}
async function approveReq(id){try{await fPost('/v1/principal/approvals/'+id+'/approve',{approver_id:state.guidedPrincipalId||prompt('Approver ID:')});toast('Approved');loadApprovals();loadOverview()}catch(e){toast(e.message,'error')}}
async function denyReq(id){try{await fPost('/v1/principal/approvals/'+id+'/deny',{approver_id:state.guidedPrincipalId||prompt('Approver ID:')});toast('Denied');loadApprovals();loadOverview()}catch(e){toast(e.message,'error')}}

// SESSIONS
async function loadSessions(){try{const s=await f('/v1/sessions');const k=s.sessions.filter(x=>x.killed).length;document.getElementById('ss-total').textContent=s.sessions.length;document.getElementById('ss-killed').textContent=k;document.getElementById('ss-active').textContent=s.sessions.length-k;document.getElementById('ss-list').innerHTML=s.sessions.map(x=>'<tr><td class="mono truncate">'+x.session_id+'</td><td class="mono">'+short(x.agent_id)+'</td><td>'+x.actions_count+'</td><td>$'+x.spend_total.toFixed(4)+'</td><td>'+x.tokens_used+'</td><td>'+(x.trust_level*100).toFixed(0)+'%</td><td>'+(x.killed?chip('killed'):chip('active'))+'</td><td>'+(x.killed?'\u2014':'<button class="btn btn-danger btn-sm" onclick="killSession(\''+encodeURIComponent(x.session_id)+'\')">Kill</button>')+'</td></tr>').join('')||'<tr><td colspan="8" class="empty">No sessions</td></tr>'}catch(e){console.error(e)}}
async function killSession(id){try{await fPost('/v1/sessions/'+id+'/kill',{});toast('Session killed');loadSessions();loadOverview()}catch(e){toast(e.message,'error')}}

// AUDIT
async function loadAudit(){try{const a=await f('/v1/admin/audit');document.getElementById('au-list').innerHTML=a.map(x=>'<tr><td class="mono">'+x.id+'</td><td>'+fmt(x.timestamp)+'</td><td class="mono">'+short(x.agent_id)+'</td><td>'+x.action+'</td><td class="truncate">'+x.resource+'</td><td>'+chip(x.decision)+'</td><td class="truncate">'+(x.reason||'\u2014')+'</td><td class="mono truncate">'+short(x.row_hash,16)+'</td><td class="mono truncate">'+short(x.prev_hash,16)+'</td></tr>').join('')||'<tr><td colspan="9" class="empty">No audit entries</td></tr>'}catch(e){console.error(e)}}

// VAULT
async function loadVault(){try{const idp=await f('/v1/idp/providers');let h='<p style="margin-bottom:12px">Federation: '+(idp.enabled?'<span style="color:var(--green)">Enabled</span>':'<span style="color:var(--red)">Disabled</span>')+'</p>';if(idp.providers?.length){h+='<table><thead><tr><th>Provider</th><th>Issuer</th><th>Client ID</th><th>Scopes</th></tr></thead><tbody>';idp.providers.forEach(p=>{h+='<tr><td>'+p.name+'</td><td class="truncate">'+p.issuer+'</td><td class="mono">'+short(p.client_id,16)+'</td><td>'+(p.scopes||[]).join(', ')+'</td></tr>'});h+='</tbody></table>'}else{h+='<p class="empty">No IdP providers configured</p>'}document.getElementById('idp-info').innerHTML=h}catch(e){console.error(e)}}
async function generateVaultKey(){try{const r=await fPost('/v1/vault/generate-key',{});showResult('vk-result',r);toast('Vault key generated')}catch(e){showResult('vk-result',e.message,false);toast(e.message,'error')}}
async function storeCredential(){try{const r=await fPost('/v1/vault/credentials',{principal_id:va_pid.value,provider:va_prov.value,refresh_token:va_token.value,scopes:va_scopes.value.split(',').map(s=>s.trim()).filter(Boolean)});showResult('va-result',r);toast('Credential stored');va_pid.value='';va_prov.value='';va_token.value='';va_scopes.value=''}catch(e){showResult('va-result',e.message,false);toast(e.message,'error')}}

function loadTab(t){if(t==='overview')loadOverview();else if(t==='guided')loadGuided();else if(t==='principals')loadPrincipals();else if(t==='agents')loadAgents();else if(t==='resources')loadResources();else if(t==='policies')loadPolicies();else if(t==='approvals')loadApprovals();else if(t==='sessions')loadSessions();else if(t==='audit')loadAudit();else if(t==='vault')loadVault();else if(t==='idp')loadVault()}

loadOverview();loadPolicies();
setInterval(()=>{const a=document.querySelector('.tab-content.active');if(a)loadTab(a.id)},5000);
</script>
</body>
</html>"##.to_string()
}
