-- Add up migration script here
CREATE TABLE genres (
id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
name TEXT NOT NULL,
UNIQUE (name)
) ;
