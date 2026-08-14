"""
Tests for the Patroclus Python SDK.

These tests run against a live Patroclus server. Start the server first:
    cargo run --release -- serve
"""

import os
import time
import pytest
import uuid

from patroclus_sdk import PatroclusClient, AccessResult, PatroclusMiddleware
from patroclus_sdk.decorators import authorized_action


PATROCLUS_URL = os.getenv("PATROCLUS_URL", "http://localhost:8484")

# Policy that allows dev reads, denies prod deletes, rate-limits API calls
TEST_POLICY = """
- name: allow-dev-reads
  actions: ["read", "query"]
  resources: ["dev-*", "test-*"]
  scopes: ["*"]
  decision: allow
  reason: Dev read access permitted

- name: allow-dev-writes
  actions: ["write", "update"]
  resources: ["dev-*"]
  scopes: ["*"]
  decision: allow
  reason: Dev write access permitted

- name: deny-prod-deletes
  actions: ["delete"]
  resources: ["prod-*"]
  scopes: ["*"]
  decision: deny
  reason: Production deletes forbidden

- name: rate-limited-api
  actions: ["call"]
  resources: ["api-*"]
  scopes: ["*"]
  decision: allow
  reason: API calls permitted
  rate_limit_per_minute: 3
"""


@pytest.fixture(scope="module")
def client():
    c = PatroclusClient(PATROCLUS_URL)
    if not c.health():
        pytest.skip("Patroclus server not running")
    yield c
    c.close()


@pytest.fixture(scope="module")
def setup_infrastructure(client):
    """Register principal, agent, and policy."""
    principal = client.register_principal(
        email=f"sdk-test-{uuid.uuid4().hex[:6]}@test.com",
        display_name="SDK Test",
    )
    agent = client.register_agent(
        name="sdk-test-agent",
        principal_type="delegated",
        owner_id=principal["id"],
    )
    client.create_policy("sdk-test-policy", TEST_POLICY, "yaml")
    time.sleep(0.5)  # Allow hot-reload to take effect

    return {"principal": principal, "agent": agent}


class TestHealth:
    def test_health_check(self, client):
        assert client.health() is True


class TestAccessControl:
    def test_check_access_allowed(self, client, setup_infrastructure):
        agent_id = setup_infrastructure["agent"]["id"]
        assert client.check_access(agent_id, "read", "dev-db", ["db:read"]) is True

    def test_check_access_denied(self, client, setup_infrastructure):
        agent_id = setup_infrastructure["agent"]["id"]
        assert client.check_access(agent_id, "delete", "prod-db", ["db:delete"]) is False

    def test_request_access_returns_token(self, client, setup_infrastructure):
        agent_id = setup_infrastructure["agent"]["id"]
        result = client.request_access(agent_id, "read", "dev-db", ["db:read"])
        assert result.allowed
        assert result.token is not None
        assert result.token_jti is not None
        assert "db:read" in result.token_scopes

    def test_request_access_denied(self, client, setup_infrastructure):
        agent_id = setup_infrastructure["agent"]["id"]
        result = client.request_access(agent_id, "delete", "prod-db", ["db:delete"])
        assert result.denied
        assert "forbidden" in result.reason.lower()
        assert result.token is None

    def test_request_access_with_session(self, client, setup_infrastructure):
        agent_id = setup_infrastructure["agent"]["id"]
        session = f"sdk-test-{uuid.uuid4().hex[:8]}"
        result = client.request_access(
            agent_id, "read", "dev-db", ["db:read"], session_id=session
        )
        assert result.allowed


class TestRateLimiting:
    def test_rate_limit_blocks(self, client, setup_infrastructure):
        agent_id = setup_infrastructure["agent"]["id"]
        session = f"rl-test-{uuid.uuid4().hex[:8]}"

        # First 3 should be allowed
        for i in range(3):
            result = client.request_access(
                agent_id, "call", "api-github", ["api:call"], session_id=session
            )
            assert result.allowed, f"Call {i+1} should be allowed"

        # 4th should be denied
        result = client.request_access(
            agent_id, "call", "api-github", ["api:call"], session_id=session
        )
        assert result.denied
        assert "rate limit" in result.reason.lower()


class TestDelegation:
    def test_delegate_permissions(self, client, setup_infrastructure):
        agent_id = setup_infrastructure["agent"]["id"]
        result = client.delegate_permissions(
            agent_id=agent_id,
            scopes=["db:read", "db:write"],
            expires_in_seconds=600,
        )
        assert result.token is not None
        assert "db:read" in result.scopes
        assert "db:write" in result.scopes

    def test_sub_delegation_narrower(self, client, setup_infrastructure):
        agent_id = setup_infrastructure["agent"]["id"]
        sub_agent = client.register_agent(name="sdk-sub-agent", principal_type="delegated")

        # Parent delegation
        parent = client.delegate_permissions(
            agent_id=agent_id,
            scopes=["db:read", "db:write"],
            expires_in_seconds=600,
        )

        # Sub-delegation with narrower scope
        result = client.delegate(
            parent_grant_token=parent.token,
            sub_agent_id=sub_agent["id"],
            scopes=["db:read"],
            expires_in_seconds=300,
        )
        assert result.token is not None
        assert result.scopes == ["db:read"]


class TestAuditAndSessions:
    def test_audit_trail(self, client, setup_infrastructure):
        agent_id = setup_infrastructure["agent"]["id"]
        # Generate an action
        client.request_access(agent_id, "read", "dev-db", ["db:read"])

        audit = client.get_audit_trail()
        assert len(audit) > 0
        # Most recent should be our action
        assert audit[0]["action"] == "read"

    def test_list_sessions(self, client, setup_infrastructure):
        agent_id = setup_infrastructure["agent"]["id"]
        session = f"sess-test-{uuid.uuid4().hex[:8]}"
        client.request_access(agent_id, "read", "dev-db", ["db:read"], session_id=session)

        sessions = client.list_sessions()
        assert any(s["session_id"] == session for s in sessions)


class TestKillSwitch:
    def test_kill_agent(self, client, setup_infrastructure):
        agent = client.register_agent(name="kill-test-agent", principal_type="delegated")
        session = f"kill-test-{uuid.uuid4().hex[:8]}"

        # Create a session
        client.request_access(agent["id"], "read", "dev-db", ["db:read"], session_id=session)

        # Kill the agent
        result = client.kill_agent(agent["id"])
        assert result["killed"] is True

        # Subsequent access should be denied
        result = client.request_access(agent["id"], "read", "dev-db", ["db:read"], session_id=session)
        assert result.denied
        assert "killed" in result.reason.lower()


class TestMiddleware:
    def test_middleware_check_and_execute(self, client, setup_infrastructure):
        agent_id = setup_infrastructure["agent"]["id"]
        mw = PatroclusMiddleware(
            client, agent_id=agent_id, session_id=f"mw-test-{uuid.uuid4().hex[:8]}"
        )

        assert mw.check("read", "dev-db", ["db:read"]) is True
        assert mw.check("delete", "prod-db", ["db:delete"]) is False

        token = mw.get_token("read", "dev-db", ["db:read"])
        assert token is not None

    def test_middleware_execute_allowed(self, client, setup_infrastructure):
        agent_id = setup_infrastructure["agent"]["id"]
        mw = PatroclusMiddleware(
            client, agent_id=agent_id, session_id=f"mw-exec-{uuid.uuid4().hex[:8]}"
        )

        def fetch_data(patroclus_token=None):
            return {"data": "success", "token_present": patroclus_token is not None}

        result = mw.execute("read", "dev-db", ["db:read"], fetch_data)
        assert result["data"] == "success"
        assert result["token_present"] is True

    def test_middleware_execute_denied(self, client, setup_infrastructure):
        agent_id = setup_infrastructure["agent"]["id"]
        mw = PatroclusMiddleware(
            client, agent_id=agent_id, session_id=f"mw-deny-{uuid.uuid4().hex[:8]}"
        )

        def delete_data():
            return "deleted"

        with pytest.raises(PermissionError, match="denied"):
            mw.execute("delete", "prod-db", ["db:delete"], delete_data)


class TestDecorator:
    def test_authorized_action_allowed(self, client, setup_infrastructure):
        agent_id = setup_infrastructure["agent"]["id"]

        @authorized_action(client, action="read", resource="dev-db", scopes=["db:read"])
        def fetch_users(agent_id, patroclus_token=None):
            return {"users": ["Alice", "Bob"], "has_token": patroclus_token is not None}

        result = fetch_users(agent_id=agent_id)
        assert result["users"] == ["Alice", "Bob"]
        assert result["has_token"] is True

    def test_authorized_action_denied(self, client, setup_infrastructure):
        agent_id = setup_infrastructure["agent"]["id"]

        @authorized_action(client, action="delete", resource="prod-db", scopes=["db:delete"])
        def delete_users(agent_id):
            return "deleted"

        with pytest.raises(PermissionError, match="denied"):
            delete_users(agent_id=agent_id)
