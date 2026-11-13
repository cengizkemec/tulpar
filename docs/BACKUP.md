# Tulpar Backup and Restore

**Status:** Phase 9 scripts and documentation are implemented. A clean-host restore drill is still required before a production release.

**Rule:** an untested backup is not a backup. Every restore procedure in this document must be exercised before a production release and periodically afterward.

---

## 1. Scope

| Data | Backup required | Method |
|---|---|---|
| PostgreSQL database | Yes | `pg_dump -Fc` nightly; optional pgBackRest/WAL for PITR |
| Local uploaded files | Yes | Filesystem snapshot, restic, or object-storage replication |
| S3/MinIO objects | Yes | Bucket versioning + replication/backup |
| Instance configuration | Yes | Versioned config without secrets |
| Secrets | Yes, separately | Encrypted offline store or secret manager |
| Caddy certificates/ACME state | Recommended | Volume backup; can be reissued |
| LiveKit/Coturn config | Yes | Git/templates; secrets separately |
| Redis | No | Ephemeral; sessions/presence/tickets rebuild |
| Monitoring history | Optional | Grafana/Prometheus volumes |
| Audit logs | Yes | Part of PostgreSQL |
| Desktop client data | No | User's device; tokens in OS keychain |

Backups contain personal data, private messages, password hashes, and possibly credentials. They are as sensitive as the live database.

---

## 2. Objectives

Default targets for a small self-hosted instance:

| Objective | Target |
|---|---|
| Recovery point objective (RPO) | 24 hours with nightly backups; 15 minutes with WAL archiving |
| Recovery time objective (RTO) | 1–2 hours for a single-VPS restore |
| Backup retention | 14 daily, 8 weekly, 12 monthly (configurable) |
| Restore test | At least quarterly and before every major upgrade |
| Off-host copy | Required; a backup on the same disk is not sufficient |

Operators with stricter requirements enable WAL archiving, more frequent dumps, and cross-region object replication.

---

## 3. Backup schedule

| Job | Frequency | Retention | Notes |
|---|---|---|---|
| PostgreSQL logical dump | Daily, low-traffic window | 14 daily + 8 weekly | `pg_dump -Fc`, compressed |
| PostgreSQL WAL archive | Continuous (optional) | 14 days | pgBackRest or WAL-G; enables PITR |
| Local uploads | Daily incremental, weekly full | 30 days | restic/rclone or filesystem snapshot |
| S3/MinIO | Continuous versioning + daily replication | Per bucket policy | Object lock optional |
| Config | On every change | Git history | No secrets |
| Secrets | On rotation | Encrypted offline | Separate from data backups |
| Restore test | Quarterly | Record result | Use an isolated environment |

Schedule backups outside peak voice/message activity. A logical dump adds database load; monitor it.

---

## 4. Backup storage

- Off-host: a different provider, region, or physical location.
- Encrypted at rest and in transit.
- Access limited to a dedicated backup identity; not the application database user.
- Immutable/object-lock retention if the provider supports it, to resist ransomware.
- Checksums recorded for every artifact.
- Separate credentials from the production server where possible.
- Do not store backups in a public bucket or a web-accessible directory.
- Test restore from the off-host copy, not just the local one.

Suggested layout:

```
backups/
  postgres/2026/11/tulpar-20261112.dump.zst
  uploads/2026/11/...
  config/...
  secrets/...          (separate encrypted store)
  manifests/20261112.json
```

---

## 5. PostgreSQL backup

### 5.1 Logical dump (default)

```bash
pg_dump \
  --format=custom \
  --compress=9 \
  --no-owner \
  --no-privileges \
  --file=/backups/postgres/tulpar-$(date +%Y%m%dT%H%M%S).dump \
  "$DATABASE_URL"
```

Then encrypt and copy off-host. Verify with `pg_restore --list` and a restore into a scratch database.

Considerations:

- `pg_dump` takes a consistent snapshot without blocking normal reads/writes for most workloads.
- It does not include roles or database-level grants by default; document the required role setup.
- Large databases take time and I/O; schedule accordingly.
- Extensions must be available on the restore target.
- Migrations table is included; the restored schema matches the release that created it.

### 5.2 Point-in-time recovery (optional)

Use pgBackRest or WAL-G with continuous WAL archiving. This enables recovery to a specific timestamp, which is valuable for accidental deletions or a bad migration.

Requirements:

- An archive repository with retention.
- A base backup schedule.
- A tested `restore` procedure.
- Monitoring for archive failures; a broken archive is a silent risk.

### 5.3 Verification

- `pg_restore --list` succeeds.
- Restore into an isolated PostgreSQL 16 instance.
- Run `SELECT count(*)` on key tables and a few application smoke queries.
- Start a Tulpar instance against the restored database in a sandbox and verify login, channel load, and message read.
- Record the duration and any errors.

### 5.4 Delivered scripts

```bash
# PostgreSQL custom dump + uploads volume archive + SHA-256 checksums + retention
sudo TULPAR_BACKUP_DIR=/var/backups/tulpar scripts/backup.sh

# Verify checksums, restore the database, and replace the uploads volume
sudo scripts/restore.sh --yes /var/backups/tulpar/<timestamp>
```

The scripts use the production Compose file and `.env` by default; override with
`TULPAR_COMPOSE_FILE`, `TULPAR_ENV_FILE`, `TULPAR_UPLOADS_VOLUME`,
`TULPAR_BACKUP_DIR`, and `TULPAR_BACKUP_RETENTION_DAYS`.

---

## 6. Object storage and local uploads

### 6.1 Local filesystem

- Stop writes or use a filesystem snapshot consistent with the database backup.
- Back up the upload directory with restic/rclone/tar.
- Preserve file metadata (names are generated keys; permissions matter).
- Encrypt the repository.
- Verify a sample of files after restore; compare hashes if available.

### 6.2 S3/MinIO

- Enable bucket versioning.
- Enable replication to a separate bucket/region if available.
- Use lifecycle rules for old versions.
- Back up bucket policy and configuration separately.
- Restore objects and re-run attachment integrity checks.
- Never make the bucket public to simplify restore.

### 6.3 Consistency between database and files

A database dump and a file backup taken at slightly different times can reference missing files. Mitigations:

- Take the file snapshot before/after the database dump and keep the window small.
- Keep soft-deleted files for a grace period so a slightly older database can still resolve attachments.
- Run an integrity job after restore: for every `attachments.file_id`, verify the object exists; missing files are reported, not silently ignored.
- Prefer object storage versioning to recover overwritten/deleted objects.

---

## 7. Configuration and secrets

- Configuration files are versioned in Git without secrets.
- Secrets are stored in a secret manager or an encrypted offline archive.
- Back up the mapping of secret names to environments, not just values.
- The JWT signing key, embed signing key, webhook encryption key, LiveKit secret, TURN secret, SMTP password, S3 credentials, and database/Redis passwords are required for a full restore.
- Rotate secrets after a suspected backup compromise.
- Do not store secrets in the same archive as the database dump.

---

## 8. Redis

Redis is not backed up as durable state. After data loss:

- Sessions and refresh tokens are in PostgreSQL; users may need to re-authenticate if gateway tickets/rate-limit state is lost.
- Presence is rebuilt from new connections.
- Typing indicators disappear.
- Voice projection is rebuilt from LiveKit webhooks/reconciliation.
- Rate limits reset, which is briefly weaker; monitor for abuse.

If Redis is configured with AOF for operational convenience, it is still treated as disposable. Do not use it to store messages or files.

---

## 9. Restore procedure

### 9.1 Preparation

1. Provision a clean host or isolated environment with the same major versions of PostgreSQL, Redis, LiveKit, Coturn, and Tulpar.
2. Install the matching Tulpar release.
3. Restore secrets/config.
4. Restore the database before starting the API.
5. Restore files/objects.
6. Start dependencies, run migrations only if the release requires it, then start Tulpar.
7. Verify.

### 9.2 Database restore

```bash
createdb tulpar
pg_restore \
  --dbname=tulpar \
  --no-owner \
  --no-privileges \
  --clean --if-exists \
  /backups/postgres/tulpar-YYYYMMDD.dump
```

- Prefer restoring into a fresh database, then switching the connection string.
- Do not run `--clean` against a live database without a verified backup.
- Check extension availability (`citext`, `pg_trgm`).
- Verify the `_sqlx_migrations` table matches the release.
- Run `ANALYZE` after restore for query planning.
- If the restored schema is older than the binary, run the release's migration command; if newer, use the matching binary.

### 9.3 File restore

- Restore the upload directory or S3 objects to the expected paths/bucket.
- Verify permissions and ownership.
- Run the attachment integrity check.
- Regenerate thumbnails if variants were not backed up.

### 9.4 Cutover

- Update DNS or the reverse proxy to the restored instance only after smoke tests.
- Keep the old instance read-only or offline to avoid split writes.
- Revoke sessions if the restore may have rolled back security state.
- Announce the recovery and any expected data loss window.

### 9.5 Smoke tests

- `/health/live` and `/health/ready`.
- Admin login.
- User login and refresh.
- Community/channel load.
- Message history and send.
- Upload/download.
- Voice token issuance and a real join.
- WebSocket ticket and event delivery.
- Audit log query.
- Background workers/outbox progress.

---

## 10. Disaster scenarios

| Scenario | Response |
|---|---|
| Accidental message/community deletion | Soft-delete grace period; restore from PITR if beyond grace |
| Bad migration | Stop, restore pre-upgrade backup, fix migration, retry |
| Disk failure | Rebuild host, restore database and files, verify |
| Ransomware | Isolate host, use immutable off-host backups, rotate all secrets, rebuild |
| Compromised database | Restore clean backup, rotate credentials, audit access, notify |
| Lost Redis | Restart; expect presence/ticket reset; no message loss |
| Lost LiveKit/Coturn config | Re-deploy from Git/templates and secrets; voice recovers |
| Lost encryption keys | Backups may be unrecoverable; keys are part of disaster recovery |
| Cloud region outage | Restore in another region from replicated backups |

---

## 11. Security of backups

- Encrypt at rest and in transit.
- Restrict restore permissions to named operators.
- Log backup/restore access.
- Use object lock/immutability where possible.
- Keep at least one copy offline or in a separate trust domain.
- Test that a compromised application credential cannot delete all backups.
- Do not email backups or put them in chat.
- Treat a leaked backup as a full data breach: rotate secrets, notify, investigate.

---

## 12. Monitoring

Alert on:

- Backup job failure or missed schedule.
- Backup size anomaly (too small/large).
- Restore test failure.
- Off-host copy lag.
- Disk usage on the backup target.
- Encryption/key access failure.
- WAL archive gaps (if PITR enabled).

A dashboard should show the last successful backup, last restore test, RPO, and retention counts.

---

## 13. Responsibilities and runbook ownership

- The instance operator owns backups and restore tests.
- The Tulpar project provides scripts, documentation, and integrity checks, not a managed backup service.
- The operator records:
  - backup locations and credentials owner;
  - retention policy;
  - last restore test date/result;
  - RPO/RTO;
  - legal retention requirements.
- A restore drill is part of every major upgrade checklist.

---

## 14. Open backup decisions

| Decision | Current position | Needed by |
|---|---|---|
| Default backup tool | `pg_dump` + restic/S3; pgBackRest optional | Phase 9 |
| PITR default | Off; documented optional | Phase 9 |
| Retention | 14 daily / 8 weekly / 12 monthly | Phase 9 |
| Backup encryption | Mandatory; age/GPG or provider KMS | Phase 9 |
| Restore test cadence | Quarterly + before major upgrades | Phase 9 |
| Managed backup service | None; operator choice | Post-v1 |
| Legal retention | Needs policy review | Before public release |
