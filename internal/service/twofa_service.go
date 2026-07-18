package service

import (
	"context"
	"crypto/rand"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"math/big"
	"strings"
	"time"

	"github.com/abdullaabdullazade/freedrive/internal/adminsettings"
	"github.com/abdullaabdullazade/freedrive/internal/domain"
	"github.com/abdullaabdullazade/freedrive/internal/email"
	"golang.org/x/crypto/bcrypt"
)

var (
	ErrIPBlocked          = errors.New("access denied from this network")
	Err2FARequired        = errors.New("two-factor authentication required")
	Err2FAUnavailable     = errors.New("email two-factor authentication is unavailable")
	ErrInvalid2FACode     = errors.New("invalid or expired verification code")
	ErrCannotDisable2FA   = errors.New("two-factor authentication is required by administrator")
)

// TwoFAChallenge is returned when login requires email verification.
type TwoFAChallenge struct {
	ChallengeID string `json:"challenge_id"`
	EmailMasked string `json:"email_masked"`
}

// Needs2FA reports whether the user must complete email 2FA at login.
func Needs2FA(user *domain.User) bool {
	if user == nil {
		return false
	}
	return adminsettings.Require2FA() || user.Email2FAEnabled
}

// CanSetEmail2FA reports whether a user may toggle their personal 2FA setting.
func CanSetEmail2FA(enabled bool) error {
	if !enabled && adminsettings.Require2FA() {
		return ErrCannotDisable2FA
	}
	return nil
}

// VerifyCredentials checks email/password without issuing tokens.
func (s *AuthService) VerifyCredentials(ctx context.Context, email, password string) (*domain.User, error) {
	email = strings.ToLower(strings.TrimSpace(email))
	user, err := s.userRepo.GetByEmail(ctx, email)
	if err != nil {
		return nil, err
	}
	if user == nil {
		return nil, ErrInvalidCredentials
	}
	if err := bcrypt.CompareHashAndPassword([]byte(user.PasswordHash), []byte(password)); err != nil {
		return nil, ErrInvalidCredentials
	}
	if user.Suspended {
		return nil, ErrAccountSuspended
	}
	return user, nil
}

// StartEmail2FA creates a challenge and emails a verification code.
func (s *AuthService) StartEmail2FA(ctx context.Context, user *domain.User) (*TwoFAChallenge, error) {
	if user == nil {
		return nil, ErrInvalidCredentials
	}
	if !adminsettings.SMTPConfigured() {
		return nil, Err2FAUnavailable
	}

	code, err := randomDigits(6)
	if err != nil {
		return nil, err
	}
	hash := hash2FACode(code)
	expiresAt := time.Now().Add(10 * time.Minute)

	_ = s.email2faRepo.DeleteByUserID(ctx, user.ID)
	challenge := &domain.Email2FAChallenge{
		UserID:    user.ID,
		CodeHash:  hash,
		ExpiresAt: expiresAt,
	}
	if err := s.email2faRepo.Create(ctx, challenge); err != nil {
		return nil, err
	}

	subject := "Your FreeDrive sign-in code"
	body := fmt.Sprintf(
		"Hello %s,\n\nYour FreeDrive sign-in verification code is:\n\n%s\n\nThis code expires in 10 minutes. If you did not try to sign in, change your password immediately.\n",
		chooseAuthDisplayName(user.Username, user.Email),
		code,
	)
	go func() {
		_ = email.SendFromSettings(user.Email, subject, body)
	}()

	return &TwoFAChallenge{
		ChallengeID: challenge.ID,
		EmailMasked: maskAuthEmail(user.Email),
	}, nil
}

// VerifyEmail2FA validates a challenge code and issues tokens.
func (s *AuthService) VerifyEmail2FA(ctx context.Context, challengeID, code string, device DeviceInfo) (*TokenPair, *domain.User, error) {
	challengeID = strings.TrimSpace(challengeID)
	code = strings.TrimSpace(code)
	if challengeID == "" || code == "" {
		return nil, nil, ErrInvalid2FACode
	}

	entry, err := s.email2faRepo.GetByID(ctx, challengeID)
	if err != nil || entry == nil {
		return nil, nil, ErrInvalid2FACode
	}
	if time.Now().After(entry.ExpiresAt) {
		_ = s.email2faRepo.DeleteByID(ctx, entry.ID)
		return nil, nil, ErrInvalid2FACode
	}
	if entry.CodeHash != hash2FACode(code) {
		return nil, nil, ErrInvalid2FACode
	}

	user, err := s.userRepo.GetByID(ctx, entry.UserID)
	if err != nil || user == nil {
		return nil, nil, ErrInvalid2FACode
	}
	if user.Suspended {
		return nil, nil, ErrAccountSuspended
	}

	_ = s.email2faRepo.DeleteByID(ctx, entry.ID)
	tokens, err := s.IssueTokens(ctx, user, device)
	if err != nil {
		return nil, nil, err
	}
	return tokens, user, nil
}

func hash2FACode(code string) string {
	h := sha256.Sum256([]byte(code))
	return hex.EncodeToString(h[:])
}

func randomDigits(n int) (string, error) {
	out := make([]byte, n)
	for i := 0; i < n; i++ {
		v, err := rand.Int(rand.Reader, big.NewInt(10))
		if err != nil {
			return "", err
		}
		out[i] = byte('0' + v.Int64())
	}
	return string(out), nil
}

func maskAuthEmail(addr string) string {
	addr = strings.TrimSpace(strings.ToLower(addr))
	parts := strings.Split(addr, "@")
	if len(parts) != 2 || parts[0] == "" || parts[1] == "" {
		return "***"
	}
	if len(parts[0]) == 1 {
		return "*@" + parts[1]
	}
	return parts[0][:1] + "***@" + parts[1]
}

func chooseAuthDisplayName(username, email string) string {
	if strings.TrimSpace(username) != "" {
		return username
	}
	return email
}
