"""
Patroclus SDK client — HTTP client for the Patroclus authorization API.
"""

import json
import time
from dataclasses import dataclass, field
from typing import Optional, List, Dict, Any

import httpx


@dataclass
class AccessResult:
    """Result of an access request."""
    decision: str  # "allow", "deny", "require_approval"
    reason: str
    token: Optional[str] = None
    token_jti: Optional[str] = None
    token_scopes: List[str] = field(default_factory=list)
    token_expires_at: Optional[str] = None
    approval_request_id: Optional[str] = None
    approved_scopes: List[str] = field(default_factory=list)

    @property
    def allowed(self) -> bool:
        return self.decision == "allow"

    @property
    def denied(self) -> bool:
        return self.decision == "deny"

    @property
    def requires_approval(self) -> bool:
        return self.decision == "require_approval"

    @classmethod
    def from_response(cls, data: dict) -> "AccessResult":
        token = data.get("token")
        approval = data.get("approval")
        return cls(
            decision=data.get("decision", "deny"),
            reason=data.get("reason", ""),
            token=token.get("jwt") if token else None,
            token_jti=token.get("jti") if token else None,
            token_scopes=token.get("scopes", []) if token else [],
            token_expires_at=token.get("expires_at") if token else None,
            approval_request_id=approval.get("request_id") if approval else None,
            approved_scopes=data.get("approved_scopes", []),
        )


@dataclass
class DelegateResult:
    """Result of a delegation."""
    token: str
    scopes: List[str]
    expires_at: str
    grant_id: Optional[str] = None


@dataclass
class ApprovalResult:
    """Result of an approval action."""
    request_id: str
    status: str
    approver_id: Optional[str] = None
    reason: Optional[str] = None
    approval_token: Optional[str] = None


class PatroclusClient:
    """
    HTTP client for the Patroclus authorization API.

    Args:
        base_url: Patroclus server URL (default: http://localhost:8484)
        timeout: Request timeout in seconds
    """

    def __init__(
        self,
        base_url: str = "http://localhost:8484",
        timeout: float = 10.0,
    ):
        self.base_url = base_url.rstrip("/")
        self._client = httpx.Client(base_url=self.base_url, timeout=timeout)

    def close(self):
        self._client.close()

    def __enter__(self):
        return self

    def __exit__(self, *args):
        self.close()

    # ── Health ─────────────────────────────────────────────────────

    def health(self) -> bool:
        """Check if Patroclus is reachable."""
        try:
            resp = self._client.get("/health")
            return resp.status_code == 200
        except Exception:
            return False

    # ── Agent-facing ───────────────────────────────────────────────

    def check_access(
        self,
        agent_id: str,
        action: str,
        resource: str,
        scopes: Optional[List[str]] = None,
        session_id: Optional[str] = None,
        delegation_token: Optional[str] = None,
    ) -> bool:
        """Dry-run check — returns True if access would be allowed."""
        payload = self._build_access_payload(
            agent_id, action, resource, scopes, session_id, delegation_token
        )
        resp = self._client.post("/v1/agent/check", json=payload)
        if resp.status_code == 200:
            return resp.json().get("allowed", False)
        return False

    def request_access(
        self,
        agent_id: str,
        action: str,
        resource: str,
        scopes: Optional[List[str]] = None,
        session_id: Optional[str] = None,
        delegation_token: Optional[str] = None,
    ) -> AccessResult:
        """
        Request access — issues a token on ALLOW, creates approval on REQUIRE_APPROVAL.

        Returns AccessResult with decision, token (if allowed), and approval info.
        """
        payload = self._build_access_payload(
            agent_id, action, resource, scopes, session_id, delegation_token
        )
        resp = self._client.post("/v1/agent/request-access", json=payload)
        if resp.status_code == 200:
            return AccessResult.from_response(resp.json())
        return AccessResult(
            decision="deny",
            reason=f"HTTP {resp.status_code}: {resp.text}",
        )

    def delegate(
        self,
        parent_grant_token: str,
        sub_agent_id: str,
        scopes: List[str],
        expires_in_seconds: int = 900,
    ) -> DelegateResult:
        """Delegate narrowed permissions to a sub-agent."""
        resp = self._client.post("/v1/agent/delegate", json={
            "parent_grant_token": parent_grant_token,
            "sub_agent_id": sub_agent_id,
            "scopes": scopes,
            "expires_in_seconds": expires_in_seconds,
        })
        if resp.status_code == 200:
            data = resp.json()
            return DelegateResult(
                token=data["delegated_token"],
                scopes=data["scopes"],
                expires_at=data["expires_at"],
            )
        raise RuntimeError(f"Delegation failed: {resp.status_code} {resp.text}")

    def get_approval_status(self, request_id: str) -> Optional[ApprovalResult]:
        """Get the status of an approval request."""
        resp = self._client.get(f"/v1/agent/approval-status/{request_id}")
        if resp.status_code == 200:
            data = resp.json()
            return ApprovalResult(
                request_id=data["id"],
                status=data["status"],
                approver_id=data.get("approver_id"),
                reason=data.get("reason"),
                approval_token=data.get("approval_token"),
            )
        return None

    # ── Principal-facing ───────────────────────────────────────────

    def delegate_permissions(
        self,
        agent_id: str,
        scopes: List[str],
        expires_in_seconds: int = 900,
        constraints: Optional[Dict[str, Any]] = None,
    ) -> DelegateResult:
        """Human delegates scoped permissions to an agent."""
        payload = {
            "agent_id": agent_id,
            "scopes": scopes,
            "expires_in_seconds": expires_in_seconds,
        }
        if constraints:
            payload["constraints"] = constraints
        resp = self._client.post("/v1/principal/delegate", json=payload)
        if resp.status_code == 200:
            data = resp.json()
            return DelegateResult(
                token=data["delegation_token"],
                scopes=data["scopes"],
                expires_at=data["expires_at"],
                grant_id=data.get("grant_id"),
            )
        raise RuntimeError(f"Delegation failed: {resp.status_code} {resp.text}")

    def list_pending_approvals(self) -> List[ApprovalResult]:
        """List all pending approval requests."""
        resp = self._client.get("/v1/principal/approvals")
        if resp.status_code == 200:
            data = resp.json()
            return [
                ApprovalResult(
                    request_id=a["id"],
                    status=a["status"],
                    reason=a.get("reason"),
                )
                for a in data
            ]
        return []

    def approve_request(
        self,
        request_id: str,
        approver_id: str,
        reason: Optional[str] = None,
    ) -> ApprovalResult:
        """Approve a pending request."""
        payload = {"approver_id": approver_id}
        if reason:
            payload["reason"] = reason
        resp = self._client.post(
            f"/v1/principal/approvals/{request_id}/approve", json=payload
        )
        if resp.status_code == 200:
            data = resp.json()
            return ApprovalResult(
                request_id=data["id"],
                status=data["status"],
                approver_id=data.get("approver_id"),
                reason=data.get("reason"),
                approval_token=data.get("approval_token"),
            )
        raise RuntimeError(f"Approval failed: {resp.status_code} {resp.text}")

    def deny_request(
        self,
        request_id: str,
        approver_id: str,
        reason: Optional[str] = None,
    ) -> ApprovalResult:
        """Deny a pending request."""
        payload = {"approver_id": approver_id}
        if reason:
            payload["reason"] = reason
        resp = self._client.post(
            f"/v1/principal/approvals/{request_id}/deny", json=payload
        )
        if resp.status_code == 200:
            data = resp.json()
            return ApprovalResult(
                request_id=data["id"],
                status=data["status"],
                approver_id=data.get("approver_id"),
                reason=data.get("reason"),
            )
        raise RuntimeError(f"Denial failed: {resp.status_code} {resp.text}")

    def revoke_grant(self, grant_id: str) -> dict:
        """Revoke a grant (cascades to children)."""
        resp = self._client.post(
            f"/v1/principal/grants/{grant_id}/revoke", json={"cascade": True}
        )
        if resp.status_code == 200:
            return resp.json()
        raise RuntimeError(f"Revoke failed: {resp.status_code} {resp.text}")

    # ── Admin ──────────────────────────────────────────────────────

    def register_agent(
        self,
        name: str,
        principal_type: str = "delegated",
        owner_id: Optional[str] = None,
    ) -> dict:
        """Register a new agent."""
        payload = {"name": name, "principal_type": principal_type}
        if owner_id:
            payload["owner_id"] = owner_id
        resp = self._client.post("/v1/admin/agents", json=payload)
        if resp.status_code == 200:
            return resp.json()
        raise RuntimeError(f"Agent registration failed: {resp.status_code} {resp.text}")

    def register_principal(
        self,
        email: str,
        display_name: str,
        external_id: Optional[str] = None,
        idp_provider: str = "local",
    ) -> dict:
        """Register a new human principal."""
        resp = self._client.post("/v1/admin/principals", json={
            "external_id": external_id or email,
            "idp_provider": idp_provider,
            "email": email,
            "display_name": display_name,
        })
        if resp.status_code == 200:
            return resp.json()
        raise RuntimeError(f"Principal registration failed: {resp.status_code} {resp.text}")

    def create_policy(
        self,
        name: str,
        definition: str,
        engine: str = "yaml",
    ) -> dict:
        """Create a new authorization policy (hot-reloads the engine)."""
        resp = self._client.post("/v1/admin/policies", json={
            "name": name,
            "engine": engine,
            "definition": definition,
        })
        if resp.status_code == 200:
            return resp.json()
        raise RuntimeError(f"Policy creation failed: {resp.status_code} {resp.text}")

    def get_audit_trail(self, limit: int = 100) -> List[dict]:
        """Get the audit trail."""
        resp = self._client.get("/v1/admin/audit")
        if resp.status_code == 200:
            return resp.json()
        return []

    def list_sessions(self) -> List[dict]:
        """List all active sessions."""
        resp = self._client.get("/v1/sessions")
        if resp.status_code == 200:
            return resp.json().get("sessions", [])
        return []

    def kill_agent(self, agent_id: str) -> dict:
        """Emergency stop — kill all agent sessions and revoke tokens."""
        resp = self._client.post(f"/v1/admin/agents/{agent_id}/kill")
        if resp.status_code == 200:
            return resp.json()
        raise RuntimeError(f"Kill failed: {resp.status_code} {resp.text}")

    def record_spend(self, agent_id: str, amount: float, session_id: Optional[str] = None) -> dict:
        """Record spend for budget tracking."""
        payload = {"amount": amount}
        if session_id:
            payload["session_id"] = session_id
        resp = self._client.post(f"/v1/admin/agents/{agent_id}/spend", json=payload)
        if resp.status_code == 200:
            return resp.json()
        raise RuntimeError(f"Spend recording failed: {resp.status_code} {resp.text}")

    def revoke_token(self, jti: str) -> dict:
        """Revoke a specific token by JTI."""
        resp = self._client.post(f"/v1/admin/tokens/{jti}/revoke")
        if resp.status_code == 200:
            return resp.json()
        raise RuntimeError(f"Token revocation failed: {resp.status_code} {resp.text}")

    # ── Vault ──────────────────────────────────────────────────────

    def store_credential(
        self,
        principal_id: str,
        provider: str,
        refresh_token: str,
        scopes: List[str],
    ) -> dict:
        """Store an encrypted credential in the vault."""
        resp = self._client.post("/v1/vault/credentials", json={
            "principal_id": principal_id,
            "provider": provider,
            "refresh_token": refresh_token,
            "scopes": scopes,
        })
        if resp.status_code == 200:
            return resp.json()
        raise RuntimeError(f"Credential storage failed: {resp.status_code} {resp.text}")

    # ── Internal ───────────────────────────────────────────────────

    def _build_access_payload(
        self,
        agent_id: str,
        action: str,
        resource: str,
        scopes: Optional[List[str]],
        session_id: Optional[str],
        delegation_token: Optional[str],
    ) -> dict:
        payload = {
            "agent_id": agent_id,
            "action": action,
            "resource": resource,
            "requested_scopes": scopes or [],
        }
        context = {}
        if session_id:
            context["session_id"] = session_id
        if context:
            payload["context"] = context
        if delegation_token:
            payload["delegation_token"] = delegation_token
        return payload
