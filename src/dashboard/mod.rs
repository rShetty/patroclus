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
.header .health.err{color:#f44336}
.header .health.err::before{background:#f44336}
.header a.logout{margin-left:12px;color:#666;font-size:12px;text-decoration:none}
.layout{display:flex;min-height:calc(100vh - 52px)}
.sidebar{width:220px;background:#141620;border-right:1px solid #2a2d3a;padding:16px 0;position:fixed;top:52px;bottom:0;left:0;overflow-y:auto;z-index:50}
.sidebar .nav-group{margin-bottom:8px}
.sidebar .nav-group-title{font-size:10px;color:#555;text-transform:uppercase;letter-spacing:1px;padding:8px 16px;font-weight:600}
.sidebar button{display:block;width:100%;text-align:left;background:transparent;color:#999;border:none;padding:10px 16px;cursor:pointer;font-size:13px;transition:all .1s;border-left:3px solid transparent}
.sidebar button:hover{background:#1a1d29;color:#e0e0e0}
.sidebar button.active{background:#1a1d29;color:#6c8aff;border-left-color:#6c8aff;font-weight:600}
.main{margin-left:220px;padding:20px;flex:1;max-width:calc(100% - 220px)}
.tab-content{display:none}
.tab-content.active{display:block}
.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(200px,1fr));gap:14px;margin-bottom:20px}
.card{background:#1a1d29;border-radius:8px;padding:18px;border:1px solid #2a2d3a}
.card h3{font-size:11px;color:#666;text-transform:uppercase;letter-spacing:1px;margin-bottom:8px}
.stat{font-size:28px;font-weight:700;color:#6c8aff}
.stat.green{color:#4caf50}.stat.red{color:#f44336}.stat.orange{color:#ff9800}
.stat-label{font-size:11px;color:#555;margin-top:4px}
.panel{background:#1a1d29;border-radius:8px;border:1px solid #2a2d3a;overflow:hidden;margin-bottom:20px}
.panel-header{padding:12px 18px;border-bottom:1px solid #2a2d3a;display:flex;align-items:center;justify-content:space-between}
.panel-header h2{font-size:14px;color:#e0e0e0}
.panel-body{padding:0}
.panel-body.padded{padding:16px}
table{width:100%;border-collapse:collapse}
th{background:#141620;padding:9px 14px;text-align:left;font-size:11px;color:#666;text-transform:uppercase;letter-spacing:.5px;font-weight:600}
td{padding:9px 14px;border-top:1px solid #2a2d3a;font-size:13px}
tr:hover{background:#1e2130}
.badge-tag{display:inline-block;padding:2px 8px;border-radius:4px;font-size:11px;font-weight:600}
.badge-active{background:#1b3a1b;color:#4caf50}.badge-suspended{background:#3a2a1b;color:#ff9800}.badge-decommissioned{background:#3a1b1b;color:#f44336}
.badge-pending{background:#3a2a1b;color:#ff9800}.badge-approved{background:#1b3a1b;color:#4caf50}.badge-denied{background:#3a1b1b;color:#f44336}
.badge-allow{background:#1b3a1b;color:#4caf50}.badge-deny{background:#3a1b1b;color:#f44336}.badge-require_approval{background:#3a2a1b;color:#ff9800}
.badge-killed{background:#3a1b1b;color:#f44336}
.badge-service{background:#1b2a3a;color:#6c8aff}.badge-autonomous{background:#3a1b3a;color:#c66aff}.badge-delegated{background:#3a3a1b;color:#ffc107}
.badge-low{background:#1b3a1b;color:#4caf50}.badge-medium{background:#3a3a1b;color:#ffc107}.badge-high{background:#3a2a1b;color:#ff9800}.badge-critical{background:#3a1b1b;color:#f44336}
.badge-mcp_server{background:#1b2a3a;color:#6c8aff}.badge-api{background:#1b3a3a;color:#4caf50}.badge-database{background:#3a3a1b;color:#ffc107}.badge-cloud_service{background:#3a1b3a;color:#c66aff}
.btn{padding:6px 14px;border-radius:5px;border:none;cursor:pointer;font-size:12px;font-weight:600;transition:all .15s}
.btn-green{background:#2e7d32;color:#fff}.btn-green:hover{background:#388e3c}
.btn-red{background:#c62828;color:#fff}.btn-red:hover{background:#d32f2f}
.btn-blue{background:#3949ab;color:#fff}.btn-blue:hover{background:#3f51b5}
.btn-sm{padding:4px 10px;font-size:11px}
.form-row{display:flex;gap:12px;margin-bottom:12px;flex-wrap:wrap}
.form-group{flex:1;min-width:180px}
.form-group label{display:block;font-size:11px;color:#888;margin-bottom:4px;text-transform:uppercase;letter-spacing:.5px}
.form-group input,.form-group select,.form-group textarea{width:100%;padding:8px 10px;background:#0f1117;border:1px solid #2a2d3a;border-radius:5px;color:#e0e0e0;font-size:13px}
.form-group textarea{min-height:100px;font-family:'SF Mono',Monaco,monospace;font-size:12px}
.form-actions{display:flex;gap:8px;margin-top:8px}
.mono{font-family:'SF Mono',Monaco,monospace;font-size:12px}
.truncate{max-width:200px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.toast{position:fixed;bottom:20px;right:20px;padding:14px 20px;border-radius:8px;font-size:13px;z-index:200;animation:slideIn .3s ease}
.toast-success{background:#2e7d32;color:#fff}.toast-error{background:#c62828;color:#fff}
@keyframes slideIn{from{transform:translateX(100%)}to{transform:translateX(0)}}
.empty{text-align:center;padding:40px;color:#555;font-style:italic}
.code-block{background:#0f1117;border:1px solid #2a2d3a;border-radius:5px;padding:12px;font-family:'SF Mono',Monaco,monospace;font-size:12px;overflow-x:auto;white-space:pre-wrap;word-break:break-all;margin-top:8px}
.result-box{background:#0f1117;border:1px solid #2a2d3a;border-radius:5px;padding:14px;margin-top:12px;font-family:'SF Mono',Monaco,monospace;font-size:12px;overflow-x:auto;white-space:pre-wrap;word-break:break-all;display:none}
.result-box.show{display:block}
.result-success{border-color:#4caf50}.result-error{border-color:#f44336}
.modal-overlay{position:fixed;top:0;left:0;right:0;bottom:0;background:rgba(0,0,0,.7);z-index:300;display:none;align-items:center;justify-content:center}
.modal-overlay.show{display:flex}
.modal{background:#1a1d29;border-radius:12px;border:1px solid #2a2d3a;padding:24px;width:700px;max-width:90vw;max-height:80vh;overflow-y:auto}
.modal h2{font-size:18px;color:#6c8aff;margin-bottom:16px}
.modal-close{float:right;cursor:pointer;color:#666;font-size:18px}
.step{display:flex;align-items:center;gap:8px;padding:8px 0}
.step-num{width:24px;height:24px;background:#6c8aff;color:#fff;border-radius:50%;display:flex;align-items:center;justify-content:center;font-size:12px;font-weight:700;flex-shrink:0}
.step-done .step-num{background:#4caf50}
.step-pending .step-num{background:#2a2d3a;color:#666}
.guided-flow{background:#1a1d29;border:1px solid #2a2d3a;border-radius:8px;padding:20px;margin-bottom:20px}
.guided-flow h2{font-size:16px;color:#6c8aff;margin-bottom:16px}
select option{background:#1a1d29}
</style>
</head>
<body>
<div class="header">
<h1>PATROCLUS</h1>
<span class="badge">Authorization Infrastructure</span>
<div class="health" id="health-status">Healthy</div>
<a class="logout" href="/logout">Logout</a>
</div>
<div class="layout">
<div class="sidebar">
<div class="nav-group">
<div class="nav-group-title">Dashboard</div>
<button class="active" onclick="showTab('overview')">Overview</button>
<button onclick="showTab('guided')">Guided Flow</button>
</div>
<div class="nav-group">
<div class="nav-group-title">Identity</div>
<button onclick="showTab('principals')">Principals</button>
<button onclick="showTab('agents')">Agents</button>
<button onclick="showTab('resources')">Resources</button>
<button onclick="showTab('idp')">IdP Federation</button>
</div>
<div class="nav-group">
<div class="nav-group-title">Authorization</div>
<button onclick="showTab('policies')">Policies</button>
<button onclick="showTab('delegation')">Delegation</button>
<button onclick="showTab('access')">Test Access</button>
<button onclick="showTab('approvals')">Approvals</button>
</div>
<div class="nav-group">
<div class="nav-group-title">Monitoring</div>
<button onclick="showTab('sessions')">Sessions</button>
<button onclick="showTab('audit')">Audit Trail</button>
</div>
<div class="nav-group">
<div class="nav-group-title">Security</div>
<button onclick="showTab('vault')">Vault</button>
</div>
</div>
<div class="main">

<div id="overview" class="tab-content active">
<div class="grid">
<div class="card"><h3>Agents</h3><div class="stat" id="ov-agents">—</div><div class="stat-label">Registered</div></div>
<div class="card"><h3>Principals</h3><div class="stat" id="ov-principals">—</div><div class="stat-label">Registered</div></div>
<div class="card"><h3>Resources</h3><div class="stat" id="ov-resources">—</div><div class="stat-label">Registered</div></div>
<div class="card"><h3>Active Sessions</h3><div class="stat green" id="ov-sessions">—</div><div class="stat-label">In-memory</div></div>
<div class="card"><h3>Pending Approvals</h3><div class="stat orange" id="ov-pending">—</div><div class="stat-label">Awaiting decision</div></div>
<div class="card"><h3>Active Grants</h3><div class="stat" id="ov-grants">—</div><div class="stat-label">Issued tokens</div></div>
<div class="card"><h3>Policies</h3><div class="stat" id="ov-policies">—</div><div class="stat-label">Active</div></div>
<div class="card"><h3>Audit Entries</h3><div class="stat" id="ov-audit">—</div><div class="stat-label">Hash-chained</div></div>
</div>
<div class="panel">
<div class="panel-header"><h2>Recent Authorization Decisions</h2></div>
<div class="panel-body">
<table><thead><tr><th>Time</th><th>Agent</th><th>Action</th><th>Resource</th><th>Decision</th><th>Reason</th><th>Token JTI</th></tr></thead>
<tbody id="ov-recent"></tbody></table>
</div>
</div>
</div>

<div id="guided" class="tab-content">
<div class="guided-flow">
<h2>End-to-End Guided Flow</h2>
<p style="color:#888;font-size:13px;margin-bottom:16px">Walk through the full authorization lifecycle: create principal → create agent → create policy → delegate scopes → request access → approve/deny → view audit trail</p>
<div id="guided-steps">
<div class="step step-pending" id="step-1"><div class="step-num">1</div><div><strong>Create Principal</strong> — Register a user who owns agents</div></div>
<div class="step step-pending" id="step-2"><div class="step-num">2</div><div><strong>Create Agent</strong> — Register an AI agent owned by the principal</div></div>
<div class="step step-pending" id="step-3"><div class="step-num">3</div><div><strong>Create Policy</strong> — Define authorization rules (allow/deny/require_approval)</div></div>
<div class="step step-pending" id="step-4"><div class="step-num">4</div><div><strong>Delegate Scopes</strong> — Principal grants scopes to the agent</div></div>
<div class="step step-pending" id="step-5"><div class="step-num">5</div><div><strong>Request Access</strong> — Agent requests access to a resource</div></div>
<div class="step step-pending" id="step-6"><div class="step-num">6</div><div><strong>Approve/Deny</strong> — Principal resolves pending approval (if triggered)</div></div>
<div class="step step-pending" id="step-7"><div class="step-num">7</div><div><strong>View Audit Trail</strong> — Verify hash-chained audit log</div></div>
</div>
<div id="guided-action" style="margin-top:16px"></div>
<div class="result-box" id="guided-result"></div>
</div>
</div>

<div id="principals" class="tab-content">
<div class="panel">
<div class="panel-header"><h2>Register New Principal</h2></div>
<div class="panel-body padded">
<div class="form-row">
<div class="form-group"><label>External ID</label><input id="pr-ext" placeholder="user-123"></div>
<div class="form-group"><label>IdP Provider</label><input id="pr-idp" value="local"></div>
<div class="form-group"><label>Email</label><input id="pr-email" placeholder="user@example.com"></div>
<div class="form-group"><label>Display Name</label><input id="pr-name" placeholder="User Name"></div>
</div>
<div class="form-actions"><button class="btn btn-blue" onclick="createPrincipal()">Create Principal</button></div>
<div class="result-box" id="pr-result"></div>
</div>
</div>
<div class="panel">
<div class="panel-header"><h2>Principals & Active Grants</h2></div>
<div class="panel-body">
<div style="padding:12px 18px"><h3 style="font-size:12px;color:#888;text-transform:uppercase;margin-bottom:8px">All Grants</h3></div>
<table><thead><tr><th>Grant ID</th><th>Agent</th><th>Scopes</th><th>Token JTI</th><th>Expires</th><th>Actions</th></tr></thead>
<tbody id="pr-grants"></tbody></table>
</div>
</div>
</div>

<div id="agents" class="tab-content">
<div class="panel">
<div class="panel-header"><h2>Register New Agent</h2></div>
<div class="panel-body padded">
<div class="form-row">
<div class="form-group"><label>Name</label><input id="ag-name" placeholder="my-agent"></div>
<div class="form-group"><label>Type</label><select id="ag-type"><option value="autonomous">autonomous</option><option value="service">service</option><option value="delegated">delegated</option></select></div>
<div class="form-group"><label>Owner ID (Principal UUID)</label><input id="ag-owner" placeholder="optional"></div>
</div>
<div class="form-actions"><button class="btn btn-blue" onclick="createAgent()">Create Agent</button></div>
<div class="result-box" id="ag-result"></div>
</div>
</div>
<div class="panel">
<div class="panel-header"><h2>Agents</h2></div>
<div class="panel-body">
<table><thead><tr><th>ID</th><th>Name</th><th>Type</th><th>Status</th><th>Owner</th><th>Created</th><th>Actions</th></tr></thead>
<tbody id="ag-list"></tbody></table>
</div>
</div>
<div class="panel">
<div class="panel-header"><h2>Record Spend</h2></div>
<div class="panel-body padded">
<div class="form-row">
<div class="form-group"><label>Agent ID</label><input id="sp-agent" placeholder="UUID"></div>
<div class="form-group"><label>Amount ($)</label><input id="sp-amount" type="number" step="0.0001" placeholder="0.002"></div>
<div class="form-group"><label>Session ID (optional)</label><input id="sp-session" placeholder="optional"></div>
</div>
<div class="form-actions"><button class="btn btn-blue" onclick="recordSpend()">Record Spend</button></div>
<div class="result-box" id="sp-result"></div>
</div>
</div>
</div>

<div id="resources" class="tab-content">
<div class="panel">
<div class="panel-header"><h2>Register New Resource</h2></div>
<div class="panel-body padded">
<div class="form-row">
<div class="form-group"><label>Name</label><input id="rs-name" placeholder="my-api"></div>
<div class="form-group"><label>Type</label><select id="rs-type"><option value="api">api</option><option value="mcp_server">mcp_server</option><option value="database">database</option><option value="cloud_service">cloud_service</option></select></div>
<div class="form-group"><label>URI</label><input id="rs-uri" placeholder="https://api.example.com"></div>
<div class="form-group"><label>Sensitivity</label><select id="rs-sens"><option value="low">low</option><option value="medium">medium</option><option value="high">high</option><option value="critical">critical</option></select></div>
</div>
<div class="form-group"><label>Actions (JSON array)</label><input id="rs-actions" placeholder='["read","write","delete"]' value='["read","write"]'></div>
<div class="form-actions"><button class="btn btn-blue" onclick="createResource()">Create Resource</button></div>
<div class="result-box" id="rs-result"></div>
</div>
</div>
<div class="panel">
<div class="panel-header"><h2>Resources</h2></div>
<div class="panel-body">
<table><thead><tr><th>ID</th><th>Name</th><th>Type</th><th>URI</th><th>Sensitivity</th><th>Actions</th></tr></thead>
<tbody id="rs-list"></tbody></table>
</div>
</div>
</div>

<div id="policies" class="tab-content">
<div class="panel">
<div class="panel-header"><h2>Create Policy</h2></div>
<div class="panel-body padded">
<div class="form-row">
<div class="form-group"><label>Policy Name</label><input id="po-name" placeholder="my-policy"></div>
<div class="form-group"><label>Engine</label><select id="po-engine"><option value="yaml">yaml</option></select></div>
</div>
<div class="form-group"><label>Definition (YAML rules array)</label><textarea id="po-def">- name: allow-read
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
  decision: require_approval
</textarea></div>
<div class="form-actions"><button class="btn btn-blue" onclick="createPolicy()">Create Policy</button></div>
<div class="result-box" id="po-result"></div>
</div>
</div>
<div class="panel">
<div class="panel-header"><h2>Policies</h2></div>
<div class="panel-body">
<table><thead><tr><th>ID</th><th>Name</th><th>Engine</th><th>Status</th><th>Actions</th></tr></thead>
<tbody id="po-list"></tbody></table>
</div>
</div>
</div>
</div>

<div id="delegation" class="tab-content">
<div class="panel">
<div class="panel-header"><h2>Principal → Agent Delegation</h2><span style="font-size:12px;color:#666">Principal grants scopes to an agent</span></div>
<div class="panel-body padded">
<div class="form-row">
<div class="form-group"><label>Agent ID</label><input id="dl-agent" placeholder="UUID"></div>
<div class="form-group"><label>Scopes (comma-separated)</label><input id="dl-scopes" placeholder="documents:read,documents:write"></div>
<div class="form-group"><label>TTL (seconds)</label><input id="dl-ttl" type="number" value="900"></div>
</div>
<div class="form-group"><label>Constraints (JSON, optional)</label><input id="dl-constraints" placeholder='{"max_amount": 10.0}'></div>
<div class="form-actions"><button class="btn btn-blue" onclick="principalDelegate()">Delegate Scopes</button></div>
<div class="result-box" id="dl-result"></div>
</div>
</div>
<div class="panel">
<div class="panel-header"><h2>Agent → Agent Delegation</h2><span style="font-size:12px;color:#666">Sub-agent delegates from parent grant token</span></div>
<div class="panel-body padded">
<div class="form-row">
<div class="form-group"><label>Parent Grant Token (JWT)</label><input id="sd-token" placeholder="eyJ0eXAi..."></div>
<div class="form-group"><label>Sub-Agent ID</label><input id="sd-agent" placeholder="UUID"></div>
<div class="form-group"><label>Scopes (comma-separated)</label><input id="sd-scopes" placeholder="documents:read"></div>
<div class="form-group"><label>TTL (seconds)</label><input id="sd-ttl" type="number" value="900"></div>
</div>
<div class="form-actions"><button class="btn btn-blue" onclick="agentDelegate()">Delegate Token</button></div>
<div class="result-box" id="sd-result"></div>
</div>
</div>
<div class="panel">
<div class="panel-header"><h2>Revoke Token by JTI</h2></div>
<div class="panel-body padded">
<div class="form-row">
<div class="form-group"><label>Token JTI</label><input id="rv-jti" placeholder="01a00042-..."></div>
</div>
<div class="form-actions"><button class="btn btn-red" onclick="revokeToken()">Revoke Token</button></div>
<div class="result-box" id="rv-result"></div>
</div>
</div>
</div>

<div id="access" class="tab-content">
<div class="panel">
<div class="panel-header"><h2>Request Access</h2><span style="font-size:12px;color:#666">Full authorization request — issues token or triggers approval</span></div>
<div class="panel-body padded">
<div class="form-row">
<div class="form-group"><label>Agent ID</label><input id="ac-agent" placeholder="UUID"></div>
<div class="form-group"><label>Action</label><input id="ac-action" placeholder="read"></div>
<div class="form-group"><label>Resource</label><input id="ac-resource" placeholder="documents/report.pdf"></div>
<div class="form-group"><label>Scopes (comma-separated)</label><input id="ac-scopes" placeholder="documents:read"></div>
</div>
<div class="form-row">
<div class="form-group"><label>Delegation Token (optional)</label><input id="ac-dtoken" placeholder="JWT from parent grant"></div>
<div class="form-group"><label>Session ID (optional)</label><input id="ac-session" placeholder="custom-session"></div>
</div>
<div class="form-actions">
<button class="btn btn-green" onclick="requestAccess()">Request Access</button>
<button class="btn btn-blue" onclick="checkAccess()">Check Access (dry-run)</button>
</div>
<div class="result-box" id="ac-result"></div>
</div>
</div>
<div class="panel">
<div class="panel-header"><h2>Check Approval Status</h2></div>
<div class="panel-body padded">
<div class="form-row">
<div class="form-group"><label>Approval Request ID</label><input id="ap-id-check" placeholder="UUID"></div>
</div>
<div class="form-actions"><button class="btn btn-blue" onclick="checkApprovalStatus()">Check Status</button></div>
<div class="result-box" id="ap-check-result"></div>
</div>
</div>
</div>

<div id="approvals" class="tab-content">
<div class="panel">
<div class="panel-header"><h2>Pending Approvals</h2></div>
<div class="panel-body">
<table><thead><tr><th>ID</th><th>Agent</th><th>Action</th><th>Resource</th><th>Scopes</th><th>Status</th><th>Expires</th><th>Actions</th></tr></thead>
<tbody id="ap-list"></tbody></table>
</div>
</div>
</div>

<div id="sessions" class="tab-content">
<div class="grid">
<div class="card"><h3>Total</h3><div class="stat" id="ss-total">—</div></div>
<div class="card"><h3>Active</h3><div class="stat green" id="ss-active">—</div></div>
<div class="card"><h3>Killed</h3><div class="stat red" id="ss-killed">—</div></div>
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
<table><thead><tr><th>#</th><th>Time</th><th>Agent</th><th>Action</th><th>Resource</th><th>Decision</th><th>Reason</th><th>Row Hash</th><th>Prev Hash</th></tr></thead>
<tbody id="au-list"></tbody></table>
</div>
</div>
</div>

<div id="vault" class="tab-content">
<div class="panel">
<div class="panel-header"><h2>Vault Key Management</h2></div>
<div class="panel-body padded">
<div class="form-actions"><button class="btn btn-blue" onclick="generateVaultKey()">Generate New Vault Key</button></div>
<div class="result-box" id="vk-result"></div>
</div>
</div>
<div class="panel">
<div class="panel-header"><h2>Store Credential</h2></div>
<div class="panel-body padded">
<div class="form-row">
<div class="form-group"><label>Principal ID</label><input id="va-pid" placeholder="UUID"></div>
<div class="form-group"><label>Provider</label><input id="va-prov" placeholder="github"></div>
<div class="form-group"><label>Refresh Token</label><input id="va-token" placeholder="token"></div>
</div>
<div class="form-row">
<div class="form-group"><label>Scopes (comma-separated)</label><input id="va-scopes" placeholder="repo,read:user"></div>
</div>
<div class="form-actions"><button class="btn btn-blue" onclick="storeCredential()">Store</button></div>
<div class="result-box" id="va-result"></div>
</div>
</div>
</div>

<div id="idp" class="tab-content">
<div class="panel">
<div class="panel-header"><h2>Identity Provider Federation</h2></div>
<div class="panel-body padded" id="idp-info"><p style="color:#666">Loading...</p></div>
</div>
</div>
</div>

<div id="toast"></div>

<div class="modal-overlay" id="modal-overlay" onclick="if(event.target===this)closeModal()">
<div class="modal">
<span class="modal-close" onclick="closeModal()">&times;</span>
<div id="modal-content"></div>
</div>
</div>

<script>
const API='';
const state={guidedStep:1,guidedPrincipalId:null,guidedAgentId:null,guidedPolicyId:null,guidedGrantToken:null,guidedApprovalId:null};

function showTab(t){document.querySelectorAll('.tab-content').forEach(e=>e.classList.remove('active'));document.getElementById(t).classList.add('active');document.querySelectorAll('.sidebar button').forEach(b=>b.classList.remove('active'));const btn=document.querySelector('.sidebar button[onclick*="'+t+'"]');if(btn)btn.classList.add('active');loadTab(t)}
async function f(url,opts){const r=await fetch(API+url,opts);const t=await r.text();if(!r.ok)throw new Error(t);try{return r.json()}catch{return t}}
async function fPost(url,body){return f(url,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)})}
function toast(msg,type='success'){const d=document.getElementById('toast');d.className='toast toast-'+type;d.textContent=msg;setTimeout(()=>d.className='',3000)}
function showResult(id,data,ok=true){const el=document.getElementById(id);el.className='result-box show '+(ok?'result-success':'result-error');el.textContent=typeof data==='string'?data:JSON.stringify(data,null,2)}
function fmt(ts){if(!ts)return '—';return new Date(ts).toLocaleString()}
function short(s,n=12){if(!s)return '—';return s.length>n?s.substring(0,n)+'…':s}
function badge(val){return '<span class="badge-tag badge-'+val+'">'+val+'</span>'}
function copyToClipboard(text){navigator.clipboard.writeText(text).then(()=>toast('Copied to clipboard'))}
function openModal(html){document.getElementById('modal-content').innerHTML=html;document.getElementById('modal-overlay').classList.add('show')}
function closeModal(){document.getElementById('modal-overlay').classList.remove('show')}

async function loadOverview(){
try{
const[agents,sessions,approvals,audit,policies,resources,grants]=await Promise.all([
f('/v1/admin/agents'),f('/v1/sessions'),f('/v1/principal/approvals'),f('/v1/admin/audit'),f('/v1/admin/policies'),f('/v1/admin/resources'),f('/v1/principal/grants')
]);
document.getElementById('ov-agents').textContent=agents.length;
document.getElementById('ov-principals').textContent='—';
document.getElementById('ov-resources').textContent=resources.length;
const killed=sessions.sessions.filter(s=>s.killed).length;
document.getElementById('ov-sessions').textContent=sessions.sessions.length-killed;
document.getElementById('ov-pending').textContent=approvals.length;
document.getElementById('ov-grants').textContent=grants.grants?.length||0;
document.getElementById('ov-policies').textContent=policies.policies?.length||0;
document.getElementById('ov-audit').textContent=audit.length;
document.getElementById('health-status').textContent='Healthy';
document.getElementById('health-status').classList.remove('err');
const recent=audit.slice(0,10);
document.getElementById('ov-recent').innerHTML=recent.map(a=>'<tr><td>'+fmt(a.timestamp)+'</td><td class="mono">'+short(a.agent_id)+'</td><td>'+a.action+'</td><td class="truncate">'+a.resource+'</td><td>'+badge(a.decision)+'</td><td class="truncate">'+(a.reason||'—')+'</td><td class="mono truncate">'+short(a.token_jti)+'</td></tr>').join('')||'<tr><td colspan="7" class="empty">No audit entries yet</td></tr>';
}catch(e){document.getElementById('health-status').textContent='Error';document.getElementById('health-status').classList.add('err');console.error(e)}
}

// === GUIDED FLOW ===
async function loadGuided(){
state.guidedStep=1;
await updateGuidedStep();
}
async function updateGuidedStep(){
for(let i=1;i<=7;i++){document.getElementById('step-'+i).className='step '+(i<state.guidedStep?'step-done':i===state.guidedStep?'':'step-pending')}
const step=state.guidedStep;
const action=document.getElementById('guided-action');
const result=document.getElementById('guided-result');
result.className='result-box';
if(step===1){
action.innerHTML='<div class="form-row"><div class="form-group"><label>External ID</label><input id="g1-ext" value="guided-user"></div><div class="form-group"><label>Email</label><input id="g1-email" value="guided@example.com"></div><div class="form-group"><label>Display Name</label><input id="g1-name" value="Guided User"></div></div><div class="form-actions"><button class="btn btn-blue" onclick="guidedStep1()">Create Principal</button></div>';
}else if(step===2){
action.innerHTML='<div class="form-row"><div class="form-group"><label>Agent Name</label><input id="g2-name" value="guided-agent"></div><div class="form-group"><label>Type</label><select id="g2-type"><option value="autonomous">autonomous</option></select></div><div class="form-group"><label>Owner ID</label><input id="g2-owner" value="'+state.guidedPrincipalId+'" readonly></div></div><div class="form-actions"><button class="btn btn-blue" onclick="guidedStep2()">Create Agent</button></div>';
}else if(step===3){
action.innerHTML='<div class="form-row"><div class="form-group"><label>Policy Name</label><input id="g3-name" value="guided-policy"></div></div><div class="form-group"><label>Definition</label><textarea id="g3-def">- name: allow-read
  agent_types: ["autonomous"]
  actions: ["read"]
  resources: ["documents/*"]
  decision: allow
- name: require-approval-write
  agent_types: ["autonomous"]
  actions: ["write"]
  resources: ["documents/*"]
  decision: require_approval
- name: deny-delete
  agent_types: ["autonomous"]
  actions: ["delete"]
  resources: ["*"]
  decision: deny</textarea></div><div class="form-actions"><button class="btn btn-blue" onclick="guidedStep3()">Create Policy</button></div>';
}else if(step===4){
action.innerHTML='<div class="form-row"><div class="form-group"><label>Agent ID</label><input id="g4-agent" value="'+state.guidedAgentId+'" readonly></div><div class="form-group"><label>Scopes</label><input id="g4-scopes" value="documents:read,documents:write"></div><div class="form-group"><label>TTL (s)</label><input id="g4-ttl" type="number" value="900"></div></div><div class="form-actions"><button class="btn btn-blue" onclick="guidedStep4()">Delegate Scopes</button></div>';
}else if(step===5){
action.innerHTML='<div class="form-row"><div class="form-group"><label>Agent ID</label><input id="g5-agent" value="'+state.guidedAgentId+'" readonly></div><div class="form-group"><label>Action</label><select id="g5-action"><option value="read">read (allow)</option><option value="write">write (require_approval)</option><option value="delete">delete (deny)</option></select></div><div class="form-group"><label>Resource</label><input id="g5-resource" value="documents/report.pdf"></div><div class="form-group"><label>Scopes</label><input id="g5-scopes" value="documents:read"></div></div><div class="form-actions"><button class="btn btn-green" onclick="guidedStep5()">Request Access</button></div>';
}else if(step===6){
action.innerHTML='<p style="color:#888;font-size:13px;margin-bottom:12px">Check the Approvals tab — if the write action triggered require_approval, approve or deny it here:</p><div class="form-row"><div class="form-group"><label>Approval ID</label><input id="g6-id" value="'+(state.guidedApprovalId||'')+'"></div><div class="form-group"><label>Approver (Principal ID)</label><input id="g6-approver" value="'+state.guidedPrincipalId+'" readonly></div></div><div class="form-actions"><button class="btn btn-green" onclick="guidedStep6(true)">Approve</button> <button class="btn btn-red" onclick="guidedStep6(false)">Deny</button></div>';
}else if(step===7){
action.innerHTML='<p style="color:#4caf50;font-size:14px">✅ Flow complete! Check the Audit Trail tab to see all decisions logged with hash chaining.</p><div class="form-actions"><button class="btn btn-blue" onclick="showTab(\'audit\');loadAudit()">View Audit Trail</button> <button class="btn btn-blue" onclick="loadGuided()">Restart Flow</button></div>';
}
}
async function guidedStep1(){
try{const r=await fPost('/v1/admin/principals',{external_id:g1_ext.value,idp_provider:'local',email:g1_email.value,display_name:g1_name.value});state.guidedPrincipalId=r.id;showResult('guided-result',r);toast('Principal created');state.guidedStep=2;updateGuidedStep()}catch(e){showResult('guided-result',e.message,false);toast(e.message,'error')}
}
async function guidedStep2(){
try{const r=await fPost('/v1/admin/agents',{name:g2_name.value,principal_type:g2_type.value,owner_id:state.guidedPrincipalId});state.guidedAgentId=r.id;showResult('guided-result',r);toast('Agent created');state.guidedStep=3;updateGuidedStep()}catch(e){showResult('guided-result',e.message,false);toast(e.message,'error')}
}
async function guidedStep3(){
try{const r=await fPost('/v1/admin/policies',{name:g3_name.value,engine:'yaml',definition:g3_def.value});showResult('guided-result',r);toast('Policy created');state.guidedStep=4;updateGuidedStep()}catch(e){showResult('guided-result',e.message,false);toast(e.message,'error')}
}
async function guidedStep4(){
try{const r=await fPost('/v1/principal/delegate',{agent_id:state.guidedAgentId,scopes:g4_scopes.value.split(','),expires_in_seconds:parseInt(g4_ttl.value)});state.guidedGrantToken=r.delegation_token;showResult('guided-result',r);toast('Scopes delegated');state.guidedStep=5;updateGuidedStep()}catch(e){showResult('guided-result',e.message,false);toast(e.message,'error')}
}
async function guidedStep5(){
try{const action=g5_action.value;const scopes=action==='write'?'documents:write':action==='delete'?'documents:delete':'documents:read';const r=await fPost('/v1/agent/request-access',{agent_id:state.guidedAgentId,action:action,resource:g5_resource.value,requested_scopes:[scopes]});showResult('guided-result',r);if(r.decision==='require_approval'&&r.approval){state.guidedApprovalId=r.approval.request_id;toast('Approval required — proceed to step 6')}else if(r.decision==='allow'){toast('Access allowed — token issued')}else{toast('Access denied')}state.guidedStep=6;updateGuidedStep()}catch(e){showResult('guided-result',e.message,false);toast(e.message,'error')}
}
async function guidedStep6(approve){
try{const id=g6_id.value||state.guidedApprovalId;if(!id){toast('No approval ID','error');return}const r=await fPost('/v1/principal/approvals/'+id+'/'+(approve?'approve':'deny'),{approver_id:g6_approver.value});showResult('guided-result',r);toast(approve?'Approved':'Denied');state.guidedStep=7;updateGuidedStep()}catch(e){showResult('guided-result',e.message,false);toast(e.message,'error')}
}

// === PRINCIPALS ===
async function loadPrincipals(){
try{const grants=await f('/v1/principal/grants');
document.getElementById('pr-grants').innerHTML=grants.grants?.map(g=>'<tr><td class="mono">'+short(g.id)+'</td><td class="mono">'+short(g.agent_id)+'</td><td>'+(g.scopes||[]).join(', ')+'</td><td class="mono truncate">'+short(g.token_jti||g.jti)+'</td><td>'+fmt(g.expires_at)+'</td><td><button class="btn btn-red btn-sm" onclick="revokeGrant(\''+g.id+'\')">Revoke</button></td></tr>').join('')||'<tr><td colspan="6" class="empty">No grants</td></tr>';
}catch(e){console.error(e)}
}
async function createPrincipal(){
try{const r=await fPost('/v1/admin/principals',{external_id:pr_ext.value,idp_provider:pr_idp.value,email:pr_email.value,display_name:pr_name.value});showResult('pr-result',r);toast('Principal created');pr_ext.value='';pr_email.value='';pr_name.value='';loadPrincipals();loadOverview()}catch(e){showResult('pr-result',e.message,false);toast(e.message,'error')}
}
async function revokeGrant(id){
try{const r=await fPost('/v1/principal/grants/'+id+'/revoke',{});showResult('pr-result',r);toast('Grant revoked');loadPrincipals();loadOverview()}catch(e){toast(e.message,'error')}
}

// === AGENTS ===
async function loadAgents(){
try{const agents=await f('/v1/admin/agents');
document.getElementById('ag-list').innerHTML=agents.map(a=>'<tr><td class="mono">'+short(a.id)+'</td><td>'+a.name+'</td><td>'+badge(a.principal_type)+'</td><td>'+badge(a.status)+'</td><td class="mono">'+short(a.owner_id)+'</td><td>'+fmt(a.created_at)+'</td><td><button class="btn btn-blue btn-sm" onclick="viewAgent(\''+a.id+'\')">View</button> <button class="btn btn-red btn-sm" onclick="killAgent(\''+a.id+'\')">Kill</button></td></tr>').join('')||'<tr><td colspan="7" class="empty">No agents registered</td></tr>';
}catch(e){console.error(e)}
}
async function createAgent(){
try{const body={name:ag_name.value,principal_type:ag_type.value};if(ag_owner.value)body.owner_id=ag_owner.value;const r=await fPost('/v1/admin/agents',body);showResult('ag-result',r);toast('Agent created');ag_name.value='';ag_owner.value='';loadAgents();loadOverview()}catch(e){showResult('ag-result',e.message,false);toast(e.message,'error')}
}
async function viewAgent(id){
try{const a=await f('/v1/admin/agents/'+id);
openModal('<h2>Agent Details</h2><table><tr><td>ID</td><td class="mono">'+a.id+'</td></tr><tr><td>Name</td><td>'+a.name+'</td></tr><tr><td>Type</td><td>'+badge(a.principal_type)+'</td></tr><tr><td>Status</td><td>'+badge(a.status)+'</td></tr><tr><td>Owner ID</td><td class="mono">'+(a.owner_id||'—')+'</td></tr><tr><td>DID</td><td class="mono">'+(a.did||'—')+'</td></tr><tr><td>Public Key</td><td class="mono truncate">'+(a.public_key||'—')+'</td></tr><tr><td>Created</td><td>'+fmt(a.created_at)+'</td></tr><tr><td>Updated</td><td>'+fmt(a.updated_at)+'</td></tr></table>');
}catch(e){toast(e.message,'error')}
}
async function killAgent(id){
if(!confirm('Kill all sessions and revoke tokens for this agent?'))return;
try{const r=await fPost('/v1/admin/agents/'+id+'/kill',{});showResult('ag-result',r);toast('Agent killed: '+r.sessions_killed+' sessions terminated');loadAgents();loadOverview()}catch(e){showResult('ag-result',e.message,false);toast(e.message,'error')}
}
async function recordSpend(){
try{const body={amount:parseFloat(sp_amount.value)};if(sp_session.value)body.session_id=sp_session.value;const r=await fPost('/v1/admin/agents/'+sp_agent.value+'/spend',body);showResult('sp-result',r);toast('Spend recorded');loadSessions();loadOverview()}catch(e){showResult('sp-result',e.message,false);toast(e.message,'error')}
}

// === RESOURCES ===
async function loadResources(){
try{const resources=await f('/v1/admin/resources');
document.getElementById('rs-list').innerHTML=resources.map(r=>'<tr><td class="mono">'+short(r.id)+'</td><td>'+r.name+'</td><td>'+badge(r.resource_type)+'</td><td class="truncate">'+r.uri+'</td><td>'+badge(r.sensitivity)+'</td><td>'+(r.actions?JSON.stringify(r.actions):'—')+'</td></tr>').join('')||'<tr><td colspan="6" class="empty">No resources</td></tr>';
}catch(e){console.error(e)}
}
async function createResource(){
try{let actions=JSON.parse(rs_actions.value||'[]');const r=await fPost('/v1/admin/resources',{name:rs_name.value,resource_type:rs_type.value,uri:rs_uri.value,actions:actions,sensitivity:rs_sens.value});showResult('rs-result',r);toast('Resource created');rs_name.value='';rs_uri.value='';loadResources();loadOverview()}catch(e){showResult('rs-result',e.message,false);toast(e.message,'error')}
}

// === POLICIES ===
async function loadPolicies(){
try{const p=await f('/v1/admin/policies');
document.getElementById('po-list').innerHTML=p.policies?.map(po=>'<tr><td class="mono">'+po.id+'</td><td>'+po.name+'</td><td>'+po.engine+'</td><td>'+badge(po.status)+'</td><td><button class="btn btn-blue btn-sm" onclick="viewPolicy('+po.id+')">View YAML</button></td></tr>').join('')||'<tr><td colspan="5" class="empty">No policies</td></tr>';
}catch(e){console.error(e)}
}
function viewPolicy(id){
const policies=window._policies_cache||[];
const po=policies.find(p=>p.id===id);
if(po)openModal('<h2>Policy: '+po.name+'</h2><div class="code-block">'+po.definition+'</div>');
}
async function createPolicy(){
try{const r=await fPost('/v1/admin/policies',{name:po_name.value,engine:po_engine.value,definition:po_def.value});showResult('po-result',r);toast('Policy created');po_name.value='';loadPolicies();loadOverview()}catch(e){showResult('po-result',e.message,false);toast(e.message,'error')}
}

// === DELEGATION ===
async function principalDelegate(){
try{const body={agent_id:dl_agent.value,scopes:dl_scopes.value.split(',').map(s=>s.trim()).filter(Boolean),expires_in_seconds:parseInt(dl_ttl.value)};if(dl_constraints.value)body.constraints=JSON.parse(dl_constraints.value);const r=await fPost('/v1/principal/delegate',body);showResult('dl-result',r);toast('Scopes delegated to agent');loadPrincipals();loadOverview()}catch(e){showResult('dl-result',e.message,false);toast(e.message,'error')}
}
async function agentDelegate(){
try{const r=await fPost('/v1/agent/delegate',{parent_grant_token:sd_token.value,sub_agent_id:sd_agent.value,scopes:sd_scopes.value.split(',').map(s=>s.trim()).filter(Boolean),expires_in_seconds:parseInt(sd_ttl.value)});showResult('sd-result',r);toast('Token delegated to sub-agent')}catch(e){showResult('sd-result',e.message,false);toast(e.message,'error')}
}
async function revokeToken(){
try{const r=await fPost('/v1/admin/tokens/'+encodeURIComponent(rv_jti.value)+'/revoke',{});showResult('rv-result',r);toast('Token revoked')}catch(e){showResult('rv-result',e.message,false);toast(e.message,'error')}
}

// === ACCESS ===
async function requestAccess(){
try{const body={agent_id:ac_agent.value,action:ac_action.value,resource:ac_resource.value,requested_scopes:ac_scopes.value.split(',').map(s=>s.trim()).filter(Boolean)};if(ac_dtoken.value)body.delegation_token=ac_dtoken.value;if(ac_session.value)body.context={session_id:ac_session.value};const r=await fPost('/v1/agent/request-access',body);showResult('ac-result',r);toast('Decision: '+r.decision);loadOverview();loadAudit()}catch(e){showResult('ac-result',e.message,false);toast(e.message,'error')}
}
async function checkAccess(){
try{const body={agent_id:ac_agent.value,action:ac_action.value,resource:ac_resource.value,requested_scopes:ac_scopes.value.split(',').map(s=>s.trim()).filter(Boolean)};if(ac_dtoken.value)body.delegation_token=ac_dtoken.value;if(ac_session.value)body.context={session_id:ac_session.value};const r=await fPost('/v1/agent/check',body);showResult('ac-result',r);toast('Decision: '+r.decision)}catch(e){showResult('ac-result',e.message,false);toast(e.message,'error')}
}
async function checkApprovalStatus(){
try{const r=await f('/v1/agent/approval-status/'+ap_id_check.value);showResult('ap-check-result',r);if(r.status==='approved')toast('Approved!');else if(r.status==='denied')toast('Denied');else toast('Still pending')}catch(e){showResult('ap-check-result',e.message,false);toast(e.message,'error')}
}

// === APPROVALS ===
async function loadApprovals(){
try{const ap=await f('/v1/principal/approvals');
document.getElementById('ap-list').innerHTML=ap.map(a=>'<tr><td class="mono">'+short(a.id)+'</td><td class="mono">'+short(a.agent_id)+'</td><td>'+a.action+'</td><td class="truncate">'+a.resource+'</td><td>'+(a.requested_scopes||[]).join(', ')+'</td><td>'+badge(a.status)+'</td><td>'+fmt(a.expires_at)+'</td><td><button class="btn btn-green btn-sm" onclick="approveReq(\''+a.id+'\')">Approve</button> <button class="btn btn-red btn-sm" onclick="denyReq(\''+a.id+'\')">Deny</button></td></tr>').join('')||'<tr><td colspan="8" class="empty">No pending approvals</td></tr>';
}catch(e){console.error(e)}
}
async function approveReq(id){
try{const r=await fPost('/v1/principal/approvals/'+id+'/approve',{approver_id:state.guidedPrincipalId||prompt('Approver ID (Principal UUID):')});toast('Approved');loadApprovals();loadOverview()}catch(e){toast(e.message,'error')}
}
async function denyReq(id){
try{const r=await fPost('/v1/principal/approvals/'+id+'/deny',{approver_id:state.guidedPrincipalId||prompt('Approver ID (Principal UUID):')});toast('Denied');loadApprovals();loadOverview()}catch(e){toast(e.message,'error')}
}

// === SESSIONS ===
async function loadSessions(){
try{const ss=await f('/v1/sessions');
const killed=ss.sessions.filter(s=>s.killed).length;
document.getElementById('ss-total').textContent=ss.sessions.length;
document.getElementById('ss-killed').textContent=killed;
document.getElementById('ss-active').textContent=ss.sessions.length-killed;
document.getElementById('ss-list').innerHTML=ss.sessions.map(s=>'<tr><td class="mono truncate">'+s.session_id+'</td><td class="mono">'+short(s.agent_id)+'</td><td>'+s.actions_count+'</td><td>$'+s.spend_total.toFixed(4)+'</td><td>'+s.tokens_used+'</td><td>'+(s.trust_level*100).toFixed(0)+'%</td><td>'+(s.killed?badge('killed'):'<span class="badge-tag badge-active">active</span>')+'</td><td>'+(s.killed?'—':'<button class="btn btn-red btn-sm" onclick="killSession(\''+encodeURIComponent(s.session_id)+'\')">Kill</button>')+'</td></tr>').join('')||'<tr><td colspan="8" class="empty">No sessions</td></tr>';
}catch(e){console.error(e)}
}
async function killSession(id){
try{await fPost('/v1/sessions/'+id+'/kill',{});toast('Session killed');loadSessions();loadOverview()}catch(e){toast(e.message,'error')}
}

// === AUDIT ===
async function loadAudit(){
try{const au=await f('/v1/admin/audit');
document.getElementById('au-list').innerHTML=au.map(a=>'<tr><td class="mono">'+a.id+'</td><td>'+fmt(a.timestamp)+'</td><td class="mono">'+short(a.agent_id)+'</td><td>'+a.action+'</td><td class="truncate">'+a.resource+'</td><td>'+badge(a.decision)+'</td><td class="truncate">'+(a.reason||'—')+'</td><td class="mono truncate">'+short(a.row_hash,16)+'</td><td class="mono truncate">'+short(a.prev_hash,16)+'</td></tr>').join('')||'<tr><td colspan="9" class="empty">No audit entries</td></tr>';
}catch(e){console.error(e)}
}

// === VAULT ===
async function loadVault(){
try{const idp=await f('/v1/idp/providers');
let html='<p style="color:#666;margin-bottom:8px">IdP Federation: '+(idp.enabled?'<span style="color:#4caf50">Enabled</span>':'<span style="color:#f44336">Disabled</span>')+'</p>';
if(idp.providers?.length){html+='<table><thead><tr><th>Provider</th><th>Issuer</th><th>Client ID</th><th>Scopes</th></tr></thead><tbody>';idp.providers.forEach(p=>{html+='<tr><td>'+p.name+'</td><td class="truncate">'+p.issuer+'</td><td class="mono">'+short(p.client_id,16)+'</td><td>'+(p.scopes||[]).join(', ')+'</td></tr>'});html+='</tbody></table>'}else{html+='<p class="empty">No IdP providers configured</p>'}
document.getElementById('idp-info').innerHTML=html;
}catch(e){console.error(e)}
}
async function generateVaultKey(){
try{const r=await fPost('/v1/vault/generate-key',{});showResult('vk-result',r);toast('Vault key generated')}catch(e){showResult('vk-result',e.message,false);toast(e.message,'error')}
}
async function storeCredential(){
try{const r=await fPost('/v1/vault/credentials',{principal_id:va_pid.value,provider:va_prov.value,refresh_token:va_token.value,scopes:va_scopes.value.split(',').map(s=>s.trim()).filter(Boolean)});showResult('va-result',r);toast('Credential stored');va_pid.value='';va_prov.value='';va_token.value='';va_scopes.value=''}catch(e){showResult('va-result',e.message,false);toast(e.message,'error')}
}

// Cache policies for modal view
async function loadPoliciesCache(){
try{const p=await f('/v1/admin/policies');window._policies_cache=p.policies||[]}catch(e){}
}

function loadTab(t){
if(t==='overview')loadOverview();
else if(t==='guided')loadGuided();
else if(t==='principals')loadPrincipals();
else if(t==='agents')loadAgents();
else if(t==='resources')loadResources();
else if(t==='policies'){loadPolicies();loadPoliciesCache()}
else if(t==='approvals')loadApprovals();
else if(t==='sessions')loadSessions();
else if(t==='audit')loadAudit();
else if(t==='vault')loadVault();
else if(t==='idp')loadVault();
}

loadOverview();
loadPoliciesCache();
setInterval(()=>{const active=document.querySelector('.tab-content.active');if(active)loadTab(active.id)},5000);
</script>
</body>
</html>"#.to_string()
}
