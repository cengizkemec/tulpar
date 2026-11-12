# Scripts

Planned development and operations scripts:

- Toolchain/bootstrap verification.
- Database migration wrapper.
- Development stack start/stop.
- Backup and restore.
- Release checksum/signing helpers.
- Diagnostic collection with secret redaction.

Scripts must be tested on the target OS before being documented as working. They must never embed secrets or require them as command-line arguments where shell history would capture them. See [docs/CONTRIBUTING.md](../docs/CONTRIBUTING.md) and [docs/BACKUP.md](../docs/BACKUP.md).
