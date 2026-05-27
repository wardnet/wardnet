-- Add optional description field to DNS filter profiles.
-- Existing rows (including custom profiles) default to NULL.

ALTER TABLE dns_filter_profiles ADD COLUMN description TEXT;

UPDATE dns_filter_profiles
SET description = 'Blocks ads, trackers, and unwanted content network-wide.'
WHERE id = '00000000-0000-0000-0000-000000000100';

UPDATE dns_filter_profiles
SET description = 'Filters adult content, social media, and distracting sites — ideal for kids'' devices.'
WHERE id = '00000000-0000-0000-0000-000000000101';

UPDATE dns_filter_profiles
SET description = 'Blocks known malware distribution and phishing domains to protect every device.'
WHERE id = '00000000-0000-0000-0000-000000000102';
