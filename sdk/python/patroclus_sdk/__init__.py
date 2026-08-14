"""
Patroclus SDK — Python client for the Patroclus authorization infrastructure.

Usage:
    pip install patroclus-sdk
    from patroclus_sdk import PatroclusClient
    client = PatroclusClient("http://localhost:8484")
    result = client.request_access(agent_id, "read", "dev-db", ["db:read"])
    if result.allowed:
        print(f"Token: {result.token}")
"""

from .client import PatroclusClient, AccessResult, DelegateResult, ApprovalResult
from .decorators import authorized_action, PatroclusMiddleware

__all__ = [
    "PatroclusClient",
    "AccessResult",
    "DelegateResult",
    "ApprovalResult",
    "authorized_action",
    "PatroclusMiddleware",
]
__version__ = "0.1.0"
