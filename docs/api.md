# FreeDrive API

Base path: `/api/v1`. Desktop-oriented notes: [desktop-api.md](desktop-api.md).

## Public auth

- `POST /auth/register`
- `POST /auth/login` - tokens, or `{ requires_2fa, challenge_id, email_masked }` when 2FA is required
- `POST /auth/verify-2fa` - `{ challenge_id, code }`
- `POST /auth/refresh`
- `POST /auth/logout`
- `POST /auth/forgot-password` - `{ email }` (SMTP; generic response)
- `POST /auth/reset-password` - `{ token, email, new_password, crypto_update? }`
- `POST /auth/reset-password/crypto-info` - `{ token, email }`
- `POST /auth/confirm-email`

## Protected

- `GET|PATCH /me`, `POST /me/email-change/request`, `GET /me/email-change/status`
- `GET /me/storage`, `GET /activity`, `GET /disk-stats`
- `GET /auth/sessions`, `DELETE /auth/sessions/{id}`, `POST /auth/sessions/revoke-others`
- `GET /search`, `GET /approvals`, `PATCH /approvals/{id}`

### Encryption

Server stores wrapped keys only; clients derive UEK from the password.

- `GET|POST|PUT /crypto/account`
- `GET /encryption-keys`, `POST /encryption-keys/bulk`
- `GET|PUT /files/{id}/encryption-key`

### Shares

Permissions: `viewer` / `commenter` (read), `editor` (write).

- `GET /shares/with-me`, `GET /shares/by-me`
- `POST|PATCH|DELETE /shares/users` / `/shares/users/{id}`
- `GET|POST|DELETE /shares/links` / `/shares/links/{id}`

### Files

- `POST /files/upload`, `GET /files`, `GET /files/trash`, `GET /files/{id}`
- `GET|POST|DELETE /files/{id}/comments` / `.../comments/{commentId}`
- `POST /files/{id}/approvals`
- `GET /files/{id}/download`, `PATCH /files/{id}`, `POST /files/{id}/content`
- `DELETE /files/{id}`, `POST /files/{id}/restore`, `DELETE /files/{id}/permanent`
- `GET /files/{id}/versions`, `POST /files/{id}/versions/{version}/restore`

### Folders

- `POST /folders`, `GET /folders/root` (paginated: `page_size`, `page_token`), `GET /folders/all`, `GET /folders/trash`
- `GET|PATCH|DELETE /folders/{id}`, `POST /folders/{id}/restore`, `DELETE /folders/{id}/permanent`
- `GET /folders/{id}/breadcrumb`

### Computers

- `GET /computers`, `GET /computers/{id}`, `POST /computers/register`, `POST /computers/{id}/heartbeat`

## Public (no auth)

- `GET /public/share/{token}`, `GET /public/share/{token}/download` (`?password=` if protected)

## Admin (role `admin`)

- Users: `GET|POST /admin/users`, `PATCH|DELETE /admin/users/{id}`, reset-password, revoke-sessions, send-2fa-reminder
- `POST /admin/sessions/revoke-all`, `GET /admin/stats`
- Invites: `POST|GET /admin/invites`, `POST /admin/invites/resend`, `DELETE /admin/invites/{id}`
- `GET /admin/activity`, `GET|POST /admin/settings`, `POST /admin/test-email`
- Backup: run / list / download / restore / delete
- Storage: `POST /admin/storage/purge-trash?days=30`, duplicates list/purge, `POST /admin/danger/wipe`

## Health

- `GET /health`
