-- Add up migration script here
CREATE TABLE artists (
    id BLOB PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    UNIQUE (name)
);
