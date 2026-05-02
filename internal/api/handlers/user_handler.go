package handlers

import (
	"net/http"

	"github.com/abdullaxows/freedrive/internal/api/middleware"
	"github.com/abdullaxows/freedrive/internal/repository"
)

// UserHandler handles user-specific endpoints.
type UserHandler struct {
	userRepo repository.UserRepository
}

// NewUserHandler creates a new user handler.
func NewUserHandler(userRepo repository.UserRepository) *UserHandler {
	return &UserHandler{userRepo: userRepo}
}

// MyStorage handles GET /api/v1/me/storage — returns the current user's quota and usage.
func (h *UserHandler) MyStorage(w http.ResponseWriter, r *http.Request) {
	userID := middleware.GetUserID(r.Context())
	if userID == "" {
		writeError(w, "unauthorized", http.StatusUnauthorized)
		return
	}

	user, err := h.userRepo.GetByID(r.Context(), userID)
	if err != nil || user == nil {
		writeError(w, "user not found", http.StatusNotFound)
		return
	}

	writeJSON(w, http.StatusOK, map[string]interface{}{
		"used_bytes":  user.UsedBytes,
		"total_bytes": user.QuotaBytes,
		"free_bytes":  user.QuotaBytes - user.UsedBytes,
	})
}
