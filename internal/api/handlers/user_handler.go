package handlers

import (
	"net/http"

	"github.com/abdullaabdullazade/freedrive/internal/api/middleware"
	"github.com/abdullaabdullazade/freedrive/internal/repository"
)

// UserHandler handles user-specific endpoints.
type UserHandler struct {
	userRepo repository.UserRepository
	fileRepo repository.FileRepository
}

// NewUserHandler creates a new user handler.
func NewUserHandler(userRepo repository.UserRepository, fileRepo repository.FileRepository) *UserHandler {
	return &UserHandler{userRepo: userRepo, fileRepo: fileRepo}
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

	// Compute real usage from the files table so the value is accurate even if
	// the tracked used_bytes counter has drifted; reconcile it when different.
	used, err := h.fileRepo.SumEncryptedSizeByOwner(r.Context(), userID)
	if err != nil {
		used = user.UsedBytes
	} else if used != user.UsedBytes {
		_ = h.userRepo.UpdateUsedBytes(r.Context(), userID, used-user.UsedBytes)
	}

	writeJSON(w, http.StatusOK, map[string]interface{}{
		"used_bytes":  used,
		"total_bytes": user.QuotaBytes,
		"free_bytes":  user.QuotaBytes - used,
	})
}
