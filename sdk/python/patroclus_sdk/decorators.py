"""
Decorators and middleware for integrating Patroclus into agent code.

Usage with decorator:
    from patroclus_sdk import PatroclusClient, authorized_action

    client = PatroclusClient("http://localhost:8484")

    @authorized_action(client, action="read", resource="dev-db", scopes=["db:read"])
    def fetch_users(agent_id: str):
        # This only runs if Patroclus allows it
        return [{"name": "Alice"}, {"name": "Bob"}]

Usage with middleware:
    from patroclus_sdk import PatroclusMiddleware

    mw = PatroclusMiddleware(client, agent_id="my-agent")
    if mw.check("read", "dev-db", ["db:read"]):
        result = mw.execute("read", "dev-db", ["db:read"], fetch_users)
"""

import functools
from typing import Callable, Optional, List, Any

from .client import PatroclusClient, AccessResult


def authorized_action(
    client: PatroclusClient,
    action: str,
    resource: str,
    scopes: Optional[List[str]] = None,
    session_id: Optional[str] = None,
):
    """
    Decorator that checks Patroclus before executing the function.

    The decorated function must accept `agent_id` as its first argument.

    If access is denied, raises PermissionError with the reason.
    If access requires approval, raises PermissionError with approval info.
    If access is allowed, the function executes normally.

    The access token (if issued) is passed as `patroclus_token` keyword arg.
    """
    def decorator(func: Callable) -> Callable:
        @functools.wraps(func)
        def wrapper(*args, agent_id: str = None, **kwargs):
            if agent_id is None and args:
                agent_id = str(args[0])

            result = client.request_access(
                agent_id=agent_id or "unknown",
                action=action,
                resource=resource,
                scopes=scopes or [],
                session_id=session_id,
            )

            if result.denied:
                raise PermissionError(
                    f"Patroclus denied {action} on {resource}: {result.reason}"
                )

            if result.requires_approval:
                raise PermissionError(
                    f"Patroclus requires approval for {action} on {resource}: "
                    f"request_id={result.approval_request_id}"
                )

            # Pass token to the function if it accepts it
            if "patroclus_token" in func.__code__.co_varnames:
                kwargs["patroclus_token"] = result.token

            # Pass agent_id if the function accepts it but it wasn't already in args
            if "agent_id" in func.__code__.co_varnames and "agent_id" not in kwargs:
                if not args:
                    kwargs["agent_id"] = agent_id or "unknown"

            return func(*args, **kwargs)

        return wrapper
    return decorator


class PatroclusMiddleware:
    """
    Middleware that wraps agent actions with Patroclus authorization.

    Usage:
        mw = PatroclusMiddleware(client, agent_id="my-agent", session_id="sess-1")

        if mw.check("read", "dev-db", ["db:read"]):
            token = mw.get_token("read", "dev-db", ["db:read"])
            # Use token to access the resource
            result = call_api(token)

        # Or use execute which checks + calls
        result = mw.execute("read", "dev-db", ["db:read"], my_function)
    """

    def __init__(
        self,
        client: PatroclusClient,
        agent_id: str,
        session_id: Optional[str] = None,
        delegation_token: Optional[str] = None,
    ):
        self.client = client
        self.agent_id = agent_id
        self.session_id = session_id
        self.delegation_token = delegation_token
        self.tokens: List[AccessResult] = []

    def check(
        self,
        action: str,
        resource: str,
        scopes: Optional[List[str]] = None,
    ) -> bool:
        """Dry-run check — returns True if access would be allowed."""
        return self.client.check_access(
            agent_id=self.agent_id,
            action=action,
            resource=resource,
            scopes=scopes,
            session_id=self.session_id,
            delegation_token=self.delegation_token,
        )

    def get_token(
        self,
        action: str,
        resource: str,
        scopes: Optional[List[str]] = None,
    ) -> Optional[str]:
        """
        Request access and return the JWT token if allowed.
        Returns None if denied or approval required.
        """
        result = self.client.request_access(
            agent_id=self.agent_id,
            action=action,
            resource=resource,
            scopes=scopes,
            session_id=self.session_id,
            delegation_token=self.delegation_token,
        )
        if result.allowed:
            self.tokens.append(result)
            return result.token
        return None

    def execute(
        self,
        action: str,
        resource: str,
        scopes: Optional[List[str]],
        func: Callable,
        *args,
        **kwargs,
    ) -> Any:
        """
        Check authorization, then execute func if allowed.
        The JWT token is passed as `patroclus_token` kwarg if func accepts it.
        """
        result = self.client.request_access(
            agent_id=self.agent_id,
            action=action,
            resource=resource,
            scopes=scopes,
            session_id=self.session_id,
            delegation_token=self.delegation_token,
        )

        if result.denied:
            raise PermissionError(
                f"Patroclus denied {action} on {resource}: {result.reason}"
            )

        if result.requires_approval:
            raise PermissionError(
                f"Approval required for {action} on {resource}: "
                f"request_id={result.approval_request_id}"
            )

        if "patroclus_token" in func.__code__.co_varnames:
            kwargs["patroclus_token"] = result.token

        return func(*args, **kwargs)

    def record_spend(self, amount: float) -> dict:
        """Record spend for budget tracking."""
        return self.client.record_spend(self.agent_id, amount, self.session_id)
