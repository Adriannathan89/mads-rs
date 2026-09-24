# Testing & Benchmark Checklist — Framework HTTP (Rust/Axum-level)

Dokumen ini adalah checklist pengujian untuk framework HTTP custom yang levelnya setara Axum (dibangun di atas Hyper + Tokio), mencakup routing, middleware/layer (Tower), extractor, dan request/response handling.

---

## 1. Layer Protokol HTTP (Hyper/Tokio dependency)

Karena Axum-level framework biasanya tidak parsing HTTP mentah sendiri (diserahkan ke Hyper), fokus di sini adalah **kepatuhan protokol** dan **kerentanan yang diwariskan** dari dependency.

- [ ] **Request smuggling / desync** — jika frameworkmu berada di belakang reverse proxy, uji ketidakcocokan parsing `Content-Length` vs `Transfer-Encoding` antara proxy dan Hyper
- [ ] **Malformed chunked encoding** — kirim chunk size invalid, chunk extension aneh, trailer header injection
- [ ] **Header folding / obsolete line folding** (RFC 9112 §5.2) — pastikan ditolak, bukan diterima secara longgar
- [ ] **Duplicate/conflicting headers** — dua `Content-Length` berbeda, `Host` header ganda
- [ ] **Slowloris / slow body** — koneksi lambat yang menahan worker/task

**Referensi:**
- RFC 9110/9111/9112 (HTTP Semantics) — httpwg.org
- Hyper security advisories — github.com/hyperium/hyper/security/advisories
- RustSec Advisory Database — rustsec.org (cek `tokio`, `hyper`, `h2`, `axum` semua)
- h2spec (HTTP/2 conformance testing tool) — github.com/summerwind/h2spec

---

## 2. Routing & Path Matching

- [ ] Path traversal via encoding (`%2e%2e%2f`, double-encoding `%252e%252e`)
- [ ] Trailing slash mismatch (`/admin` vs `/admin/` — beda handler/middleware?)
- [ ] Nested router merge conflict (Axum `nest()` — pastikan middleware induk tidak ke-skip di sub-router)
- [ ] Wildcard/catch-all route (`{*rest}`) bocor ke route yang seharusnya lebih spesifik
- [ ] Case sensitivity pada path (`/Admin` vs `/admin`)
- [ ] Method override / verb tunneling jika kamu implement `X-HTTP-Method-Override`

**Referensi:**
- Axum routing docs (edge case `MethodRouter`, `nest`) — docs.rs/axum
- OWASP Testing Guide — Path Traversal section — owasp.org/www-project-web-security-testing-guide

---

## 3. Extractor & Deserialisasi (paling kritis di Rust web framework)

- [ ] **Body size limit** — pastikan ada default limit (DoS via body raksasa), Axum punya `DefaultBodyLimit`, uji apakah custom extractor kamu ikut menghormatinya
- [ ] **JSON deserialization bomb** — nested object sangat dalam (stack overflow di Serde), array sangat besar
- [ ] **Query string parsing** — duplicate key, array notation (`a[]=1&a[]=2`), karakter non-UTF8
- [ ] **Multipart form** — file upload tanpa limit ukuran/jumlah part, nama file dengan path traversal
- [ ] **Content-Type confusion** — extractor JSON dipanggil padahal `Content-Type: text/plain`, cek apakah validasi ketat
- [ ] **Rejection handling** — pastikan custom extractor error tidak bocor informasi internal (stack trace, path server) ke response

**Referensi:**
- Serde security considerations — serde.rs (khususnya soal `#[serde(deny_unknown_fields)]` vs mass assignment)
- Axum extractor docs — docs.rs/axum/latest/axum/extract

---

## 4. Middleware / Tower Layer

- [ ] **Urutan layer salah** — auth layer terpasang setelah handler yang seharusnya diproteksi
- [ ] **Layer yang panic** — pastikan panic di satu request tidak mematikan seluruh worker (uji `catch_unwind` / `tower::catch_panic`)
- [ ] **Timeout layer** — request yang menggantung tanpa timeout (resource exhaustion)
- [ ] **Rate limiting bypass** — uji apakah rate limiter bisa dilewati dengan header `X-Forwarded-For` palsu

**Referensi:**
- Tower crate docs — docs.rs/tower
- Tower-http middleware (timeout, limit, trace) — docs.rs/tower-http

---

## 5. Header & Response Handling

- [ ] **Host header injection** — jika ada logic yang trust `Host` untuk generate URL (reset password link, redirect)
- [ ] **CRLF injection ke response header** — kalau kamu set header dari input user tanpa sanitasi
- [ ] **CORS misconfiguration** — reflect `Origin` tanpa validasi, `Access-Control-Allow-Credentials: true` + wildcard origin
- [ ] **Security headers default** — cek apakah framework kasih opsi mudah untuk `Content-Security-Policy`, `X-Content-Type-Options`, dll (bukan wajib ada, tapi harus mudah dipasang)

**Referensi:**
- OWASP Cheat Sheet Series — HTTP Headers, CORS — cheatsheetseries.owasp.org
- PortSwigger Web Security Academy — CORS & Host header attacks — portswigger.net/web-security

---

## 6. Async/Concurrency-Specific (khas Rust/Tokio)

- [ ] **Task starvation** — satu handler blocking (misalnya sync I/O tanpa `spawn_blocking`) memblokir runtime
- [ ] **Shared state race condition** — `Arc<Mutex<T>>` atau state global yang diakses banyak handler concurrent
- [ ] **Graceful shutdown** — koneksi in-flight saat SIGTERM, pastikan tidak drop request begitu saja
- [ ] **Connection pool exhaustion** — kalau frameworkmu proxy ke backend/database, uji behavior saat pool habis

**Referensi:**
- Tokio docs — Blocking & Async best practices — tokio.rs/tokio/topics
- "Async Rust footguns" — cari talks/blog dari tim Tokio (tokio.rs/blog)

---

## 7. Business Logic & Auth (kalau framework include ini)

- [ ] Mass assignment (field tak di-whitelist ke-assign ke struct)
- [ ] JWT: algoritma confusion (`alg: none`), signature bypass, expired token masih diterima
- [ ] Session fixation
- [ ] IDOR (Insecure Direct Object Reference) pada endpoint CRUD generated otomatis

**Referensi:**
- OWASP ASVS (Application Security Verification Standard) — checklist per kontrol, paling cocok untuk uji arsitektur framework — owasp.org/www-project-application-security-verification-standard
- PortSwigger — Business logic vulnerabilities — portswigger.net/web-security/logic-flaws

---

## 8. Benchmark Performa

- [ ] Throughput & latency dasar — bandingkan dengan Axum asli sebagai baseline
- [ ] Behavior di bawah load tinggi (concurrent connections, keep-alive reuse)

**Tools:**
- `wrk` — github.com/wg/wrk
- `oha` (Rust-based load tester, HTTP/2 friendly) — github.com/hatoo/oha
- `k6` — k6.io
- TechEmpower Framework Benchmarks (sebagai referensi metodologi) — techempower.com/benchmarks

---

## Urutan Prioritas yang Disarankan

1. RustSec advisory scan di semua dependency (`cargo audit`) — paling murah, cek dulu sebelum manual testing
2. Body size limit & deserialization DoS — paling gampang jadi bug fatal
3. Routing edge case (path traversal, trailing slash)
4. Middleware ordering & panic isolation
5. Header injection & CORS
6. Baru masuk ke load/benchmark testing

**Tool wajib jalan rutin:** `cargo audit` (cek RustSec DB) dan `cargo clippy` (banyak lint yang menangkap footgun async/concurrency sebelum jadi vuln).