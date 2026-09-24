import unittest

from run import Measurements, Response, json_body, json_body_at_size, percentiles, work_chunks


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

    def test_body_limit_payloads_have_exact_byte_lengths_and_valid_json(self):
        for size in (2_097_151, 2_097_152, 2_097_153):
            with self.subTest(size=size):
                payload = json_body_at_size(size)
                self.assertEqual(len(payload), size)
                self.assertEqual(json_body(payload)["password"], "short")
                self.assertTrue(json_body(payload)["username"])


if __name__ == "__main__":
    unittest.main()
