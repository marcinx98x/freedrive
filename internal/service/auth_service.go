package service

import (
	"context"
	"crypto/rand"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"time"

	"github.com/abdullaabdullazade/freedrive/internal/domain"
	"github.com/abdullaabdullazade/freedrive/internal/repository"
	"github.com/golang-jwt/jwt/v5"
	"golang.org/x/crypto/bcrypt"
)

var (
	ErrInvalidCredentials = errors.New("invalid email or password")
	ErrUserExists         = errors.New("user with this email already exists")
	ErrInvalidInvite      = errors.New("invalid or expired invite code")
	ErrInvalidToken       = errors.New("invalid or expired token")
)

// AuthService handles authentication and authorization.
type AuthService struct {
	userRepo  repository.UserRepository
	jwtSecret []byte
}

// NewAuthService creates a new auth service.
func NewAuthService(userRepo repository.UserRepository, jwtSecret string) *AuthService {
	return &AuthService{
		userRepo:  userRepo,
		jwtSecret: []byte(jwtSecret),
	}
}

// TokenPair contains access and refresh tokens.
type TokenPair struct {
	AccessToken  string `json:"access_token"`
	RefreshToken string `json:"refresh_token"`
	ExpiresIn    int    `json:"expires_in"`
}

// Register creates a new user account.
func (s *AuthService) Register(ctx context.Context, email, username, password, inviteCode string) (*domain.User, error) {
	// Check if first user (skip invite for first user)
	userCount, err := s.userRepo.Count(ctx)
	if err != nil {
		return nil, fmt.Errorf("count users: %w", err)
	}

	role := domain.RoleUser
	var quotaBytes int64 = 10737418240 // 10 GB default
	if userCount == 0 {
		role = domain.RoleAdmin
	} else if inviteCode != "" {
		invite, err := s.userRepo.GetInviteByCode(ctx, inviteCode)
		if err != nil {
			return nil, fmt.Errorf("get invite: %w", err)
		}
		if invite == nil {
			return nil, ErrInvalidInvite
		}
		if invite.MaxUses > 0 && invite.UsedCount >= invite.MaxUses {
			return nil, ErrInvalidInvite
		}
		if invite.ExpiresAt != nil && invite.ExpiresAt.Before(time.Now()) {
			return nil, ErrInvalidInvite
		}
		role = invite.Role
		if invite.QuotaBytes > 0 {
			quotaBytes = invite.QuotaBytes
		}
		if err := s.userRepo.IncrementInviteUsage(ctx, invite.ID); err != nil {
			return nil, err
		}
	} else if userCount > 0 {
		return nil, ErrInvalidInvite
	}

	// Check if email exists
	existing, err := s.userRepo.GetByEmail(ctx, email)
	if err != nil {
		return nil, err
	}
	if existing != nil {
		return nil, ErrUserExists
	}

	// Hash password
	hash, err := bcrypt.GenerateFromPassword([]byte(password), bcrypt.DefaultCost)
	if err != nil {
		return nil, fmt.Errorf("hash password: %w", err)
	}

	user := &domain.User{
		Email:        email,
		Username:     username,
		PasswordHash: string(hash),
		Role:         role,
		QuotaBytes:   quotaBytes,
	}

	if err := s.userRepo.Create(ctx, user); err != nil {
		return nil, fmt.Errorf("create user: %w", err)
	}

	return user, nil
}

// Login authenticates a user and returns JWT tokens.
func (s *AuthService) Login(ctx context.Context, email, password string) (*TokenPair, *domain.User, error) {
	user, err := s.userRepo.GetByEmail(ctx, email)
	if err != nil {
		return nil, nil, err
	}
	if user == nil {
		return nil, nil, ErrInvalidCredentials
	}

	if err := bcrypt.CompareHashAndPassword([]byte(user.PasswordHash), []byte(password)); err != nil {
		return nil, nil, ErrInvalidCredentials
	}

	// Update last login
	now := time.Now()
	user.LastLoginAt = &now
	if err := s.userRepo.Update(ctx, user); err != nil {
		return nil, nil, err
	}

	tokens, err := s.generateTokenPair(ctx, user)
	if err != nil {
		return nil, nil, err
	}

	return tokens, user, nil
}

// Refresh generates a new access token from a valid refresh token.
func (s *AuthService) Refresh(ctx context.Context, refreshToken string) (*TokenPair, error) {
	tokenHash := hashToken(refreshToken)
	stored, err := s.userRepo.GetRefreshToken(ctx, tokenHash)
	if err != nil {
		return nil, err
	}
	if stored == nil || stored.ExpiresAt.Before(time.Now()) {
		return nil, ErrInvalidToken
	}

	// Delete old refresh token (rotation)
	if err := s.userRepo.DeleteRefreshToken(ctx, tokenHash); err != nil {
		return nil, err
	}

	user, err := s.userRepo.GetByID(ctx, stored.UserID)
	if err != nil || user == nil {
		return nil, ErrInvalidToken
	}

	return s.generateTokenPair(ctx, user)
}

// Logout revokes a refresh token.
func (s *AuthService) Logout(ctx context.Context, refreshToken string) error {
	tokenHash := hashToken(refreshToken)
	return s.userRepo.DeleteRefreshToken(ctx, tokenHash)
}

// ResetPasswordByEmail updates user's password using email.
func (s *AuthService) ResetPasswordByEmail(ctx context.Context, email, newPassword string) error {
	user, err := s.userRepo.GetByEmail(ctx, email)
	if err != nil {
		return err
	}
	if user == nil {
		return ErrInvalidCredentials
	}

	hash, err := bcrypt.GenerateFromPassword([]byte(newPassword), bcrypt.DefaultCost)
	if err != nil {
		return fmt.Errorf("hash password: %w", err)
	}
	user.PasswordHash = string(hash)
	return s.userRepo.Update(ctx, user)
}

// ValidateAccessToken validates a JWT access token and returns the claims.
func (s *AuthService) ValidateAccessToken(tokenStr string) (*Claims, error) {
	token, err := jwt.ParseWithClaims(tokenStr, &Claims{}, func(t *jwt.Token) (interface{}, error) {
		if _, ok := t.Method.(*jwt.SigningMethodHMAC); !ok {
			return nil, fmt.Errorf("unexpected signing method: %v", t.Header["alg"])
		}
		return s.jwtSecret, nil
	})
	if err != nil {
		return nil, ErrInvalidToken
	}

	claims, ok := token.Claims.(*Claims)
	if !ok || !token.Valid {
		return nil, ErrInvalidToken
	}

	return claims, nil
}

// EnsureAdmin creates the admin user from config if no users exist.
func (s *AuthService) EnsureAdmin(ctx context.Context, email, password string) error {
	count, err := s.userRepo.Count(ctx)
	if err != nil {
		return err
	}
	if count > 0 {
		return nil
	}

	_, err = s.Register(ctx, email, "Admin", password, "")
	return err
}

// Claims represents JWT claims.
type Claims struct {
	UserID   string      `json:"uid"`
	Email    string      `json:"email"`
	Username string      `json:"username"`
	Role     domain.Role `json:"role"`
	jwt.RegisteredClaims
}

func (s *AuthService) generateTokenPair(ctx context.Context, user *domain.User) (*TokenPair, error) {
	// Access token (15 minutes)
	accessClaims := &Claims{
		UserID:   user.ID,
		Email:    user.Email,
		Username: user.Username,
		Role:     user.Role,
		RegisteredClaims: jwt.RegisteredClaims{
			ExpiresAt: jwt.NewNumericDate(time.Now().Add(24 * time.Hour)),
			IssuedAt:  jwt.NewNumericDate(time.Now()),
			Issuer:    "freedrive",
		},
	}
	accessToken := jwt.NewWithClaims(jwt.SigningMethodHS256, accessClaims)
	accessStr, err := accessToken.SignedString(s.jwtSecret)
	if err != nil {
		return nil, fmt.Errorf("sign access token: %w", err)
	}

	// Refresh token (7 days)
	refreshBytes := make([]byte, 32)
	if _, err := rand.Read(refreshBytes); err != nil {
		return nil, err
	}
	refreshStr := hex.EncodeToString(refreshBytes)
	refreshHash := hashToken(refreshStr)

	rt := &domain.RefreshToken{
		UserID:    user.ID,
		TokenHash: refreshHash,
		ExpiresAt: time.Now().Add(30 * 24 * time.Hour),
	}
	if err := s.userRepo.CreateRefreshToken(ctx, rt); err != nil {
		return nil, err
	}

	return &TokenPair{
		AccessToken:  accessStr,
		RefreshToken: refreshStr,
		ExpiresIn:    86400, // 24 hours in seconds
	}, nil
}

func hashToken(token string) string {
	h := sha256.Sum256([]byte(token))
	return hex.EncodeToString(h[:])
}
