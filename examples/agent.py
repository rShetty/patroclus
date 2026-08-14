"""
Patroclus Agent — A real AI agent that uses Patroclus for authorization
on every action, powered by OpenRouter for LLM inference.

Usage:
    1. Start Patroclus:  cargo run --release -- serve
    2. Run this agent:   python examples/agent.py

The agent:
1. Registers itself with Patroclus
2. Gets a policy created that allows it to read dev resources
3. Asks the LLM (via OpenRouter) what it wants to do
4. Before each action, checks with Patroclus for authorization
5. If allowed, executes the "action" (simulated) using a scoped token
6. If denied, reports the denial reason
7. If approval is required, creates an approval request

This demonstrates the full flow: Agent → Patroclus (policy check) → Action
"""

import json
import os
import sys
import time
import uuid
from datetime import datetime, timezone

import httpx

PATROCLUS_URL = os.getenv("PATROCLUS_URL", "http://localhost:8484")
OPENROUTER_API_KEY = os.getenv("OPENROUTER_API_KEY", "")
OPENROUTER_URL = "https://openrouter.ai/api/v1/chat/completions"
MODEL = os.getenv("OPENROUTER_MODEL", "openai/gpt-4o-mini")


class PatroclusAgent:
    """A real AI agent that checks Patroclus before every action."""

    def __init__(self, name: str = "demo-agent"):
        self.name = name
        self.agent_id = None
        self.principal_id = None
        self.session_id = f"agent-session-{uuid.uuid4().hex[:8]}"
        self.actions_taken = []
        self.actions_denied = []
        self.client = httpx.Client(base_url=PATROCLUS_URL, timeout=10.0)

    def setup(self):
        """Register agent + principal, create a policy."""
        print(f"\n{'='*60}")
        print("PATROCLUS AGENT — SETUP")
        print(f"{'='*60}")

        # Check health
        resp = self.client.get("/health")
        print(f"  Patroclus health: {resp.json()}")

        # Register principal (human who owns the agent)
        resp = self.client.post("/v1/admin/principals", json={
            "external_id": "demo-human",
            "idp_provider": "local",
            "email": "human@demo.com",
            "display_name": "Demo Human",
        })
        if resp.status_code == 200:
            self.principal_id = resp.json()["id"]
            print(f"  Registered principal: {self.principal_id}")
        else:
            # Maybe already exists, try to find it
            print(f"  Principal registration: {resp.status_code} {resp.text}")
            # Just create a new one
            resp = self.client.post("/v1/admin/principals", json={
                "external_id": f"demo-human-{uuid.uuid4().hex[:4]}",
                "idp_provider": "local",
                "email": f"human-{uuid.uuid4().hex[:4]}@demo.com",
                "display_name": "Demo Human",
            })
            self.principal_id = resp.json()["id"]
            print(f"  Registered principal: {self.principal_id}")

        # Register agent
        resp = self.client.post("/v1/admin/agents", json={
            "name": self.name,
            "principal_type": "delegated",
            "owner_id": self.principal_id,
        })
        self.agent_id = resp.json()["id"]
        print(f"  Registered agent: {self.agent_id} ({self.name})")

        # Create a policy that allows reads on dev resources
        # and requires approval for prod writes
        policy = """
- name: allow-dev-reads
  actions: ["read", "query", "list"]
  resources: ["dev-*", "test-*"]
  scopes: ["*"]
  decision: allow
  reason: Dev and test read access permitted

- name: allow-dev-writes
  actions: ["write", "update", "create"]
  resources: ["dev-*"]
  scopes: ["*"]
  decision: allow
  reason: Dev write access permitted

- name: require-approval-prod
  actions: ["write", "update", "delete", "deploy"]
  resources: ["prod-*"]
  scopes: ["*"]
  decision: require_approval
  reason: Production operations require human approval

- name: deny-prod-deletes
  actions: ["delete"]
  resources: ["prod-*"]
  scopes: ["*"]
  decision: deny
  reason: Production deletes are strictly forbidden

- name: rate-limited-api
  actions: ["call"]
  resources: ["api-*"]
  scopes: ["*"]
  decision: allow
  reason: API calls permitted (rate limited)
  rate_limit_per_minute: 5
"""
        resp = self.client.post("/v1/admin/policies", json={
            "name": "demo-policy",
            "engine": "yaml",
            "definition": policy,
        })
        print(f"  Policy created: {resp.status_code}")
        print(f"  Session: {self.session_id}")
        print()

    def check_access(self, action: str, resource: str, scopes: list = None) -> dict:
        """Check with Patroclus if this action is allowed (dry-run)."""
        resp = self.client.post("/v1/agent/check", json={
            "agent_id": self.agent_id,
            "action": action,
            "resource": resource,
            "requested_scopes": scopes or [],
            "context": {"session_id": self.session_id},
        })
        return resp.json()

    def request_access(self, action: str, resource: str, scopes: list = None) -> dict:
        """Request access from Patroclus — issues token on allow."""
        resp = self.client.post("/v1/agent/request-access", json={
            "agent_id": self.agent_id,
            "action": action,
            "resource": resource,
            "requested_scopes": scopes or [],
            "context": {"session_id": self.session_id},
        })
        return resp.json()

    def ask_llm(self, messages: list) -> str:
        """Call OpenRouter for LLM inference."""
        headers = {
            "Authorization": f"Bearer {OPENROUTER_API_KEY}",
            "Content-Type": "application/json",
        }
        payload = {
            "model": MODEL,
            "messages": messages,
            "max_tokens": 1024,
            "temperature": 0.7,
        }
        resp = httpx.post(OPENROUTER_URL, json=payload, headers=headers, timeout=30.0)
        resp.raise_for_status()
        return resp.json()["choices"][0]["message"]["content"]

    def run_task(self, task: str):
        """Run a task: LLM decides actions, Patroclus authorizes each one."""
        print(f"\n{'='*60}")
        print(f"TASK: {task}")
        print(f"{'='*60}\n")

        system_prompt = f"""You are an AI agent operating under Patroclus authorization infrastructure.
Every action you propose will be checked against Patroclus policies before execution.

You have access to these simulated resources:
- dev-database (read, query, write)
- dev-config (read, update)
- prod-database (read, write — requires approval)
- api-github (call — rate limited to 5/min)
- api-slack (call — rate limited to 5/min)

Available actions: read, query, write, update, create, delete, deploy, call

Respond with a JSON array of actions you want to take. Each action should be:
{{"action": "<action>", "resource": "<resource>", "reason": "<why>"}}

Respond ONLY with the JSON array, no other text."""

        user_prompt = f"Task: {task}\n\nPropose the actions you would take. Be specific about which resource and action for each step."

        messages = [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_prompt},
        ]

        print("Asking LLM to plan actions...")
        try:
            response = self.ask_llm(messages)
            print(f"LLM response:\n{response}\n")
        except Exception as e:
            print(f"LLM error: {e}")
            return

        # Parse the LLM's proposed actions
        try:
            # Try to extract JSON from the response
            json_start = response.find("[")
            json_end = response.rfind("]") + 1
            if json_start >= 0 and json_end > json_start:
                actions = json.loads(response[json_start:json_end])
            else:
                print("Could not parse actions from LLM response")
                return
        except json.JSONDecodeError as e:
            print(f"JSON parse error: {e}")
            return

        print(f"LLM proposed {len(actions)} actions. Checking with Patroclus...\n")

        for i, action in enumerate(actions, 1):
            act = action.get("action", "unknown")
            res = action.get("resource", "unknown")
            reason = action.get("reason", "")

            print(f"  [{i}/{len(actions)}] {act} → {res}")
            print(f"       Reason: {reason}")

            # Check with Patroclus
            check = self.check_access(act, res, [f"{res.split('-')[0]}:{act}"])
            allowed = check.get("allowed", False)
            check_decision = check.get("decision", "unknown")
            check_reason = check.get("reason", "")

            print(f"       Check: {check_decision} — {check_reason}")

            if not allowed:
                # Try full request (may trigger approval)
                result = self.request_access(act, res, [f"{res.split('-')[0]}:{act}"])
                decision = result.get("decision", "deny")
                result_reason = result.get("reason", "")

                if decision == "allow":
                    token = result.get("token", {})
                    print(f"       ✓ ALLOWED — token issued (jti: {token.get('jti', 'N/A')[:12]}...)")
                    print(f"         Scopes: {token.get('scopes', [])}")
                    print(f"         Expires: {token.get('expires_at', 'N/A')}")
                    self.actions_taken.append({"action": act, "resource": res, "token": token.get("jti")})
                elif decision == "require_approval":
                    approval = result.get("approval", {})
                    print(f"       ⏳ APPROVAL REQUIRED — request_id: {approval.get('request_id', 'N/A')[:12]}...")
                    print(f"          {result_reason}")
                    self.actions_denied.append({"action": act, "resource": res, "reason": "approval_required"})
                else:
                    print(f"       ✗ DENIED — {result_reason}")
                    self.actions_denied.append({"action": act, "resource": res, "reason": result_reason})
            else:
                # Allowed by check, now get a real token
                result = self.request_access(act, res, [f"{res.split('-')[0]}:{act}"])
                decision = result.get("decision", "deny")
                if decision == "allow":
                    token = result.get("token", {})
                    print(f"       ✓ ALLOWED — token issued (jti: {token.get('jti', 'N/A')[:12]}...)")
                    self.actions_taken.append({"action": act, "resource": res, "token": token.get("jti")})
                else:
                    print(f"       ✗ Unexpected: {decision} — {result.get('reason', '')}")
                    self.actions_denied.append({"action": act, "resource": res, "reason": result.get("reason", "")})

            print()

    def report(self):
        """Print summary of what happened."""
        print(f"\n{'='*60}")
        print("SUMMARY")
        print(f"{'='*60}")
        print(f"  Actions taken (allowed): {len(self.actions_taken)}")
        print(f"  Actions denied:          {len(self.actions_denied)}")

        if self.actions_taken:
            print(f"\n  Allowed actions:")
            for a in self.actions_taken:
                print(f"    ✓ {a['action']} → {a['resource']}")

        if self.actions_denied:
            print(f"\n  Denied actions:")
            for a in self.actions_denied:
                print(f"    ✗ {a['action']} → {a['resource']} ({a['reason']})")

        # Get audit trail from Patroclus
        resp = self.client.get("/v1/admin/audit")
        audit = resp.json()
        print(f"\n  Audit trail: {len(audit)} entries")
        for entry in audit[:10]:
            decision = entry.get("decision", "?")
            icon = "✓" if decision == "allow" else "✗" if decision == "deny" else "⏳"
            print(f"    {icon} {entry.get('action', '?')} → {entry.get('resource', '?')} [{decision}]")

        # Get session state
        resp = self.client.get("/v1/sessions")
        sessions = resp.json().get("sessions", [])
        for s in sessions:
            if s.get("session_id") == self.session_id:
                print(f"\n  Session state:")
                print(f"    Actions count: {s.get('actions_count', 0)}")
                print(f"    Trust level:   {s.get('trust_level', 1.0)}")
                print(f"    Killed:        {s.get('killed', False)}")
                break

        print(f"\n{'='*60}\n")

    def cleanup(self):
        self.client.close()


def main():
    if not OPENROUTER_API_KEY:
        print("ERROR: OPENROUTER_API_KEY not set")
        sys.exit(1)

    agent = PatroclusAgent(name="openrouter-demo-agent")

    try:
        agent.setup()

        tasks = [
            "Read the development database to check user counts, then update the dev config with the result.",
            "Try to deploy to production and delete a production database table.",
            "Make 8 API calls to the GitHub API (this should hit the rate limit).",
        ]

        for task in tasks:
            agent.run_task(task)
            time.sleep(1)

        agent.report()

    except KeyboardInterrupt:
        print("\n\nInterrupted by user.")
        agent.report()
    except Exception as e:
        print(f"\nError: {e}")
        import traceback
        traceback.print_exc()
    finally:
        agent.cleanup()


if __name__ == "__main__":
    main()
