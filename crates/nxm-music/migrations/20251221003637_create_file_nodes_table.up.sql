CREATE TABLE IF NOT EXISTS filenodes (
    id        BLOB(16) NOT NULL,

    inode     INTEGER  NOT NULL,
    device    INTEGER  NOT NULL,

    parent_id BLOB(16),
    name      TEXT     NOT NULL,

    mtime     INTEGER  NOT NULL,
    size      INTEGER  NOT NULL,

    node_type CHAR     NOT NULL CHECK (node_type IN ('F', 'D')),

    -- audio_hash BLOB(32),
    -- meta_hash  BLOB(32),
    -- node_hash  BLOB(32),

    PRIMARY KEY (id),
    FOREIGN KEY (parent_id) REFERENCES filenodes (id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_filenodes_name_parent ON filenodes (parent_id, name);
