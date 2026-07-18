package service_test

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"testing"
	"time"

	"github.com/abdullaabdullazade/freedrive/internal/domain"
	"github.com/abdullaabdullazade/freedrive/internal/repository/sqlite"
	"github.com/abdullaabdullazade/freedrive/internal/service"
	"golang.org/x/crypto/bcrypt"
)

func sha256Hex(s string) string {
	sum := sha256.Sum256([]byte(s))
	return hex.EncodeToString(sum[:])
}

func newTestAuth(t *testing.T) (*service.AuthService, *sqlite.UserRepo, *sqlite.DB) {
	t.Helper()
	db, err := sqlite.New(t.TempDir())
	if err != nil {
		t.Fatalf("open db: %v", err)
	}
	if err := db.Migrate(); err != nil {
		t.Fatalf("migrate: %v", err)
	}
	userRepo := sqlite.NewUserRepo(db)
	auth := service.NewAuthService(userRepo, sqlite.NewEmail2FARepo(db), sqlite.NewSessionRepo(db), "test-secret-key-at-least-32-bytes!!")
	return auth, userRepo, db
}

func createTestUser(t *testing.T, userRepo *sqlite.UserRepo, id, email string) *domain.User {
	t.Helper()
	hash, err := bcrypt.GenerateFromPassword([]byte("password1"), bcrypt.MinCost)
	if err != nil {
		t.Fatalf("hash: %v", err)
	}
	user := &domain.User{
		ID:           id,
		Email:        email,
		Username:     id,
		PasswordHash: string(hash),
		Role:         domain.RoleUser,
		QuotaBytes:   1 << 30,
	}
	if err := userRepo.Create(context.Background(), user); err != nil {
		t.Fatalf("create user: %v", err)
	}
	return user
}

func TestIssueTokensCreatesSessionWithSID(t *testing.T) {
	auth, userRepo, db := newTestAuth(t)
	defer db.Close()
	ctx := context.Background()
	user := createTestUser(t, userRepo, "user-a", "a@test.local")

	tokens, err := auth.IssueTokens(ctx, user, service.DeviceInfo{
		DeviceName: "Chrome on Windows",
		DeviceType: domain.DeviceTypeWeb,
		IPAddress:  "127.0.0.1",
	})
	if err != nil {
		t.Fatalf("issue tokens: %v", err)
	}

	claims, err := auth.ValidateAccessToken(tokens.AccessToken)
	if err != nil {
		t.Fatalf("validate: %v", err)
	}
	if claims.SessionID == "" {
		t.Fatal("expected sid claim")
	}
	if err := auth.EnsureSessionActive(ctx, claims.SessionID); err != nil {
		t.Fatalf("session should be active: %v", err)
	}

	sessions, err := auth.ListSessions(ctx, user.ID)
	if err != nil {
		t.Fatalf("list: %v", err)
	}
	if len(sessions) != 1 {
		t.Fatalf("expected 1 session, got %d", len(sessions))
	}
	if sessions[0].DeviceName != "Chrome on Windows" {
		t.Fatalf("device name = %q", sessions[0].DeviceName)
	}
}

func TestRefreshKeepsSameSessionID(t *testing.T) {
	auth, userRepo, db := newTestAuth(t)
	defer db.Close()
	ctx := context.Background()
	user := createTestUser(t, userRepo, "user-b", "b@test.local")

	tokens, err := auth.IssueTokens(ctx, user, service.DeviceInfo{
		DeviceName: "Desktop",
		DeviceType: domain.DeviceTypeDesktop,
	})
	if err != nil {
		t.Fatalf("issue: %v", err)
	}
	before, err := auth.ValidateAccessToken(tokens.AccessToken)
	if err != nil {
		t.Fatalf("validate: %v", err)
	}

	refreshed, err := auth.Refresh(ctx, tokens.RefreshToken, service.DeviceInfo{
		DeviceName: "Desktop",
		DeviceType: domain.DeviceTypeDesktop,
	})
	if err != nil {
		t.Fatalf("refresh: %v", err)
	}
	after, err := auth.ValidateAccessToken(refreshed.AccessToken)
	if err != nil {
		t.Fatalf("validate refreshed: %v", err)
	}
	if before.SessionID != after.SessionID {
		t.Fatalf("sid changed on refresh: %s -> %s", before.SessionID, after.SessionID)
	}
	if _, err := auth.Refresh(ctx, tokens.RefreshToken, service.DeviceInfo{}); err == nil {
		t.Fatal("expected old refresh token to fail")
	}
}

func TestRevokeSessionBlocksAccess(t *testing.T) {
	auth, userRepo, db := newTestAuth(t)
	defer db.Close()
	ctx := context.Background()
	user := createTestUser(t, userRepo, "user-c", "c@test.local")

	tokens, err := auth.IssueTokens(ctx, user, service.DeviceInfo{DeviceName: "Phone"})
	if err != nil {
		t.Fatalf("issue: %v", err)
	}
	claims, _ := auth.ValidateAccessToken(tokens.AccessToken)

	if err := auth.RevokeSession(ctx, user.ID, claims.SessionID); err != nil {
		t.Fatalf("revoke: %v", err)
	}
	if err := auth.EnsureSessionActive(ctx, claims.SessionID); err == nil {
		t.Fatal("expected revoked session to fail EnsureSessionActive")
	}
}

func TestRevokeOtherSessionsKeepsCurrent(t *testing.T) {
	auth, userRepo, db := newTestAuth(t)
	defer db.Close()
	ctx := context.Background()
	user := createTestUser(t, userRepo, "user-d", "d@test.local")

	current, err := auth.IssueTokens(ctx, user, service.DeviceInfo{DeviceName: "Current"})
	if err != nil {
		t.Fatalf("issue current: %v", err)
	}
	other, err := auth.IssueTokens(ctx, user, service.DeviceInfo{DeviceName: "Other"})
	if err != nil {
		t.Fatalf("issue other: %v", err)
	}
	currentClaims, _ := auth.ValidateAccessToken(current.AccessToken)
	otherClaims, _ := auth.ValidateAccessToken(other.AccessToken)

	if err := auth.RevokeOtherSessions(ctx, user.ID, currentClaims.SessionID); err != nil {
		t.Fatalf("revoke others: %v", err)
	}
	if err := auth.EnsureSessionActive(ctx, currentClaims.SessionID); err != nil {
		t.Fatalf("current should remain active: %v", err)
	}
	if err := auth.EnsureSessionActive(ctx, otherClaims.SessionID); err == nil {
		t.Fatal("other session should be revoked")
	}

	sessions, err := auth.ListSessions(ctx, user.ID)
	if err != nil {
		t.Fatalf("list: %v", err)
	}
	if len(sessions) != 1 || sessions[0].ID != currentClaims.SessionID {
		t.Fatalf("expected only current session, got %+v", sessions)
	}
}

func TestLegacyRefreshTokenMigratesToSession(t *testing.T) {
	auth, userRepo, db := newTestAuth(t)
	defer db.Close()
	ctx := context.Background()
	user := createTestUser(t, userRepo, "user-e", "e@test.local")

	refresh := "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
	rt := &domain.RefreshToken{
		UserID:    user.ID,
		TokenHash: sha256Hex(refresh),
		ExpiresAt: time.Now().Add(30 * 24 * time.Hour),
	}
	if err := userRepo.CreateRefreshToken(ctx, rt); err != nil {
		t.Fatalf("create legacy refresh: %v", err)
	}

	tokens, err := auth.Refresh(ctx, refresh, service.DeviceInfo{
		DeviceName: "Migrated Desktop",
		DeviceType: domain.DeviceTypeDesktop,
	})
	if err != nil {
		t.Fatalf("refresh legacy: %v", err)
	}
	claims, err := auth.ValidateAccessToken(tokens.AccessToken)
	if err != nil {
		t.Fatalf("validate: %v", err)
	}
	if claims.SessionID == "" {
		t.Fatal("expected sid after migration")
	}
	if err := auth.EnsureSessionActive(ctx, claims.SessionID); err != nil {
		t.Fatalf("migrated session inactive: %v", err)
	}
}
