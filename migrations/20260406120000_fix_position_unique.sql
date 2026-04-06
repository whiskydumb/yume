WITH numbered AS (
    SELECT id, ROW_NUMBER() OVER (ORDER BY position ASC, created_at ASC) AS new_pos
    FROM sites
)
UPDATE sites
SET position = numbered.new_pos
FROM numbered
WHERE sites.id = numbered.id;

ALTER TABLE sites ADD CONSTRAINT sites_position_unique UNIQUE (position);
