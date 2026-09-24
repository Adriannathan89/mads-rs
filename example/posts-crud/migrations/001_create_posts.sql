CREATE TABLE IF NOT EXISTS posts (
    id integer GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    title text NOT NULL,
    body text NOT NULL
);
