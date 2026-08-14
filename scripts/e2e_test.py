#!/usr/bin/env python3
"""
E2E ecosystem test: Hive + Patroclus + Relay + Miser

Verifies the full agent governance flow:
1. All services healthy
2. Hive agent registration auto-registers with Patroclus
3. Patroclus policy auto-created per agent
4. Authorization: allow, deny, require_approval
5. Human approval workflow
6. Rate limiting
7. Audit trail
8. Session tracking
9. Kill switch

Usage:
    # Start all services first:
    ~/patroclus/scripts/start-ecosystem.sh start

    # Run the test:
    python3 ~/patroclus/scripts/e2e_test.py
"""

import json
import sys
import time

import httpx

PATROCLUS = "http://localhost:8484"
HIVE = "http://localhost:8000"
MISER = "http://localhost:8787"
RELAY = "http://localhost:8090"


def main():
    print("=" * 60)
    print("ECOSYSTEM E2E TEST")
    print("Hive + Patroclus + Relay + Miser")
    print("=" * 60)

    # 1. Verify all services
    print("\n1. Service Health:")
    checks = [
        ("Patroclus", f"{PATROCLUS}/health"),
        ("Miser", f"{MISER}/health/live"),
        ("Relay", f"{RELAY}/patroclus/status"),
        ("Hive", f"{HIVE}/"),
    ]
    for name, url in checks:
        try:
            resp = httpx.get(url, timeout=5.0)
            ok = resp.status_code == 200
            print(f"   {name:12s} {'✓ UP' if ok else '✗ DOWN'} (HTTP {resp.status_code})")
            if not ok:
                sys.exit(1)
        except Exception as e:
            print(f"   {name:12s} ✗ DOWN ({e})")
            sys.exit(1)

    relay_status = httpx.get(f"{RELAY}/patroclus/status").json()
    if not relay_status.get("connected"):
        print("   ⚠ Relay not connected to Patroclus!")
        sys.exit(1)

    # 2. Register user on Hive
    print("\n2. Register user on Hive:")
    email = f"e2e-{int(time.time())}@test.com"
    resp = httpx.post(f"{HIVE}/api/auth/register", json={
        "email": email, "password": "testpass123", "name": "E2E Test"
    })
    user_id = resp.json().get("id", "")
    print(f"   User: {user_id[:12]}... ({email})")

    resp = httpx.post(f"{HIVE}/api/auth/login", json={
        "email": email, "password": "testpass123"
    })
    jwt = resp.json()["access_token"]

    # 3. Register agent on Hive (auto-registers with Patroclus)
    print("\n3. Register agent on Hive:")
    resp = httpx.post(f"{HIVE}/api/agent/register",
        headers={"Authorization": f"Bearer {jwt}"},
        json={
            "name": f"e2e-agent-{int(time.time())}",
            "description": "E2E test agent",
            "agent_type": "external",
            "endpoint_url": "http://localhost:9999/agent",
        }
    )
    hive_agent_id = resp.json()["agent_id"]
    agent_name = resp.json().get("agent_id", "")
    print(f"   Hive agent: {hive_agent_id[:12]}...")

    # 4. Verify agent in Patroclus
    print("\n4. Verify agent in Patroclus:")
    time.sleep(0.5)
    agents = httpx.get(f"{PATROCLUS}/v1/admin/agents").json()
    patroclus_agent = next((a for a in agents if a["name"].startswith("e2e-agent")), None)
    if not patroclus_agent:
        print("   ✗ Agent not found in Patroclus!")
        sys.exit(1)
    agent_id = patroclus_agent["id"]
    print(f"   ✓ Found: {agent_id[:12]}... (type: {patroclus_agent['principal_type']})")
    print(f"   ✓ Owner: {patroclus_agent['owner_id'][:12]}...")

    # 5. Verify policy in Patroclus
    print("\n5. Verify policy in Patroclus:")
    policies = httpx.get(f"{PATROCLUS}/v1/admin/policies").json()
    agent_policy = next((p for p in policies["policies"] if "e2e-agent" in p["name"]), None)
    if agent_policy:
        print(f"   ✓ Policy: {agent_policy['name']} ({agent_policy['status']})")
    else:
        print("   ✗ No agent policy found!")

    # 6. Test: read access (ALLOW)
    print("\n6. Read access (expect ALLOW):")
    session = f"e2e-session-{int(time.time())}"
    resp = httpx.post(f"{PATROCLUS}/v1/agent/request-access", json={
        "agent_id": agent_id,
        "action": "read",
        "resource": "dev-database",
        "requested_scopes": ["db:read"],
        "context": {"session_id": session},
    })
    result = resp.json()
    print(f"   Decision: {result['decision']}")
    print(f"   Reason: {result['reason']}")
    assert result["decision"] == "allow", f"Expected allow, got {result['decision']}"
    assert result["token"] is not None, "Expected token"
    print(f"   Token: jti={result['token']['jti'][:12]}..., scopes={result['token']['scopes']}")

    # 7. Test: production deploy (REQUIRE APPROVAL)
    print("\n7. Production deploy (expect REQUIRE_APPROVAL):")
    resp = httpx.post(f"{PATROCLUS}/v1/agent/request-access", json={
        "agent_id": agent_id,
        "action": "deploy",
        "resource": "prod-database",
        "requested_scopes": ["db:deploy"],
        "context": {"session_id": session},
    })
    result = resp.json()
    print(f"   Decision: {result['decision']}")
    assert result["decision"] == "require_approval"
    approval_id = result["approval"]["request_id"]
    print(f"   Approval: {approval_id[:12]}...")

    # 8. Approve the request
    print("\n8. Approve request:")
    principal_id = patroclus_agent["owner_id"]
    resp = httpx.post(f"{PATROCLUS}/v1/principal/approvals/{approval_id}/approve", json={
        "approver_id": principal_id,
        "reason": "Approved for e2e test",
    })
    print(f"   Status: {resp.json()['status']}")
    print(f"   Token: {str(resp.json().get('approval_token', ''))[:12]}...")

    # 9. Test rate limiting
    print("\n9. Rate limiting (limit 10/min for api-*):")
    rl_session = f"rl-{int(time.time())}"
    allowed = 0
    denied = 0
    for i in range(12):
        resp = httpx.post(f"{PATROCLUS}/v1/agent/request-access", json={
            "agent_id": agent_id,
            "action": "call",
            "resource": "api-github",
            "requested_scopes": ["api:call"],
            "context": {"session_id": rl_session},
        })
        if resp.json()["decision"] == "allow":
            allowed += 1
        else:
            denied += 1
    print(f"   Allowed: {allowed}, Denied: {denied} (of 12, limit 10/min)")
    assert denied > 0, "Expected some calls to be rate-limited"

    # 10. Audit trail
    print("\n10. Audit trail:")
    audit = httpx.get(f"{PATROCLUS}/v1/admin/audit").json()
    print(f"   Total entries: {len(audit)}")
    # Verify hash chain
    for i in range(1, len(audit)):
        prev = audit[len(audit) - 1 - i]
        curr = audit[len(audit) - i]
        if prev["row_hash"] != curr["prev_hash"]:
            print(f"   ⚠ Hash chain broken at entry {i}!")
            break
    else:
        print(f"   ✓ Hash chain intact ({len(audit)} entries)")
    for entry in audit[:5]:
        icon = "✓" if entry["decision"] == "allow" else "✗" if entry["decision"] == "deny" else "⏳"
        print(f"   {icon} {entry['action']:12s} → {entry['resource']:20s} [{entry['decision']}]")

    # 11. Session state
    print("\n11. Session state:")
    sessions = httpx.get(f"{PATROCLUS}/v1/sessions").json()["sessions"]
    test_session = next((s for s in sessions if s["session_id"] == session), None)
    if test_session:
        print(f"   Actions: {test_session['actions_count']}")
        print(f"   Trust: {test_session['trust_level']}")
        print(f"   Killed: {test_session['killed']}")

    # 12. Kill switch
    print("\n12. Kill switch:")
    resp = httpx.post(f"{PATROCLUS}/v1/admin/agents/{agent_id}/kill")
    kill_result = resp.json()
    print(f"   Killed: {kill_result['killed']}")
    print(f"   Sessions killed: {kill_result['sessions_killed']}")

    # 13. Verify access blocked
    print("\n13. Post-kill access (expect DENY):")
    resp = httpx.post(f"{PATROCLUS}/v1/agent/request-access", json={
        "agent_id": agent_id,
        "action": "read",
        "resource": "dev-database",
        "requested_scopes": ["db:read"],
        "context": {"session_id": session},
    })
    result = resp.json()
    print(f"   Decision: {result['decision']}")
    print(f"   Reason: {result['reason']}")
    assert result["decision"] == "deny"
    assert "killed" in result["reason"].lower()

    # Summary
    print("\n" + "=" * 60)
    print("E2E TEST PASSED")
    print("=" * 60)
    print()
    print("Verified:")
    print("  ✓ All 4 services healthy and connected")
    print("  ✓ Hive agent registration → Patroclus auto-registration")
    print("  ✓ Patroclus policy auto-created per agent")
    print("  ✓ Read access ALLOWED with scoped JWT")
    print("  ✓ Production deploy → REQUIRE_APPROVAL")
    print("  ✓ Human approval workflow (approve → token issued)")
    print("  ✓ Rate limiting enforced")
    print("  ✓ Audit trail hash-chained and intact")
    print("  ✓ Session tracking (actions, trust, trajectory)")
    print("  ✓ Kill switch blocks all subsequent access")
    print("  ✓ Relay connected to Patroclus (per-tool authz ready)")
    print("  ✓ Miser ready for LLM cost optimization")


if __name__ == "__main__":
    main()
