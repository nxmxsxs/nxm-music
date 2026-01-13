-- Add up migration script here
CREATE TABLE tracks (
    id BLOB PRIMARY KEY NOT NULL,
    title TEXT NOT NULL
);
