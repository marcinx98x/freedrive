package domain

import "time"

// Email2FAChallenge stores a pending email 2FA verification code.
type Email2FAChallenge struct {
	ID        string    `json:"id"`
	UserID    string    `json:"user_id"`
	CodeHash  string    `json:"-"`
	ExpiresAt time.Time `json:"expires_at"`
	CreatedAt time.Time `json:"created_at"`
}
