-- Add up migration script here
CREATE TABLE track_genres (
    track_id BLOB NOT NULL REFERENCES tracks (id) ON DELETE CASCADE,
    genre_id INTEGER NOT NULL REFERENCES genres (id) ON DELETE CASCADE,
    PRIMARY KEY (track_id, genre_id)
);

CREATE INDEX idx_track_genres_genre ON track_genres (genre_id);
