import unittest
import os
import datetime
from scripts.watchdog import parse_anchors

class TestWatchdog(unittest.TestCase):
    def test_parse_anchors(self):
        content = """
// ANCHOR:ARCH:CORE-001 — Triebwerk-Fundament
// WP:WP-0.0 PRIO:1 NEEDS:NONE
// AGENT:01 DATE:2026-05-09 STATUS:DONE

// ANCHOR:TEST-WIP STATUS:WIP
// AGENT:01 DATE:2026-05-20 STATUS:WIP
// CREATED:2026-05-10 DEADLINE:NONE

// ANCHOR:TEST-BLOCKED STATUS:BLOCKED NEEDS:DEP-1
// AGENT:02 DATE:2026-05-15 STATUS:BLOCKED
"""
        anchors = parse_anchors(content)
        self.assertIn("ARCH:CORE-001", anchors)
        self.assertEqual(anchors["ARCH:CORE-001"]["status"], "DONE")

        self.assertIn("TEST-WIP", anchors)
        self.assertEqual(anchors["TEST-WIP"]["status"], "WIP")
        self.assertEqual(anchors["TEST-WIP"]["created"], "2026-05-10")

        self.assertIn("TEST-BLOCKED", anchors)
        self.assertEqual(anchors["TEST-BLOCKED"]["status"], "BLOCKED")
        self.assertEqual(anchors["TEST-BLOCKED"]["needs"], ["DEP-1"])

if __name__ == "__main__":
    unittest.main()
