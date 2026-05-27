import unittest
import os
import shutil
import tempfile
import sys
from datetime import date, timedelta

# Add scripts to path
sys.path.append(os.path.join(os.path.dirname(__file__), "..", "scripts"))
import watchdog

class TestWatchdog(unittest.TestCase):
    def setUp(self):
        self.test_dir = tempfile.mkdtemp()
        watchdog.PROJECT_ROOT = self.test_dir
        watchdog.CURRENT_DATE = date.today()

    def tearDown(self):
        shutil.rmtree(self.test_dir)

    def test_phase1_stale_anchor(self):
        file_path = os.path.join(self.test_dir, "test.rs")
        stale_date = date.today() - timedelta(days=1)
        with open(file_path, 'w') as f:
            f.write(f"// ANCHOR:FIXME:001 AGENT:01 DATE:{stale_date.isoformat()} STATUS:WIP\n")
            f.write("// WP:WP-1.1 PRIO:1 NEEDS:NONE\n")

        watchdog.phase1_stale_anchors([file_path])

        with open(file_path, 'r') as f:
            content = f.read()
            self.assertIn("STATUS:OPEN", content)
            self.assertIn("// WATCHDOG: Reset WIP due to timeout.", content)

    def test_phase2_deadlock(self):
        file1 = os.path.join(self.test_dir, "file1.rs")
        file2 = os.path.join(self.test_dir, "file2.rs")

        with open(file1, 'w') as f:
            f.write("// ANCHOR:FIXME:A AGENT:01 DATE:2026-06-15 STATUS:BLOCKED\n")
            f.write("// WP:WP-1.1 PRIO:1 NEEDS:B\n")

        with open(file2, 'w') as f:
            f.write("// ANCHOR:FIXME:B AGENT:02 DATE:2026-06-15 STATUS:BLOCKED\n")
            f.write("// WP:WP-1.2 PRIO:1 NEEDS:A\n")

        watchdog.phase2_deadlocks([file1, file2])

        with open(file1, 'r') as f1, open(file2, 'r') as f2:
            c1 = f1.read()
            c2 = f2.read()
            self.assertTrue("STATUS:OPEN" in c1 or "STATUS:OPEN" in c2)
            self.assertTrue("// WATCHDOG: Broken cyclic dependency." in c1 or "// WATCHDOG: Broken cyclic dependency." in c2)

    def test_phase3_fv_gate_crate_specific(self):
        # Create core crate with gate
        core_dir = os.path.join(self.test_dir, "crates/memfuse-core/src")
        os.makedirs(core_dir)
        gate_file = os.path.join(core_dir, "lib.rs")
        with open(gate_file, 'w') as f:
            f.write("// ANCHOR:ARCH:GATE-FV STATUS:DONE\n")

        # Create store crate with REVIEW component but NO proof
        store_dir = os.path.join(self.test_dir, "crates/memfuse-store/src")
        os.makedirs(store_dir)
        store_file = os.path.join(store_dir, "lsm.rs")
        with open(store_file, 'w') as f:
            f.write("// AGENT:02 DATE:2026-06-15 STATUS:REVIEW\n")

        # Create crypto crate with proof
        crypto_dir = os.path.join(self.test_dir, "crates/memfuse-crypto/src")
        os.makedirs(crypto_dir)
        crypto_file = os.path.join(crypto_dir, "lib.rs")
        with open(crypto_file, 'w') as f:
            f.write("// kani::proof\n")

        watchdog.phase3_fv_gate([gate_file, store_file, crypto_file])

        with open(gate_file, 'r') as f:
            content = f.read()
            self.assertIn("STATUS:OPEN", content)
            self.assertIn("Blocking merges due to missing Kani/TLA+ proofs in: memfuse-store", content)
            self.assertNotIn("memfuse-crypto", content)

if __name__ == "__main__":
    unittest.main()
