# Posts CRUD with PostgreSQL

This independent MADS 0.9 project connects to PostgreSQL through the opt-in
`mads-persistence` SeaORM connector. `DatabaseModule` supplies the native
`DatabaseConnection` to `PostRepository`; `PostService` handles the use case and
`PostController` exposes the HTTP routes. The connector checks readiness before
the HTTP listener binds and closes the connection during graceful shutdown.

Requires Rust 1.94 or newer, PostgreSQL, and `psql`. From this directory:

```sh
createdb -h 127.0.0.1 -U postgres mads_posts_example
cp .env.example .env
# Edit DATABASE_URL in .env for your local PostgreSQL credentials.
# Run psql with the same URL (the .env file is loaded by MADS, not by psql).
psql 'postgres://postgres:postgres@127.0.0.1:5432/mads_posts_example' -f migrations/001_create_posts.sql
cargo run
```

The `mads db` CLI commands were removed in 0.9. Apply the SQL file with `psql`
before starting this example. `mads.toml` reads `DATABASE_URL` through dotenv
interpolation; `.env` is ignored by Git.

Try the CRUD routes in a second terminal:

```sh
curl -i -X POST http://127.0.0.1:3001/posts \
  -H 'Content-Type: application/json' \
  -d '{"title":"First post","body":"Hello from MADS"}'
curl http://127.0.0.1:3001/posts
curl http://127.0.0.1:3001/posts/1
curl -X PUT http://127.0.0.1:3001/posts/1 \
  -H 'Content-Type: application/json' \
  -d '{"title":"Updated post","body":"New body"}'
curl -i -X DELETE http://127.0.0.1:3001/posts/1
```

The create route returns 201; delete returns 204. A missing post returns 404.
`ValidatedJson<PostInput>` rejects an empty or too-long title before the
controller runs. Database failures become redacted 500 responses, while the
underlying error remains available to the server for diagnosis.
