import unittest

from run import Measurements, Response, percentiles, work_chunks


class BenchmarkMathTests(unittest.TestCase):
    def test_percentiles_use_nearest_rank_and_keep_tail_latency(self):
        self.assertEqual(
            percentiles([1.0, 2.0, 3.0, 4.0, 100.0]),
            {"p50_ms": 3.0, "p95_ms": 100.0, "p99_ms": 100.0},
        )

    def test_work_chunks_assign_every_request_once(self):
        self.assertEqual(work_chunks(10, 3), [4, 3, 3])
        self.assertEqual(work_chunks(2, 5), [1, 1])

    def test_contract_mismatch_is_a_benchmark_failure(self):
        stats = Measurements()
        response = Response(200, b"wrong", {}, 2.0)
        self.assertFalse(stats.check("hello", response, 200, lambda body: body == b"Hello, world!"))
        self.assertEqual(stats.error_count, 1)
        self.assertEqual(stats.statuses[200], 1)


if __name__ == "__main__":
    unittest.main()
