-- Add up migration script here
CREATE TABLE IF NOT EXISTS tracks (
    id          BLOB(16) NOT NULL,

    filenode_id BLOB(16) NOT NULL,

    artist      TEXT,
    title       TEXT,

    PRIMARY KEY (id),

    FOREIGN KEY (filenode_id) REFERENCES filenodes (id) ON DELETE CASCADE
);
