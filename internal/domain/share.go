package domain

import "time"

// Permission represents an access level for sharing.
type Permission string

const (
	PermRead   Permission = "read"
	PermWrite  Permission = "write"
	PermUpload Permission = "upload"
)

// ShareLink represents a public or password-protected share link.
type ShareLink struct {
	ID            string     `json:"id"`
	FileID        *string    `json:"file_id,omitempty"`
	FolderID      *string    `json:"folder_id,omitempty"`
	CreatedBy     string     `json:"created_by"`
	Token         string     `json:"token"`
	Permission    Permission `json:"permission"`
	PasswordHash  string     `json:"-"`
	HasPassword   bool       `json:"has_password"`
	ExpiresAt     *time.Time `json:"expires_at,omitempty"`
	MaxDownloads  *int       `json:"max_downloads,omitempty"`
	DownloadCount int        `json:"download_count"`
	IsActive      bool       `json:"is_active"`
	CreatedAt     time.Time  `json:"created_at"`
}

// UserShare represents a direct share with a specific user.
type UserShare struct {
	ID         string     `json:"id"`
	FileID     *string    `json:"file_id,omitempty"`
	FolderID   *string    `json:"folder_id,omitempty"`
	SharedBy   string     `json:"shared_by"`
	SharedWith string     `json:"shared_with"`
	Permission Permission `json:"permission"`
	CreatedAt  time.Time  `json:"created_at"`
}
