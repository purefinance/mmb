INSERT INTO settings(code, content)
VALUES ($1, $2)
ON CONFLICT (code) DO UPDATE SET content = $2