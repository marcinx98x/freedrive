package sqlite

import (
	"context"
	"database/sql"
	"fmt"
	"time"

	"github.com/abdullaabdullazade/freedrive/internal/domain"
	"github.com/google/uuid"
)

// UserRepo implements repository.UserRepository with SQLite.
type UserRepo struct {
	writer *sql.DB
	reader *sql.DB
}

// NewUserRepo creates a new user repository.
func NewUserRepo(db *DB) *UserRepo {
	return &UserRepo{writer: db.Writer, reader: db.Reader}
}

func (r *UserRepo) Create(ctx context.Context, user *domain.User) error {
	if user.ID == "" {
		user.ID = uuid.New().String()
	}
	now := time.Now()
	user.CreatedAt = now
	user.UpdatedAt = now

	_, err := r.writer.ExecContext(ctx,
		`INSERT INTO users (id, email, username, password_hash, role, quota_bytes, used_bytes, avatar_url, created_at, updated_at)
		 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
		user.ID, user.Email, user.Username, user.PasswordHash, user.Role,
		user.QuotaBytes, user.UsedBytes, user.AvatarURL, user.CreatedAt, user.UpdatedAt,
	)
	if err != nil {
		return fmt.Errorf("create user: %w", err)
	}
	return nil
}

func (r *UserRepo) GetByID(ctx context.Context, id string) (*domain.User, error) {
	user := &domain.User{}
	err := r.reader.QueryRowContext(ctx,
		`SELECT id, email, username, password_hash, role, quota_bytes, used_bytes, avatar_url, created_at, updated_at, last_login_at
		 FROM users WHERE id = ?`, id,
	).Scan(&user.ID, &user.Email, &user.Username, &user.PasswordHash, &user.Role,
		&user.QuotaBytes, &user.UsedBytes, &user.AvatarURL, &user.CreatedAt, &user.UpdatedAt, &user.LastLoginAt)
	if err == sql.ErrNoRows {
		return nil, nil
	}
	if err != nil {
		return nil, fmt.Errorf("get user by id: %w", err)
	}
	return user, nil
}

func (r *UserRepo) GetByEmail(ctx context.Context, email string) (*domain.User, error) {
	user := &domain.User{}
	err := r.reader.QueryRowContext(ctx,
		`SELECT id, email, username, password_hash, role, quota_bytes, used_bytes, avatar_url, created_at, updated_at, last_login_at
		 FROM users WHERE email = ? COLLATE NOCASE`, email,
	).Scan(&user.ID, &user.Email, &user.Username, &user.PasswordHash, &user.Role,
		&user.QuotaBytes, &user.UsedBytes, &user.AvatarURL, &user.CreatedAt, &user.UpdatedAt, &user.LastLoginAt)
	if err == sql.ErrNoRows {
		return nil, nil
	}
	if err != nil {
		return nil, fmt.Errorf("get user by email: %w", err)
	}
	return user, nil
}

func (r *UserRepo) Update(ctx context.Context, user *domain.User) error {
	user.UpdatedAt = time.Now()
	_, err := r.writer.ExecContext(ctx,
		`UPDATE users SET email=?, username=?, password_hash=?, role=?, quota_bytes=?, used_bytes=?, avatar_url=?, updated_at=?, last_login_at=?
		 WHERE id=?`,
		user.Email, user.Username, user.PasswordHash, user.Role,
		user.QuotaBytes, user.UsedBytes, user.AvatarURL, user.UpdatedAt, user.LastLoginAt, user.ID,
	)
	if err != nil {
		return fmt.Errorf("update user: %w", err)
	}
	return nil
}

func (r *UserRepo) Delete(ctx context.Context, id string) error {
	_, err := r.writer.ExecContext(ctx, "DELETE FROM users WHERE id = ?", id)
	if err != nil {
		return fmt.Errorf("delete user: %w", err)
	}
	return nil
}

func (r *UserRepo) List(ctx context.Context) ([]domain.User, error) {
	rows, err := r.reader.QueryContext(ctx,
		`SELECT id, email, username, password_hash, role, quota_bytes, used_bytes, avatar_url, created_at, updated_at, last_login_at
		 FROM users ORDER BY created_at DESC`,
	)
	if err != nil {
		return nil, fmt.Errorf("list users: %w", err)
	}
	defer rows.Close()

	var users []domain.User
	for rows.Next() {
		var u domain.User
		if err := rows.Scan(&u.ID, &u.Email, &u.Username, &u.PasswordHash, &u.Role,
			&u.QuotaBytes, &u.UsedBytes, &u.AvatarURL, &u.CreatedAt, &u.UpdatedAt, &u.LastLoginAt); err != nil {
			return nil, fmt.Errorf("scan user: %w", err)
		}
		users = append(users, u)
	}
	return users, nil
}

func (r *UserRepo) UpdateUsedBytes(ctx context.Context, userID string, delta int64) error {
	_, err := r.writer.ExecContext(ctx,
		"UPDATE users SET used_bytes = MAX(0, used_bytes + ?) WHERE id = ?", delta, userID,
	)
	if err != nil {
		return fmt.Errorf("update used bytes: %w", err)
	}
	return nil
}

func (r *UserRepo) Count(ctx context.Context) (int, error) {
	var count int
	err := r.reader.QueryRowContext(ctx, "SELECT COUNT(*) FROM users").Scan(&count)
	return count, err
}

// --- Refresh Tokens ---

func (r *UserRepo) CreateRefreshToken(ctx context.Context, token *domain.RefreshToken) error {
	if token.ID == "" {
		token.ID = uuid.New().String()
	}
	token.CreatedAt = time.Now()

	_, err := r.writer.ExecContext(ctx,
		`INSERT INTO refresh_tokens (id, user_id, token_hash, expires_at, created_at)
		 VALUES (?, ?, ?, ?, ?)`,
		token.ID, token.UserID, token.TokenHash, token.ExpiresAt, token.CreatedAt,
	)
	return err
}

func (r *UserRepo) GetRefreshToken(ctx context.Context, tokenHash string) (*domain.RefreshToken, error) {
	t := &domain.RefreshToken{}
	err := r.reader.QueryRowContext(ctx,
		"SELECT id, user_id, token_hash, expires_at, created_at FROM refresh_tokens WHERE token_hash = ?",
		tokenHash,
	).Scan(&t.ID, &t.UserID, &t.TokenHash, &t.ExpiresAt, &t.CreatedAt)
	if err == sql.ErrNoRows {
		return nil, nil
	}
	return t, err
}

func (r *UserRepo) DeleteRefreshToken(ctx context.Context, tokenHash string) error {
	_, err := r.writer.ExecContext(ctx, "DELETE FROM refresh_tokens WHERE token_hash = ?", tokenHash)
	return err
}

func (r *UserRepo) DeleteUserRefreshTokens(ctx context.Context, userID string) error {
	_, err := r.writer.ExecContext(ctx, "DELETE FROM refresh_tokens WHERE user_id = ?", userID)
	return err
}

// --- Invite Links ---

func (r *UserRepo) CreateInvite(ctx context.Context, invite *domain.InviteLink) error {
	if invite.ID == "" {
		invite.ID = uuid.New().String()
	}
	invite.CreatedAt = time.Now()

	_, err := r.writer.ExecContext(ctx,
		`INSERT INTO invite_links (id, code, created_by, role, quota_bytes, max_uses, used_count, expires_at, created_at)
		 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`,
		invite.ID, invite.Code, invite.CreatedBy, invite.Role, invite.QuotaBytes,
		invite.MaxUses, invite.UsedCount, invite.ExpiresAt, invite.CreatedAt,
	)
	return err
}

func (r *UserRepo) GetInviteByCode(ctx context.Context, code string) (*domain.InviteLink, error) {
	inv := &domain.InviteLink{}
	err := r.reader.QueryRowContext(ctx,
		`SELECT id, code, created_by, role, quota_bytes, max_uses, used_count, expires_at, created_at
		 FROM invite_links WHERE code = ?`, code,
	).Scan(&inv.ID, &inv.Code, &inv.CreatedBy, &inv.Role, &inv.QuotaBytes,
		&inv.MaxUses, &inv.UsedCount, &inv.ExpiresAt, &inv.CreatedAt)
	if err == sql.ErrNoRows {
		return nil, nil
	}
	return inv, err
}

func (r *UserRepo) IncrementInviteUsage(ctx context.Context, id string) error {
	_, err := r.writer.ExecContext(ctx,
		"UPDATE invite_links SET used_count = used_count + 1 WHERE id = ?", id)
	return err
}

func (r *UserRepo) ListInvites(ctx context.Context) ([]domain.InviteLink, error) {
	rows, err := r.reader.QueryContext(ctx,
		`SELECT id, code, created_by, role, quota_bytes, max_uses, used_count, expires_at, created_at
		 FROM invite_links ORDER BY created_at DESC`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var invites []domain.InviteLink
	for rows.Next() {
		var inv domain.InviteLink
		if err := rows.Scan(&inv.ID, &inv.Code, &inv.CreatedBy, &inv.Role, &inv.QuotaBytes,
			&inv.MaxUses, &inv.UsedCount, &inv.ExpiresAt, &inv.CreatedAt); err != nil {
			return nil, err
		}
		invites = append(invites, inv)
	}
	return invites, nil
}

func (r *UserRepo) DeleteInvite(ctx context.Context, id string) error {
	_, err := r.writer.ExecContext(ctx, "DELETE FROM invite_links WHERE id = ?", id)
	return err
}
