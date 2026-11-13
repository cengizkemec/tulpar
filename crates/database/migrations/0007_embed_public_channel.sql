-- Phase 8: optional public channel preview for embed applications.

ALTER TABLE embed_apps
    ADD COLUMN public_channel_id uuid REFERENCES channels(id) ON DELETE SET NULL;
