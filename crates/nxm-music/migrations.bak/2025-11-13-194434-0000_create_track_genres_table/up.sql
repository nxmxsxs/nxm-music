CREATE TABLE track_genres (
    track_id BLOB NOT NULL REFERENCES tracks (id) ON DELETE CASCADE,
    genre_id INTEGER NOT NULL REFERENCES genres (id) ON DELETE CASCADE,
    PRIMARY KEY (track_id, genre_id)
);
