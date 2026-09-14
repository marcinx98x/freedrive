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

// ShareInvite is a pending share for an email that has no account yet.
type ShareInvite struct {
	ID         string     `json:"id"`
	Email      string     `json:"email"`
	FileID     *string    `json:"file_id,omitempty"`
	FolderID   *string    `json:"folder_id,omitempty"`
	SharedBy   string     `json:"shared_by"`
	Permission Permission `json:"permission"`
	Token      string     `json:"token"`
	Message    string     `json:"message,omitempty"`
	CreatedAt  time.Time  `json:"created_at"`
	ClaimedAt  *time.Time `json:"claimed_at,omitempty"`
}

// ShareItemSettings stores per-item sharing restrictions shown in the web dialog.
type ShareItemSettings struct {
	EditorsCanShare    bool `json:"editors_can_share"`
	EditorsCanDownload bool `json:"editors_can_download"`
	ViewersCanDownload bool `json:"viewers_can_download"`
}

// DefaultShareItemSettings matches the Google dialog: all access allowed.
func DefaultShareItemSettings() ShareItemSettings {
	return ShareItemSettings{
		EditorsCanShare:    true,
		EditorsCanDownload: true,
		ViewersCanDownload: true,
	}
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
