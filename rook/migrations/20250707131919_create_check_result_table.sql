CREATE TABLE repo (
    id SERIAL PRIMARY KEY,
    repo_url TEXT NOT NULL,
    branch TEXT,
    checked_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE TABLE check_result (
    id SERIAL PRIMARY KEY,
    repo_id INTEGER NOT NULL REFERENCES repo(id) ON DELETE CASCADE,
    file_path TEXT NOT NULL,
    line_number INTEGER NOT NULL,
    old_content TEXT NOT NULL,
    new_content TEXT NOT NULL
);
