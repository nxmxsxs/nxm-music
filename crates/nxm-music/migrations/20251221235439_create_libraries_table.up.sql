-- Add up migration script here
CREATE TABLE IF NOT EXISTS libraries (
    id   BLOB(16) NOT NULL,
    path TEXT     NOT NULL,
    node BLOB(16),

    PRIMARY KEY (id),
    FOREIGN KEY (node) REFERENCES filenodes (id) ON DELETE SET NULL
);
