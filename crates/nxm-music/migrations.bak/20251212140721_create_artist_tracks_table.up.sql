-- Add up migration script here
CREATE TABLE artist_tracks (
    artist_id BLOB NOT NULL REFERENCES artists (id) ON DELETE CASCADE,
    track_id BLOB NOT NULL REFERENCES tracks (id) ON DELETE CASCADE,
    role INTEGER NOT NULL CHECK (
        -- 1=Creator
        -- 2=Feature
        -- 3=Remixer
        -- 4=Vocalist
        -- 5=Composer
        role IN (1, 2, 3, 4, 5)
    ),
    PRIMARY KEY (artist_id, track_id, role)
);

CREATE INDEX idx_artist_tracks_artist ON artist_tracks (artist_id);
CREATE INDEX idx_artist_tracks_track ON artist_tracks (track_id);
