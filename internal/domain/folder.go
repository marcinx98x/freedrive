package domain

import "time"

// Folder represents a directory in the virtual file system.
type Folder struct {
	ID        string    `json:"id"`
	Name      string    `json:"name"`
	ParentID  *string   `json:"parent_id,omitempty"`
	OwnerID   string    `json:"owner_id"`
	Color     string    `json:"color,omitempty"`
	IsStarred bool      `json:"is_starred"`
	CreatedAt time.Time `json:"created_at"`
	UpdatedAt time.Time `json:"updated_at"`
}

// FolderContents represents a folder with its child items.
type FolderContents struct {
	Folder  *Folder  `json:"folder"`
	Folders []Folder `json:"folders"`
	Files   []File   `json:"files"`
}

// Breadcrumb represents one segment in a folder path.
type Breadcrumb struct {
	ID   string `json:"id"`
	Name string `json:"name"`
}
