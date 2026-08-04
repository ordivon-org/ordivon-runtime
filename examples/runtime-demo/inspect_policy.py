from __future__ import annotations

import time

from policy import recovery_policy


print(f"INSPECT policy={recovery_policy()}", flush=True)
time.sleep(1.0)
