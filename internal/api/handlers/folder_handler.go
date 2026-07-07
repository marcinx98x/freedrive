package handlers

import (
	"encoding/json"
	"net/http"

	"github.com/abdullaabdullazade/freedrive/internal/api/middleware"
	"github.com/abdullaabdullazade/freedrive/internal/domain"
	"github.com/abdullaabdullazade/freedrive/internal/service"
	"github.com/go-chi/chi/v5"
)

// FolderHandler handles folder endpoints.
type FolderHandler struct {
	folderService *service.FolderService
}

// NewFolderHandler creates a new folder handler.
func NewFolderHandler(folderService *service.FolderService) *FolderHandler {
	return &FolderHandler{folderService: folderService}
}

// Create handles POST /api/v1/folders
func (h *FolderHandler) Create(w http.ResponseWriter, r *http.Request) {
	userID := middleware.GetUserID(r.Context())

	var req struct {
		Name     string  `json:"name"`
		ParentID *string `json:"parent_id"`
		Color    string  `json:"color"`
	}
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeError(w, "invalid request body", http.StatusBadRequest)
		return
	}

	if req.Name == "" {
		writeError(w, "folder name is required", http.StatusBadRequest)
		return
	}

	folder := &domain.Folder{
		Name:     req.Name,
		ParentID: req.ParentID,
		OwnerID:  userID,
		Color:    req.Color,
	}

	if err := h.folderService.Create(r.Context(), folder); err != nil {
		writeError(w, err.Error(), http.StatusBadRequest)
		return
	}

	writeJSON(w, http.StatusCreated, folder)
}

// Get handles GET /api/v1/folders/{id}
func (h *FolderHandler) Get(w http.ResponseWriter, r *http.Request) {
	folderID := chi.URLParam(r, "id")
	userID := middleware.GetUserID(r.Context())

	contents, err := h.folderService.GetContents(r.Context(), &folderID, userID)
	if err != nil {
		writeError(w, err.Error(), http.StatusNotFound)
		return
	}

	writeJSON(w, http.StatusOK, contents)
}

// GetRoot handles GET /api/v1/folders/root
func (h *FolderHandler) GetRoot(w http.ResponseWriter, r *http.Request) {
	userID := middleware.GetUserID(r.Context())

	contents, err := h.folderService.GetContents(r.Context(), nil, userID)
	if err != nil {
		writeError(w, err.Error(), http.StatusInternalServerError)
		return
	}

	writeJSON(w, http.StatusOK, contents)
}

// ListAll handles GET /api/v1/folders/all
func (h *FolderHandler) ListAll(w http.ResponseWriter, r *http.Request) {
	userID := middleware.GetUserID(r.Context())
	search := r.URL.Query().Get("search")

	folders, err := h.folderService.ListAll(r.Context(), userID, search)
	if err != nil {
		writeError(w, "failed to list folders", http.StatusInternalServerError)
		return
	}

	writeJSON(w, http.StatusOK, map[string]interface{}{
		"folders": folders,
	})
}

// Update handles PATCH /api/v1/folders/{id}
func (h *FolderHandler) Update(w http.ResponseWriter, r *http.Request) {
	folderID := chi.URLParam(r, "id")
	userID := middleware.GetUserID(r.Context())

	var req struct {
		Name     *string `json:"name"`
		ParentID *string `json:"parent_id"`
		Color    *string `json:"color"`
		Star     *bool   `json:"is_starred"`
	}
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeError(w, "invalid request body", http.StatusBadRequest)
		return
	}

	if req.Name != nil {
		if err := h.folderService.Rename(r.Context(), folderID, userID, *req.Name); err != nil {
			writeError(w, err.Error(), http.StatusBadRequest)
			return
		}
	}

	if req.ParentID != nil {
		if err := h.folderService.Move(r.Context(), folderID, userID, req.ParentID); err != nil {
			writeError(w, err.Error(), http.StatusBadRequest)
			return
		}
	}

	if req.Color != nil {
		if err := h.folderService.SetColor(r.Context(), folderID, userID, *req.Color); err != nil {
			writeError(w, err.Error(), http.StatusBadRequest)
			return
		}
	}

	if req.Star != nil {
		if err := h.folderService.ToggleStar(r.Context(), folderID, userID); err != nil {
			writeError(w, err.Error(), http.StatusBadRequest)
			return
		}
	}

	writeJSON(w, http.StatusOK, map[string]string{"message": "updated"})
}

// Delete handles DELETE /api/v1/folders/{id}
func (h *FolderHandler) Delete(w http.ResponseWriter, r *http.Request) {
	folderID := chi.URLParam(r, "id")
	userID := middleware.GetUserID(r.Context())

	if err := h.folderService.Delete(r.Context(), folderID, userID); err != nil {
		writeError(w, err.Error(), http.StatusBadRequest)
		return
	}

	writeJSON(w, http.StatusOK, map[string]string{"message": "deleted"})
}

// GetBreadcrumb handles GET /api/v1/folders/{id}/breadcrumb
func (h *FolderHandler) GetBreadcrumb(w http.ResponseWriter, r *http.Request) {
	folderID := chi.URLParam(r, "id")

	crumbs, err := h.folderService.GetBreadcrumb(r.Context(), folderID)
	if err != nil {
		writeError(w, err.Error(), http.StatusInternalServerError)
		return
	}

	writeJSON(w, http.StatusOK, map[string]interface{}{"breadcrumb": crumbs})
}
