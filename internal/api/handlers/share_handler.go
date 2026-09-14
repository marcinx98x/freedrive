package handlers

import (
	"encoding/json"
	"fmt"
	"io"
	"log"
	"net/http"
	"net/url"
	"strconv"
	"strings"
	"time"

	"github.com/abdullaabdullazade/freedrive/internal/adminsettings"
	"github.com/abdullaabdullazade/freedrive/internal/email"

	"github.com/abdullaabdullazade/freedrive/internal/api/middleware"
	"github.com/abdullaabdullazade/freedrive/internal/domain"
	"github.com/abdullaabdullazade/freedrive/internal/repository"
	"github.com/abdullaabdullazade/freedrive/internal/service"
	"github.com/abdullaabdullazade/freedrive/internal/storage"
	"github.com/go-chi/chi/v5"
)

// ShareHandler handles sharing endpoints.
type ShareHandler struct {
	shareService *service.ShareService
	fileRepo     repository.FileRepository
	userRepo     repository.UserRepository
	storage      *storage.DiskStorage
	bandwidth    repository.BandwidthRepository
}

// NewShareHandler creates a share handler.
func NewShareHandler(
	shareService *service.ShareService,
	fileRepo repository.FileRepository,
	userRepo repository.UserRepository,
	store *storage.DiskStorage,
	bandwidth repository.BandwidthRepository,
) *ShareHandler {
	return &ShareHandler{
		shareService: shareService,
		fileRepo:     fileRepo,
		userRepo:     userRepo,
		storage:      store,
		bandwidth:    bandwidth,
	}
}

// CreateUserShare handles POST /api/v1/shares/users
func (h *ShareHandler) CreateUserShare(w http.ResponseWriter, r *http.Request) {
	userID := middleware.GetUserID(r.Context())
	var req struct {
		FileID      *string `json:"file_id"`
		FolderID    *string `json:"folder_id"`
		SharedWith  string  `json:"shared_with"`
		SharedEmail string  `json:"shared_email"`
		Permission  string  `json:"permission"`
	}
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeError(w, "invalid request body", http.StatusBadRequest)
		return
	}

	recipientID := strings.TrimSpace(req.SharedWith)
	if recipientID == "" && strings.TrimSpace(req.SharedEmail) != "" {
		user, err := h.userRepo.GetByEmail(r.Context(), strings.ToLower(strings.TrimSpace(req.SharedEmail)))
		if err != nil || user == nil {
			writeError(w, "recipient not found", http.StatusBadRequest)
			return
		}
		recipientID = user.ID
	}
	if recipientID == "" {
		writeError(w, "shared_with or shared_email required", http.StatusBadRequest)
		return
	}

	share := &domain.UserShare{
		FileID:     req.FileID,
		FolderID:   req.FolderID,
		SharedWith: recipientID,
		Permission: service.ParsePermission(req.Permission),
	}

	created, err := h.shareService.CreateUserShare(r.Context(), userID, share)
	if err != nil {
		writeError(w, err.Error(), http.StatusBadRequest)
		return
	}
	writeJSON(w, http.StatusCreated, created)
}

// SendShares handles POST /api/v1/shares/send
func (h *ShareHandler) SendShares(w http.ResponseWriter, r *http.Request) {
	userID := middleware.GetUserID(r.Context())
	var req struct {
		FileID     *string `json:"file_id"`
		FolderID   *string `json:"folder_id"`
		ItemName   string  `json:"item_name"`
		Notify     bool    `json:"notify"`
		Message    string  `json:"message"`
		Recipients []struct {
			Email      string `json:"email"`
			Permission string `json:"permission"`
		} `json:"recipients"`
	}
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeError(w, "invalid request body", http.StatusBadRequest)
		return
	}
	recipients := make([]service.ShareRecipient, 0, len(req.Recipients))
	for _, rec := range req.Recipients {
		recipients = append(recipients, service.ShareRecipient{Email: rec.Email, Permission: rec.Permission})
	}
	shared, invited, deliveries, err := h.shareService.DeliverShares(r.Context(), userID, strings.TrimSpace(req.Message), req.FileID, req.FolderID, recipients)
	if err != nil {
		writeError(w, err.Error(), http.StatusBadRequest)
		return
	}

	emailError := ""
	if req.Notify && len(deliveries) > 0 {
		actorName := "Someone"
		if actor, aerr := h.userRepo.GetByID(r.Context(), userID); aerr == nil && actor != nil {
			if strings.TrimSpace(actor.Username) != "" {
				actorName = actor.Username
			} else if actor.Email != "" {
				actorName = actor.Email
			}
		}
		itemName := strings.TrimSpace(req.ItemName)
		if itemName == "" {
			itemName = "an item"
		}
		base := siteBaseURL(adminsettings.SiteURL(), r)
		openID := ""
		if req.FileID != nil && *req.FileID != "" {
			openID = *req.FileID
		} else if req.FolderID != nil && *req.FolderID != "" {
			openID = *req.FolderID
		}
		for _, delivery := range deliveries {
			body := fmt.Sprintf("%s shared \"%s\" with you on FreeDrive.\n", actorName, itemName)
			if msg := strings.TrimSpace(req.Message); msg != "" {
				body += "\n" + msg + "\n"
			}
			body += fmt.Sprintf("\nSign in to open it:\n%s/#/open/%s\n", base, url.PathEscape(openID))
			if !delivery.Existing {
				body += fmt.Sprintf("\nDon't have an account? Register with this email and the item will be waiting:\n%s/#/register?email=%s\n", base, url.QueryEscape(delivery.Email))
			}
			if err := email.SendFromSettings(delivery.Email, fmt.Sprintf("%s shared \"%s\" with you", actorName, itemName), body); err != nil && emailError == "" {
				emailError = err.Error()
			}
		}
	}

	writeJSON(w, http.StatusOK, map[string]interface{}{
		"shared":      shared,
		"invited":     invited,
		"email_error": emailError,
	})
}

// GetItemSettings handles GET /api/v1/shares/settings
func (h *ShareHandler) GetItemSettings(w http.ResponseWriter, r *http.Request) {
	fileID := strings.TrimSpace(r.URL.Query().Get("file_id"))
	folderID := strings.TrimSpace(r.URL.Query().Get("folder_id"))
	settings, err := h.shareService.ItemSettings(r.Context(), fileID, folderID)
	if err != nil {
		writeError(w, err.Error(), http.StatusBadRequest)
		return
	}
	writeJSON(w, http.StatusOK, settings)
}

// SaveItemSettings handles PUT /api/v1/shares/settings
func (h *ShareHandler) SaveItemSettings(w http.ResponseWriter, r *http.Request) {
	userID := middleware.GetUserID(r.Context())
	var req struct {
		FileID             string `json:"file_id"`
		FolderID           string `json:"folder_id"`
		EditorsCanShare    bool   `json:"editors_can_share"`
		EditorsCanDownload bool   `json:"editors_can_download"`
		ViewersCanDownload bool   `json:"viewers_can_download"`
	}
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeError(w, "invalid request body", http.StatusBadRequest)
		return
	}
	settings := domain.ShareItemSettings{
		EditorsCanShare:    req.EditorsCanShare,
		EditorsCanDownload: req.EditorsCanDownload,
		ViewersCanDownload: req.ViewersCanDownload,
	}
	if err := h.shareService.SaveItemSettings(r.Context(), userID, req.FileID, req.FolderID, settings); err != nil {
		writeError(w, err.Error(), http.StatusBadRequest)
		return
	}
	writeJSON(w, http.StatusOK, settings)
}

// WebAccess handles GET /api/v1/shares/access
func (h *ShareHandler) WebAccess(w http.ResponseWriter, r *http.Request) {
	userID := middleware.GetUserID(r.Context())
	fileID := strings.TrimSpace(r.URL.Query().Get("file_id"))
	folderID := strings.TrimSpace(r.URL.Query().Get("folder_id"))
	canShare, canDownload, err := h.shareService.WebAccess(r.Context(), userID, fileID, folderID)
	if err != nil {
		writeError(w, err.Error(), http.StatusBadRequest)
		return
	}
	writeJSON(w, http.StatusOK, map[string]bool{
		"can_share":    canShare,
		"can_download": canDownload,
	})
}

// DeleteUserShare handles DELETE /api/v1/shares/users/{id}
func (h *ShareHandler) DeleteUserShare(w http.ResponseWriter, r *http.Request) {
	userID := middleware.GetUserID(r.Context())
	shareID := chi.URLParam(r, "id")
	if err := h.shareService.DeleteUserShare(r.Context(), userID, shareID); err != nil {
		writeError(w, err.Error(), http.StatusBadRequest)
		return
	}
	writeJSON(w, http.StatusOK, map[string]string{"status": "deleted"})
}

// UpdateUserShare handles PATCH /api/v1/shares/users/{id}
func (h *ShareHandler) UpdateUserShare(w http.ResponseWriter, r *http.Request) {
	userID := middleware.GetUserID(r.Context())
	shareID := chi.URLParam(r, "id")
	var req struct {
		Permission string `json:"permission"`
	}
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeError(w, "invalid request body", http.StatusBadRequest)
		return
	}
	if strings.TrimSpace(req.Permission) == "" {
		writeError(w, "permission is required", http.StatusBadRequest)
		return
	}
	updated, err := h.shareService.UpdateUserShare(r.Context(), userID, shareID, service.ParsePermission(req.Permission))
	if err != nil {
		writeError(w, err.Error(), http.StatusBadRequest)
		return
	}
	writeJSON(w, http.StatusOK, updated)
}

// SharedWithMe handles GET /api/v1/shares/with-me
func (h *ShareHandler) SharedWithMe(w http.ResponseWriter, r *http.Request) {
	userID := middleware.GetUserID(r.Context())
	items, err := h.shareService.ListSharedWithMe(r.Context(), userID)
	if err != nil {
		writeError(w, "failed to list shares", http.StatusInternalServerError)
		return
	}
	writeJSON(w, http.StatusOK, map[string]interface{}{"items": items})
}

// SharedByMe handles GET /api/v1/shares/by-me
func (h *ShareHandler) SharedByMe(w http.ResponseWriter, r *http.Request) {
	userID := middleware.GetUserID(r.Context())
	items, err := h.shareService.ListSharedByMe(r.Context(), userID)
	if err != nil {
		writeError(w, "failed to list shares", http.StatusInternalServerError)
		return
	}
	writeJSON(w, http.StatusOK, map[string]interface{}{"items": items})
}

// CreateLink handles POST /api/v1/shares/links
func (h *ShareHandler) CreateLink(w http.ResponseWriter, r *http.Request) {
	userID := middleware.GetUserID(r.Context())
	var req struct {
		FileID       *string `json:"file_id"`
		FolderID     *string `json:"folder_id"`
		Permission   string  `json:"permission"`
		Password     string  `json:"password"`
		MaxDownloads *int    `json:"max_downloads"`
		ExpiresAt    *string `json:"expires_at"`
	}
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeError(w, "invalid request body", http.StatusBadRequest)
		return
	}

	link := &domain.ShareLink{
		FileID:       req.FileID,
		FolderID:     req.FolderID,
		Permission:   service.ParsePermission(req.Permission),
		MaxDownloads: req.MaxDownloads,
	}
	if req.ExpiresAt != nil && *req.ExpiresAt != "" {
		t, err := time.Parse(time.RFC3339, *req.ExpiresAt)
		if err == nil {
			link.ExpiresAt = &t
		}
	}

	created, err := h.shareService.CreateLink(r.Context(), userID, link, req.Password)
	if err != nil {
		writeError(w, err.Error(), http.StatusBadRequest)
		return
	}
	writeJSON(w, http.StatusCreated, created)
}

// DeleteLink handles DELETE /api/v1/shares/links/{id}
func (h *ShareHandler) DeleteLink(w http.ResponseWriter, r *http.Request) {
	userID := middleware.GetUserID(r.Context())
	linkID := chi.URLParam(r, "id")
	if err := h.shareService.DeleteLink(r.Context(), userID, linkID); err != nil {
		writeError(w, err.Error(), http.StatusBadRequest)
		return
	}
	writeJSON(w, http.StatusOK, map[string]string{"status": "deleted"})
}

// ListLinks handles GET /api/v1/shares/links
func (h *ShareHandler) ListLinks(w http.ResponseWriter, r *http.Request) {
	userID := middleware.GetUserID(r.Context())
	links, err := h.shareService.ListLinks(r.Context(), userID)
	if err != nil {
		writeError(w, "failed to list links", http.StatusInternalServerError)
		return
	}
	writeJSON(w, http.StatusOK, map[string]interface{}{"links": links})
}

// PublicLinkInfo handles GET /api/v1/public/share/{token}
func (h *ShareHandler) PublicLinkInfo(w http.ResponseWriter, r *http.Request) {
	token := chi.URLParam(r, "token")
	password := r.Header.Get("X-Share-Password")
	link, err := h.shareService.ResolveLink(r.Context(), token, password)
	if err != nil {
		writeError(w, "invalid or expired share link", http.StatusBadRequest)
		return
	}
	writeJSON(w, http.StatusOK, link)
}

// PublicLinkDownload handles GET /api/v1/public/share/{token}/download
func (h *ShareHandler) PublicLinkDownload(w http.ResponseWriter, r *http.Request) {
	token := chi.URLParam(r, "token")
	password := r.Header.Get("X-Share-Password")
	link, err := h.shareService.ResolveLink(r.Context(), token, password)
	if err != nil || link.FileID == nil {
		writeError(w, "invalid or expired share link", http.StatusBadRequest)
		return
	}

	file, err := h.fileRepo.GetByID(r.Context(), *link.FileID)
	if err != nil || file == nil {
		writeError(w, "file not found", http.StatusNotFound)
		return
	}

	reader, err := h.storage.Get(file.BlobPath)
	if err != nil {
		writeError(w, "failed to read file", http.StatusInternalServerError)
		return
	}
	defer reader.Close()

	_ = h.shareService.RecordLinkDownload(r.Context(), link.ID)

	if h.bandwidth != nil && file.OwnerID != "" && file.EncryptedSize > 0 {
		if err := h.bandwidth.AddDownload(r.Context(), file.OwnerID, file.EncryptedSize); err != nil {
			log.Printf("bandwidth download user=%s bytes=%d: %v", file.OwnerID, file.EncryptedSize, err)
		}
	}

	w.Header().Set("Content-Type", "application/octet-stream")
	w.Header().Set("Content-Disposition", "attachment; filename=\""+file.Name+"\"")
	w.Header().Set("Content-Length", strconv.FormatInt(file.EncryptedSize, 10))
	w.Header().Set("X-File-IV", file.IV)
	w.Header().Set("X-File-Mime", file.MimeType)
	w.Header().Set("X-Original-Size", strconv.FormatInt(file.Size, 10))
	io.Copy(w, reader)
}
