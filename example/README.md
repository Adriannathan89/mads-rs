# MADS 0.9 examples

These are three independent Rust projects. Run each command from its example
directory so `Mads::run` loads that project's `mads.toml` and optional `.env`.
Each project targets MADS 0.9.1 crates from crates.io; none uses
workspace path dependencies or the removed `mads db` commands.

| Project | What it demonstrates | Port |
| --- | --- | --- |
| [hello-world](hello-world/) | The smallest MADS HTTP application | 3000 |
| [posts-crud](posts-crud/) | PostgreSQL, `mads-persistence`, SeaORM, and post CRUD | 3001 |
| [protected-route](protected-route/) | TPRS, validated login, Passport JWT guard, and logger | 3002 |

Start with Hello World, then use the PostgreSQL example when you need a real
database. The protected-route example is self-contained and needs no database.
Each directory contains its own setup steps and `curl` requests.

The [TPRS reference project](https://github.com/Adriannathan89/mads-rs-example)
defines Trait–Provider–Repository–Service: traits express application contracts,
providers bind them to implementations, repositories handle data access, and
services implement use cases. Controllers are the HTTP boundary. The JWT
example follows that pattern with an in-memory repository. The reference uses
MADS 0.8 APIs, so these projects use the current 0.9 crate boundaries.
