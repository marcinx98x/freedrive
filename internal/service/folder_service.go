package service

import (
	"context"
	"fmt"

	"github.com/abdullaabdullazade/freedrive/internal/domain"
	"github.com/abdullaabdullazade/freedrive/internal/repository"
	"github.com/abdullaabdullazade/freedrive/internal/storage"
)

// FolderService handles folder business logic.
type FolderService struct {
	folderRepo   repository.FolderRepository
	fileRepo     repository.FileRepository
	userRepo     repository.UserRepository
	storage      *storage.DiskStorage
	activityRepo repository.ActivityRepository
	computerRepo repository.ComputerRepository
	access       *AccessService
}

// NewFolderService creates a new folder service.
func NewFolderService(
	folderRepo repository.FolderRepository,
	fileRepo repository.FileRepository,
	userRepo repository.UserRepository,
	store *storage.DiskStorage,
	activityRepo repository.ActivityRepository,
	computerRepo repository.ComputerRepository,
	access *AccessService,
) *FolderService {
	return &FolderService{
		folderRepo:   folderRepo,
		fileRepo:     fileRepo,
		userRepo:     userRepo,
		storage:      store,
		activityRepo: activityRepo,
		computerRepo: computerRepo,
		access:       access,
	}
}

// Create creates a new folder.
func (s *FolderService) Create(ctx context.Context, folder *domain.Folder) error {
	if folder.ParentID != nil && *folder.ParentID != "" {
		if err := s.access.CanWriteFolder(ctx, *folder.ParentID, folder.OwnerID); err != nil {
			return err
		}
	}
	if err := s.folderRepo.Create(ctx, folder); err != nil {
		return err
	}

	_ = s.activityRepo.Create(ctx, &domain.ActivityLog{
		UserID:     folder.OwnerID,
		Action:     domain.ActionCreate,
		TargetType: "folder",
		TargetID:   folder.ID,
		TargetName: folder.Name,
	})
	return nil
}

// GetContents returns a folder's children (folders + files).
func (s *FolderService) GetContents(ctx context.Context, folderID *string, ownerID string) (*domain.FolderContents, error) {
	var folder *domain.Folder
	listOwner := ownerID
	if folderID != nil {
		var err error
		folder, err = s.folderRepo.GetByID(ctx, *folderID)
		if err != nil {
			return nil, err
		}
		if folder == nil {
			return nil, fmt.Errorf("folder not found")
		}
		if err := s.access.CanReadFolder(ctx, *folderID, ownerID); err != nil {
			return nil, err
		}
		listOwner = folder.OwnerID
	}

	folders, err := s.folderRepo.GetChildren(ctx, folderID, listOwner)
	if err != nil {
		return nil, err
	}

	files, err := s.fileRepo.GetByFolderID(ctx, folderID, listOwner)
	if err != nil {
		return nil, err
	}

	return &domain.FolderContents{
		Folder:  folder,
		Folders: folders,
		Files:   files,
	}, nil
}

// ListAll returns all of an owner's folders (flat), optionally filtered by name.
func (s *FolderService) ListAll(ctx context.Context, ownerID, search string) ([]domain.Folder, error) {
	return s.folderRepo.ListAll(ctx, ownerID, search)
}

// Rename renames a folder.
func (s *FolderService) Rename(ctx context.Context, folderID, ownerID, newName string) error {
	if err := s.access.CanWriteFolder(ctx, folderID, ownerID); err != nil {
		return err
	}
	folder, err := s.folderRepo.GetByID(ctx, folderID)
	if err != nil {
		return err
	}
	if folder == nil {
		return fmt.Errorf("folder not found")
	}

	folder.Name = newName
	return s.folderRepo.Update(ctx, folder)
}

// Move moves a folder to a new parent.
func (s *FolderService) Move(ctx context.Context, folderID, ownerID string, newParentID *string) error {
	if err := s.access.CanWriteFolder(ctx, folderID, ownerID); err != nil {
		return err
	}
	folder, err := s.folderRepo.GetByID(ctx, folderID)
	if err != nil {
		return err
	}
	if folder == nil {
		return fmt.Errorf("folder not found")
	}

	if newParentID != nil && *newParentID != "" {
		if err := s.access.CanWriteFolder(ctx, *newParentID, ownerID); err != nil {
			return err
		}
	}

	isComputerRoot, err := s.computerRepo.IsComputerRoot(ctx, folderID)
	if err != nil {
		return err
	}
	if isComputerRoot {
		return fmt.Errorf("cannot move a registered computer folder")
	}

	sourceInComputer, err := s.computerRepo.IsInComputerTree(ctx, folderID)
	if err != nil {
		return err
	}

	var destInComputer bool
	if newParentID != nil {
		destInComputer, err = s.computerRepo.IsInComputerTree(ctx, *newParentID)
		if err != nil {
			return err
		}
	}

	if sourceInComputer != destInComputer {
		return fmt.Errorf("cannot move items between My Drive and Computers")
	}

	// Prevent moving folder into its own descendant
	if newParentID != nil {
		isDesc, err := s.folderRepo.IsDescendant(ctx, folderID, *newParentID)
		if err != nil {
			return err
		}
		if isDesc {
			return fmt.Errorf("cannot move folder into its own subfolder")
		}
	}

	folder.ParentID = newParentID
	return s.folderRepo.Update(ctx, folder)
}

// Delete moves a folder and all its contents to trash.
func (s *FolderService) Delete(ctx context.Context, folderID, ownerID string) error {
	if err := s.access.CanWriteFolder(ctx, folderID, ownerID); err != nil {
		return err
	}
	folder, err := s.folderRepo.GetByID(ctx, folderID)
	if err != nil {
		return err
	}
	if folder == nil {
		return fmt.Errorf("folder not found")
	}

	// Soft-delete the whole subtree so files are not orphaned to root and the
	// folder can be restored as a whole.
	if err := s.folderRepo.MoveToTrash(ctx, folderID); err != nil {
		return err
	}

	_ = s.activityRepo.Create(ctx, &domain.ActivityLog{
		UserID:     ownerID,
		Action:     domain.ActionDelete,
		TargetType: "folder",
		TargetID:   folderID,
		TargetName: folder.Name,
	})
	return nil
}

// ListTrash returns the roots of the owner's trashed folder subtrees.
func (s *FolderService) ListTrash(ctx context.Context, ownerID string) ([]domain.Folder, error) {
	return s.folderRepo.GetTrashedFolders(ctx, ownerID)
}

// Restore restores a trashed folder and its whole subtree.
func (s *FolderService) Restore(ctx context.Context, folderID, ownerID string) error {
	folder, err := s.folderRepo.GetByID(ctx, folderID)
	if err != nil {
		return err
	}
	if folder == nil || folder.OwnerID != ownerID {
		return fmt.Errorf("folder not found")
	}

	if err := s.folderRepo.RestoreFromTrash(ctx, folderID); err != nil {
		return err
	}

	_ = s.activityRepo.Create(ctx, &domain.ActivityLog{
		UserID:     ownerID,
		Action:     domain.ActionRestore,
		TargetType: "folder",
		TargetID:   folderID,
		TargetName: folder.Name,
	})
	return nil
}

// PermanentDelete removes a folder subtree together with every contained file's
// blob, refunds quota, and deletes the folder rows.
func (s *FolderService) PermanentDelete(ctx context.Context, folderID, ownerID string) error {
	folder, err := s.folderRepo.GetByID(ctx, folderID)
	if err != nil {
		return err
	}
	if folder == nil || folder.OwnerID != ownerID {
		return fmt.Errorf("folder not found")
	}

	ids, err := s.folderRepo.ListSubtreeIDs(ctx, folderID)
	if err != nil {
		return err
	}

	files, err := s.fileRepo.GetByFolderIDs(ctx, ids)
	if err != nil {
		return err
	}

	for _, f := range files {
		if f.OwnerID != ownerID {
			continue
		}
		// Remove version blobs
		versions, _ := s.fileRepo.GetVersions(ctx, f.ID)
		for _, v := range versions {
			_ = s.storage.Delete(v.BlobPath)
		}
		// Remove main blob
		_ = s.storage.Delete(f.BlobPath)
		if err := s.fileRepo.Delete(ctx, f.ID); err != nil {
			return err
		}
		_ = s.userRepo.UpdateUsedBytes(ctx, ownerID, -f.EncryptedSize)
	}

	// Deleting the root folder cascades removal of descendant folder rows.
	if err := s.folderRepo.Delete(ctx, folderID); err != nil {
		return err
	}

	_ = s.activityRepo.Create(ctx, &domain.ActivityLog{
		UserID:     ownerID,
		Action:     domain.ActionDelete,
		TargetType: "folder",
		TargetID:   folderID,
		TargetName: folder.Name,
	})
	return nil
}

// ToggleStar toggles starred status.
func (s *FolderService) ToggleStar(ctx context.Context, folderID, ownerID string) error {
	if err := s.access.CanWriteFolder(ctx, folderID, ownerID); err != nil {
		return err
	}
	folder, err := s.folderRepo.GetByID(ctx, folderID)
	if err != nil {
		return err
	}
	if folder == nil {
		return fmt.Errorf("folder not found")
	}

	folder.IsStarred = !folder.IsStarred
	return s.folderRepo.Update(ctx, folder)
}

// SetColor sets a folder's color label.
func (s *FolderService) SetColor(ctx context.Context, folderID, ownerID, color string) error {
	if err := s.access.CanWriteFolder(ctx, folderID, ownerID); err != nil {
		return err
	}
	folder, err := s.folderRepo.GetByID(ctx, folderID)
	if err != nil {
		return err
	}
	if folder == nil {
		return fmt.Errorf("folder not found")
	}

	folder.Color = color
	return s.folderRepo.Update(ctx, folder)
}

// GetBreadcrumb returns the path from root to the given folder.
func (s *FolderService) GetBreadcrumb(ctx context.Context, folderID, userID string) ([]domain.Breadcrumb, error) {
	if err := s.access.CanReadFolder(ctx, folderID, userID); err != nil {
		return nil, err
	}
	return s.folderRepo.GetBreadcrumb(ctx, folderID)
}
