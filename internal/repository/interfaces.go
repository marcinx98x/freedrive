package repository

import (
	"context"

	"github.com/abdullaabdullazade/freedrive/internal/domain"
)

// UserRepository defines data access for users.
type UserRepository interface {
	Create(ctx context.Context, user *domain.User) error
	GetByID(ctx context.Context, id string) (*domain.User, error)
	GetByEmail(ctx context.Context, email string) (*domain.User, error)
	Update(ctx context.Context, user *domain.User) error
	Delete(ctx context.Context, id string) error
	List(ctx context.Context) ([]domain.User, error)
	UpdateUsedBytes(ctx context.Context, userID string, delta int64) error
	Count(ctx context.Context) (int, error)

	// Refresh tokens
	CreateRefreshToken(ctx context.Context, token *domain.RefreshToken) error
	GetRefreshToken(ctx context.Context, tokenHash string) (*domain.RefreshToken, error)
	DeleteRefreshToken(ctx context.Context, tokenHash string) error
	DeleteUserRefreshTokens(ctx context.Context, userID string) error

	// Invite links
	CreateInvite(ctx context.Context, invite *domain.InviteLink) error
	GetInviteByCode(ctx context.Context, code string) (*domain.InviteLink, error)
	IncrementInviteUsage(ctx context.Context, id string) error
	ListInvites(ctx context.Context) ([]domain.InviteLink, error)
	DeleteInvite(ctx context.Context, id string) error
}

// FileRepository defines data access for files.
type FileRepository interface {
	Create(ctx context.Context, file *domain.File) error
	GetByID(ctx context.Context, id string) (*domain.File, error)
	Update(ctx context.Context, file *domain.File) error
	Delete(ctx context.Context, id string) error
	List(ctx context.Context, opts domain.FileListOptions) ([]domain.File, int, error)
	GetByFolderID(ctx context.Context, folderID *string, ownerID string) ([]domain.File, error)
	GetByFolderIDs(ctx context.Context, folderIDs []string) ([]domain.File, error)
	MoveToTrash(ctx context.Context, id string) error
	RestoreFromTrash(ctx context.Context, id string) error
	GetTrashedFiles(ctx context.Context, ownerID string) ([]domain.File, error)
	PurgeOldTrashed(ctx context.Context, days int) ([]domain.File, error)
	CountByOwner(ctx context.Context, ownerID string) (int, error)
	SumEncryptedSizeByOwner(ctx context.Context, ownerID string) (int64, error)
	ListFileMetaByOwner(ctx context.Context, ownerID string) ([]domain.FileMeta, error)

	// Versioning
	CreateVersion(ctx context.Context, version *domain.FileVersion) error
	GetVersions(ctx context.Context, fileID string) ([]domain.FileVersion, error)
	GetVersion(ctx context.Context, fileID string, version int) (*domain.FileVersion, error)
	DeleteOldVersions(ctx context.Context, fileID string, keepCount int) ([]domain.FileVersion, error)
}

// ComputerRepository defines data access for registered desktop devices.
type ComputerRepository interface {
	Create(ctx context.Context, computer *domain.Computer) error
	GetByID(ctx context.Context, id string) (*domain.Computer, error)
	ListByOwner(ctx context.Context, ownerID string) ([]domain.Computer, error)
	IsComputerRoot(ctx context.Context, folderID string) (bool, error)
	IsInComputerTree(ctx context.Context, folderID string) (bool, error)
}

// FolderRepository defines data access for folders.
type FolderRepository interface {
	Create(ctx context.Context, folder *domain.Folder) error
	GetByID(ctx context.Context, id string) (*domain.Folder, error)
	Update(ctx context.Context, folder *domain.Folder) error
	Delete(ctx context.Context, id string) error
	GetChildren(ctx context.Context, parentID *string, ownerID string) ([]domain.Folder, error)
	ListAll(ctx context.Context, ownerID, search string) ([]domain.Folder, error)
	GetBreadcrumb(ctx context.Context, id string) ([]domain.Breadcrumb, error)
	IsDescendant(ctx context.Context, folderID, potentialParentID string) (bool, error)
	MoveToTrash(ctx context.Context, id string) error
	RestoreFromTrash(ctx context.Context, id string) error
	GetTrashedFolders(ctx context.Context, ownerID string) ([]domain.Folder, error)
	ListSubtreeIDs(ctx context.Context, id string) ([]string, error)
}

// ShareRepository defines data access for sharing.
type ShareRepository interface {
	CreateLink(ctx context.Context, link *domain.ShareLink) error
	GetLinkByToken(ctx context.Context, token string) (*domain.ShareLink, error)
	GetLinkByID(ctx context.Context, id string) (*domain.ShareLink, error)
	UpdateLink(ctx context.Context, link *domain.ShareLink) error
	DeleteLink(ctx context.Context, id string) error
	ListLinksByUser(ctx context.Context, userID string) ([]domain.ShareLink, error)
	IncrementDownloadCount(ctx context.Context, id string) error

	CreateUserShare(ctx context.Context, share *domain.UserShare) error
	DeleteUserShare(ctx context.Context, id string) error
	ListSharedByUser(ctx context.Context, userID string) ([]domain.UserShare, error)
	ListSharedWithUser(ctx context.Context, userID string) ([]domain.UserShare, error)
}

// CommentRepository defines data access for comments.
type CommentRepository interface {
	Create(ctx context.Context, comment *domain.Comment) error
	GetByFileID(ctx context.Context, fileID string) ([]domain.Comment, error)
	Delete(ctx context.Context, id string) error
}

// ActivityRepository defines data access for activity logs.
type ActivityRepository interface {
	Create(ctx context.Context, log *domain.ActivityLog) error
	List(ctx context.Context, userID string, page, pageSize int) ([]domain.ActivityLog, int, error)
	ListAll(ctx context.Context, page, pageSize int) ([]domain.ActivityLog, int, error)
}
