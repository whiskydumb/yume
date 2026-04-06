ALTER TABLE sites DROP CONSTRAINT sites_position_unique;
ALTER TABLE sites ADD CONSTRAINT sites_position_unique UNIQUE (position) DEFERRABLE INITIALLY DEFERRED;