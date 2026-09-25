import tempfile
import unittest
from pathlib import Path

from build_mads import stage_example


class StageExampleTests(unittest.TestCase):
    def test_stage_excludes_old_lockfile_and_target(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source"
            source.mkdir()
            (source / "Cargo.toml").write_text("[package]\nname='demo'\nversion='0.1.0'\n")
            (source / "Cargo.lock").write_text("stale")
            (source / ".env").write_text("secret=do-not-copy")
            (source / "target").mkdir()
            (source / "target" / "artifact").write_text("old")
            destination = stage_example(source, root / "staged")
            self.assertTrue((destination / "Cargo.toml").is_file())
            self.assertFalse((destination / "Cargo.lock").exists())
            self.assertFalse((destination / ".env").exists())
            self.assertFalse((destination / "target").exists())

    def test_stage_uses_benchmark_lock_instead_of_example_lock(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source"
            source.mkdir()
            (source / "Cargo.toml").write_text("[package]\nname='demo'\nversion='0.1.0'\n")
            (source / "Cargo.lock").write_text("stale")
            lock = root / "benchmark.lock"
            lock.write_text("pinned")
            destination = stage_example(source, root / "staged", lock)
            self.assertEqual((destination / "Cargo.lock").read_text(), "pinned")


if __name__ == "__main__":
    unittest.main()
