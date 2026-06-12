-- Rename the "remote_access" setup-wizard step to "remote_access".
--
-- The HTTPS/DDNS step is being rebranded as "Remote access" (the surface that
-- will also host the inbound VPN tunnel), which renames the `WizardStep`
-- variant and therefore its persisted string. A DB left mid-wizard on the old
-- value would no longer parse after the rename, so migrate any in-flight value.
UPDATE system_config
   SET value = 'remote_access'
 WHERE key = 'wizard_step'
   AND value = 'remote_access';
