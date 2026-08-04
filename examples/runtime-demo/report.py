from __future__ import annotations

import hashlib
import json
import time

from policy import recovery_policy


payload = {
    "policy": recovery_policy(),
    "proof": "same-job-recovery",
}
encoded = json.dumps(payload, sort_keys=True, separators=(",", ":")).encode("utf-8")
time.sleep(1.0)
print(
    json.dumps(
        {
            **payload,
            "digest": f"sha256:{hashlib.sha256(encoded).hexdigest()}",
        },
        sort_keys=True,
    ),
    flush=True,
)
