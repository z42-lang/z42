"""Check how the PR workflow handles newly registered Criterion cases."""

from pathlib import Path
import re
import shlex
import unittest


class BenchBaselineTests(unittest.TestCase):
    def test_criterion_compares_only_existing_baselines(self):
        workflow = Path(__file__).parents[1] / "workflows/bench-pr.yml"
        bench_args = [
            shlex.split(args)
            for args in re.findall(r"--bench gc_cycle_bench -- ([^\n]+) \)", workflow.read_text())
        ]
        # Base saves its samples. Head measures every case and compares shared IDs.
        self.assertEqual(bench_args, [["--save-baseline", "ab-base"], ["--baseline-lenient", "ab-base"]])
