package handlers

import (
	"sync"
	"time"
)

type passwordResetEntry struct {
	Email     string
	ExpiresAt time.Time
}

var (
	passwordResetMu    sync.Mutex
	passwordResetStore = map[string]passwordResetEntry{}
)

func createPasswordResetToken(email string) string {
	token := generateRandomString(64)
	passwordResetMu.Lock()
	passwordResetStore[token] = passwordResetEntry{
		Email:     email,
		ExpiresAt: time.Now().Add(30 * time.Minute),
	}
	passwordResetMu.Unlock()
	return token
}

func consumePasswordResetToken(token, email string) bool {
	now := time.Now()
	passwordResetMu.Lock()
	defer passwordResetMu.Unlock()

	entry, ok := passwordResetStore[token]
	if !ok {
		return false
	}
	delete(passwordResetStore, token)

	if now.After(entry.ExpiresAt) {
		return false
	}
	return entry.Email == email
}
