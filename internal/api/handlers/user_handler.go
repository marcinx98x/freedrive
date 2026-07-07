package handlers

import (
	"net/http"
	"path/filepath"
	"strings"

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

	// Break usage down by category over the same (non-trashed) file set so the
	// four buckets add up exactly to used_bytes.
	breakdown := map[string]int64{"images": 0, "videos": 0, "documents": 0, "other": 0}
	fileCount := 0
	if metas, err := h.fileRepo.ListFileMetaByOwner(r.Context(), userID); err == nil {
		fileCount = len(metas)
		for _, m := range metas {
			breakdown[storageCategory(m.MimeType, m.Name)] += m.EncryptedSize
		}
	}

	writeJSON(w, http.StatusOK, map[string]interface{}{
		"used_bytes":  used,
		"total_bytes": user.QuotaBytes,
		"free_bytes":  user.QuotaBytes - used,
		"breakdown":   breakdown,
		"file_count":  fileCount,
	})
}

// storageCategory maps a file to one of four storage buckets (images, videos,
// documents, other) using both MIME type and extension. Mirrors the frontend
// getStorageCategory so the UI and backend agree. Unknown types (audio,
// archives, binaries, fonts, ...) fall into "other".
func storageCategory(mime, name string) string {
	mt := strings.ToLower(strings.TrimSpace(mime))
	ext := strings.ToLower(strings.TrimPrefix(filepath.Ext(name), "."))

	if strings.HasPrefix(mt, "image/") || imageExts[ext] {
		return "images"
	}
	if strings.HasPrefix(mt, "video/") || videoExts[ext] {
		return "videos"
	}
	if mt == "application/pdf" ||
		strings.HasPrefix(mt, "text/") ||
		strings.Contains(mt, "word") || strings.Contains(mt, "opendocument") ||
		strings.Contains(mt, "spreadsheet") || strings.Contains(mt, "ms-excel") || strings.Contains(mt, "spreadsheetml") ||
		strings.Contains(mt, "presentation") || strings.Contains(mt, "powerpoint") ||
		mt == "application/json" ||
		docExts[ext] {
		return "documents"
	}
	return "other"
}

func toSet(items []string) map[string]bool {
	s := make(map[string]bool, len(items))
	for _, it := range items {
		s[it] = true
	}
	return s
}

var (
	imageExts = toSet([]string{"jpg", "jpeg", "png", "gif", "webp", "bmp", "svg", "ico", "tif", "tiff", "heic", "heif", "avif", "raw", "cr2", "nef", "arw", "dng", "psd"})
	videoExts = toSet([]string{"mp4", "mov", "avi", "mkv", "webm", "flv", "wmv", "m4v", "mpg", "mpeg", "3gp", "ts", "m2ts", "ogv", "mts"})
	docExts   = toSet([]string{
		"pdf", "doc", "docx", "odt", "rtf", "txt", "md", "markdown", "pages",
		"ppt", "pptx", "odp", "key",
		"xls", "xlsx", "ods", "csv", "tsv", "numbers",
		"json", "jsonc", "xml", "html", "htm", "css", "js", "ts", "jsx", "tsx",
		"py", "c", "cpp", "h", "hpp", "sh", "bash", "go", "java", "php", "rb", "swift",
		"ini", "cfg", "conf", "yml", "yaml", "toml", "log",
	})
)
