import unittest
import os
import sys
sys.path.append(os.getcwd())
from scripts.watchdog import parse_anchors

class TestWatchdog(unittest.TestCase):
    def test_parse_anchors(self):
        content = """
// ANCHOR:TEST-1 STATUS:DONE NEEDS:NONE
// AGENT:01 DATE:2026-05-09 STATUS:DONE

// ANCHOR:TEST-2 STATUS:WIP
// CREATED:2026-05-10
// AGENT:02 DATE:2026-05-10 STATUS:WIP
"""
        anchors = parse_anchors(content)
        self.assertIn("TEST-1", anchors)
        self.assertEqual(anchors["TEST-1"]["status"], "DONE")
        self.assertIn("TEST-2", anchors)
        self.assertEqual(anchors["TEST-2"]["status"], "WIP")
        self.assertEqual(anchors["TEST-2"]["created"], "2026-05-10")

if __name__ == "__main__":
    unittest.main()
