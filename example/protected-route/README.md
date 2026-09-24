# Protected route, validation, and logger

This independent MADS 0.9 project demonstrates the
[TPRS pattern](https://github.com/Adriannathan89/mads-rs-example): Trait,
Provider, Repository, Service. `src/auth/traits.rs` declares the contracts;
`provider.rs` binds them to concrete types; `repository.rs` holds one demo user
in memory; `service.rs` signs a JWT and resolves identity; `controller.rs`
handles HTTP. Passport verifies the Bearer JWT before the guarded handler runs.

Requires Rust 1.94 or newer. From this directory:

```sh
cp .env.example .env
cargo run
```

The `.env` file supplies a demo username/password and a JWT signing secret via
`mads.toml` interpolation. The secret must contain at least 32 bytes for HS256.
MADS loads `.env` only for this local run and does not commit it.

Request a token with valid input:

```sh
curl -i -X POST http://127.0.0.1:3002/auth/login \
  -H 'Content-Type: application/json' \
  -d '{"username":"demo","password":"correct-horse-battery-staple"}'
```

Copy the `access_token` value from the JSON response and call the protected
route:

```sh
curl -i http://127.0.0.1:3002/auth/me \
  -H 'Authorization: Bearer YOUR_ACCESS_TOKEN'
```

Try the failure paths too: omit the header for a 401, use the wrong password
for a 401, or send an empty username/short password for a 422. `ValidatedJson`
rejects invalid input before `login` runs. The guard verifies the token and
requires the `reader` role. `LoggerModule` provides the default console logger;
the service logs login outcomes and the controller logs protected reads without
printing passwords or tokens.

This intentionally stores a plaintext demo password in memory. It has no user
registration, password hashing, token revocation, or durable sessions. For a
real application, replace the repository with persistent users, store password
hashes, and apply your session and rotation policy.
