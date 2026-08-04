from __future__ import annotations

import time
import unittest

from policy import recovery_policy


class RecoveryPolicyTests(unittest.TestCase):
    def test_uncertain_delivery_recovers_the_recorded_job(self) -> None:
        time.sleep(1.0)
        self.assertEqual(recovery_policy(), "recover-recorded-job")


if __name__ == "__main__":
    unittest.main()
